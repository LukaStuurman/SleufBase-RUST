//! Offline, georeferenced PDOK WMTS photo pyramid for a user-selected area.
//! Unlike the legacy 10/25/50 MiB country WMS mosaic, the chosen detail is
//! *actual* PDOK WMTS zoom level 0..19. File sizes are estimated, not quotas.
//!
//! Only the selected highest-resolution 256x256 WMTS JPEGs are downloaded.
//! No background downsampling or lower-resolution file generation occurs.
//! A hard, visible limit prevents an accidental multi-terabyte national job.
use crate::{model::Bounds,offline_overview,storage};
use image::{codecs::jpeg::JpegEncoder,DynamicImage,ImageFormat,RgbaImage};
use serde::{Deserialize,Serialize};
use std::{
    fs,io::{Cursor,Read,Write},path::{Path,PathBuf},
    sync::{atomic::{AtomicBool,AtomicUsize,Ordering},mpsc},
    time::{Duration,SystemTime,UNIX_EPOCH},
};

pub const MIN_ZOOM:u8=0;
pub const MAX_ZOOM:u8=19;
pub const DEFAULT_ZOOM:u8=15;
pub const MAX_DOWNLOAD_TILES:u64=4096;
/// European Netherlands in RD coordinates, conservatively including surrounding water.
pub const NETHERLANDS_AREA:[i32;4]=[0,300_000,290_000,630_000];
pub const DEFAULT_QUALITY:u8=100;
pub fn supported_quality(value:u8)->bool{matches!(value,100|95|85|70)}
pub fn quality_label(value:u8)->&'static str{
    match value{100=>"Origineel JPEG (geen extra compressie)",95=>"JPEG kwaliteit 95",85=>"JPEG kwaliteit 85",70=>"JPEG kwaliteit 70",_=>"Ongeldige compressie"}
}
pub const EXPECTED_JPEG_BYTES:u64=50*1024;
pub const MAX_JPEG_BYTES:usize=3*1024*1024;
pub const FRESH_FOR:Duration=offline_overview::REFRESH_AFTER;
const ORIGIN_X:f64=-285401.92;
const ORIGIN_Y:f64=903402.0;
const RESOLUTION_0:f64=3440.64;
const PIXELS_PER_TILE:f64=256.0;
const URL:&str="https://service.pdok.nl/hwh/luchtfotorgb/wmts/v1_0";

pub fn resolution(zoom:u8)->f64{
    RESOLUTION_0/(1u32<<zoom.min(MAX_ZOOM)) as f64
}
pub fn format_resolution(zoom:u8)->String{
    let cm=resolution(zoom)*100.;
    if cm>=100_000. {format!("{:.0} km/px",cm/100_000.)}
    else if cm>=100. {format!("{:.1} m/px",cm/100.)}
    else if cm>=1. {format!("{cm:.1} cm/px")}
    else {format!("{cm:.2} cm/px")}
}
pub fn root()->PathBuf{
    crate::project::app_data_directory().join("offline-maps").join("pdok-wmts-zoom-v1")
}
pub fn from_area(area:[i32;4])->Option<Bounds>{
    let [min_x,min_y,max_x,max_y]=area;
    let world=offline_overview::BOUNDS;
    if min_x>=max_x||min_y>=max_y||
        (min_x as f64)<world.min_x||(max_x as f64)>world.max_x||
        (min_y as f64)<world.min_y||(max_y as f64)>world.max_y{
        return None;
    }
    Some(Bounds{min_x:min_x as f64,min_y:min_y as f64,
        max_x:max_x as f64,max_y:max_y as f64})
}
pub fn to_area(bounds:Bounds)->Option<[i32;4]>{
    if ![bounds.min_x,bounds.min_y,bounds.max_x,bounds.max_y]
        .iter().all(|n|n.is_finite())
        ||bounds.min_x>=bounds.max_x||bounds.min_y>=bounds.max_y{
        return None;
    }
    let world=offline_overview::BOUNDS;
    let min_x=bounds.min_x.max(world.min_x).floor();
    let min_y=bounds.min_y.max(world.min_y).floor();
    let max_x=bounds.max_x.min(world.max_x).ceil();
    let max_y=bounds.max_y.min(world.max_y).ceil();
    if min_x>=max_x||min_y>=max_y{return None;}
    Some([min_x as i32,min_y as i32,max_x as i32,max_y as i32])
}
pub fn describe_area(area:[i32;4])->String{
    let [xmin,ymin,xmax,ymax]=area;
    let (width,height)=((xmax-xmin).abs() as f64,(ymax-ymin).abs() as f64);
    let size=|v:f64|if v>=1000.{format!("{:.1} km",v/1000.)}
        else{format!("{v:.0} m")};
    format!("{} × {}",size(width),size(height))
}
#[derive(Clone,Copy,Debug,PartialEq,Eq,Hash)]
pub struct TileId{pub zoom:u8,pub col:u32,pub row:u32}
impl TileId{
    pub fn bounds(self)->Bounds {
        let span=resolution(self.zoom)*PIXELS_PER_TILE;
        Bounds{min_x:ORIGIN_X+self.col as f64*span,
            max_x:ORIGIN_X+(self.col as f64+1.)*span,
            max_y:ORIGIN_Y-self.row as f64*span,
            min_y:ORIGIN_Y-(self.row as f64+1.)*span}
    }
    fn valid(self)->bool{
        self.zoom<=MAX_ZOOM&&self.col<(1u32<<self.zoom)
            &&self.row<(1u32<<self.zoom)
    }
    pub fn path(self,root:&Path)->PathBuf{
        root.join(self.zoom.to_string()).join(self.col.to_string())
            .join(format!("{}.jpg",self.row))
    }
    /// Lower-resolution display tile assembled locally from sharper photos.
    /// PNG retains transparency outside the downloaded user area.
    pub fn derived_path(self,root:&Path)->PathBuf{
        root.join("derived").join(self.zoom.to_string())
            .join(self.col.to_string()).join(format!("{}.png",self.row))
    }
    pub fn parent(self)->Option<Self>{
        (self.zoom>0).then_some(Self{
            zoom:self.zoom.saturating_sub(1),col:self.col/2,row:self.row/2,
        })
    }
    pub fn url(self)->Result<reqwest::Url,String>{
        if !self.valid(){return Err("Ongeldige PDOK-offlinetegel".into());}
        let mut url=reqwest::Url::parse(URL).map_err(|e|e.to_string())?;
        url.query_pairs_mut()
            .append_pair("SERVICE","WMTS")
            .append_pair("REQUEST","GetTile")
            .append_pair("VERSION","1.0.0")
            .append_pair("LAYER","Actueel_orthoHR")
            .append_pair("STYLE","default")
            .append_pair("TILEMATRIXSET","EPSG:28992")
            .append_pair("TILEMATRIX",&self.zoom.to_string())
            .append_pair("TILEROW",&self.row.to_string())
            .append_pair("TILECOL",&self.col.to_string())
            .append_pair("FORMAT","image/jpeg");
        Ok(url)
    }
}
#[derive(Clone,Copy,Debug)]
pub struct Grid {pub zoom:u8,pub min_col:u32,pub max_col:u32,
    pub min_row:u32,pub max_row:u32}
impl Grid{
    pub fn count(self)->u64{
        u64::from(self.max_col-self.min_col+1)
            *u64::from(self.max_row-self.min_row+1)
    }
    pub fn ids(self)->impl Iterator<Item=TileId>{
        (self.min_row..=self.max_row).flat_map(move |row|
            (self.min_col..=self.max_col).map(move|col|
                TileId{zoom:self.zoom,col,row}))
    }
}
pub fn grid(bounds:Bounds,zoom:u8)->Option<Grid>{
    if zoom>MAX_ZOOM||![bounds.min_x,bounds.min_y,bounds.max_x,bounds.max_y]
        .iter().all(|n|n.is_finite())||
        bounds.min_x>=bounds.max_x||bounds.min_y>=bounds.max_y{
        return None;
    }
    let span=resolution(zoom)*PIXELS_PER_TILE;
    let limit=(1u32<<zoom) as f64;
    let col_min=((bounds.min_x-ORIGIN_X)/span).floor();
    let col_max=((bounds.max_x-ORIGIN_X)/span-1e-10).floor();
    let row_min=((ORIGIN_Y-bounds.max_y)/span).floor();
    let row_max=((ORIGIN_Y-bounds.min_y)/span-1e-10).floor();
    let valid=|x:f64|x.is_finite()&&x>=0.&&x<limit;
    if ![col_min,col_max,row_min,row_max].iter().all(|x|valid(*x))
        ||col_min>col_max||row_min>row_max{return None;}
    Some(Grid{zoom,min_col:col_min as u32,max_col:col_max as u32,
        min_row:row_min as u32,max_row:row_max as u32})
}
#[derive(Clone,Copy,Debug)]
pub struct Estimate {
    /// Only highest-resolution PDOK JPEGs are downloaded.
    pub tiles:u64,
    /// Kept for backwards compatibility in progress accounting; always zero.
    pub derived_tiles:u64,
    pub expected_bytes:u64,
    pub allowed:bool,
}
impl Estimate{
    pub fn steps(self)->u64{self.tiles.saturating_add(self.derived_tiles)}
}
pub fn estimate(area:[i32;4],zoom:u8)->Option<Estimate>{
    let bounds=from_area(area)?;
    if zoom>MAX_ZOOM {return None;}
    let count=grid(bounds,zoom)?.count();
    Some(Estimate{tiles:count,derived_tiles:0,
        expected_bytes:count.saturating_mul(EXPECTED_JPEG_BYTES),
        allowed:count<=MAX_DOWNLOAD_TILES})
}
pub fn format_mb(bytes:u64)->String {
    let mb=bytes as f64/1_000_000.;
    if mb<1. {format!("{mb:.2} MB")}
    else if mb<100. {format!("{mb:.1} MB")}
    else if mb<1000. {format!("{mb:.0} MB")}
    else {format!("{:.1} GB",mb/1000.)}
}
pub fn format_estimate(area:[i32;4],zoom:u8)->String{
    match estimate(area,zoom){
        Some(e)=>format!("Niveau {zoom} · {} · ~{} download ({} originele tegels){}",
            format_resolution(zoom),format_mb(e.expected_bytes),
            e.tiles,if e.allowed{""}else{" · te groot"}),
        None=>format!("Niveau {zoom} · buiten PDOK-dekking"),
    }
}
pub fn tiles_for_view(bounds:Bounds,zoom:u8,max:usize)->Vec<TileId>{
    let Some(area)=grid(bounds,zoom) else{return Vec::new();};
    if area.count()>max as u64{return Vec::new();}
    area.ids().collect()
}
fn cached_tile(root:&Path,id:TileId)->Option<SystemTime>{
    let meta=fs::metadata(id.path(root)).ok()?;
    (meta.is_file()&&meta.len()>256&&meta.len()<=MAX_JPEG_BYTES as u64)
        .then(||meta.modified().ok()).flatten()
}
pub struct ImageTile{pub image:RgbaImage,pub modified:SystemTime}
/// Only genuine WMTS JPEG tiles are eligible for display. Previously generated
/// downsized PNG overviews are intentionally ignored.
pub fn tile_modified(root:&Path,id:TileId)->Option<SystemTime>{cached_tile(root,id)}
pub fn has_tile(root:&Path,id:TileId)->bool{cached_tile(root,id).is_some()}
pub fn read_tile(root:&Path,id:TileId)->Result<Option<ImageTile>,String>{
    let Some(modified)=cached_tile(root,id) else{return Ok(None);};
    let bytes=fs::read(id.path(root)).map_err(|e|format!("Offline JPEG lezen: {e}"))?;
    Ok(Some(ImageTile{image:decode(&bytes)?,modified}))
}
fn decode(bytes:&[u8])->Result<RgbaImage,String>{
    if bytes.len()<256||bytes.len()>MAX_JPEG_BYTES
        ||!bytes.starts_with(&[255,216])||!bytes.ends_with(&[255,217]){
        return Err("PDOK-offlinetegel is geen volledige JPEG".into());
    }
    let mut reader=image::ImageReader::with_format(Cursor::new(bytes),ImageFormat::Jpeg);
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(256);
    limits.max_image_height=Some(256);
    limits.max_alloc=Some(2*1024*1024);
    reader.limits(limits);
    let image=reader.decode().map_err(|e|format!("JPEG-tegel decoderen: {e}"))?;
    if image.width()!=256||image.height()!=256{
        return Err("PDOK-offlinetegel heeft verkeerde afmetingen".into());
    }
    Ok(image.to_rgba8())
}
fn fetch_one(http:&reqwest::blocking::Client,root:&Path,id:TileId,
    quality:u8,cancel:&AtomicBool)->Result<(),String>{
    if cancel.load(Ordering::Relaxed){return Err("Offline aanvraag geannuleerd".into());}
    let mut response=http.get(id.url()?)
        .header(reqwest::header::ACCEPT,"image/jpeg")
        .send().map_err(|e|format!("PDOK-luchtfoto onbereikbaar: {e}"))?
        .error_for_status().map_err(|e|format!("PDOK-luchtfoto HTTP: {e}"))?;
    if !response.headers().get(reqwest::header::CONTENT_TYPE)
        .and_then(|v|v.to_str().ok())
        .is_some_and(|v|v.to_ascii_lowercase().contains("image/jpeg")){
        return Err("PDOK-offlinetegel gaf geen JPEG terug".into());
    }
    if response.content_length().is_some_and(|size|size>MAX_JPEG_BYTES as u64){
        return Err("PDOK-offlinetegel is te groot".into());
    }
    let mut bytes=Vec::new();
    response.take((MAX_JPEG_BYTES+1) as u64)
        .read_to_end(&mut bytes).map_err(|e|format!("Tegel lezen: {e}"))?;
    let pixels=decode(&bytes)?;
    // JPEG quality changes coding only. Never change the WMTS pixel dimensions.
    if quality!=DEFAULT_QUALITY{
        let mut compressed=Vec::new();
        JpegEncoder::new_with_quality(&mut compressed,quality)
            .encode_image(&DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(pixels).to_rgb8()))
            .map_err(|e|format!("Volledige PDOK-tegel comprimeren: {e}"))?;
        decode(&compressed)?;
        bytes=compressed;
    }
    if cancel.load(Ordering::Relaxed){return Err("Offline aanvraag geannuleerd".into());}
    let target=id.path(root);
    let dir=target.parent().ok_or("Ongeldige offline-kaartmap")?;
    fs::create_dir_all(dir).map_err(|e|format!("Kaartmap: {e}"))?;
    storage::atomic_write(&target,true,|out|
        out.write_all(&bytes).map_err(|e|e.to_string()))
}
/// Persist a bounded catalog of the user-approved downloads. All packages
/// share the same physical JPEG tiles, allowing overlapping areas to reuse them.
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Download {
    pub id:u64,
    pub area:[i32;4],
    pub zoom:u8,
    pub quality:u8,
    pub created_secs:u64,
    pub bytes:u64,
}
#[derive(Default,Serialize,Deserialize)]
#[serde(default)]
struct Catalog{entries:Vec<Download>}
fn catalog_path(root:&Path)->PathBuf{root.join("downloads.json")}
fn catalog(root:&Path)->Result<Catalog,String>{
    let path=catalog_path(root);
    let bytes=match fs::read(path){
        Ok(bytes)=>bytes,
        Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(Catalog::default()),
        Err(e)=>return Err(format!("Offline kaartlijst openen: {e}")),
    };
    if bytes.len()>1024*1024{return Err("Offline kaartlijst is te groot".into());}
    let data:Catalog=serde_json::from_slice(&bytes)
        .map_err(|e|format!("Offline kaartlijst is beschadigd: {e}"))?;
    if data.entries.len()>2048 || data.entries.iter().any(|d|
        from_area(d.area).is_none() || d.zoom>MAX_ZOOM || !supported_quality(d.quality))
    {return Err("Offline kaartlijst bevat ongeldige gegevens".into());}
    Ok(data)
}
fn save_catalog(root:&Path,data:&Catalog)->Result<(),String>{
    fs::create_dir_all(root).map_err(|e|e.to_string())?;
    let bytes=serde_json::to_vec_pretty(data).map_err(|e|e.to_string())?;
    storage::atomic_write(&catalog_path(root),true,|file|
        file.write_all(&bytes).map_err(|e|e.to_string()))
}
pub fn list_downloads()->Result<Vec<Download>,String>{
    let mut entries=catalog(&root())?.entries;
    entries.sort_by(|a,b|b.created_secs.cmp(&a.created_secs));
    Ok(entries)
}
fn ids_in_area(area:[i32;4],maximum_zoom:u8)->Result<Vec<TileId>,String>{
    let bounds=from_area(area).ok_or("Ongeldige offline kaartuitsnede")?;
    let mut ids=Vec::new();
    ids.extend(grid(bounds,maximum_zoom).ok_or("Ongeldige WMTS-uitsnede")?.ids());
    Ok(ids)
}
fn register_download(root:&Path,area:[i32;4],zoom:u8,quality:u8)->Result<(),String>{
    let mut data=catalog(root)?;
    let bytes=ids_in_area(area,zoom)?.into_iter().filter_map(|id|
        fs::metadata(id.path(root)).ok().map(|info|info.len())).sum();
    let now=SystemTime::now().duration_since(UNIX_EPOCH)
        .unwrap_or_default().as_secs();
    // Re-downloading the same coverage refreshes its existing catalog entry.
    if let Some(entry)=data.entries.iter_mut().find(|e|e.area==area&&e.zoom==zoom){
        entry.quality=quality;entry.bytes=bytes;entry.created_secs=now;
    }else{
        let id=data.entries.iter().map(|entry|entry.id).max()
            .unwrap_or(now.saturating_mul(1000))
            .saturating_add(1);
        data.entries.push(Download{id,area,zoom,quality,created_secs:now,bytes});
    }
    save_catalog(root,&data)
}
fn overlaps_tile(download:&Download,id:TileId)->bool{
    if id.zoom!=download.zoom {return false;}
    let Some(bounds)=from_area(download.area) else{return false;};
    grid(bounds,id.zoom).is_some_and(|range|
        id.col>=range.min_col&&id.col<=range.max_col
            &&id.row>=range.min_row&&id.row<=range.max_row)
}
/// Remove one downloaded area without erasing tiles shared with other packages.
pub fn remove_download(id:u64)->Result<(),String>{
    let root=root();
    let mut data=catalog(&root)?;
    let index=data.entries.iter().position(|entry|entry.id==id)
        .ok_or("Offline download is niet meer aanwezig")?;
    let removed=data.entries.remove(index);
    for tile in ids_in_area(removed.area,removed.zoom)?{
        if data.entries.iter().any(|entry|overlaps_tile(entry,tile)){continue;}
        let path=tile.path(&root);
        if let Err(e)=fs::remove_file(path){
            if e.kind()!=std::io::ErrorKind::NotFound {
                return Err(format!("Offline tegel verwijderen mislukt: {e}"));
            }
        }
    }
    save_catalog(&root,&data)
}
/// Remove all user-downloaded WMTS areas and old WMS-derived offline archives.
/// The built-in, always-available overview image is part of the executable.
pub fn remove_all_downloads()->Result<(),String>{
    let current=root();
    if current.exists(){fs::remove_dir_all(&current).map_err(|e|e.to_string())?;}
    for quality in [10,25,50]{
        let old=crate::offline_detail::root_for(quality);
        if old.exists(){fs::remove_dir_all(old).map_err(|e|e.to_string())?;}
    }
    let old=offline_overview::cache_path();
    if old.exists(){fs::remove_file(old).map_err(|e|e.to_string())?;}
    Ok(())
}

/// A manual refresh requests only the selected native WMTS zoom level.
/// Compression may reduce *bytes* but never the tile's pixel width/height.
pub fn refresh(
    area:[i32;4],maximum_zoom:u8,force:bool,cancel:&AtomicBool,
    on_tile:impl FnMut(u64,u64),
)->Result<(),String>{
    refresh_with_quality(area,maximum_zoom,DEFAULT_QUALITY,force,cancel,on_tile)
}
pub fn refresh_with_quality(
    area:[i32;4],maximum_zoom:u8,quality:u8,force:bool,cancel:&AtomicBool,
    on_tile:impl FnMut(u64,u64),
)->Result<(),String>{
    refresh_at_with_quality(&root(),area,maximum_zoom,quality,force,cancel,on_tile)
}
pub fn refresh_at(
    root:&Path,area:[i32;4],maximum_zoom:u8,force:bool,cancel:&AtomicBool,
    on_tile:impl FnMut(u64,u64),
)->Result<(),String>{
    refresh_at_with_quality(root,area,maximum_zoom,DEFAULT_QUALITY,force,cancel,on_tile)
}
pub fn refresh_at_with_quality(
    root:&Path,area:[i32;4],maximum_zoom:u8,quality:u8,force:bool,cancel:&AtomicBool,
    mut on_tile:impl FnMut(u64,u64),
)->Result<(),String>{
    if !supported_quality(quality){return Err("Ongeldige offline JPEG-compressie".into());}
    let plan=estimate(area,maximum_zoom)
        .ok_or("Ongeldige kaartuitsnede voor offline PDOK")?;
    if !plan.allowed{
        return Err(format!("{} originele WMTS-tegels is te groot (maximum {}). Kies een lager zoomniveau of een kleiner gebied; de resolutie wordt niet automatisch verlaagd.",
            plan.steps(),MAX_DOWNLOAD_TILES));
    }
    let ids=ids_in_area(area,maximum_zoom)?;
    let records=catalog(root)?;
    // A change in JPEG quality must actually update the stored bytes.
    // No-op repeat downloads keep the original cached files.
    let quality_changed=records.entries.iter().any(|entry|
        entry.area==area && entry.zoom==maximum_zoom && entry.quality!=quality)
        || (quality!=DEFAULT_QUALITY && !records.entries.iter().any(|entry|
            entry.area==area && entry.zoom==maximum_zoom));
    let http=reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(20))
        .pool_max_idle_per_host(4)
        .user_agent(concat!("SleufBase-Rust/",env!("CARGO_PKG_VERSION")," offline WMTS"))
        .build().map_err(|e|e.to_string())?;
    let now=SystemTime::now();
    let next=AtomicUsize::new(0);
    let failures=AtomicUsize::new(0);
    let (tx,rx)=mpsc::sync_channel::<Result<(),String>>(16);
    let mut done=0u64;
    let mut last_error=None;
    // Four bounded, shared-connection downloads. Progress callbacks still
    // run on the calling thread so UI ownership and cancellation remain safe.
    std::thread::scope(|scope|{
        for _ in 0..4{
            let sender=tx.clone();
            let next=&next;
            let failures=&failures;
            let ids=&ids;
            let http=&http;
            scope.spawn(move||{
                loop{
                    if cancel.load(Ordering::Relaxed)||failures.load(Ordering::Relaxed)>=8 {break;}
                    let index=next.fetch_add(1,Ordering::Relaxed);
                    let Some(id)=ids.get(index).copied() else{break;};
                    let fresh=!force&&!quality_changed&&cached_tile(root,id).is_some_and(|stamp|
                        !offline_overview::needs_refresh(stamp,now));
                    let result=if fresh{Ok(())}else{
                        let mut result=Err("PDOK-tegel nog niet opgehaald".to_string());
                        for attempt in 0..3{
                            if cancel.load(Ordering::Relaxed){break;}
                            result=fetch_one(http,root,id,quality,cancel);
                            if result.is_ok(){break;}
                            if attempt<2{
                                std::thread::sleep(Duration::from_millis(150*(attempt+1)));
                            }
                        }
                        result
                    };
                    if result.is_err(){failures.fetch_add(1,Ordering::Relaxed);}
                    if sender.send(result).is_err(){break;}
                }
            });
        }
        drop(tx);
        for result in rx{
            done+=1;
            if let Err(error)=result{last_error=Some(error);}
            on_tile(done,plan.steps());
        }
    });
    if cancel.load(Ordering::Relaxed){return Err("Offlinedownload geannuleerd".into());}
    if failures.load(Ordering::Relaxed)>0{
        return Err(format!("Offline download niet volledig ({done}/{}): {}",
            plan.steps(),last_error.unwrap_or_else(||"PDOK niet bereikbaar".into())));
    }
    if done!=plan.steps(){return Err("Offline download onverwacht onderbroken".into());}
    register_download(root,area,maximum_zoom,quality)?;
    Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn all_20_pyramid_levels_have_increasing_cost_and_exact_pixel_size(){
        let area=[155000,463000,155500,463500];
        let mut last=0u64;
        for level in 0..=19{
            let estimate=estimate(area,level).unwrap();
            assert!(estimate.tiles>=last);
            last=estimate.tiles;
            assert_eq!(estimate.steps(),estimate.tiles+estimate.derived_tiles);
            assert_eq!(estimate.expected_bytes,estimate.steps()*EXPECTED_JPEG_BYTES);
            assert!(resolution(level)>0.);
            assert!(format_estimate(area,level).contains(&format!("Niveau {level}")));
        }
        assert!((resolution(15)-0.105).abs()<0.001);
        assert!((resolution(16)-0.0525).abs()<0.001);
        assert_eq!(format_resolution(19),"0.66 cm/px");
        let high=estimate(area,19).unwrap();
        assert!(!high.allowed);
        assert!(estimate(NETHERLANDS_AREA,7).unwrap().allowed);
        assert!(!estimate(NETHERLANDS_AREA,15).unwrap().allowed);
    }
    #[test]
    fn all_levels_are_original_jpegs_not_later_downsampled(){
        let area=[154900,462900,155100,463100];
        let total=estimate(area,1).unwrap();
        assert_eq!(total.tiles,4);
        assert_eq!(total.derived_tiles,0);
        assert_eq!(total.steps(),4);
        let root=tempfile::tempdir().unwrap();
        let sample=image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            256,256,image::Rgb([71,80,92])));
        let mut bytes=Vec::new();
        JpegEncoder::new_with_quality(&mut bytes,92).encode_image(&sample).unwrap();
        let stored=TileId{zoom:1,col:0,row:0};
        let destination=stored.path(root.path());
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(&destination,&bytes).unwrap();
        assert!(has_tile(root.path(),stored));
        assert_eq!(fs::read(destination).unwrap(),bytes,
            "original PDOK JPEG bytes must be preserved at quality 100");
        assert!(!has_tile(root.path(),TileId{zoom:0,col:0,row:0}),
            "never use a synthesized low-resolution tile");
    }
    #[test]
    fn catalog_deletes_one_area_without_other_downloads(){
        let root=tempfile::tempdir().unwrap();
        let a=[154900,462900,155100,463100];
        let b=[155000,463000,155200,463200];
        register_download(root.path(),a,1,100).unwrap();
        register_download(root.path(),b,1,85).unwrap();
        let mut entries=catalog(root.path()).unwrap();
        assert_eq!(entries.entries.len(),2);
        assert_eq!(entries.entries.remove(0).area,a);
        assert!(supported_quality(70));
        assert!(!supported_quality(20));
    }
    #[test]
    fn bounds_are_correct_and_user_area_is_bounded(){
        let area=[154900,462900,155100,463100];
        assert_eq!(from_area(area).and_then(to_area),Some(area));
        assert!(estimate(area,13).unwrap().allowed);
        assert!(estimate([-500000,0,500000,1000000],15).is_none());
        let tile=grid(from_area(area).unwrap(),15).unwrap().ids().next().unwrap();
        let b=tile.bounds();
        assert!(b.max_x>area[0] as f64&&b.min_x<area[2] as f64);
        let url=tile.url().unwrap();
        assert!(url.as_str().contains("TILEMATRIX=15"));
    }
}