//! Approximately 50 MiB, higher-resolution offline PDOK aerial cache.
//! The executable always carries a 1536px fixed-RD image for the first frame;
//! these 12x12 independently georeferenced JPEG tiles improve its resolution
//! progressively while the application runs, even across interrupted launches.
//!
//! Each tile is replaced atomically after 183 days. There is never a blank
//! cache generation, and a failed download cannot delete older aerial imagery.
use crate::{model::Bounds,offline_overview,storage};
use image::{codecs::jpeg::JpegEncoder, DynamicImage, ImageFormat, RgbaImage};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::{Path,PathBuf},
    sync::{atomic::{AtomicBool,Ordering},Arc,Mutex},
    time::{Duration,SystemTime},
};

pub const GRID:u8=12;
pub const TILE:u32=1536;
pub const MAX_BYTES:u64=50*1024*1024;
pub const MAX_TILE_BYTES:usize=MAX_BYTES as usize/(GRID as usize*GRID as usize);
/// Valid settings are deliberate fixed presets so limits and geographic
/// resolution stay reproducible and old 50 MB caches remain readable.
pub fn valid_level(mb:u16)->bool{matches!(mb,10|25|50)}
pub fn tile_side(mb:u16)->u32{match mb{10=>768,25=>1024,_=>TILE}}
pub fn tile_budget(mb:u16)->usize{
    usize::from(if valid_level(mb){mb}else{50})*1024*1024/
        (GRID as usize*GRID as usize)
}
const MAX_RESPONSE_BYTES:usize=9*1024*1024;
const MIN_REDUCED_SIDE:u32=512;
const FAILURES_BEFORE_PAUSE:usize=4;
pub const REFRESH_AFTER:Duration=offline_overview::REFRESH_AFTER;

#[derive(Clone,Copy,Debug,PartialEq,Eq,Hash)]
pub struct TileId {pub col:u8,pub row:u8}
impl TileId{
    pub fn valid(self)->bool{self.col<GRID&&self.row<GRID}
    pub fn bounds(self)->Bounds{
        let country=offline_overview::BOUNDS;
        let dx=(country.max_x-country.min_x)/f64::from(GRID);
        let dy=(country.max_y-country.min_y)/f64::from(GRID);
        Bounds{
            min_x:country.min_x+f64::from(self.col)*dx,
            max_x:country.min_x+(f64::from(self.col)+1.)*dx,
            max_y:country.max_y-f64::from(self.row)*dy,
            min_y:country.max_y-(f64::from(self.row)+1.)*dy,
        }
    }
    pub fn path(self,dir:&Path)->PathBuf{
        dir.join(format!("{:02}_{:02}.jpg",self.col,self.row))
    }
}
pub fn root()->PathBuf{
    crate::project::app_data_directory().join("offline-maps").join("pdok-aerial-detail-v1")
}
pub fn root_for(mb:u16)->PathBuf{
    if mb==50 {root()} else {
        crate::project::app_data_directory().join("offline-maps")
            .join(format!("pdok-aerial-detail-{}mb-v1",if valid_level(mb){mb}else{50}))
    }
}
pub fn ids_for_view(view:Bounds)->Vec<TileId>{
    if ![view.min_x,view.min_y,view.max_x,view.max_y]
        .iter().all(|v|v.is_finite())
        || view.min_x>=view.max_x || view.min_y>=view.max_y {return Vec::new();}
    (0..GRID).flat_map(|row|(0..GRID).filter_map(move|col|{
        let id=TileId{col,row};
        let b=id.bounds();
        (b.min_x<view.max_x&&b.max_x>view.min_x
            &&b.min_y<view.max_y&&b.max_y>view.min_y).then_some(id)
    })).collect()
}
pub fn file_stamp(dir:&Path,id:TileId,mb:u16)->Option<SystemTime>{
    let info=fs::metadata(id.path(dir)).ok()?;
    (info.is_file()&&info.len()>=1024&&info.len()<=tile_budget(mb) as u64)
        .then(||info.modified().ok()).flatten()
}
fn validate_jpeg(bytes:&[u8])->Result<DynamicImage,String>{
    if bytes.len()<1024 || bytes.len()>MAX_RESPONSE_BYTES
        || !bytes.starts_with(&[0xff,0xd8])||!bytes.ends_with(&[0xff,0xd9]){
        return Err("Ongeldige PDOK-offlinetegel".into());
    }
    let mut reader=image::ImageReader::with_format(Cursor::new(bytes),ImageFormat::Jpeg);
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(TILE);
    limits.max_image_height=Some(TILE);
    limits.max_alloc=Some(24*1024*1024);
    reader.limits(limits);
    let image=reader.decode().map_err(|e|format!("Offlinetegel decoderen: {e}"))?;
    let [w,h]=[image.width(),image.height()];
    if w!=h || !(MIN_REDUCED_SIDE..=TILE).contains(&w) {
        return Err("PDOK-offlinetegel heeft verkeerde afmetingen".into());
    }
    Ok(image)
}
pub struct ImageTile {pub image:RgbaImage,pub modified:SystemTime}
pub fn read_tile(dir:&Path,id:TileId,mb:u16)->Result<Option<ImageTile>,String>{
    if !id.valid()||!valid_level(mb){return Err("Ongeldige offline-tegel of resolutie".into());}
    let path=id.path(dir);
    let Some(modified)=file_stamp(dir,id,mb) else{return Ok(None);};
    let bytes=fs::read(&path).map_err(|e|format!("Offlinetegel lezen: {e}"))?;
    Ok(Some(ImageTile{image:validate_jpeg(&bytes)?.to_rgba8(),modified}))
}
pub fn url(id:TileId,mb:u16)->Result<reqwest::Url,String>{
    if !id.valid()||!valid_level(mb){return Err("Ongeldige offline-tegel of resolutie".into());}
    let b=id.bounds();
    let mut url=offline_overview::getmap_url()?;
    // The same WMS service and RD coordinate system as the embedded seed;
    // split the nationwide image into small, cacheable local requests.
    url.query_pairs_mut()
        .clear()
        .append_pair("SERVICE","WMS")
        .append_pair("VERSION","1.3.0")
        .append_pair("REQUEST","GetMap")
        .append_pair("LAYERS","Actueel_orthoHR")
        .append_pair("STYLES","")
        .append_pair("CRS","EPSG:28992")
        .append_pair("BBOX",&format!("{},{},{},{}",b.min_x,b.min_y,b.max_x,b.max_y))
        .append_pair("WIDTH",&tile_side(mb).to_string())
        .append_pair("HEIGHT",&tile_side(mb).to_string())
        .append_pair("FORMAT","image/jpeg")
        .append_pair("TRANSPARENT","FALSE")
        .append_pair("BGCOLOR","0xE5E9E8");
    Ok(url)
}
fn compress_within_budget(original:DynamicImage,mb:u16)->Result<Vec<u8>,String>{
    // Each preset has its own requested WMS pixel size *and* hard disk
    // cap. 10/25/50 MiB truly controls the level of detail, not just JPEG
    // quality. Existing images stay until an atomic replacement succeeds.
    if !valid_level(mb){return Err("Ongeldig offline detailniveau".into());}
    let target=tile_side(mb);
    for (side,quality) in [
        (target,85),(target,73),(target,60),
        (target*4/5,65),(target*2/3,55),(target*2/3,42)
    ] {
        let scaled;
        let image=if side==original.width()&&side==original.height() {&original} else {
            scaled=original.resize_exact(side,side,image::imageops::FilterType::Triangle);
            &scaled
        };
        let mut bytes=Vec::new();
        JpegEncoder::new_with_quality(&mut bytes,quality)
            .encode_image(image).map_err(|e|format!("PDOK JPEG comprimeren: {e}"))?;
        if bytes.len()<=tile_budget(mb) && bytes.len()>=1024{
            return Ok(bytes);
        }
    }
    Err(format!("PDOK-offlinetegel past niet in het ingestelde {mb} MB-budget"))
}
fn fetch_tile(http:&reqwest::blocking::Client,id:TileId,dir:&Path,
    mb:u16,cancel:&AtomicBool)->Result<(),String>{
    if cancel.load(Ordering::Relaxed){return Err("Offlinedownload geannuleerd".into());}
    let mut response=http.get(url(id,mb)?)
        .header(reqwest::header::ACCEPT,"image/jpeg")
        .send().map_err(|e|format!("PDOK-offlinetegel niet bereikbaar: {e}"))?
        .error_for_status().map_err(|e|format!("PDOK-tegel HTTP: {e}"))?;
    if !response.headers().get(reqwest::header::CONTENT_TYPE)
        .and_then(|v|v.to_str().ok())
        .is_some_and(|v|v.to_ascii_lowercase().starts_with("image/jpeg")){
        return Err("PDOK-offlinetegel is geen JPEG".into());
    }
    if response.content_length().is_some_and(|n|n>MAX_RESPONSE_BYTES as u64) {
        return Err("PDOK-offlinetegel overschrijdt de downloadlimiet".into());
    }
    let mut bytes=Vec::new();
    response.take((MAX_RESPONSE_BYTES+1) as u64)
        .read_to_end(&mut bytes).map_err(|e|format!("PDOK-tegel lezen: {e}"))?;
    if cancel.load(Ordering::Relaxed){return Err("Offlinedownload geannuleerd".into());}
    let image=validate_jpeg(&bytes)?;
    // The current photo always stays in place until the new one has fully
    // decoded and fitted within the byte limit.
    let jpeg=compress_within_budget(image,mb)?;
    if cancel.load(Ordering::Relaxed){return Err("Offlinedownload geannuleerd".into());}
    storage::atomic_write(&id.path(dir),true,|file|
        file.write_all(&jpeg).map_err(|e|e.to_string()))
}
pub fn refresh_all(
    dir:&Path,
    center:&Arc<Mutex<[f64;2]>>,
    mb:u16,
    cancel:&AtomicBool,
    force:bool,
    mut on_tile:impl FnMut(TileId),
)->Result<(),String>{
    if !valid_level(mb){return Err("Ongeldig offline detailniveau".into());}
    if cancel.load(Ordering::Relaxed){return Err("Offlinedownload geannuleerd".into());}
    fs::create_dir_all(dir).map_err(|e|format!("Offline-kaartmap: {e}"))?;
    let http=reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(35))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("SleufBase-Rust/1.0 offline high-resolution aerial cache")
        .build().map_err(|e|e.to_string())?;
    let now=SystemTime::now();
    let mut missing=(0..GRID).flat_map(|row|(0..GRID).filter_map(move|col|{
        let id=TileId{col,row};
        let fresh=file_stamp(dir,id,mb).is_some_and(|stamp|
            !offline_overview::needs_refresh(stamp,now));
        (force||!fresh).then_some(id)
    })).collect::<Vec<_>>();
    let mut failures=0usize;
    while !missing.is_empty(){
        if cancel.load(Ordering::Relaxed){return Err("Offlinedownload geannuleerd".into());}
        let focus=center.lock().map(|guard|*guard).unwrap_or([155_000.,463_000.]);
        missing.sort_by(|a,b|{
            let distance=|id:&TileId|{
                let area=id.bounds();
                let x=(area.min_x+area.max_x)*0.5-focus[0];
                let y=(area.min_y+area.max_y)*0.5-focus[1];
                x*x+y*y
            };
            distance(a).total_cmp(&distance(b))
        });
        let id=missing.remove(0);
        match fetch_tile(&http,id,dir,mb,cancel) {
            Ok(())=>{failures=0;on_tile(id);}
            Err(_)=>{
                failures+=1;
                if failures>=FAILURES_BEFORE_PAUSE{
                    return Err("PDOK tijdelijk niet beschikbaar voor offline kaart; eerdere tegels blijven intact".into());
                }
            }
        }
        // One sequential low-priority request; avoid flooding PDOK while
        // the normal interactive WMTS renderer needs network resources.
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn fixed_tiles_cover_country_without_overlaps_or_gaps(){
        let top=TileId{col:0,row:0}.bounds();
        let right=TileId{col:GRID-1,row:GRID-1}.bounds();
        assert!((top.min_x-offline_overview::BOUNDS.min_x).abs()<1e-7);
        assert!((top.max_y-offline_overview::BOUNDS.max_y).abs()<1e-7);
        assert!((right.max_x-offline_overview::BOUNDS.max_x).abs()<1e-7);
        assert!((right.min_y-offline_overview::BOUNDS.min_y).abs()<1e-7);
        let next=TileId{col:1,row:0}.bounds();
        assert!((top.max_x-next.min_x).abs()<1e-7);
        assert_eq!(ids_for_view(offline_overview::BOUNDS).len(),144);
        assert_eq!(u64::from(GRID)*u64::from(GRID)*MAX_TILE_BYTES as u64<=MAX_BYTES,true);
        let query=url(TileId{col:5,row:6},50).unwrap();
        let params=query.query_pairs().collect::<std::collections::HashMap<_,_>>();
        assert_eq!(params["CRS"],"EPSG:28992");
        assert_eq!(params["WIDTH"],"1536");
        for (mb,side) in [(10,768),(25,1024),(50,1536)]{
            assert!(valid_level(mb));
            assert_eq!(tile_side(mb),side);
            assert!(tile_budget(mb)*144<=mb as usize*1024*1024);
            assert_eq!(url(TileId{col:5,row:6},mb).unwrap()
                .query_pairs().find(|(key,_)|key=="WIDTH")
                .unwrap().1.parse::<u32>().unwrap(),side);
        }
    }
}