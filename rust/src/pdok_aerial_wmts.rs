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
    sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, mpsc, Arc, OnceLock},
    time::{Duration, Instant, SystemTime},
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
const PREVIEW_INTERVAL: Duration = Duration::from_millis(110);
const MAX_MEMORY_TILES: usize = 192;
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
            .append_pair("FORMAT","image/png");
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
        let (tile,time,fresh)=read_disk(id,dir,cancel)?;
        self.remember(id,time,Arc::clone(&tile));
        Some((tile,fresh))
    }
    fn fetch_batch(
        ids:&[TileId],root:&Path,cancel:&AtomicBool,
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
                for _ in 0..ids.len(){
                    let (id,result)=loop {
                        input::check(cancel)?;
                        match rx.recv_timeout(Duration::from_millis(35)){
                            Ok(result)=>break result,
                            Err(mpsc::RecvTimeoutError::Timeout)=>continue,
                            Err(_) =>return Err("PDOK-tegelwerker is gestopt".into()),
                        }
                    };
                    let (tile,modified,stale)=result?;
                    on_tile(id,tile,modified,stale)?;
                }
                Ok(())
            })();
            drop(rx);
            result
        })
    }
    fn reproject(&self,b:Bounds,width:u32,height:u32,cancel:&AtomicBool)
        ->Result<Vec<u8>,String>
    {
        let mut output=vec![255u8;width as usize*height as usize*4];
        let Some(old)=&self.previous else{return Ok(output);};
        if old.width==0 || old.height==0 || old.rgba.len()!=old.width as usize*old.height as usize*4{
            return Ok(output);
        }
        let dx=(b.max_x-b.min_x)/f64::from(width);
        let dy=(b.max_y-b.min_y)/f64::from(height);
        let sx=f64::from(old.width)/(old.bounds.max_x-old.bounds.min_x);
        let sy=f64::from(old.height)/(old.bounds.max_y-old.bounds.min_y);
        for row in 0..height {
            input::check(cancel)?;
            let y=b.max_y-(f64::from(row)+0.5)*dy;
            let oy=((old.bounds.max_y-y)*sy).floor();
            if oy<0.||oy>=f64::from(old.height){continue;}
            for col in 0..width{
                let x=b.min_x+(f64::from(col)+0.5)*dx;
                let ox=((x-old.bounds.min_x)*sx).floor();
                if ox<0.||ox>=f64::from(old.width){continue;}
                let from=(oy as usize*old.width as usize+ox as usize)*4;
                let to=(row as usize*width as usize+col as usize)*4;
                output[to..to+4].copy_from_slice(&old.rgba[from..from+4]);
            }
        }
        Ok(output)
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
            return Ok(raster);
        }
        let old_overlap=self.previous.as_ref().is_some_and(|old|
            old.bounds.min_x<bounds.max_x && old.bounds.max_x>bounds.min_x
            && old.bounds.min_y<bounds.max_y && old.bounds.max_y>bounds.min_y);
        let mut rgba=self.reproject(bounds,width,height,cancel)?;
        let mut preview_source=old_overlap;
        let mut previews=0usize;
        let mut last_preview=Instant::now()-PREVIEW_INTERVAL;
        let mut stale=false;

        // Google-Maps-style parent tiles are used ONLY when they are already
        // cached. Never download a coarse level ahead of the target detail:
        // the old blocking three-level prefetch could delay sharp imagery
        // by several seconds on a cold network.
        //
        // Paint coarser parents first and then sharper parents, retaining
        // the previous world-space image in portions with no cached tiles.
        if !old_overlap && target.zoom>=2 && high.len()*2<total {
            let first=target.zoom.saturating_sub(3);
            for zoom in first..target.zoom {
                let Some(parent)=grid(bounds,zoom) else {continue;};
                if parent.count()>16 {continue;}
                let mut cached=HashMap::new();
                for id in parent.ids(){
                    input::check(cancel)?;
                    if let Some((tile,_))=self.from_cache(id,&root,cancel){
                        cached.insert(id,tile);
                    }
                }
                if !cached.is_empty(){
                    Self::paint(bounds,width,height,zoom,&cached,&mut rgba,cancel)?;
                    preview_source=true;
                }
            }
        }
        if !high.is_empty(){
            // Cached detail overlays all parents exactly once, not after
            // each cached or downloaded tile.
            Self::paint(bounds,width,height,target.zoom,&high,&mut rgba,cancel)?;
            preview_source=true;
        }
        let mut completed=total-missing.len();
        if preview_source {
            progress(Progress{bounds,width,height,rgba:&rgba,
                completed,total,stale_cache:false,changed_region:None})?;
            previews=1;
            last_preview=Instant::now();
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
        let mut dirty:Option<[u32;4]>=None;
        let preview_budget=if u64::from(width)*u64::from(height)>6_000_000{
            2usize
        }else if u64::from(width)*u64::from(height)>2_000_000{
            3usize
        }else{5usize};
        Self::fetch_batch(&missing,&root,cancel,|id,tile,when,is_stale|{
            completed+=1;
            stale|=is_stale;
            high.insert(id,Arc::clone(&tile));
            // Keep successful downloads in the in-memory LRU immediately.
            // An interrupted pan/zoom can now use every completed tile
            // instead of losing an entire cancelled batch.
            self.remember(id,when,tile);
            let region=Self::tile_region(bounds,width,height,id);
            Self::paint_region(bounds,width,height,target.zoom,
                &high,&mut rgba,region,cancel)?;
            if region[0]<region[2] && region[1]<region[3] {
                dirty=Some(match dirty {
                    Some(old)=>[old[0].min(region[0]),old[1].min(region[1]),
                        old[2].max(region[2]),old[3].max(region[3])],
                    None=>region,
                });
            }
            if completed<total && previews<preview_budget
                && (previews==0 || last_preview.elapsed()>=PREVIEW_INTERVAL)
            {
                // The first frame always contains the full map. Every later
                // frame may upload just the union of touched tile rectangles.
                let changed_region=if previews==0 {None} else {dirty.take()};
                progress(Progress{bounds,width,height,rgba:&rgba,
                    completed,total,stale_cache:stale,changed_region})?;
                dirty=None;
                previews+=1;
                last_preview=Instant::now();
            }
            Ok(())
        })?;
        input::check(cancel)?;
        // Exact final bilinear raster: repaint one last time to ensure
        // cross-tile edges are identical regardless of download order.
        rgba.fill(255);
        Self::paint(bounds,width,height,target.zoom,&high,&mut rgba,cancel)?;
        let raster=Raster{bounds,width,height,rgba,stale_cache:stale};
        if u64::from(width)*u64::from(height)<=12_000_000 {
            self.previous=Some(raster.clone());
        }
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
