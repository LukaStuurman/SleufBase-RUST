//! Interactive PDOK RGB aerial photography via the official EPSG:28992 WMTS
//! tile pyramid. Large DXF template exports deliberately retain the exact
//! BBOX WMS path; this renderer is only for interactive pan/zoom.
//!
//! WMTS is preferred by PDOK for interactive use. The same 256px tile is
//! reused after pan/zoom, with a coarse level as the first visual placeholder
//! and the target-resolution tiles progressively composited in world space.
use crate::{input, model::Bounds};
use image::{DynamicImage, ImageDecoder, ImageFormat, RgbaImage};
use std::{
    collections::{HashMap, VecDeque}, fs, io::{Cursor, Write}, path::{Path, PathBuf},
    sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, mpsc, Arc, Condvar, Mutex, OnceLock},
    time::{Duration, SystemTime},
};

const BASE: &str = "https://service.pdok.nl/hwh/luchtfotorgb/wmts/v1_0";
const LAYER: &str = "Actueel_orthoHR";
const TILE: u32 = 256;
// The fixed Dutch RD New WMTS tile matrix (Geonovum); units are metres.
const ORIGIN_X: f64 = -285401.92;
const ORIGIN_Y: f64 = 903402.0;
const LEVEL_ZERO_RESOLUTION: f64 = 3440.64;
const MAX_ZOOM: u8 = 19;
const MAX_VIEW_TILES: usize = 256;
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const FRESH_FOR: Duration = Duration::from_secs(6 * 60 * 60);
const MAX_MEMORY_TILES: usize = 192;
// A tiny, low-priority ring outside the current viewport prevents white
// strips during a nearby pan/zoom. Never prefetch full-resolution tiles.
const MAX_NEIGHBOR_TILES: usize = 16;
const MAX_NEIGHBOR_MEMORY: usize = 32;
const MAX_DISK_TILES: usize = 4096;
const MAX_DISK_BYTES: u64 = 512 * 1024 * 1024;
static DISK_WRITES: AtomicUsize = AtomicUsize::new(0);

// Cache writes must never hold a network worker, decode thread or map
// rendering progress callback. A single bounded background writer retains
// atomic publication; a full queue simply skips optional disk caching.
struct SaveJob {
    root: PathBuf,
    path: PathBuf,
    bytes: Vec<u8>,
}
fn tile_writer()->Option<&'static mpsc::SyncSender<SaveJob>> {
    static WRITER:OnceLock<Option<mpsc::SyncSender<SaveJob>>>=OnceLock::new();
    WRITER.get_or_init(||{
        let (tx,rx)=mpsc::sync_channel::<SaveJob>(8);
        std::thread::Builder::new()
            .name("sleufbase-luchtfoto-wmts-cache".into())
            .spawn(move||{
                while let Ok(job)=rx.recv(){
                    let saved=crate::storage::atomic_write(&job.path,true,|file|
                        file.write_all(&job.bytes).map_err(|e|e.to_string()));
                    if saved.is_ok() && DISK_WRITES.fetch_add(1,Ordering::Relaxed)%64==0 {
                        prune_disk(&job.root);
                    }
                }
            }).ok().map(|_|tx)
    }).as_ref()
}
fn remember_disk(root:&Path, id:TileId, bytes:Vec<u8>){
    if let Some(writer)=tile_writer(){
        let _=writer.try_send(SaveJob{
            root:root.to_path_buf(),path:id.path(root),bytes,
        });
    }
}

// Disk use stays bounded even after years of browsing many locations.
// The known 3-level zoom/column/row path is traversed without following
// symlinks, and in-flight atomic temporary files are never touched.
fn prune_disk(root:&Path){
    let mut entries=Vec::new();
    let mut total=0u64;
    let Ok(levels)=fs::read_dir(root) else{return;};
    for level in levels.flatten(){
        if !level.file_type().is_ok_and(|ft|ft.is_dir()){continue;}
        let Ok(columns)=fs::read_dir(level.path()) else{continue;};
        for column in columns.flatten(){
            if !column.file_type().is_ok_and(|ft|ft.is_dir()){continue;}
            let Ok(files)=fs::read_dir(column.path()) else{continue;};
            for file in files.flatten(){
                let path=file.path();
                if path.extension().and_then(|value|value.to_str())!=Some("img"){continue;}
                let Ok(info)=fs::symlink_metadata(&path) else{continue;};
                if !info.file_type().is_file(){continue;}
                total=total.saturating_add(info.len());
                entries.push((path,info.len(),info.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
            }
        }
    }
    if total<=MAX_DISK_BYTES && entries.len()<=MAX_DISK_TILES{return;}
    entries.sort_by_key(|(_,_,when)|*when);
    let mut count=entries.len();
    for (path,bytes,_) in entries{
        if count<=MAX_DISK_TILES && total<=MAX_DISK_BYTES{break;}
        if fs::remove_file(&path).is_ok(){
            count=count.saturating_sub(1);
            total=total.saturating_sub(bytes);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TileId { pub zoom: u8, pub col: u32, pub row: u32 }
impl TileId {
    fn valid(self) -> bool {
        self.zoom <= MAX_ZOOM && self.col < 1u32 << self.zoom
            && self.row < 1u32 << self.zoom
    }
    fn path(self, root: &Path) -> PathBuf {
        root.join(self.zoom.to_string()).join(self.col.to_string())
            .join(format!("{}.img", self.row))
    }
    fn url(self) -> Result<reqwest::Url, String> {
        if !self.valid() {return Err("Ongeldige PDOK-luchtfototegel".into());}
        let mut url=reqwest::Url::parse(BASE).map_err(|e|e.to_string())?;
        url.query_pairs_mut()
            .append_pair("SERVICE","WMTS")
            .append_pair("REQUEST","GetTile")
            .append_pair("VERSION","1.0.0")
            .append_pair("LAYER",LAYER)
            .append_pair("STYLE","default")
            .append_pair("TILEMATRIXSET","EPSG:28992")
            .append_pair("TILEMATRIX",&self.zoom.to_string())
            .append_pair("TILEROW",&self.row.to_string())
            .append_pair("TILECOL",&self.col.to_string())
            .append_pair("FORMAT","image/jpeg");
        Ok(url)
    }
}
fn resolution(zoom: u8) -> f64 {
    LEVEL_ZERO_RESOLUTION / f64::from(1u32 << zoom)
}
fn span(zoom: u8) -> f64 { resolution(zoom) * f64::from(TILE) }
#[derive(Clone, Copy, Debug)]
struct Grid {
    zoom: u8,
    min_col: u32, max_col: u32, min_row: u32, max_row: u32,
}
impl Grid {
    fn ids(self) -> impl Iterator<Item=TileId> {
        (self.min_row..=self.max_row).flat_map(move |row|
            (self.min_col..=self.max_col).map(move |col|TileId{
                zoom:self.zoom,col,row
            }))
    }
    fn count(self) -> usize {
        (self.max_col-self.min_col+1) as usize * (self.max_row-self.min_row+1) as usize
    }
}
fn grid(bounds:Bounds,zoom:u8) -> Option<Grid> {
    let size=span(zoom);
    let limit=f64::from(1u32<<zoom);
    let col_min=((bounds.min_x-ORIGIN_X)/size).floor();
    let col_max=((bounds.max_x-ORIGIN_X)/size - 1e-10).floor();
    let row_min=((ORIGIN_Y-bounds.max_y)/size).floor();
    let row_max=((ORIGIN_Y-bounds.min_y)/size - 1e-10).floor();
    if ![col_min,col_max,row_min,row_max].iter().all(|v|
        v.is_finite()&&*v>=0.&&*v<limit) || col_min>col_max || row_min>row_max {
        return None;
    }
    Some(Grid {zoom,min_col:col_min as u32,max_col:col_max as u32,
        min_row:row_min as u32,max_row:row_max as u32})
}
fn plan(bounds:Bounds,width:u32,height:u32) -> Result<Grid,String> {
    if width==0 || height==0 || width>8192 || height>8192
        || u64::from(width)*u64::from(height)>16_777_216
        || [bounds.min_x,bounds.min_y,bounds.max_x,bounds.max_y].iter()
            .any(|n|!n.is_finite())
        || bounds.min_x>=bounds.max_x || bounds.min_y>=bounds.max_y {
        return Err("Ongeldige interactieve PDOK-uitsnede".into());
    }
    let pixel=((bounds.max_x-bounds.min_x)/f64::from(width))
        .max((bounds.max_y-bounds.min_y)/f64::from(height));
    let wanted=(LEVEL_ZERO_RESOLUTION/pixel).log2().ceil().clamp(0., f64::from(MAX_ZOOM)) as u8;
    for level in (0..=wanted).rev(){
        if let Some(grid)=grid(bounds,level){
            if grid.count()<=MAX_VIEW_TILES{return Ok(grid);}
        }
    }
    Err("Gebied valt buiten de Nederlandse PDOK-tegelkaart".into())
}
// A handful of coarse tiles can cover a cold viewport while detailed tiles
// load. Keep this bounded: parent work shares the existing PDOK request slots.
fn coarse_preview_grid(bounds:Bounds,target:Grid)->Option<Grid>{
    if target.zoom<2{return None;}
    (0..target.zoom).rev().filter_map(|zoom|grid(bounds,zoom))
        .find(|coarse|coarse.count()<=4)
}
// Predict only coarse tiles in a bounded ring around the current view:
// this also covers a typical one-level zoom-out without expanding the
// expensive full-resolution viewport or downloading an entire neighborhood.
fn neighbor_preview_ids(bounds:Bounds,target:Grid)->Vec<TileId>{
    if target.zoom==0{return Vec::new();}
    let w=bounds.max_x-bounds.min_x;
    let h=bounds.max_y-bounds.min_y;
    let padded=Bounds {
        min_x:bounds.min_x-w*0.6,max_x:bounds.max_x+w*0.6,
        min_y:bounds.min_y-h*0.6,max_y:bounds.max_y+h*0.6,
    };
    let Some(first)=(0..target.zoom).rev().filter_map(|zoom|grid(padded,zoom))
        .find(|g|g.count()<=12) else{return Vec::new();};
    let mut ids=first.ids().collect::<Vec<_>>();
    // Keep one extra low-res level for a second zoom-out step. Limit
    // network work even on unusually large 4K/ultrawide viewports.
    if first.zoom>0 {
        if let Some(second)=grid(padded,first.zoom-1) {
            let remaining=MAX_NEIGHBOR_TILES.saturating_sub(ids.len());
            ids.extend(second.ids().take(remaining));
        }
    }
    ids
}
fn decode(bytes:&[u8]) -> Result<RgbaImage,String> {
    if bytes.len() as u64>MAX_BYTES {return Err("PDOK-tegel overschrijdt de downloadlimiet".into());}
    let format=image::guess_format(bytes).map_err(|_|"PDOK gaf geen afbeelding terug")?;
    if !matches!(format,ImageFormat::Png|ImageFormat::Jpeg){
        return Err("PDOK-tegel heeft een onbekend formaat".into());
    }
    let mut reader=image::ImageReader::with_format(Cursor::new(bytes),format);
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(TILE);
    limits.max_image_height=Some(TILE);
    limits.max_alloc=Some(4*1024*1024);
    reader.limits(limits);
    let decoder=reader.into_decoder().map_err(|e|format!("PDOK-tegel lezen: {e}"))?;
    if decoder.dimensions()!=(TILE,TILE) {
        return Err("PDOK-tegel heeft onjuiste afmetingen".into());
    }
    DynamicImage::from_decoder(decoder)
        .map_err(|e|format!("PDOK-tegel decoderen: {e}"))
        .map(|image|image.to_rgba8())
}
fn read_disk(id:TileId, root:&Path, cancel:&AtomicBool)->Option<(Arc<RgbaImage>,SystemTime,bool)>{
    let path=id.path(root);
    let meta=fs::symlink_metadata(&path).ok()?;
    if !meta.file_type().is_file() || meta.len()==0 || meta.len()>MAX_BYTES {return None;}
    let timestamp=meta.modified().ok()?;
    let bytes=input::read(&path,MAX_BYTES,"PDOK WMTS-cache",cancel).ok()?;
    let image=decode(&bytes).ok()?;
    let fresh=SystemTime::now().duration_since(timestamp).map_or(true,|age|age<=FRESH_FOR);
    Some((Arc::new(image),timestamp,fresh))
}
fn http_client()->Result<&'static reqwest::blocking::Client,String>{
    static CLIENT: OnceLock<Result<reqwest::blocking::Client,String>>=OnceLock::new();
    CLIENT.get_or_init(||reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(12)).connect_timeout(Duration::from_secs(5))
        .user_agent(concat!("SleufBase-Rust/",env!("CARGO_PKG_VERSION")))
        .build().map_err(|e|e.to_string())).as_ref().map_err(Clone::clone)
}
fn download(id:TileId,cancel:&AtomicBool)->Result<Vec<u8>,String>{
    input::check(cancel)?;
    let url=id.url()?;
    let client=http_client()?.clone();
    // Share PDOK's existing global four-request ceiling with WMS/WFS.
    crate::cadastral::cancellable_request(cancel,move|stop|{
        let mut response=client.get(url).send()
            .map_err(|e|format!("PDOK WMTS niet bereikbaar: {e}"))?
            .error_for_status().map_err(|e|format!("PDOK WMTS HTTP-fout: {e}"))?;
        if response.content_length().is_some_and(|n|n>MAX_BYTES){
            return Err("PDOK WMTS-antwoord te groot".into());
        }
        if !response.headers().get(reqwest::header::CONTENT_TYPE)
            .and_then(|v|v.to_str().ok())
            .is_some_and(|s|s.to_ascii_lowercase().contains("image"))
        {
            return Err("PDOK gaf geen WMTS-afbeelding terug".into());
        }
        input::read_chunks(&mut response,MAX_BYTES,"PDOK WMTS-tegel",&stop)
    })?
}
fn load_tile(id:TileId,root:&Path,cancel:&AtomicBool)
    ->Result<(Arc<RgbaImage>,SystemTime,bool),String>
{
    input::check(cancel)?;
    if let Some((tile,modified,true))=read_disk(id,root,cancel){
        return Ok((tile,modified,false));
    }
    let mut last_error=String::new();
    for attempt in 0..2{
        input::check(cancel)?;
        match download(id,cancel).and_then(|bytes|{
            input::check(cancel)?;
            let tile=Arc::new(decode(&bytes)?);
            // Disk I/O runs in a bounded writer rather than blocking one
            // of the four valuable WMTS network/conversion workers.
            remember_disk(root,id,bytes);
            Ok(tile)
        }){
            Ok(tile)=>return Ok((tile,SystemTime::now(),false)),
            Err(e)=>last_error=e,
        }
        if attempt==0{
            crate::cadastral::cancel_sleep(cancel,Duration::from_millis(180))?;
        }
    }
    if let Some((tile,old,_))=read_disk(id,root,cancel){
        return Ok((tile,old,true));
    }
    Err(last_error)
}
// One latest-only background worker warms a handful of *coarse* nearby tiles
// after the visible map is complete. Any new visible request cancels the job.
// Results stay in a small shared memory LRU; the normal disk cache is optional.
struct NeighborJob { root:PathBuf, ids:Vec<TileId> }
#[derive(Default)]
struct NeighborState {
    pending:Option<NeighborJob>,
    active_cancel:Option<Arc<AtomicBool>>,
}
#[derive(Default)]
struct NeighborCache {
    control:Mutex<NeighborState>,
    wake:Condvar,
    tiles:Mutex<VecDeque<(TileId,SystemTime,Arc<RgbaImage>)>>,
}
impl NeighborCache {
    fn cancel(&self) {
        if let Ok(mut guard)=self.control.lock() {
            if let Some(flag)=&guard.active_cancel {
                flag.store(true,Ordering::Relaxed);
            }
            guard.pending=None;
        }
    }
    fn queue(&self,job:NeighborJob) {
        if job.ids.is_empty(){return;}
        if let Ok(mut guard)=self.control.lock() {
            if let Some(flag)=&guard.active_cancel {
                flag.store(true,Ordering::Relaxed);
            }
            guard.pending=Some(job);
            self.wake.notify_one();
        }
    }
    fn get(&self,id:TileId)->Option<(Arc<RgbaImage>,SystemTime)> {
        let mut tiles=self.tiles.lock().ok()?;
        let index=tiles.iter().position(|item|item.0==id)?;
        let entry=tiles.remove(index)?;
        let result=(Arc::clone(&entry.2),entry.1);
        tiles.push_back(entry);
        Some(result)
    }
    fn insert(&self,id:TileId,time:SystemTime,tile:Arc<RgbaImage>) {
        if let Ok(mut tiles)=self.tiles.lock() {
            tiles.retain(|item|item.0!=id);
            tiles.push_back((id,time,tile));
            while tiles.len()>MAX_NEIGHBOR_MEMORY {tiles.pop_front();}
        }
    }
}
fn neighbor_service()->Option<&'static Arc<NeighborCache>> {
    static SERVICE:OnceLock<Option<Arc<NeighborCache>>>=OnceLock::new();
    SERVICE.get_or_init(||{
        let service=Arc::new(NeighborCache::default());
        let worker=Arc::clone(&service);
        std::thread::Builder::new()
            .name("sleufbase-wmts-neighbor-preload".into())
            .spawn(move||loop {
                let (job,cancel)={
                    let Ok(mut guard)=worker.control.lock() else{return;};
                    while guard.pending.is_none() {
                        guard=match worker.wake.wait(guard) {
                            Ok(guard)=>guard,Err(_)=>return,
                        };
                    }
                    let job=guard.pending.take().unwrap();
                    let cancel=Arc::new(AtomicBool::new(false));
                    guard.active_cancel=Some(Arc::clone(&cancel));
                    (job,cancel)
                };
                for id in job.ids {
                    if cancel.load(Ordering::Relaxed){break;}
                    if worker.get(id).is_some(){continue;}
                    // A single prefetch request at a time leaves the other
                    // PDOK request slots free for the user's visible tiles.
                    if let Ok((tile,when,_))=load_tile(id,&job.root,&cancel) {
                        // A completed download stays reusable even if the
                        // user changed the camera just before it finished.
                        worker.insert(id,when,tile);
                    }
                }
                let Ok(mut guard)=worker.control.lock() else{return;};
                if guard.active_cancel.as_ref().is_some_and(|value|
                    Arc::ptr_eq(value,&cancel)) {
                    guard.active_cancel=None;
                }
            }).ok().map(|_|service)
    }).as_ref()
}

#[derive(Clone)]
pub struct Raster {
    pub bounds:Bounds, pub width:u32, pub height:u32,
    pub rgba:Vec<u8>, pub stale_cache:bool,
}
pub struct Progress<'a> {
    pub bounds:Bounds, pub width:u32, pub height:u32, pub rgba:&'a [u8],
    pub completed:usize, pub total:usize, pub stale_cache:bool,
    /// Updated rectangle in full viewport pixels, [left,top,right,bottom].
    /// None means a full-frame preview (needed to initialize a GPU texture).
    pub changed_region:Option<[u32;4]>,
}
#[derive(Default)]
pub struct ViewportRenderer {
    previous:Option<Raster>,
    memory:VecDeque<(TileId,SystemTime,Arc<RgbaImage>)>,
}
impl ViewportRenderer {
    fn memory_get(&mut self,id:TileId)->Option<(Arc<RgbaImage>,bool)> {
        let i=self.memory.iter().position(|entry|entry.0==id)?;
        let entry=self.memory.remove(i)?;
        let fresh=SystemTime::now().duration_since(entry.1).map_or(true,|d|d<=FRESH_FOR);
        let tile=Arc::clone(&entry.2);
        self.memory.push_back(entry);
        Some((tile,fresh))
    }
    fn remember(&mut self,id:TileId,when:SystemTime,tile:Arc<RgbaImage>){
        self.memory.retain(|entry|entry.0!=id);
        self.memory.push_back((id,when,tile));
        while self.memory.len()>MAX_MEMORY_TILES{self.memory.pop_front();}
    }
    fn from_cache(&mut self,id:TileId,dir:&Path,cancel:&AtomicBool)
        ->Option<(Arc<RgbaImage>,bool)>
    {
        if let Some(result)=self.memory_get(id){return Some(result);}
        if let Some((tile,time))=neighbor_service().and_then(|service|service.get(id)){
            let fresh=SystemTime::now().duration_since(time).map_or(true,|age|age<=FRESH_FOR);
            self.remember(id,time,Arc::clone(&tile));
            return Some((tile,fresh));
        }
        let (tile,time,fresh)=read_disk(id,dir,cancel)?;
        self.remember(id,time,Arc::clone(&tile));
        Some((tile,fresh))
    }
    fn warm_neighbors(&self,bounds:Bounds,target:Grid,root:&Path) {
        let mut ids=neighbor_preview_ids(bounds,target);
        // Already-reused visible/parent tiles need no extra network or disk IO.
        ids.retain(|id|!self.memory.iter().any(|item|item.0==*id));
        if let Some(service)=neighbor_service(){
            service.queue(NeighborJob{root:root.to_path_buf(),ids});
        }
    }
    fn fetch_batch(
        ids:&[TileId],root:&Path,cancel:&AtomicBool,detail_zoom:u8,
        mut on_tile:impl FnMut(TileId,Arc<RgbaImage>,SystemTime,bool)->Result<(),String>,
    )->Result<(),String>{
        if ids.is_empty(){return Ok(());}
        let index=AtomicUsize::new(0);
        // Four bounded requests use the existing PDOK network limit without
        // creating one thread per tile.
        let workers=ids.len().min(4);
        let (tx,rx)=mpsc::sync_channel(workers);
        std::thread::scope(|scope| -> Result<(),String> {
            for _ in 0..workers{
                let tx=tx.clone(); let index=&index;
                scope.spawn(move||{
                    while !cancel.load(Ordering::Relaxed){
                        let position=index.fetch_add(1,Ordering::Relaxed);
                        let Some(&id)=ids.get(position) else{break;};
                        let result=load_tile(id,root,cancel);
                        if tx.send((id,result)).is_err(){break;}
                    }
                });
            }
            drop(tx);
            // Always release the receiver before scoped worker threads join.
            // Otherwise a cancelled or failed render may leave workers blocked
            // forever on a full bounded channel.
            let result=(|| -> Result<(),String> {
                let mut failed_detail=0usize;
                let mut last_detail_error=String::new();
                for _ in 0..ids.len(){
                    let (id,result)=loop {
                        input::check(cancel)?;
                        match rx.recv_timeout(Duration::from_millis(35)){
                            Ok(result)=>break result,
                            Err(mpsc::RecvTimeoutError::Timeout)=>continue,
                            Err(_) =>return Err("PDOK-tegelwerker is gestopt".into()),
                        }
                    };
                    let (tile,modified,stale)=match result {
                        Ok(result)=>result,
                        // A coarse preview is optional. A transient PDOK
                        // error must not throw away successful detailed tiles.
                        Err(_) if id.zoom<detail_zoom=>continue,
                        Err(error)=>{
                            // Preserve successful detail from other workers even
                            // if one PDOK tile times out or returns an error.
                            failed_detail+=1;
                            last_detail_error=error;
                            continue;
                        },
                    };
                    on_tile(id,tile,modified,stale)?;
                }
                if failed_detail>0 {
                    Err(format!("{failed_detail} PDOK-detailtegels mislukt: {last_detail_error}"))
                } else {Ok(())}
            })();
            drop(rx);
            result
        })
    }
    // Overlay only the geographically covered part of the previous image.
    // The uncovered area retains whichever coarse parents were drawn first.
    fn overlay_previous(&self,b:Bounds,width:u32,height:u32,
        target:&mut[u8],rect:[u32;4],cancel:&AtomicBool)->Result<(),String>
    {
        let Some(old)=&self.previous else{return Ok(());};
        if old.width==0 || old.height==0
            || old.rgba.len()!=old.width as usize*old.height as usize*4 {
            return Ok(());
        }
        let dx=(b.max_x-b.min_x)/f64::from(width);
        let dy=(b.max_y-b.min_y)/f64::from(height);
        let sx=f64::from(old.width)/(old.bounds.max_x-old.bounds.min_x);
        let sy=f64::from(old.height)/(old.bounds.max_y-old.bounds.min_y);
        for row in rect[1]..rect[3] {
            input::check(cancel)?;
            let y=b.max_y-(f64::from(row)+0.5)*dy;
            let oy=((old.bounds.max_y-y)*sy).floor();
            if oy<0.||oy>=f64::from(old.height){continue;}
            for col in rect[0]..rect[2]{
                let x=b.min_x+(f64::from(col)+0.5)*dx;
                let ox=((x-old.bounds.min_x)*sx).floor();
                if ox<0.||ox>=f64::from(old.width){continue;}
                let from=(oy as usize*old.width as usize+ox as usize)*4;
                let to=(row as usize*width as usize+col as usize)*4;
                target[to..to+4].copy_from_slice(&old.rgba[from..from+4]);
            }
        }
        Ok(())
    }
    /// Screen-space rectangle touched by a tile. A two-pixel halo redraws
    /// bilinear samples at the tile boundaries when adjoining tiles arrive.
    /// All coordinates are computed in f64 RD, independent of pan/zoom state.
    fn tile_region(bounds:Bounds,width:u32,height:u32,id:TileId)->[u32;4]{
        let span=span(id.zoom);
        let left=ORIGIN_X+f64::from(id.col)*span;
        let right=left+span;
        let top=ORIGIN_Y-f64::from(id.row)*span;
        let bottom=top-span;
        let dx=(bounds.max_x-bounds.min_x)/f64::from(width);
        let dy=(bounds.max_y-bounds.min_y)/f64::from(height);
        let x0=(((left-bounds.min_x)/dx).floor() as i64-2)
            .clamp(0,i64::from(width)) as u32;
        let x1=(((right-bounds.min_x)/dx).ceil() as i64+3)
            .clamp(0,i64::from(width)) as u32;
        let y0=(((bounds.max_y-top)/dy).floor() as i64-2)
            .clamp(0,i64::from(height)) as u32;
        let y1=(((bounds.max_y-bottom)/dy).ceil() as i64+3)
            .clamp(0,i64::from(height)) as u32;
        [x0,y0,x1,y1]
    }
    /// Draw only the changed tile's visible rectangle. Previously, each new
    /// tile led to a full multi-megapixel interpolation and full-frame clone.
    /// This keeps exactly the same bilinear samples as the original painter.
    fn paint_region(bounds:Bounds,width:u32,height:u32,zoom:u8,
        tiles:&HashMap<TileId,Arc<RgbaImage>>,target:&mut [u8],
        rect:[u32;4],cancel:&AtomicBool)->Result<(),String>
    {
        if rect[0]>=rect[2] || rect[1]>=rect[3] {return Ok(());}
        let res=resolution(zoom);
        let tile_span=span(zoom);
        let dx=(bounds.max_x-bounds.min_x)/f64::from(width);
        let dy=(bounds.max_y-bounds.min_y)/f64::from(height);
        for row in rect[1]..rect[3] {
            input::check(cancel)?;
            let wy=bounds.max_y-(f64::from(row)+0.5)*dy;
            let src_y=(ORIGIN_Y-wy)/res-0.5;
            let source_row=((ORIGIN_Y-wy)/tile_span).floor() as i64;
            if source_row<0 || source_row>=(1i64<<zoom) {continue;}
            let iy=src_y.floor() as i64;
            let fy=src_y-iy as f64;
            for col in rect[0]..rect[2] {
                let wx=bounds.min_x+(f64::from(col)+0.5)*dx;
                let source_col=((wx-ORIGIN_X)/tile_span).floor() as i64;
                if source_col<0 || source_col>=(1i64<<zoom) {continue;}
                let id=TileId{zoom,col:source_col as u32,row:source_row as u32};
                let Some(primary)=tiles.get(&id) else {continue;};
                let src_x=(wx-ORIGIN_X)/res-0.5;
                let ix=src_x.floor() as i64;
                let fx=src_x-ix as f64;
                let sample=|x:i64,y:i64| -> [u8;4] {
                    if x>=0 && y>=0 {
                        let neighbor=TileId{zoom,col:(x/256) as u32,row:(y/256) as u32};
                        if let Some(tile)=tiles.get(&neighbor) {
                            return tile.get_pixel((x%256) as u32,(y%256) as u32).0;
                        }
                    }
                    let lx=(x-source_col*256).clamp(0,255) as u32;
                    let ly=(y-source_row*256).clamp(0,255) as u32;
                    primary.get_pixel(lx,ly).0
                };
                let [a,b,c,d]=[sample(ix,iy),sample(ix+1,iy),
                    sample(ix,iy+1),sample(ix+1,iy+1)];
                let out=(row as usize*width as usize+col as usize)*4;
                for channel in 0..4 {
                    let top=f64::from(a[channel])*(1.-fx)+f64::from(b[channel])*fx;
                    let bot=f64::from(c[channel])*(1.-fx)+f64::from(d[channel])*fx;
                    target[out+channel]=(top*(1.-fy)+bot*fy).round() as u8;
                }
            }
        }
        Ok(())
    }
    fn paint(bounds:Bounds,width:u32,height:u32,zoom:u8,
        tiles:&HashMap<TileId,Arc<RgbaImage>>,target:&mut [u8],
        cancel:&AtomicBool)->Result<(),String>
    {
        Self::paint_region(bounds,width,height,zoom,tiles,target,[0,0,width,height],cancel)
    }
    pub fn request(&mut self,bounds:Bounds,width:u32,height:u32,cache:&Path,
        cancel:&AtomicBool,progress:&mut dyn FnMut(Progress<'_>)->Result<(),String>
    )->Result<Raster,String>{
        input::check(cancel)?;
        // Visible work always preempts the optional neighboring preview ring.
        if let Some(service)=neighbor_service(){service.cancel();}
        let target=plan(bounds,width,height)?;
        fs::create_dir_all(cache).map_err(|e|format!("PDOK WMTS-cache: {e}"))?;
        let root=fs::canonicalize(cache).map_err(|e|format!("PDOK WMTS-cache: {e}"))?;
        let total=target.count();
        let mut high=HashMap::with_capacity(total);
        let mut missing=Vec::new();
        for id in target.ids(){
            input::check(cancel)?;
            match self.from_cache(id,&root,cancel){
                Some((tile,true))=>{high.insert(id,tile);},
                Some((tile,false))=>{high.insert(id,tile);missing.push(id);},
                None=>missing.push(id),
            }
        }
        if missing.is_empty(){
            // A warm, fully cached view needs one rasterization and no
            // intermediate uploads or low-resolution network requests.
            let mut rgba=vec![255; width as usize*height as usize*4];
            Self::paint(bounds,width,height,target.zoom,&high,&mut rgba,cancel)?;
            let raster=Raster{bounds,width,height,rgba,stale_cache:false};
            if u64::from(width)*u64::from(height)<=12_000_000{
                self.previous=Some(raster.clone());
            }
            self.warm_neighbors(bounds,target,&root);
            return Ok(raster);
        }
        let previous_coverage=self.previous.as_ref().map(|old|{
            let w=(bounds.max_x.min(old.bounds.max_x)
                -bounds.min_x.max(old.bounds.min_x)).max(0.);
            let h=(bounds.max_y.min(old.bounds.max_y)
                -bounds.min_y.max(old.bounds.min_y)).max(0.);
            w*h/((bounds.max_x-bounds.min_x)*(bounds.max_y-bounds.min_y))
        }).unwrap_or(0.);
        let mut rgba=vec![255u8;width as usize*height as usize*4];
        let mut preview_source=previous_coverage>0.;
        let mut stale=false;
        let mut parent_layers:Vec<(u8,HashMap<TileId,Arc<RgbaImage>>)>=Vec::new();
        let mut coarse_missing=Vec::new();

        // Draw any cached low-resolution parents, including for the edges
        // beyond the previous viewport. Sharper parents overwrite coarse ones.
        // A small cold preview is requested separately, interleaved with
        // full-detail tiles, instead of blocking the detailed request queue.
        if target.zoom>=2 && (previous_coverage<0.95 || high.len()*2<total){
            let preview=if previous_coverage<0.95 {
                coarse_preview_grid(bounds,target)
            }else{None};
            // Also consider parents pre-warmed for a one/two-step zoom-out.
            let first=preview.map_or(target.zoom.saturating_sub(5),
                |coarse|coarse.zoom.min(target.zoom.saturating_sub(5)));
            for zoom in first..target.zoom {
                let Some(parent)=grid(bounds,zoom) else{continue;};
                if parent.count()>16{continue;}
                let mut cached=HashMap::new();
                for id in parent.ids(){
                    input::check(cancel)?;
                    if let Some((tile,_))=self.from_cache(id,&root,cancel){
                        cached.insert(id,tile);
                    }else if preview.is_some_and(|coarse|coarse.zoom==zoom){
                        coarse_missing.push(id);
                    }
                }
                if !cached.is_empty(){
                    Self::paint(bounds,width,height,zoom,&cached,&mut rgba,cancel)?;
                    preview_source=true;
                }
                parent_layers.push((zoom,cached));
            }
        }
        // Reuse the older sharper picture without allowing it to erase the
        // coarse preview in newly exposed areas.
        self.overlay_previous(bounds,width,height,&mut rgba,[0,0,width,height],cancel)?;
        if !high.is_empty(){
            Self::paint(bounds,width,height,target.zoom,&high,&mut rgba,cancel)?;
            preview_source=true;
        }
        let mut completed=total-missing.len();
        if preview_source {
            progress(Progress{bounds,width,height,rgba:&rgba,
                completed,total,stale_cache:false,changed_region:None})?;
        }

        // Highest-detail tiles always start immediately and are prioritized
        // from the centre, where users are most likely to look.
        let center_col=(target.min_col+target.max_col) as f64*0.5;
        let center_row=(target.min_row+target.max_row) as f64*0.5;
        missing.sort_by(|a,b|{
            let d=|id:&TileId|{
                (f64::from(id.col)-center_col).powi(2)
                    +(f64::from(id.row)-center_row).powi(2)
            };
            d(a).total_cmp(&d(b))
        });
        // Never make the network wait for every coarse tile. Start one
        // low-resolution tile alongside three detailed tiles, then interleave
        // the few remaining parents. All share the same four-request limit.
        let mut jobs=Vec::with_capacity(missing.len()+coarse_missing.len());
        let mut details=missing.into_iter();
        let mut parents=coarse_missing.into_iter();
        if let Some(id)=parents.next(){jobs.push(id);}
        for _ in 0..3{if let Some(id)=details.next(){jobs.push(id);}}
        for id in parents {
            jobs.push(id);
            for _ in 0..3{if let Some(id)=details.next(){jobs.push(id);}}
        }
        jobs.extend(details);
        // UI coalesces updates during 30 ms repaints. Publish each completed
        // tile immediately; never freeze sharper imagery after a preview cap.
        let mut has_frame=preview_source;
        Self::fetch_batch(&jobs,&root,cancel,target.zoom,|id,tile,when,is_stale|{
            stale|=is_stale;
            let region=Self::tile_region(bounds,width,height,id);
            if id.zoom<target.zoom{
                if let Some((_,cached))=parent_layers.iter_mut()
                    .find(|(zoom,_)|*zoom==id.zoom){
                    cached.insert(id,Arc::clone(&tile));
                }
                // New parents fill all blank pixels, but never downgrade an
                // overlapping old frame or a previously arrived sharp tile.
                for (zoom,cached) in &parent_layers {
                    Self::paint_region(bounds,width,height,*zoom,
                        cached,&mut rgba,region,cancel)?;
                }
                self.overlay_previous(bounds,width,height,&mut rgba,region,cancel)?;
                Self::paint_region(bounds,width,height,target.zoom,
                    &high,&mut rgba,region,cancel)?;
            }else{
                completed+=1;
                high.insert(id,Arc::clone(&tile));
                Self::paint_region(bounds,width,height,target.zoom,
                    &high,&mut rgba,region,cancel)?;
            }
            // Retain both coarse and sharp downloads through interrupted pans.
            self.remember(id,when,tile);
            if region[0]<region[2] && region[1]<region[3] {
                // Publish even if this was the last sharp tile and an optional
                // coarse download has not completed yet.
                let changed_region=if has_frame {Some(region)} else {None};
                progress(Progress{bounds,width,height,rgba:&rgba,
                    completed,total,stale_cache:stale,changed_region})?;
                has_frame=true;
            }
            Ok(())
        })?;
        input::check(cancel)?;
        // Every incoming tile was painted into its exact RD rectangle with
        // a two-pixel halo, so the final image is already pixel-identical
        // to a complete resample. Do not recompute millions of pixels again.
        // See tile_regions_compose_the_same_bilinear_pixels_as_full_frame.
        let raster=Raster{bounds,width,height,rgba,stale_cache:stale};
        if u64::from(width)*u64::from(height)<=12_000_000 {
            self.previous=Some(raster.clone());
        }
        self.warm_neighbors(bounds,target,&root);
        Ok(raster)
    }

}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn tile_regions_compose_the_same_bilinear_pixels_as_full_frame() {
        let cancel=AtomicBool::new(false);
        let zoom=12;
        let tile=span(zoom);
        let left=ORIGIN_X+2047.*tile;
        let top=ORIGIN_Y-2047.*tile;
        let bounds=Bounds{
            min_x:left+0.13*tile,max_x:left+1.87*tile,
            min_y:top-1.89*tile,max_y:top-0.11*tile,
        };
        let (width,height)=(420u32,360u32);
        let grid=grid(bounds,zoom).unwrap();
        let mut tiles=HashMap::new();
        let mut incremental=vec![255; width as usize*height as usize*4];
        for id in grid.ids(){
            let pixels=image::Rgba([
                (id.col%191) as u8,
                (id.row%223) as u8,
                (id.col.wrapping_mul(7)%127) as u8,
                255
            ]);
            tiles.insert(id,Arc::new(RgbaImage::from_pixel(256,256,pixels)));
            ViewportRenderer::paint_region(
                bounds,width,height,zoom,&tiles,&mut incremental,
                ViewportRenderer::tile_region(bounds,width,height,id),&cancel
            ).unwrap();
        }
        let mut full=vec![255; width as usize*height as usize*4];
        ViewportRenderer::paint(bounds,width,height,zoom,&tiles,&mut full,&cancel).unwrap();
        assert_eq!(incremental,full,"incremental tile painting changed the final RD pixels");
    }
    #[test]
    fn out_of_order_tiles_leave_no_unpainted_or_misaligned_pixels() {
        let cancel=AtomicBool::new(false);
        let zoom=13u8;
        let side=span(zoom);
        let bx=ORIGIN_X+4021.*side;
        let by=ORIGIN_Y-3048.*side;
        for (width,height,minx,miny) in [
            (177u32,310u32,0.03,0.17),
            (1024,683,0.47,0.62),
            (721,721,0.77,0.81),
        ] {
            let bounds=Bounds{
                min_x:bx+minx*side,max_x:bx+(2.92+minx)*side,
                min_y:by-(2.88+miny)*side,max_y:by-miny*side,
            };
            let mut ids=grid(bounds,zoom).unwrap().ids().collect::<Vec<_>>();
            let all=ids.iter().map(|id|(*id,Arc::new(RgbaImage::from_fn(256,256,|x,y|{
                let r=(id.col as u64*3+x as u64*2)%255;
                let g=(id.row as u64*5+y as u64*7)%255;
                image::Rgba([r as u8,g as u8,((x+y)%255) as u8,255])
            })))).collect::<HashMap<_,_>>();
            let mut expected=vec![255; width as usize*height as usize*4];
            ViewportRenderer::paint(bounds,width,height,zoom,&all,&mut expected,&cancel).unwrap();
            ids.reverse();
            let mut arrived=HashMap::new();
            let mut actual=vec![255; width as usize*height as usize*4];
            for id in ids {
                arrived.insert(id,Arc::clone(&all[&id]));
                let region=ViewportRenderer::tile_region(bounds,width,height,id);
                ViewportRenderer::paint_region(bounds,width,height,zoom,&arrived,
                    &mut actual,region,&cancel).unwrap();
            }
            assert_eq!(actual,expected,"partial compositing diverged at {width}x{height}");
        }
    }

    #[test]
    fn computed_screen_rect_tracks_world_position_without_shifting() {
        let zoom=12;
        let size=span(zoom);
        let id=TileId{zoom,col:2047,row:2047};
        let x0=ORIGIN_X+2047.*size;
        let y0=ORIGIN_Y-2047.*size;
        let bounds=Bounds{min_x:x0,max_x:x0+2.*size,
            min_y:y0-2.*size,max_y:y0};
        let region=ViewportRenderer::tile_region(bounds,512,512,id);
        assert_eq!(region[0],0);
        assert_eq!(region[1],0);
        assert!(region[2]>=256 && region[2]<=260);
        assert!(region[3]>=256 && region[3]<=260);
    }
    #[test]
    fn fixed_rd_grid_zoom_and_wmts_kvp_addresses(){
        let bounds=Bounds{min_x:155000.,min_y:463000.,max_x:155300.,max_y:463200.};
        let plan=plan(bounds,1200,800).unwrap();
        assert!(plan.zoom>=13 && plan.zoom<=17);
        assert!(plan.count()<=MAX_VIEW_TILES);
        let id=plan.ids().next().unwrap();
        assert!(id.valid());
        let url=id.url().unwrap();
        let p=url.query_pairs().collect::<HashMap<_,_>>();
        assert_eq!(p["TILEMATRIXSET"],"EPSG:28992");
        assert_eq!(p["LAYER"],LAYER);
        assert_eq!(p["TILEMATRIX"],id.zoom.to_string());
        assert_eq!(p["TILEROW"],id.row.to_string());
        assert_eq!(p["TILECOL"],id.col.to_string());
        assert_eq!(p["FORMAT"],"image/jpeg");
    }
    #[test]
    fn neighboring_zoomout_preview_stays_bounded_and_outside_target_resolution(){
        let bounds=Bounds{min_x:155000.,min_y:463000.,max_x:155300.,max_y:463200.};
        let target=plan(bounds,1200,800).unwrap();
        let ids=neighbor_preview_ids(bounds,target);
        assert!(!ids.is_empty());
        assert!(ids.len()<=MAX_NEIGHBOR_TILES);
        assert!(ids.iter().all(|id|id.zoom<target.zoom && id.valid()));
        let outward=ids.iter().any(|id|{
            let side=span(id.zoom);
            let left=ORIGIN_X+f64::from(id.col)*side;
            let top=ORIGIN_Y-f64::from(id.row)*side;
            left<bounds.min_x || left+side>bounds.max_x ||
                top>bounds.max_y || top-side<bounds.min_y
        });
        assert!(outward,"preview does not cover any neighboring terrain");
    }
    #[test]
    fn zoomed_out_cold_view_can_request_coarse_previews_even_under_eight_tiles(){
        let bounds=Bounds{min_x:155000.,min_y:463000.,max_x:155100.,max_y:463100.};
        let target=plan(bounds,128,128).unwrap();
        assert!(target.count()<=8);
        assert!(coarse_preview_grid(bounds,target).is_some());
    }
    #[test]
    fn a_cold_view_needs_at_most_four_coarse_preview_tiles(){
        let bounds=Bounds{min_x:155000.,min_y:463000.,max_x:155300.,max_y:463200.};
        let target=plan(bounds,1200,800).unwrap();
        let coarse=coarse_preview_grid(bounds,target).unwrap();
        assert!(coarse.zoom<target.zoom);
        assert!(coarse.count()<=4);
        assert!(coarse.count()>0);
    }
    #[test]
    fn tile_world_coordinates_do_not_shift_between_zoom_levels(){
        let b=Bounds{min_x:155010.,min_y:463010.,max_x:155070.,max_y:463080.};
        for zoom in [9u8,12,15,18] {
            let g=grid(b,zoom).unwrap();
            assert!(g.count()>0);
            let id=g.ids().next().unwrap();
            let x0=ORIGIN_X+f64::from(id.col)*span(zoom);
            let y0=ORIGIN_Y-f64::from(id.row)*span(zoom);
            assert!(x0<=b.max_x && x0+span(zoom)>=b.min_x);
            assert!(y0>=b.min_y && y0-span(zoom)<=b.max_y);
        }
    }
}
