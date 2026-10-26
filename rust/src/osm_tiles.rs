//! On-demand OpenStreetMap raster tiles for the native RD (EPSG:28992) map.
//! Only the user-requested viewport is fetched; no prefetch or offline bulk download.
//! See https://operations.osmfoundation.org/policies/tiles/
use crate::{input, model::Bounds};
use image::{ImageFormat, RgbaImage};
use std::{
    collections::{HashMap, VecDeque},
    io::Write,
    path::{Path, PathBuf},
    sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, Arc, Mutex, OnceLock, Weak},
    time::{Duration, SystemTime},
};

const CACHE_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const MAX_TILES: usize = 24;
const MAX_TILE_BYTES: u64 = 2 * 1024 * 1024;
const TILE_SIZE: u32 = 256;
const OSM_HOST: &str = "https://tile.openstreetmap.org";

#[derive(Debug)]
pub struct OsmRaster {
    pub bounds: Bounds,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// True when the network was unavailable and an expired cache tile was reused.
    pub stale_cache: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TileId {
    z: u8,
    x: u32,
    y: u32,
}
impl TileId {
    fn url(self) -> String {
        format!("{OSM_HOST}/{}/{}/{}.png", self.z, self.x, self.y)
    }
    fn path(self, cache: &Path) -> PathBuf {
        cache.join(self.z.to_string()).join(self.x.to_string()).join(format!("{}.png", self.y))
    }
}

#[derive(Clone, Copy, Debug)]
struct Grid {
    z: u8,
    min_x: u32,
    max_x: u32,
    min_y: u32,
    max_y: u32,
}
impl Grid {
    fn count(self) -> usize {
        (self.max_x - self.min_x + 1) as usize * (self.max_y - self.min_y + 1) as usize
    }
    fn ids(self) -> impl Iterator<Item = TileId> {
        (self.min_y..=self.max_y).flat_map(move |y| {
            (self.min_x..=self.max_x).map(move |x| TileId { z: self.z, x, y })
        })
    }
}

/// Standard Netherlands RD -> WGS84 approximation. This is not RDNAPTRANS
/// and must not be used for surveying or exact cadastral position.
fn rd_to_wgs84(x: f64, y: f64) -> (f64, f64) {
    let dx = (x - 155_000.0) / 100_000.0;
    let dy = (y - 463_000.0) / 100_000.0;
    let lat = 52.155_174_40
        + (3235.65389 * dy
            - 32.58297 * dx.powi(2)
            - 0.2475 * dy.powi(2)
            - 0.84978 * dx.powi(2) * dy
            - 0.0655 * dy.powi(3)
            - 0.01709 * dx.powi(2) * dy.powi(2)
            - 0.00738 * dx
            + 0.0053 * dx.powi(4)
            - 0.00039 * dx.powi(2) * dy.powi(3)
            + 0.00033 * dx.powi(4) * dy
            - 0.00012 * dx * dy)
            / 3600.0;
    let lon = 5.387_206_21
        + (5260.52916 * dx
            + 105.94684 * dx * dy
            + 2.45656 * dx * dy.powi(2)
            - 0.81885 * dx.powi(3)
            + 0.05594 * dx * dy.powi(3)
            - 0.05607 * dx.powi(3) * dy
            + 0.01199 * dy
            - 0.00256 * dx.powi(3) * dy.powi(2)
            + 0.00128 * dx * dy.powi(4)
            + 0.00022 * dy.powi(2)
            - 0.00022 * dx.powi(2)
            + 0.00026 * dx.powi(5))
            / 3600.0;
    (lat, lon)
}
fn mercator(x: f64, y: f64, z: u8) -> (f64, f64) {
    let (lat, lon) = rd_to_wgs84(x, y);
    let n = f64::from(1u32 << z);
    let phi = lat.clamp(-85.051_128_78, 85.051_128_78).to_radians();
    ((lon + 180.0) / 360.0 * n, (1.0 - phi.tan().asinh() / std::f64::consts::PI) / 2.0 * n)
}
fn check_bounds(b: Bounds, width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 || width > 2048 || height > 2048
        || u64::from(width) * u64::from(height) > 2_000_000
    {
        return Err("OSM-kaart overschrijdt het rasterbudget".into());
    }
    check_extent(b)
}
fn check_extent(b:Bounds)->Result<(),String>{
    if ![b.min_x, b.min_y, b.max_x, b.max_y].iter().all(|n| n.is_finite())
        || b.min_x >= b.max_x || b.min_y >= b.max_y
        || b.min_x < -50_000.0 || b.max_x > 350_000.0
        || b.min_y < 250_000.0 || b.max_y > 650_000.0
    {
        return Err("OSM ondersteunt hier alleen geldige Nederlandse RD-grenzen".into());
    }
    Ok(())
}
fn grid_for(b: Bounds, width: u32, height: u32) -> Result<Grid, String> {
    check_bounds(b, width, height)?;
    choose_grid(b,width,height)
}
fn choose_grid(b:Bounds,width:u32,height:u32)->Result<Grid,String>{
    let (center_lat, _) = rd_to_wgs84((b.min_x + b.max_x) * 0.5, (b.min_y + b.max_y) * 0.5);
    let meters_per_pixel = ((b.max_x - b.min_x) / f64::from(width))
        .max((b.max_y - b.min_y) / f64::from(height));
    let target = (156_543.033_928 * center_lat.to_radians().cos() / meters_per_pixel)
        .log2().floor().clamp(0.0, 18.0) as u8;
    for z in (0..=target).rev() {
        let mut min_x = u32::MAX;
        let mut max_x = 0u32;
        let mut min_y = u32::MAX;
        let mut max_y = 0u32;
        let side = (1u32 << z) - 1;
        // Include middle samples, as the RD -> Mercator transform isn't linear.
        for yi in 0..=4 {
            for xi in 0..=4 {
                let x = b.min_x + (b.max_x - b.min_x) * f64::from(xi) / 4.0;
                let y = b.min_y + (b.max_y - b.min_y) * f64::from(yi) / 4.0;
                let (tx, ty) = mercator(x, y, z);
                let tx = (tx.floor().max(0.0) as u32).min(side);
                let ty = (ty.floor().max(0.0) as u32).min(side);
                min_x = min_x.min(tx);
                max_x = max_x.max(tx);
                min_y = min_y.min(ty);
                max_y = max_y.max(ty);
            }
        }
        let grid = Grid { z, min_x, max_x, min_y, max_y };
        if grid.count() <= MAX_TILES {
            return Ok(grid);
        }
    }
    Err("OSM-kaartgebied is te groot".into())
}
fn decode_png(bytes: &[u8]) -> Result<RgbaImage, String> {
    if bytes.len() as u64 > MAX_TILE_BYTES {
        return Err("OSM-tegel overschrijdt de downloadlimiet".into());
    }
    if image::guess_format(bytes).ok() != Some(ImageFormat::Png) {
        return Err("OSM-tegel is geen PNG".into());
    }
    let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(TILE_SIZE);
    limits.max_image_height = Some(TILE_SIZE);
    limits.max_alloc = Some(4 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| format!("OSM-tegel ongeldig: {e}"))?.into_rgba8();
    if image.dimensions() != (TILE_SIZE, TILE_SIZE) {
        return Err("OSM-tegel heeft geen 256x256 pixels".into());
    }
    Ok(image)
}
#[derive(Clone)]
struct MemoryTile { path: PathBuf, rgba: Arc<[u8]>, timestamp: SystemTime, signature: Option<(u64, SystemTime)> }
struct TileMemory { entries: VecDeque<MemoryTile>, limit: usize, bytes: usize, byte_limit: usize }
impl TileMemory {
    fn new(limit: usize) -> Self {
        let limit = limit.max(64);
        Self { entries: VecDeque::new(), limit, bytes: 0,
            byte_limit: limit.saturating_mul((TILE_SIZE * TILE_SIZE * 4) as usize) }
    }
    fn get(&mut self, path: &Path, now: SystemTime, signature: Option<(u64, SystemTime)>) -> Option<(RgbaImage, bool)> {
        let index = self.entries.iter().position(|entry| entry.path == path)?;
        let entry = self.entries.remove(index)?;
        if entry.signature != signature {
            self.bytes -= entry.rgba.len();
            return None;
        }
        // A future timestamp is fresh too, matching Python's signed timedelta.
        let fresh = now.duration_since(entry.timestamp).map_or(true, |age| age <= CACHE_AGE);
        let image = RgbaImage::from_raw(TILE_SIZE, TILE_SIZE, entry.rgba.to_vec())?;
        self.entries.push_back(entry);
        Some((image, fresh))
    }
    fn remember(&mut self, path: &Path, image: &RgbaImage, timestamp: SystemTime) {
        if let Some(index) = self.entries.iter().position(|entry| entry.path == path) {
            self.bytes -= self.entries.remove(index).unwrap().rgba.len();
        }
        let rgba: Arc<[u8]> = Arc::from(image.as_raw().as_slice());
        self.bytes += rgba.len();
        let signature = std::fs::metadata(path).ok().and_then(|meta| Some((meta.len(), meta.modified().ok()?)));
        self.entries.push_back(MemoryTile { path: path.to_path_buf(), rgba, timestamp, signature });
        while self.entries.len() > self.limit || self.bytes > self.byte_limit {
            if let Some(old) = self.entries.pop_front() { self.bytes -= old.rgba.len(); }
        }
    }
}
fn tile_memory() -> &'static Mutex<TileMemory> {
    static MEMORY: OnceLock<Mutex<TileMemory>> = OnceLock::new();
    MEMORY.get_or_init(|| Mutex::new(TileMemory::new(crate::resource_policy::get().tile_memory_cache_entries)))
}
fn persistent_client() -> Result<&'static reqwest::blocking::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
    CLIENT.get_or_init(|| reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15)).connect_timeout(Duration::from_secs(8))
        .user_agent("SleufBase-Rust/0.1 (+https://github.com/LukaStuurman/SleufBase)")
        .build().map_err(|e| e.to_string())).as_ref().map_err(Clone::clone)
}
fn tile_lock(path: &Path) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) { return lock; }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
    lock
}
static ACTIVE_DOWNLOADS: AtomicUsize = AtomicUsize::new(0);
struct DownloadPermit;
impl Drop for DownloadPermit { fn drop(&mut self) { ACTIVE_DOWNLOADS.fetch_sub(1, Ordering::AcqRel); } }
fn download_permit(cancel: &AtomicBool) -> Result<DownloadPermit, String> {
    let limit = crate::resource_policy::get().network_workers.max(1);
    loop {
        input::check(cancel)?;
        let active = ACTIVE_DOWNLOADS.load(Ordering::Acquire);
        if active < limit && ACTIVE_DOWNLOADS.compare_exchange_weak(active, active + 1, Ordering::AcqRel, Ordering::Acquire).is_ok() {
            return Ok(DownloadPermit);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn cached(path: &Path) -> Option<(RgbaImage, bool)> {
    // Downloads remain usable in memory when the best-effort disk write fails.
    // load_tile supplies an absolute path under its canonical namespace.
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| {
        if path.is_absolute() { path.to_path_buf() }
        else { std::env::current_dir().unwrap_or_default().join(path) }
    });
    let path = canonical.as_path();
    let metadata = std::fs::metadata(path).ok();
    let signature = metadata.as_ref().and_then(|meta| Some((meta.len(), meta.modified().ok()?)));
    if let Some(hit) = tile_memory().lock().ok()?.get(path, SystemTime::now(), signature) { return Some(hit); }
    let meta = metadata?;
    if meta.len() > MAX_TILE_BYTES { return None; }
    let timestamp = meta.modified().ok()?;
    let fresh = SystemTime::now().duration_since(timestamp).map_or(true, |age| age <= CACHE_AGE);
    let bytes = std::fs::read(path).ok()?;
    let image = decode_png(&bytes).ok()?;
    tile_memory().lock().ok()?.remember(path, &image, timestamp);
    Some((image, fresh))
}
fn store(path: &Path, bytes: &[u8]) {
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() { return; }
    if let Ok(mut temp) = tempfile::NamedTempFile::new_in(dir) {
        if temp.write_all(bytes).is_ok() && temp.flush().is_ok() {
            // Best effort: if replacement fails (e.g. Windows locks), the next
            // viewport request may refresh the tile again.
            let _ = temp.persist(path);
        }
    }
}
fn load_tile(id: TileId, cache: &Path, cancel: &AtomicBool)
    -> Result<(RgbaImage, bool, bool), String>
{
    input::check(cancel)?;
    // Normalize the namespace before registering overlapping requests.
    std::fs::create_dir_all(cache).map_err(|e| format!("OSM-cachemap: {e}"))?;
    let root = std::fs::canonicalize(cache).map_err(|e| format!("OSM-cachemap: {e}"))?;
    let path = id.path(&root);
    if let Some((tile, true)) = cached(&path) { return Ok((tile, false, false)); }
    let lock = tile_lock(&path);
    let _guard = loop {
        input::check(cancel)?;
        match lock.try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return Err("OSM-tegelslot niet beschikbaar".into()),
        }
    };
    let old = cached(&path);
    if let Some((tile, true)) = &old { return Ok((tile.clone(), false, false)); }
    let fetch = || -> Result<RgbaImage, String> {
        input::check(cancel)?;
        let _permit = download_permit(cancel)?;
        let client = persistent_client()?;
        let response = client.get(id.url()).send()
            .map_err(|e| format!("OSM-tegel onbereikbaar: {e}"))?
            .error_for_status().map_err(|e| format!("OSM HTTP-fout: {e}"))?;
        if response.content_length().is_some_and(|size| size > MAX_TILE_BYTES) {
            return Err("OSM-tegel overschrijdt de downloadlimiet".into());
        }
        let mut response = response;
        let bytes = input::read_chunks(&mut response, MAX_TILE_BYTES, "OSM-tegel", cancel)?;
        let decoded = decode_png(&bytes)?;
        input::check(cancel)?;
        store(&path, &bytes);
        tile_memory().lock().map_err(|_| "OSM-geheugencache niet beschikbaar")?
            .remember(&std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone()), &decoded,
                SystemTime::now());
        Ok(decoded)
    };
    let mut remote = Err("OSM-tegel niet opgehaald".into());
    for attempt in 0..3 {
        remote = fetch();
        if remote.is_ok() { break; }
        input::check(cancel)?;
        if attempt < 2 { cancellable_delay(Duration::from_millis(400 * (attempt + 1)), cancel)?; }
    }
    match remote {
        Ok(tile) => Ok((tile, false, true)),
        Err(e) => {
            input::check(cancel)?;
            if let Some((tile, _)) = old {
                Ok((tile, true, false))
            } else {
                Err(e)
            }
        }
    }
}
fn cancellable_delay(delay: Duration, cancel: &AtomicBool) -> Result<(), String> {
    let deadline = std::time::Instant::now() + delay;
    loop {
        input::check(cancel)?;
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() { return Ok(()); }
        std::thread::sleep(remaining.min(Duration::from_millis(10)));
    }
}
fn render(b: Bounds, width: u32, height: u32, grid: Grid,
    tiles: &HashMap<TileId, RgbaImage>, cancel: &AtomicBool) -> Result<Vec<u8>, String>
{
    let mut rgba = vec![0u8; width as usize * height as usize * 4];
    for py in 0..height {
        input::check(cancel)?;
        let y = b.max_y - (f64::from(py) + 0.5) / f64::from(height) * (b.max_y - b.min_y);
        for px in 0..width {
            let x = b.min_x + (f64::from(px) + 0.5) / f64::from(width) * (b.max_x - b.min_x);
            let (tx, ty) = mercator(x, y, grid.z);
            let col = tx.floor() as u32;
            let row = ty.floor() as u32;
            let id = TileId { z: grid.z, x: col, y: row };
            if let Some(tile) = tiles.get(&id) {
                let ix = ((tx - tx.floor()) * f64::from(TILE_SIZE)).floor().clamp(0.0, 255.0) as u32;
                let iy = ((ty - ty.floor()) * f64::from(TILE_SIZE)).floor().clamp(0.0, 255.0) as u32;
                let offset = (py as usize * width as usize + px as usize) * 4;
                rgba[offset..offset + 4].copy_from_slice(&tile.get_pixel(ix, iy).0);
            }
        }
    }
    Ok(rgba)
}

/// Fetch only tiles needed for the RD map viewport, with a 7-day disk cache.
/// No offline area downloads, bulk requests, or arbitrary tile endpoints.
pub fn request_map(bounds: Bounds, width: u32, height: u32, cache_dir: PathBuf)
    -> Result<OsmRaster, String>
{
    request_map_with_cancel(bounds, width, height, &cache_dir, &AtomicBool::new(false))
}
pub fn request_map_with_cancel(bounds: Bounds, width: u32, height: u32,
    cache_dir: &Path, cancel: &AtomicBool) -> Result<OsmRaster, String>
{
    let grid = grid_for(bounds, width, height)?;
    request_grid(bounds,width,height,grid,cache_dir,cancel)
}
/// Full export resolution, still at most 24 on-demand server tiles. Desktop
/// preview budgets stay unchanged; no extra network area is prefetched.
pub fn request_export_map_with_cancel(bounds:Bounds,width:u32,height:u32,cache_dir:&Path,cancel:&AtomicBool)->Result<OsmRaster,String>{
    input::check(cancel)?;
    if width==0||height==0||width>10_000||height>10_000||u64::from(width)*u64::from(height)>50_000_000{
        return Err("OSM-exportkaart overschrijdt het rasterbudget".into());
    }
    check_extent(bounds)?;
    let grid=choose_grid(bounds,width,height)?;
    request_grid(bounds,width,height,grid,cache_dir,cancel)
}
fn request_grid(bounds:Bounds,width:u32,height:u32,grid:Grid,cache_dir:&Path,cancel:&AtomicBool)->Result<OsmRaster,String>{
    input::check(cancel)?;
    let mut tiles = HashMap::with_capacity(grid.count());
    let mut stale_cache = false;
    for id in grid.ids() {
        input::check(cancel)?;
        let (tile, stale, downloaded) = load_tile(id, cache_dir, cancel)?;
        stale_cache |= stale;
        tiles.insert(id, tile);
        // User-initiated viewport only; avoid hammering the public tile server.
        if downloaded { cancellable_delay(Duration::from_millis(300), cancel)?; }
    }
    let rgba = render(bounds, width, height, grid, &tiles, cancel)?;
    Ok(OsmRaster { bounds, width, height, rgba, stale_cache })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn immutable_tile_lru_count_bytes_ttl_and_disk_signature() {
        let mut memory = TileMemory::new(1);
        assert_eq!(memory.limit, 64); // Original client minimum, including policy overrides.
        let image = RgbaImage::from_pixel(256, 256, image::Rgba([1, 2, 3, 255]));
        let timestamp = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        for index in 0..64 {
            memory.remember(Path::new(&format!("missing-test-tile-{index}")), &image, timestamp);
        }
        let first = Path::new("missing-test-tile-0");
        let (mut hit, fresh) = memory.get(first, timestamp + CACHE_AGE, None).unwrap();
        assert!(fresh);
        hit.as_mut()[0] = 99;
        assert_eq!(memory.get(first, timestamp, None).unwrap().0.as_raw()[0], 1);
        // Python datetime resolves microseconds; Windows SystemTime cannot
        // represent a one-nanosecond step past the exact freshness boundary.
        assert!(!memory.get(first, timestamp + CACHE_AGE + Duration::from_micros(1), None).unwrap().1);
        memory.remember(Path::new("missing-test-tile-64"), &image, timestamp);
        assert!(memory.get(Path::new("missing-test-tile-1"), timestamp, None).is_none());
        assert_eq!(memory.entries.len(), 64);
        assert_eq!(memory.bytes, memory.byte_limit);
        assert!(memory.get(first, timestamp, Some((12, timestamp))).is_none());
        assert_eq!(memory.bytes, 63 * 256 * 256 * 4);
    }
    #[test]
    fn cache_paths_are_isolated_and_modified_files_invalidate_memory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tile.png");
        let image = RgbaImage::from_pixel(256, 256, image::Rgba([1, 2, 3, 255]));
        image.save(&path).unwrap();
        assert_eq!(cached(&path).unwrap().0.as_raw()[0], 1);
        std::fs::write(&path, b"changed disk file").unwrap();
        assert!(cached(&path).is_none());
        assert!(cached(&dir.path().join("other/tile.png")).is_none());
    }
    #[test]
    fn downloaded_memory_survives_failed_disk_store_with_fresh_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let image = RgbaImage::from_pixel(256, 256, image::Rgba([7, 8, 9, 255]));
        let missing = root.join("missing-parent/tile.png");
        // Equivalent to a download whose best-effort store produced no file.
        tile_memory().lock().unwrap().remember(&missing, &image, SystemTime::now());
        let (hit, fresh) = cached(&missing).unwrap();
        assert!(fresh);
        assert_eq!(hit.as_raw()[0], 7);

        let old_path = root.join("old.png");
        image.save(&old_path).unwrap();
        let old = SystemTime::now() - CACHE_AGE - Duration::from_secs(1);
        std::fs::File::options().write(true).open(&old_path).unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(old)).unwrap();
        let disk_timestamp = std::fs::metadata(&old_path).unwrap().modified().unwrap();
        assert!(!cached(&old_path).unwrap().1);
        // A failed replacement leaves the old disk signature, but freshly
        // downloaded memory pixels receive their own current freshness clock.
        let downloaded = RgbaImage::from_pixel(256, 256, image::Rgba([42, 8, 9, 255]));
        tile_memory().lock().unwrap().remember(&old_path, &downloaded, SystemTime::now());
        let (hit, fresh) = cached(&old_path).unwrap();
        assert!(fresh);
        assert_eq!(hit.as_raw()[0], 42);
        assert_eq!(std::fs::metadata(&old_path).unwrap().modified().unwrap(), disk_timestamp);
    }
    #[test]
    fn cancellation_precedes_download_gate_and_retry_delay() {
        let cancel = AtomicBool::new(true);
        assert!(download_permit(&cancel).is_err());
        assert!(cancellable_delay(Duration::from_secs(1), &cancel).is_err());
    }
    #[test]
    fn full_a4_export_reuses_cached_tiles_beyond_desktop_preview_budget(){
        let bounds=Bounds{min_x:154980.,min_y:462980.,max_x:155040.,max_y:463020.};
        let [width,height]=[3344,2316];
        assert!(check_bounds(bounds,width,height).is_err());
        let grid=choose_grid(bounds,width,height).unwrap();assert!(grid.count()<=MAX_TILES);
        let dir=tempfile::tempdir().unwrap();let color=[29,44,61,255];
        let image=image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(256,256,image::Rgba(color)));
        let mut png=Vec::new();image.write_to(&mut std::io::Cursor::new(&mut png),ImageFormat::Png).unwrap();
        for id in grid.ids(){store(&id.path(dir.path()),&png);}
        let map=request_export_map_with_cancel(bounds,width,height,dir.path(),&AtomicBool::new(false)).unwrap();
        assert!(!map.stale_cache);assert_eq!(map.rgba.len(),width as usize*height as usize*4);
        assert!(map.rgba.as_chunks::<4>().0.iter().all(|p|*p==color));
        assert!(request_export_map_with_cancel(bounds,10001,1,dir.path(),&AtomicBool::new(false)).is_err());
        assert!(request_export_map_with_cancel(bounds,width,height,dir.path(),&AtomicBool::new(true)).is_err());
    }
    #[test]
    fn rd_reference_and_zoom_tile_are_finite() {
        let (lat, lon) = rd_to_wgs84(155_000.0, 463_000.0);
        assert!((lat - 52.155_174_40).abs() < 1e-12);
        assert!((lon - 5.387_206_21).abs() < 1e-12);
        let (x, y) = mercator(155_000.0, 463_000.0, 12);
        assert!(x > 2100.0 && x < 2200.0 && y > 1300.0 && y < 1400.0);
    }
    #[test]
    fn tile_budget_and_invalid_extents() {
        let b = Bounds { min_x: 153000., min_y: 461000., max_x: 157000., max_y: 465000. };
        let grid = grid_for(b, 1200, 900).unwrap();
        assert!(grid.count() > 0 && grid.count() <= MAX_TILES);
        assert_eq!(grid.ids().count(), grid.count());
        assert!(grid_for(b, 0, 100).is_err());
        assert!(grid_for(Bounds { min_x: f64::NAN, ..b }, 100, 100).is_err());
        assert!(grid_for(Bounds { min_x: -100000., ..b }, 100, 100).is_err());
    }
    #[test]
    fn samples_real_tile_grid_without_network_and_honors_cancel() {
        let bounds = Bounds { min_x: 154900., min_y: 462900., max_x: 155100., max_y: 463100. };
        let grid = grid_for(bounds, 96, 96).unwrap();
        let tiles: HashMap<_, _> = grid.ids().map(|id| {
            (id, RgbaImage::from_pixel(256, 256, image::Rgba([14, 61, 80, 255])))
        }).collect();
        let rgba = render(bounds, 96, 96, grid, &tiles, &AtomicBool::new(false)).unwrap();
        assert!(rgba.chunks_exact(4).all(|px| px == [14, 61, 80, 255]));
        assert!(render(bounds, 96, 96, grid, &tiles, &AtomicBool::new(true)).is_err());
    }
    #[test]
    fn cached_viewport_reprojects_without_any_network_access() {
        let bounds = Bounds { min_x: 154800., min_y: 462800., max_x: 155200., max_y: 463200. };
        let grid = grid_for(bounds, 96, 96).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let image = image::DynamicImage::ImageRgba8(
            RgbaImage::from_pixel(256, 256, image::Rgba([29, 44, 61, 255]))
        );
        let mut png = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png).unwrap();
        for id in grid.ids() { store(&id.path(dir.path()), &png); }
        let view = request_map_with_cancel(bounds, 96, 96, dir.path(), &AtomicBool::new(false)).unwrap();
        assert!(!view.stale_cache);
        assert_eq!(view.rgba.len(), 96 * 96 * 4);
        assert!(view.rgba.chunks_exact(4).all(|px| px == [29, 44, 61, 255]));
    }
    #[test]
    fn bad_cache_or_png_is_never_used() {
        assert!(decode_png(b"not a PNG").is_err());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("1/2/3.png");
        store(&path, b"not a PNG");
        assert!(cached(&path).is_none());
        let image = RgbaImage::from_pixel(256, 256, image::Rgba([1, 2, 3, 255]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(image).write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png).unwrap();
        std::fs::remove_file(&path).unwrap();
        store(&path, &png);
        assert_eq!(cached(&path).unwrap().0.get_pixel(7, 7).0, [1, 2, 3, 255]);
    }
}
