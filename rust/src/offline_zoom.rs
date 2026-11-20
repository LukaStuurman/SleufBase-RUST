//! Offline, georeferenced PDOK WMTS photo pyramid for a user-selected area.
//! Unlike the legacy 10/25/50 MiB country WMS mosaic, the chosen detail is
//! *actual* PDOK WMTS zoom level 0..19. File sizes are estimated, not quotas.
//!
//! A manual job downloads ONLY the chosen highest-detail WMTS level. The
//! zoomed-out levels are generated locally from those JPEGs, not downloaded.
//! An explicit safety limit prevents accidentally requesting millions of
//! copyrighted/third-party service tiles for all of the Netherlands.
use crate::{model::Bounds,offline_overview,storage};
use image::{ImageFormat,RgbaImage};
use image::imageops::{resize,FilterType};
use std::{
    fs,io::{Cursor,Read,Write},path::{Path,PathBuf},
    sync::atomic::{AtomicBool,Ordering},
    time::{Duration,SystemTime},
};

pub const MIN_ZOOM:u8=0;
pub const MAX_ZOOM:u8=19;
pub const DEFAULT_ZOOM:u8=15;
pub const MAX_DOWNLOAD_TILES:u64=4096;
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
    /// Lower-resolution PNGs are made locally from the downloaded JPEGs.
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
    let mut generated=0u64;
    for level in MIN_ZOOM..zoom{
        generated=generated.saturating_add(grid(bounds,level)?.count());
    }
    Some(Estimate{tiles:count,derived_tiles:generated,
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
        Some(e)=>format!("Niveau {zoom} · {} · ~{} download ({} tegels){}",
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
fn derived_stamp(root:&Path,id:TileId)->Option<SystemTime>{
    let meta=fs::metadata(id.derived_path(root)).ok()?;
    (meta.is_file() && meta.len()>128 && meta.len()<=MAX_JPEG_BYTES as u64)
        .then(||meta.modified().ok()).flatten()
}
fn decode_derived(bytes:&[u8])->Result<RgbaImage,String>{
    if bytes.len()<128 || bytes.len()>MAX_JPEG_BYTES{
        return Err("Lokaal verkleinde PDOK-tegel heeft ongeldige grootte".into());
    }
    let mut reader=image::ImageReader::with_format(Cursor::new(bytes),ImageFormat::Png);
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(256);
    limits.max_image_height=Some(256);
    limits.max_alloc=Some(2*1024*1024);
    reader.limits(limits);
    let image=reader.decode().map_err(|e|format!("Lokaal verkleinde PNG lezen: {e}"))?;
    if image.width()!=256 || image.height()!=256{
        return Err("Verkleinde PDOK-tegel heeft verkeerde afmetingen".into());
    }
    Ok(image.to_rgba8())
}
/// A real WMTS JPEG wins if it is newer; otherwise use the locally derived
/// photo. Older, downloaded lower levels remain readable but are never
/// fetched again by the new offline downloader.
pub fn read_tile(root:&Path,id:TileId)->Result<Option<ImageTile>,String>{
    let jpg=cached_tile(root,id);
    let derived=derived_stamp(root,id);
    let order=if derived>jpg{[true,false]}else{[false,true]};
    for png in order {
        let timestamp=if png{derived}else{jpg};
        let Some(modified)=timestamp else{continue;};
        let path=if png{id.derived_path(root)}else{id.path(root)};
        if let Ok(bytes)=fs::read(path){
            if let Ok(image)=if png{decode_derived(&bytes)}else{decode(&bytes)}{
                return Ok(Some(ImageTile{image,modified}));
            }
        }
    }
    Ok(None)
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
    cancel:&AtomicBool)->Result<(),String>{
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
    decode(&bytes)?;
    if cancel.load(Ordering::Relaxed){return Err("Offline aanvraag geannuleerd".into());}
    let target=id.path(root);
    let dir=target.parent().ok_or("Ongeldige offline-kaartmap")?;
    fs::create_dir_all(dir).map_err(|e|format!("Kaartmap: {e}"))?;
    storage::atomic_write(&target,true,|out|
        out.write_all(&bytes).map_err(|e|e.to_string()))
}
/// Build each less-detailed level from four already stored sharper child
/// tiles. Missing child quadrants stay transparent, revealing the bundled
/// nationwide preview instead of an opaque blank square. Existing derived
/// pixels outside the current download area are retained.
fn downsample_parent(root:&Path,id:TileId)->Result<bool,String>{
    let mut output=match fs::read(id.derived_path(root)){
        Ok(bytes)=>decode_derived(&bytes).unwrap_or_else(|_|RgbaImage::new(256,256)),
        Err(_)=>RgbaImage::new(256,256),
    };
    let mut any=false;
    for dy in 0..2u32{
        for dx in 0..2u32{
            let child=TileId{zoom:id.zoom+1,
                col:id.col*2+dx,row:id.row*2+dy};
            let Some(child)=read_tile(root,child)?.map(|v|v.image) else{continue;};
            let reduced=resize(&child,128,128,FilterType::Triangle);
            for y in 0..128u32{
                for x in 0..128u32{
                    let color=*reduced.get_pixel(x,y);
                    if color[3]>0 {
                        output.put_pixel(dx*128+x,dy*128+y,color);
                        any=true;
                    }
                }
            }
        }
    }
    if !any{return Ok(false);}
    let mut buf=Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(output)
        .write_to(&mut buf,ImageFormat::Png)
        .map_err(|e|format!("Verkleinde offline luchtfoto opslaan: {e}"))?;
    let target=id.derived_path(root);
    fs::create_dir_all(target.parent().ok_or("Ongeldige offline tegelmap")?)
        .map_err(|e|e.to_string())?;
    storage::atomic_write(&target,true,|writer|
        writer.write_all(&buf.into_inner()).map_err(|e|e.to_string()))?;
    Ok(true)
}
/// Derive the complete multiscale pyramid locally. PDOK is never contacted
/// for these levels. Regeneration is possible with no internet connection.
pub fn build_local_overviews(
    root:&Path,area:[i32;4],maximum_zoom:u8,cancel:&AtomicBool,
    mut on_step:impl FnMut(u64,u64),
)->Result<(),String>{
    let estimate=estimate(area,maximum_zoom).ok_or("Ongeldig overzichtsgebied")?;
    if !estimate.allowed{return Err("Offline gebied bevat te veel tegels".into());}
    let bounds=from_area(area).ok_or("Ongeldige offline uitsnede")?;
    let mut completed=0u64;
    for level in (MIN_ZOOM..maximum_zoom).rev(){
        let cells=grid(bounds,level).ok_or("Offline overzichtsniveau buiten kaart")?;
        for id in cells.ids(){
            if cancel.load(Ordering::Relaxed){
                return Err("Offline overzicht maken geannuleerd".into());
            }
            downsample_parent(root,id)?;
            completed+=1;
            on_step(completed,estimate.derived_tiles);
        }
    }
    Ok(())
}

/// Fetches only the user-approved, bounded area. Never enumerate or download
/// an unbounded high-resolution nationwide pyramid by accident.
pub fn refresh(
    area:[i32;4],maximum_zoom:u8,force:bool,cancel:&AtomicBool,
    on_tile:impl FnMut(u64,u64),
)->Result<(),String>{
    refresh_at(&root(),area,maximum_zoom,force,cancel,on_tile)
}
pub fn refresh_at(
    root:&Path,area:[i32;4],maximum_zoom:u8,force:bool,cancel:&AtomicBool,
    mut on_tile:impl FnMut(u64,u64),
)->Result<(),String>{
    let plan=estimate(area,maximum_zoom)
        .ok_or("Ongeldige kaartuitsnede voor offline PDOK")?;
    if !plan.allowed{
        return Err(format!("{} benodigde tegels is te groot. Zoom verder in op de kaart of kies een lager niveau (maximum {} tegels)",
            plan.tiles,MAX_DOWNLOAD_TILES));
    }
    let bounds=from_area(area).ok_or("Ongeldige offline kaartuitsnede")?;
    // Fetch only the highest WMTS resolution from PDOK. Smaller zoom levels
    // are produced by the CPU from these sharp images after the download.
    let ids=grid(bounds,maximum_zoom)
        .ok_or("Ongeldige WMTS-uitsnede")?.ids();
    let http=reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(6))
        .timeout(Duration::from_secs(20))
        .user_agent(concat!("SleufBase-Rust/",env!("CARGO_PKG_VERSION")," offline WMTS"))
        .build().map_err(|e|e.to_string())?;
    let now=SystemTime::now();
    let mut consecutive_failures=0usize;
    let mut total_failed=0usize;
    let mut done=0u64;
    for id in ids{
        if cancel.load(Ordering::Relaxed){return Err("Offlinedownload geannuleerd".into());}
        let fresh=cached_tile(root,id).is_some_and(|stamp|
            !offline_overview::needs_refresh(stamp,now));
        if force || !fresh {
            let mut result=Err("PDOK-tegel nog niet opgehaald".to_string());
            for attempt in 0..3 {
                if cancel.load(Ordering::Relaxed){
                    return Err("Offlinedownload geannuleerd".into());
                }
                result=fetch_one(&http,root,id,cancel);
                if result.is_ok(){break;}
                if attempt<2{
                    std::thread::sleep(Duration::from_millis(150*(attempt+1)));
                }
            }
            match result {
                Ok(())=>consecutive_failures=0,
                Err(error)=>{
                    total_failed+=1;
                    consecutive_failures+=1;
                    if consecutive_failures>=4{
                        return Err(format!("Offline PDOK niet beschikbaar na meerdere fouten: {error}"));
                    }
                }
            }
        }
        done+=1;
        on_tile(done,plan.steps());
        // One low-priority sequential background connection leaves capacity
        // for the interactive live PDOK map and other cadastral operations.
    }
    // Build the zoomed-out view from the sharp images on this computer;
    // this works offline and makes zero additional requests to PDOK.
    // Even if one image failed, use any good sharp images saved so far.
    build_local_overviews(root,area,maximum_zoom,cancel,|local,_total|{
        on_tile(plan.tiles+local,plan.steps());
    })?;
    if total_failed>0 {
        return Err(format!("{total_failed} offline luchtfototegels konden niet worden bijgewerkt"));
    }
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
            assert_eq!(estimate.expected_bytes,estimate.tiles*EXPECTED_JPEG_BYTES);
            assert!(resolution(level)>0.);
            assert!(format_estimate(area,level).contains(&format!("Niveau {level}")));
        }
        assert!((resolution(15)-0.105).abs()<0.001);
        assert!((resolution(16)-0.0525).abs()<0.001);
        assert_eq!(format_resolution(19),"0.66 cm/px");
        let high=estimate(area,19).unwrap();
        assert!(!high.allowed);
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