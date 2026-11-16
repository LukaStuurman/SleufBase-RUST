//! Progressive, pixel-density-aware OSM backgrounds for interactive viewing.
//! Only the current viewport is requested. Exports retain their separate budget.
use super::*;
use std::{sync::mpsc, time::Instant};

pub const MAX_PIXELS: u64 = 16_777_216;
pub const MAX_SIDE: u32 = 8192;
const MAX_VIEW_TILES: usize = 256;

pub struct Progress<'a> {
    pub bounds: Bounds,
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
    pub completed: usize,
    pub total: usize,
    pub stale_cache: bool,
}

#[derive(Default)]
pub struct Renderer {
    // Reproject the last complete RD image into interim frames, so a pan/zoom
    // does not replace useful map pixels with transparent missing-tile holes.
    previous: Option<OsmRaster>,
}

fn plan(bounds: Bounds, width: u32, height: u32) -> Result<Grid, String> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE
        || u64::from(width) * u64::from(height) > MAX_PIXELS
    {
        return Err("OSM-werkkaart overschrijdt het rasterbudget".into());
    }
    check_extent(bounds)?;
    let (latitude, _) = rd_to_wgs84(
        (bounds.min_x + bounds.max_x) * 0.5,
        (bounds.min_y + bounds.max_y) * 0.5,
    );
    let pixel = ((bounds.max_x - bounds.min_x) / f64::from(width))
        .max((bounds.max_y - bounds.min_y) / f64::from(height));
    // Use physical display pixels and round up, rather than magnifying an
    // already coarser zoom level. OSM's standard raster tiles end at zoom 19.
    let target = (156_543.033_928 * latitude.to_radians().cos() / pixel)
        .log2().ceil().clamp(0., 19.) as u8;
    for zoom in (0..=target).rev() {
        let grid = grid_at(bounds, zoom);
        if grid.count() <= MAX_VIEW_TILES { return Ok(grid); }
    }
    Err("OSM-kaartgebied is te groot".into())
}

fn previous_pixels(previous: Option<&OsmRaster>, bounds: Bounds,
    width: u32, height: u32, cancel: &AtomicBool) -> Result<Vec<u8>, String>
{
    let mut rgba = vec![0; width as usize * height as usize * 4];
    let Some(old) = previous else { return Ok(rgba); };
    let dx = (bounds.max_x - bounds.min_x) / f64::from(width);
    let dy = (bounds.max_y - bounds.min_y) / f64::from(height);
    let sx = f64::from(old.width) / (old.bounds.max_x - old.bounds.min_x);
    let sy = f64::from(old.height) / (old.bounds.max_y - old.bounds.min_y);
    for row in 0..height {
        input::check(cancel)?;
        let y = bounds.max_y - (f64::from(row) + 0.5) * dy;
        let source_y = ((old.bounds.max_y - y) * sy).floor();
        if source_y < 0. || source_y >= f64::from(old.height) { continue; }
        for col in 0..width {
            let x = bounds.min_x + (f64::from(col) + 0.5) * dx;
            let source_x = ((x - old.bounds.min_x) * sx).floor();
            if source_x < 0. || source_x >= f64::from(old.width) { continue; }
            let from = (source_y as usize * old.width as usize + source_x as usize) * 4;
            let to = (row as usize * width as usize + col as usize) * 4;
            rgba[to..to + 4].copy_from_slice(&old.rgba[from..from + 4]);
        }
    }
    Ok(rgba)
}

fn tile_at<'a>(tiles: &[Option<&'a RgbaImage>], grid: Grid, x: i64, y: i64)
    -> Option<&'a RgbaImage>
{
    if x < i64::from(grid.min_x) || x > i64::from(grid.max_x)
        || y < i64::from(grid.min_y) || y > i64::from(grid.max_y)
    { return None; }
    let index = (y as u32 - grid.min_y) as usize * (grid.max_x - grid.min_x + 1) as usize
        + (x as u32 - grid.min_x) as usize;
    tiles[index]
}

fn paint(bounds: Bounds, width: u32, height: u32, grid: Grid,
    tiles: &HashMap<TileId, RgbaImage>, rgba: &mut [u8], cancel: &AtomicBool) -> Result<(), String>
{
    let dense: Vec<_> = grid.ids().map(|id| tiles.get(&id)).collect();
    let dx = (bounds.max_x - bounds.min_x) / f64::from(width);
    let dy = (bounds.max_y - bounds.min_y) / f64::from(height);
    for row in 0..height {
        input::check(cancel)?;
        let y = bounds.max_y - (f64::from(row) + 0.5) * dy;
        // Over short horizontal strips the smooth RD transform is effectively
        // linear. Evaluate exact endpoints every 16 display pixels, avoiding
        // millions of polynomial/trigonometric evaluations for each preview.
        for left in (0..width).step_by(16) {
            let end = (left + 16).min(width);
            let a = mercator(bounds.min_x + (f64::from(left) + 0.5) * dx, y, grid.z);
            let b = mercator(bounds.min_x + (f64::from(end) + 0.5) * dx, y, grid.z);
            let step = ((b.0 - a.0) / f64::from(end - left), (b.1 - a.1) / f64::from(end - left));
            for col in left..end {
                let offset = f64::from(col - left);
                let tx = a.0 + offset * step.0;
                let ty = a.1 + offset * step.1;
                let tile_x = tx.floor() as i64;
                let tile_y = ty.floor() as i64;
                let Some(primary) = tile_at(&dense, grid, tile_x, tile_y) else { continue; };
                // Sample around pixel centres, including adjoining cached tiles.
                // A missing neighbour temporarily extends the primary tile edge.
                let x = tx * f64::from(TILE_SIZE) - 0.5;
                let y = ty * f64::from(TILE_SIZE) - 0.5;
                let ix = x.floor() as i64;
                let iy = y.floor() as i64;
                let fx = x - ix as f64;
                let fy = y - iy as f64;
                let sample = |x: i64, y: i64| {
                    if let Some(tile) = tile_at(&dense, grid, x.div_euclid(256), y.div_euclid(256)) {
                        tile.get_pixel(x.rem_euclid(256) as u32, y.rem_euclid(256) as u32).0
                    } else {
                        primary.get_pixel((x - tile_x * 256).clamp(0, 255) as u32,
                            (y - tile_y * 256).clamp(0, 255) as u32).0
                    }
                };
                let [a, b, c, d] = [sample(ix, iy), sample(ix + 1, iy),
                    sample(ix, iy + 1), sample(ix + 1, iy + 1)];
                let target = (row as usize * width as usize + col as usize) * 4;
                for channel in 0..4 {
                    let top = f64::from(a[channel]) * (1. - fx) + f64::from(b[channel]) * fx;
                    let bottom = f64::from(c[channel]) * (1. - fx) + f64::from(d[channel]) * fx;
                    rgba[target + channel] = (top * (1. - fy) + bottom * fy).round() as u8;
                }
            }
        }
    }
    Ok(())
}

impl Renderer {
    pub fn request(&mut self, bounds: Bounds, width: u32, height: u32,
        cache: &Path, cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress<'_>) -> Result<(), String>) -> Result<OsmRaster, String>
    {
        input::check(cancel)?;
        let grid = plan(bounds, width, height)?;
        std::fs::create_dir_all(cache).map_err(|e| format!("OSM-cachemap: {e}"))?;
        let root = std::fs::canonicalize(cache).map_err(|e| format!("OSM-cachemap: {e}"))?;
        let mut ids: Vec<_> = grid.ids().collect();
        let center = mercator((bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5, grid.z);
        ids.sort_by(|a, b| {
            let distance = |id: &TileId| (f64::from(id.x) + 0.5 - center.0).powi(2)
                + (f64::from(id.y) + 0.5 - center.1).powi(2);
            distance(a).total_cmp(&distance(b))
        });
        let total = ids.len();
        let mut tiles = HashMap::with_capacity(total);
        let mut pending = Vec::new();
        let mut completed = 0;
        for id in ids {
            input::check(cancel)?;
            if let Some((tile, fresh)) = cached(&id.path(&root)) {
                tiles.insert(id, tile);
                if fresh { completed += 1; } else { pending.push(id); }
            } else { pending.push(id); }
        }
        let mut rgba = previous_pixels(self.previous.as_ref(), bounds, width, height, cancel)?;
        let mut stale_cache = false;
        let mut preview_count = 0;
        let mut last_preview = None::<Instant>;
        // At most four full-size previews per request, at least 150 ms apart.
        // Advance thresholds with cache hits, rather than redrawing per tile.
        let marks = [1, total.div_ceil(4), total.div_ceil(2), (total * 3).div_ceil(4)];
        let mut mark = 0;
        let mut publish = |tiles: &HashMap<TileId, RgbaImage>, completed: usize,
                           stale: bool, force: bool| -> Result<(), String> {
            if completed == total || preview_count >= 4 { return Ok(()); }
            if !force && (mark >= marks.len() || completed < marks[mark]
                || last_preview.is_some_and(|at| at.elapsed() < Duration::from_millis(150)))
            { return Ok(()); }
            paint(bounds, width, height, grid, tiles, &mut rgba, cancel)?;
            progress(Progress { bounds, width, height, rgba: &rgba, completed, total, stale_cache: stale })?;
            preview_count += 1;
            last_preview = Some(Instant::now());
            while mark < marks.len() && completed >= marks[mark] { mark += 1; }
            Ok(())
        };
        if !tiles.is_empty() { publish(&tiles, completed, completed < tiles.len(), true)?; }
        if !pending.is_empty() {
            let stop = AtomicBool::new(false);
            let next = AtomicUsize::new(0);
            let (tx, rx) = mpsc::sync_channel(2);
            std::thread::scope(|scope| -> Result<(), String> {
                for _ in 0..pending.len().min(2) {
                    let (pending, root, stop, next, tx) = (&pending, &root, &stop, &next, tx.clone());
                    scope.spawn(move || {
                        while !stop.load(Ordering::Relaxed) && !cancel.load(Ordering::Relaxed) {
                            let index = next.fetch_add(1, Ordering::Relaxed);
                            let Some(&id) = pending.get(index) else { break; };
                            let result = load_tile_for(id, root, cancel, true);
                            if tx.send((id, result)).is_err() { break; }
                        }
                    });
                }
                drop(tx);
                let result = (|| {
                    for _ in 0..pending.len() {
                        let (id, result) = loop {
                            input::check(cancel)?;
                            match rx.recv_timeout(Duration::from_millis(50)) {
                                Ok(value) => break value,
                                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                                Err(_) => return Err("OSM-kaartdownload onderbroken".into()),
                            }
                        };
                        let (tile, stale, _) = result?;
                        stale_cache |= stale;
                        tiles.insert(id, tile);
                        completed += 1;
                        publish(&tiles, completed, stale_cache, false)?;
                    }
                    Ok(())
                })();
                stop.store(true, Ordering::Relaxed);
                // Release blocked result senders before the scoped threads join.
                drop(rx);
                result
            })?;
        }
        drop(publish);
        input::check(cancel)?;
        paint(bounds, width, height, grid, &tiles, &mut rgba, cancel)?;
        self.previous = Some(OsmRaster { bounds, width, height, rgba: rgba.clone(), stale_cache });
        Ok(OsmRaster { bounds, width, height, rgba, stale_cache })
    }
}
