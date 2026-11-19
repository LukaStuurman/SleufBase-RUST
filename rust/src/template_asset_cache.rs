//! Persistent, opt-in-by-availability image cache for deterministic local
//! template assets. The export transaction always owns a fresh copy of each
//! PNG; cache paths are never embedded in the DXF or exposed as input paths.
//! Online map backgrounds expire after 15 minutes; no cached result survives
//! indefinitely when PDOK/OSM source imagery could have changed.
use super::{Kind, copy_asset};
use crate::{export_control::Control, template_raster, map_export, asset_budget::Budget};
use ring::digest::{self, SHA256};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, Ordering}},
    time::{Duration,SystemTime},
};

const MAX_ENTRY: u64 = 256 * 1024 * 1024;
const MAX_TOTAL: u64 = 512 * 1024 * 1024;
const MAX_COUNT: usize = 2048;
const CACHE_FORMAT: &str = "template-local-png-v2";
const REMOTE_MAP_TTL: Duration = Duration::from_secs(15 * 60);

pub(super) struct Persistent {
    root: PathBuf,
    file_hashes: Mutex<HashMap<PathBuf, Arc<OnceLock<Option<String>>>>>,
    wrote: AtomicBool,
}
impl Persistent {
    pub(super) fn open() -> Option<Self> {
        // Existing unit tests must never write into an installed user's cache.
        #[cfg(test)]
        { None }
        #[cfg(not(test))]
        {
            let root = crate::project::app_data_directory().join("template-image-cache-v1");
            fs::create_dir_all(&root).ok()?;
            Some(Self::at(root))
        }
    }
    fn at(root: PathBuf) -> Self {
        Self { root, file_hashes: Mutex::new(HashMap::new()), wrote: AtomicBool::new(false) }
    }
    fn hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len()*2);
        for b in bytes { use std::fmt::Write; let _ = write!(s, "{b:02x}"); }
        s
    }
    // Digest the complete input, not only the path/mtime: replacing a TIFF or
    // changing its pixels cannot result in reuse of an obsolete PNG.
    fn file_digest(&self, path: &Path, control: &Control) -> Option<String> {
        let identity = fs::canonicalize(path).ok()?;
        let cell = {
            let mut hashes = self.file_hashes.lock().ok()?;
            Arc::clone(hashes.entry(identity.clone()).or_insert_with(|| Arc::new(OnceLock::new())))
        };
        cell.get_or_init(|| {
            let before = fs::metadata(&identity).ok()?;
            if !before.is_file() { return None; }
            let mut file = File::open(&identity).ok()?;
            let mut context = digest::Context::new(&SHA256);
            let mut buffer = [0u8; 1024 * 1024];
            loop {
                if control.check().is_err() { return None; }
                let read = file.read(&mut buffer).ok()?;
                if read == 0 { break; }
                context.update(&buffer[..read]);
            }
            let after = fs::metadata(&identity).ok()?;
            if before.len()!=after.len() || before.modified().ok()?!=after.modified().ok()? {
                return None;
            }
            Some(Self::hex(context.finish().as_ref()))
        }).clone()
    }
    fn physical(&self, source: &template_raster::Source, control: &Control) -> Option<serde_json::Value> {
        Some(serde_json::json!({
            "pixels": self.file_digest(&source.path, control)?,
            "transform": source.transform,
            "orientation": source.reverse_orientation,
            "axis": source.axis,
        }))
    }
    fn virtual_source(source: &crate::virtual_raster::Source) -> serde_json::Value {
        serde_json::json!({
            "trench": source.trench,
            "quality": source.quality,
            "reverse": source.reverse,
        })
    }
    fn map_raster(&self, fallback: map_export::Raster<'_>, control: &Control) -> Option<serde_json::Value> {
        match fallback {
            map_export::Raster::Tiff(source) => Some(serde_json::json!({"tiff": self.physical(source,control)?})),
            map_export::Raster::Virtual(source) => Some(serde_json::json!({"virtual": Self::virtual_source(source)})),
        }
    }
    // Only pure local assets are reusable across exports. Every influencing
    // field is included: geometry, orientation, object colors, raster inputs,
    // typography and photo transforms.
    pub(super) fn key(&self, kind: &Kind<'_>, request: &super::Request, control: &Control) -> Option<String> {
        let mut remote_map = false;
        let data = match kind {
            Kind::Copy(_) => return None, // A validated binary copy is already inexpensive.
            Kind::Tiff {source,axis,reverse,lines} => serde_json::json!({
                "type": "tiff", "source": self.physical(source,control)?,
                "axis": axis, "reverse": reverse,
                "lines": lines.iter().map(|line| serde_json::json!({
                    "vertices": line.vertices, "color": line.color
                })).collect::<Vec<_>>(),
                "roads": request.roads, "terrain": request.terrain_boundaries,
            }),
            Kind::Virtual {source,reverse} => serde_json::json!({
                "type": "virtual", "source": Self::virtual_source(source),
                "reverse": reverse, "rules": request.rules,
            }),
            Kind::Photo(photo) => serde_json::json!({
                "type": "photo", "bytes": self.file_digest(&photo.path,control)?,
                "turns": photo.turns, "area": photo.area,
            }),
            Kind::Map {source,bounds,fallback,reference} => {
                let background = match &source.background {
                    map_export::Background::Solid {color} => serde_json::json!({"solid":color}),
                    map_export::Background::Png {path} =>
                        serde_json::json!({"png":self.file_digest(path,control)?}),
                    background => {
                        // A short-lived remote snapshot accelerates a repeat
                        // export without extending the services' existing
                        // tile-cache freshness assumptions indefinitely.
                        remote_map = true;
                        serde_json::json!({"remote":format!("{background:?}")})
                    }
                };
                let raster = if source.include_tiff {
                    if let Some(src) = source.tiff.as_ref() {
                        serde_json::json!({"tiff": self.physical(src,control)?})
                    } else {
                        self.map_raster(match fallback.as_ref()? {
                            map_export::Raster::Tiff(t) => map_export::Raster::Tiff(*t),
                            map_export::Raster::Virtual(v) => map_export::Raster::Virtual(*v)
                        },control)?
                    }
                } else { serde_json::Value::Null };
                let fonts = source.fonts.as_ref().map(|p| Some(serde_json::json!({
                    "regular":self.file_digest(&p.regular,control)?,
                    "bold":self.file_digest(&p.bold,control)?
                }))).unwrap_or(Some(serde_json::Value::Null))?;
                serde_json::json!({
                    "type":"map", "bounds": bounds, "settings": source.settings,
                    "background":background,"raster":raster, "include_tiff":source.include_tiff,
                    "opacity":source.tiff_opacity,"overlays":source.overlays,
                    "attribution":source.attribution,"reference":source.reference.or(*reference),
                    "fonts":fonts,"rules":request.rules,
                })
            }
        };
        if control.check().is_err() { return None; }
        let input = serde_json::to_vec(&(CACHE_FORMAT, env!("CARGO_PKG_VERSION"), data)).ok()?;
        Some(format!("{}{}", if remote_map {"remote-"} else {"local-"},
            Self::hex(digest::digest(&SHA256,&input).as_ref())))
    }
    fn path(&self, key: &str) -> PathBuf { self.root.join(format!("{key}.png")) }
    fn valid(&self, path: &Path) -> bool {
        let Ok(metadata) = fs::symlink_metadata(path) else {return false;};
        if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > MAX_ENTRY {
            return false;
        }
        let Ok(mut reader) = image::ImageReader::open(path) else {return false;};
        reader.set_format(image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().is_ok()
    }
    pub(super) fn restore(&self,key:&str,dest:&Path,control:&Control,budget:&Budget)->Result<bool,String>{
        let path = self.path(key);
        if key.starts_with("remote-") {
            let fresh = fs::metadata(&path).ok()
                .and_then(|m|m.modified().ok())
                .and_then(|timestamp|SystemTime::now().duration_since(timestamp).ok())
                .is_some_and(|elapsed|elapsed<=REMOTE_MAP_TTL);
            if !fresh {return Ok(false);}
        }
        if !self.valid(&path) {
            // Clear a corrupt immutable entry so the freshly rendered PNG can
            // replace it. Only remove ordinary owned files, never links.
            if fs::symlink_metadata(&path).is_ok_and(|m|m.file_type().is_file()) {
                let _=fs::remove_file(&path);
            }
            return Ok(false);
        }
        // The usual quota accounting and post-copy PNG validation still apply.
        // The cache file was fully decoded above. A second PNG decode after
        // copying would needlessly double the cost of a cache hit.
        copy_asset(&path,dest,control,budget,false)?;
        Ok(true)
    }
    pub(super) fn remember(&self,key:&str,asset:&Path,control:&Control){
        let path = self.path(key);
        // A stale remote snapshot is replaced atomically; permanent local
        // snapshots remain immutable and keyed by source content.
        if path.exists() && !key.starts_with("remote-") {return;}
        let Ok(metadata)=fs::metadata(asset) else {return;};
        if !metadata.is_file()||metadata.len()==0||metadata.len()>MAX_ENTRY{return;}
        let result=crate::storage::atomic_write_checked(&path,key.starts_with("remote-"),|out|{
            let mut input=File::open(asset).map_err(|e|e.to_string())?;
            let mut buf=[0u8;512*1024];
            loop{
                control.check()?;
                let n=input.read(&mut buf).map_err(|e|e.to_string())?;
                if n==0 {break;}
                out.write_all(&buf[..n]).map_err(|e|e.to_string())?;
            }
            Ok(())
        },||control.check());
        if result.is_ok(){self.wrote.store(true,Ordering::Relaxed);}
    }
    pub(super) fn prune(&self){
        if !self.wrote.load(Ordering::Relaxed){return;}
        let Ok(entries)=fs::read_dir(&self.root)else{return;};
        let mut files=Vec::new();
        let mut total=0u64;
        for entry in entries.flatten(){
            let path=entry.path();
            if path.extension().and_then(|v|v.to_str()) != Some("png"){continue;}
            let Ok(m)=fs::symlink_metadata(&path) else {continue;};
            if !m.file_type().is_file(){continue;}
            total=total.saturating_add(m.len());
            files.push((path,m.len(),m.modified().unwrap_or(SystemTime::UNIX_EPOCH)));
        }
        if total<=MAX_TOTAL && files.len()<=MAX_COUNT {return;}
        files.sort_by_key(|(_,_,modified)|*modified);
        let mut count=files.len();
        for (path,size,_) in files{
            if total<=MAX_TOTAL && count<=MAX_COUNT{break;}
            if fs::remove_file(path).is_ok(){total=total.saturating_sub(size);count-=1;}
        }
    }
}
