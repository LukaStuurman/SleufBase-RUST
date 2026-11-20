//! Fixed-RD, tiny nationwide PDOK aerial preview for the interactive map.
//! The actual image is bundled into the executable so no internet connection
//! is needed to avoid white gaps during fast pan/zoom. The source is the
//! Samenwerkingsverband Beeldmateriaal aerial imagery (CC BY 4.0).
//!
//! An updated JPEG is fetched at most every 183 days, atomically installed
//! only after full JPEG decoding/dimension checks, and reused across restarts.
use crate::model::Bounds;
use image::{DynamicImage, ImageFormat, RgbaImage};
use std::{
    fs, io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

pub const BOUNDS:Bounds=Bounds {
    min_x:-100_000.,min_y:200_000.,max_x:400_000.,max_y:700_000.,
};
pub const SIZE:u32=1536;
pub const CREDIT:&str="Luchtfoto © Samenwerkingsverband Beeldmateriaal · CC BY 4.0";
pub const REFRESH_AFTER:Duration=Duration::from_secs(183*24*60*60);
const RETRY_MAX_BYTES:usize=9*1024*1024;
const SEED_TIMESTAMP_SECS:u64=1_795_132_800; // 20 November 2026, UTC
const WMS:&str="https://service.pdok.nl/hwh/luchtfotorgb/wms/v1_0";
const JPEG_SEED:&[u8]=include_bytes!("../assets/pdok_offline_overview.jpg");

pub struct Overview {
    pub image:RgbaImage,
    pub updated_at:SystemTime,
}
pub fn cache_path()->PathBuf {
    crate::project::app_data_directory()
        .join("offline-maps").join("pdok-aerial-overview.jpg")
}
pub fn seed_timestamp()->SystemTime {
    SystemTime::UNIX_EPOCH+Duration::from_secs(SEED_TIMESTAMP_SECS)
}
pub fn needs_refresh(updated_at:SystemTime,now:SystemTime)->bool {
    now.duration_since(updated_at).is_ok_and(|age|age>=REFRESH_AFTER)
}
fn decode(bytes:&[u8])->Result<RgbaImage,String>{
    if bytes.len()<12_000 || bytes.len()>RETRY_MAX_BYTES
        || !bytes.starts_with(&[0xff,0xd8])
        || !bytes.ends_with(&[0xff,0xd9]) {
        return Err("Ongeldige of te grote offline-PDOK-luchtfoto".into());
    }
    let mut reader=image::ImageReader::with_format(Cursor::new(bytes),ImageFormat::Jpeg);
    let mut limits=image::Limits::default();
    limits.max_image_width=Some(SIZE);
    limits.max_image_height=Some(SIZE);
    limits.max_alloc=Some(24*1024*1024);
    reader.limits(limits);
    let decoder=reader.into_decoder().map_err(|e|format!("Overzichtsfoto lezen: {e}"))?;
    if image::ImageDecoder::dimensions(&decoder)!=(SIZE,SIZE) {
        return Err("Offline-PDOK-luchtfoto heeft verkeerde afmetingen".into());
    }
    DynamicImage::from_decoder(decoder)
        .map_err(|e|format!("Overzichtsfoto decoderen: {e}"))
        .map(|image|image.to_rgba8())
}
/// Invalid updates do not replace the genuine bundled map.
pub fn load_initial(path:&Path)->Result<Overview,String>{
    if let Ok(meta)=fs::metadata(path){
        if meta.is_file() && meta.len()>=12_000 && meta.len()<=RETRY_MAX_BYTES as u64{
            if let Ok(bytes)=fs::read(path){
                if let Ok(image)=decode(&bytes){
                    return Ok(Overview{
                        image,
                        updated_at:meta.modified().unwrap_or_else(|_|seed_timestamp()),
                    });
                }
            }
        }
    }
    Ok(Overview{image:decode(JPEG_SEED)?,updated_at:seed_timestamp()})
}
pub fn getmap_url()->Result<reqwest::Url,String>{
    let mut url=reqwest::Url::parse(WMS).map_err(|e|e.to_string())?;
    url.query_pairs_mut()
        .append_pair("SERVICE","WMS")
        .append_pair("VERSION","1.3.0")
        .append_pair("REQUEST","GetMap")
        .append_pair("LAYERS","Actueel_orthoHR")
        .append_pair("STYLES","")
        .append_pair("CRS","EPSG:28992")
        .append_pair("BBOX","-100000,200000,400000,700000")
        .append_pair("WIDTH","1536")
        .append_pair("HEIGHT","1536")
        .append_pair("FORMAT","image/jpeg")
        .append_pair("TRANSPARENT","FALSE")
        .append_pair("BGCOLOR","0xE5E9E8");
    Ok(url)
}
/// The caller runs this on a background worker. No partial network or disk
/// failure can overwrite the currently displayed (or stored) overview.
pub fn download_and_store(path:&Path)->Result<Overview,String>{
    let client=reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(65))
        .connect_timeout(Duration::from_secs(12))
        .user_agent("SleufBase-Rust/1.0 offline aerial overview")
        .build().map_err(|e|e.to_string())?;
    let mut response=client.get(getmap_url()?)
        .header(reqwest::header::ACCEPT,"image/jpeg")
        .send().map_err(|e|format!("PDOK-overzicht niet bereikbaar: {e}"))?
        .error_for_status().map_err(|e|format!("PDOK-overzicht HTTP: {e}"))?;
    let content_type=response.headers().get(reqwest::header::CONTENT_TYPE)
        .and_then(|value|value.to_str().ok()).unwrap_or("");
    if !content_type.to_ascii_lowercase().starts_with("image/jpeg"){
        return Err("PDOK-overzicht gaf geen JPEG-afbeelding terug".into());
    }
    if response.content_length().is_some_and(|n|n>RETRY_MAX_BYTES as u64){
        return Err("PDOK-overzicht is te groot".into());
    }
    let mut bytes=Vec::new();
    response.take((RETRY_MAX_BYTES+1) as u64)
        .read_to_end(&mut bytes).map_err(|e|format!("PDOK-overzicht lezen: {e}"))?;
    let image=decode(&bytes)?;
    let directory=path.parent().ok_or("Ongeldige offline-kaartmap")?;
    fs::create_dir_all(directory).map_err(|e|format!("Offline-kaartmap: {e}"))?;
    crate::storage::atomic_write(path,true,|file|
        file.write_all(&bytes).map_err(|e|e.to_string()))?;
    let updated_at=fs::metadata(path)
        .and_then(|meta|meta.modified()).unwrap_or_else(|_|SystemTime::now());
    Ok(Overview{image,updated_at})
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn bundled_map_is_real_pdok_jpeg_and_matches_fixed_rd_bounds(){
        let image=decode(JPEG_SEED).unwrap();
        assert_eq!(image.dimensions(),(SIZE,SIZE));
        assert_eq!(BOUNDS.max_x-BOUNDS.min_x,500_000.);
        assert_eq!(BOUNDS.max_y-BOUNDS.min_y,500_000.);
        let url=getmap_url().unwrap();
        let query=url.query_pairs().collect::<std::collections::HashMap<_,_>>();
        assert_eq!(query["CRS"],"EPSG:28992");
        assert_eq!(query["LAYERS"],"Actueel_orthoHR");
        assert_eq!(query["BBOX"],"-100000,200000,400000,700000");
    }
    #[test]
    fn six_month_refresh_gate_keeps_old_photo_until_valid_new_image(){
        let start=seed_timestamp();
        assert!(!needs_refresh(start,start+REFRESH_AFTER-Duration::from_secs(1)));
        assert!(needs_refresh(start,start+REFRESH_AFTER));
        let root=tempfile::tempdir().unwrap();
        let image=load_initial(&root.path().join("missing.jpg")).unwrap();
        assert_eq!(image.updated_at,start);
        fs::write(root.path().join("corrupt.jpg"),b"xml error").unwrap();
        assert_eq!(load_initial(&root.path().join("corrupt.jpg"))
            .unwrap().image.dimensions(),(SIZE,SIZE));
    }
}
