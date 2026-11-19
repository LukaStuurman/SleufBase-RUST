//! Original desktop background request scheduling, without an app dependency.
use crate::model::Bounds;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Provider {
    Aerial,
    Cadastral,
    Osm,
    AerialCadastral,
    OsmCadastral,
}
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Viewport {
    pub source: Provider,
    pub bounds: Bounds,
    pub size: [u32; 2],
    #[serde(default)]
    pub logical_meters_per_pixel: Option<f64>,
}
impl Viewport {
    fn validate(self) -> Result<(), String> {
        let b = self.bounds;
        if self
            .logical_meters_per_pixel
            .is_some_and(|m| !m.is_finite() || m <= 0.)
            || self.size.contains(&0)
            || self.size.iter().any(|n| *n > 10_000)
            || u64::from(self.size[0]) * u64::from(self.size[1]) > 50_000_000
            || [b.min_x, b.min_y, b.max_x, b.max_y]
                .iter()
                .any(|v| !v.is_finite())
            || b.min_x >= b.max_x
            || b.min_y >= b.max_y
        {
            return Err("Ongeldige kaartuitsnede".into());
        }
        Ok(())
    }
    fn key(self) -> (Provider, [u64; 4], [u32; 2]) {
        (
            self.source,
            [
                self.bounds.min_x,
                self.bounds.min_y,
                self.bounds.max_x,
                self.bounds.max_y,
            ]
            .map(|v| {
                let v = format!("{v:.2}").parse::<f64>().unwrap();
                if v == 0. {
                    0
                } else {
                    v.to_bits()
                }
            }),
            self.size,
        )
    }
    pub fn covers(self, view: Self) -> bool {
        if self.source != view.source {
            return false;
        }
        let a = self.bounds;
        let b = view.bounds;
        let epsilon = (b.max_x - b.min_x).max(b.max_y - b.min_y).max(1.) * 1e-9;
        a.min_x <= b.min_x + epsilon
            && a.min_y <= b.min_y + epsilon
            && a.max_x >= b.max_x - epsilon
            && a.max_y >= b.max_y - epsilon
            && (a.max_x - a.min_x) / self.size[0].max(1) as f64
                <= (b.max_x - b.min_x) / view.size[0].max(1) as f64 * 1.15
            && (a.max_y - a.min_y) / self.size[1].max(1) as f64
                <= (b.max_y - b.min_y) / view.size[1].max(1) as f64 * 1.15
    }
    /// Stable world-grid requests retain neighbouring pixels and share WMS
    /// tiles across pans. Quarter-octave levels keep detail within 19%.
    pub fn buffered(self) -> Self {
        // OSM already caches reusable world tiles. A WMS overscan buffer both
        // fetches unseen tiles and forces a coarser level under its tile budget.
        if matches!(self.source, Provider::Osm | Provider::OsmCadastral) { return self; }
        let pixel=((self.bounds.max_x-self.bounds.min_x)/self.size[0] as f64)
            .min((self.bounds.max_y-self.bounds.min_y)/self.size[1] as f64);
        let resolution=2_f64.powf((pixel.log2()*4.).floor()/4.);
        if !resolution.is_finite() || resolution<=0. {return self;}
        let tile=512.*resolution;
        let pad=256.*resolution;
        let left=((self.bounds.min_x-pad)/tile).floor();
        let bottom=((self.bounds.min_y-pad)/tile).floor();
        let right=((self.bounds.max_x+pad)/tile).ceil();
        let top=((self.bounds.max_y+pad)/tile).ceil();
        let width=(right-left)*512.; let height=(top-bottom)*512.;
        // Large/high-DPI views retain the existing bounded exact-view path.
        if width>10000. || height>10000. || width*height>50_000_000. {return self;}
        let planned=Self {bounds:Bounds{min_x:left*tile,min_y:bottom*tile,
            max_x:right*tile,max_y:top*tile},size:[width as u32,height as u32],..self};
        if planned.validate().is_ok() && planned.covers(self) {planned} else {self}
    }
    pub fn cadastral_too_far(self) -> bool {
        // WMTS supports several zoom levels. Restrict only country-scale
        // requests, instead of hiding the raster map past 2 m per pixel.
        self.source == Provider::Cadastral
            && self.logical_meters_per_pixel.unwrap_or(
                (self.bounds.max_x - self.bounds.min_x) / self.size[0].max(1) as f64
            ) >= 250.
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    pub generation: u64,
    pub view: Viewport,
}
#[derive(Default)]
pub struct Scheduler {
    desired: Option<Viewport>,
    cached: Option<Viewport>,
    active: Option<Request>,
    deadline: Option<Duration>,
    dragging: bool,
    last_started: Option<Duration>,
    generation: u64,
}
impl Scheduler {
    /// Call each frame with the real canvas view; None means background disabled.
    /// true invalidates the worker's previous generation immediately.
    pub fn update(
        &mut self,
        view: Option<Viewport>,
        dragging: bool,
        now: Duration,
    ) -> Result<bool, String> {
        if let Some(v) = view {
            v.validate()?;
        }
        let same = match (self.desired, view) {
            (Some(a), Some(b)) => {
                a.key() == b.key() && a.cadastral_too_far() == b.cadastral_too_far()
            }
            (None, None) => true,
            _ => false,
        };
        let drag_changed = self.dragging != dragging;
        self.dragging = dragging;
        if same && !drag_changed {
            return Ok(false);
        }
        let provider_changed = self.desired.map(|v| v.source) != view.map(|v| v.source);
        self.desired = view;
        if provider_changed {
            self.last_started = None;
        }
        // A pan inside the active/cached buffer must not cancel its fetch.
        // Drag transitions alone never invalidate a usable world-space image.
        if !provider_changed && view.is_some_and(|v| !v.cadastral_too_far()
            && (self.active.is_some_and(|r| r.view.covers(v))
                || (self.active.is_none() && self.cached.is_some_and(|c| c.covers(v))))) {
            self.deadline=None;
            return Ok(false);
        }
        let deadline = view
            .filter(|v| {
                v.size[0] >= 20
                    && v.size[1] >= 20
                    && !v.cadastral_too_far()
                    && !self.cached.is_some_and(|c| c.covers(*v) || c.key() == v.key())
            })
            .map(|_| {
                let debounce = now + Duration::from_millis(45);
                if dragging && !provider_changed {
                    // Keep the first deadline while the pointer keeps moving.
                    // A fresh pan request starts at most once per 160 ms.
                    let pending = self.deadline.unwrap_or(debounce);
                    self.last_started.map_or(pending, |last| {
                        pending.max(last + Duration::from_millis(160))
                    })
                } else {
                    debounce
                }
            });
        // OSM uses exact viewports, so even a one-pixel pan exposes a new edge.
        // Let useful work finish instead of cancelling it on every mouse move.
        // Only one latest follow-up is retained; no per-frame fetch queue grows.
        if dragging && !provider_changed && deadline.is_some()
            && view.is_some_and(|v| self.active.is_some_and(|r| useful_during_pan(r.view, v)))
        {
            self.deadline = deadline;
            return Ok(false);
        }
        self.generation = self.generation.wrapping_add(1);
        self.active = None;
        if provider_changed || view.is_none() || view.is_some_and(Viewport::cadastral_too_far) {
            self.cached = None;
        }
        self.deadline = deadline;
        Ok(true)
    }
    pub fn due(&mut self, now: Duration) -> Option<Request> {
        if self.active.is_some() || now < self.deadline? {
            return None;
        }
        let request = Request {
            generation: self.generation,
            view: self.desired?.buffered(),
        };
        self.deadline = None;
        self.active = Some(request);
        self.last_started = Some(now);
        Some(request)
    }
    pub fn accepts(&self, request: Request) -> bool {
        self.active == Some(request) && request.generation == self.generation
    }
    /// An incomplete image replaced the old visible texture; its old coverage
    /// metadata must no longer suppress the next camera request.
    pub fn preview(&mut self, request: Request) -> bool {
        if !self.accepts(request) {
            return false;
        }
        self.cached = None;
        true
    }
    pub fn complete(&mut self, request: Request, success: bool) -> bool {
        if !self.accepts(request) {
            return false;
        }
        self.active = None;
        if success {
            self.cached = Some(request.view);
            if self.desired.is_some_and(|v| request.view.covers(v)) {
                self.deadline = None;
            }
        }
        true
    }
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.desired = None;
        self.cached = None;
        self.active = None;
        self.deadline = None;
        self.last_started = None;
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        // Active work already asks the UI to repaint. An elapsed follow-up
        // deadline must not create a 1 ms repaint loop while it finishes.
        if self.active.is_some() { None } else { self.deadline }
    }
    pub fn suppressed(&self) -> bool {
        self.desired.is_some_and(Viewport::cadastral_too_far)
    }
}
fn useful_during_pan(active: Viewport, desired: Viewport) -> bool {
    let a = active.bounds;
    let b = desired.bounds;
    if active.source != desired.source || desired.cadastral_too_far()
        || a.max_x <= b.min_x || b.max_x <= a.min_x
        || a.max_y <= b.min_y || b.max_y <= a.min_y
    {
        return false;
    }
    // Retain same-scale pans, but promptly replace work after a zoom change.
    let x = ((a.max_x - a.min_x) / f64::from(active.size[0]))
        / ((b.max_x - b.min_x) / f64::from(desired.size[0]));
    let y = ((a.max_y - a.min_y) / f64::from(active.size[1]))
        / ((b.max_y - b.min_y) / f64::from(desired.size[1]));
    (0.7..=1.15).contains(&x) && (0.7..=1.15).contains(&y)
}
#[derive(Clone)]
pub struct Pixels {
    pub bounds: Bounds,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub stale_cache: bool,
}
pub enum Event {
    Preview {
        request: Request,
        pixels: Pixels,
        completed: usize,
        total: usize,
    },
    Complete {
        request: Request,
        result: Result<Pixels, String>,
    },
}
struct Task {
    epoch: u64,
    request: Request,
    cancel: Arc<AtomicBool>,
}
type Preview = Option<(u64, Request, Pixels, usize, usize)>;
/// One worker/queued task/latest pending request and one replaceable preview.
/// Pan/zoom/provider changes cancel obsolete work without accumulating threads.
pub struct Worker {
    tx: mpsc::SyncSender<Task>,
    rx: mpsc::Receiver<(u64, Request, Result<Pixels, String>)>,
    preview: Arc<Mutex<Preview>>,
    cancel: Arc<AtomicBool>,
    epoch: u64,
    pending: Option<Task>,
    busy: bool,
    progressive_pdok: bool,
}
// A block-by-block PDOK preview, with previously visible map pixels
// reprojected underneath missing tiles. This avoids blank/checkerboard
// flashing while panning or zooming and never changes the final raster.
struct ProgressivePdok {
    view: Viewport,
    rgba: Vec<u8>,
    dirty: Vec<[u32;4]>,
    previews: usize,
    last_preview: Option<std::time::Instant>,
}
impl ProgressivePdok {
    fn new(view:Viewport,previous:Option<&Pixels>,cancel:&AtomicBool)->Result<Self,String>{
        let (w,h)=(view.size[0] as usize,view.size[1] as usize);
        let bytes=w.checked_mul(h).and_then(|p|p.checked_mul(4))
            .ok_or("Kaartvoorbeeld is te groot")?;
        let mut rgba=vec![255u8;bytes];
        if let Some(old)=previous.filter(|old|
            old.width>0 && old.height>0 && old.rgba.len()==old.width as usize*old.height as usize*4)
        {
            let sx=f64::from(old.width)/(old.bounds.max_x-old.bounds.min_x);
            let sy=f64::from(old.height)/(old.bounds.max_y-old.bounds.min_y);
            let dx=(view.bounds.max_x-view.bounds.min_x)/f64::from(view.size[0]);
            let dy=(view.bounds.max_y-view.bounds.min_y)/f64::from(view.size[1]);
            for y in 0..view.size[1] {
                crate::input::check(cancel)?;
                let wy=view.bounds.max_y-(f64::from(y)+0.5)*dy;
                let oy=((old.bounds.max_y-wy)*sy).floor();
                if oy<0. || oy>=f64::from(old.height) {continue;}
                for x in 0..view.size[0] {
                    let wx=view.bounds.min_x+(f64::from(x)+0.5)*dx;
                    let ox=((wx-old.bounds.min_x)*sx).floor();
                    if ox<0. || ox>=f64::from(old.width) {continue;}
                    let src=(oy as usize*old.width as usize+ox as usize)*4;
                    let dst=(y as usize*w+x as usize)*4;
                    rgba[dst..dst+4].copy_from_slice(&old.rgba[src..src+4]);
                }
            }
        }
        Ok(Self{view,rgba,dirty:Vec::new(),previews:0,last_preview:None})
    }
    fn progress(
        &mut self,
        progress:crate::pdok_wms::Progress<'_>,
        cancel:&AtomicBool,
        publish:&mut dyn FnMut(Pixels,usize,usize)->Result<(),String>,
    )->Result<(),String>{
        crate::input::check(cancel)?;
        self.dirty.push(progress.tile);
        if progress.total<=1 || progress.completed>=progress.total {return Ok(());}
        let pixels=u64::from(self.view.size[0])*u64::from(self.view.size[1]);
        let limit=if pixels>4_000_000{2}else{4};
        let threshold=match self.previews {
            0=>1,
            1=>progress.total.div_ceil(4),
            2=>progress.total.div_ceil(2),
            _=>(3*progress.total).div_ceil(4),
        };
        if self.previews>=limit || progress.completed<threshold
            || self.last_preview.is_some_and(|at|at.elapsed()<Duration::from_millis(100))
        {return Ok(());}
        let width=self.view.size[0] as usize;
        let source=progress.image.as_raw();
        for [left,top,w,h] in self.dirty.drain(..) {
            if left.saturating_add(w)>self.view.size[0]||top.saturating_add(h)>self.view.size[1]{
                return Err("PDOK-tegel valt buiten de kaart".into());
            }
            for y in top..top+h {
                let offset=(y as usize*width+left as usize)*4;
                let len=w as usize*4;
                self.rgba[offset..offset+len].copy_from_slice(&source[offset..offset+len]);
            }
        }
        publish(Pixels{
            bounds:self.view.bounds,width:self.view.size[0],height:self.view.size[1],
            rgba:self.rgba.clone(),stale_cache:false,
        },progress.completed,progress.total)?;
        self.previews+=1;
        self.last_preview=Some(std::time::Instant::now());
        Ok(())
    }
}

// Interactive aerials use the official PDOK WMTS pyramid (cacheable 256px
// zoom-level tiles). Keep the exact BBOX WMS renderer as a safety fallback.
fn aerial_wmts(
    renderer:&mut crate::pdok_aerial_wmts::ViewportRenderer,
    view:Viewport,cancel:&AtomicBool,
    publish:&mut dyn FnMut(Pixels,usize,usize)->Result<(),String>,
)->Result<Pixels,String>{
    let cache=crate::project::app_data_directory().join("pdok-aerial-wmts");
    let mut update=|p:crate::pdok_aerial_wmts::Progress<'_>|{
        publish(Pixels{
            bounds:p.bounds,width:p.width,height:p.height,
            rgba:p.rgba.to_vec(),stale_cache:p.stale_cache,
        },p.completed,p.total)
    };
    let raster=renderer.request(view.bounds,view.size[0],view.size[1],
        &cache,cancel,&mut update)?;
    Ok(Pixels{bounds:raster.bounds,width:raster.width,height:raster.height,
        rgba:raster.rgba,stale_cache:raster.stale_cache})
}
fn aerial_wms_fallback(
    view:Viewport,previous:Option<&Pixels>,cancel:&AtomicBool,
    publish:&mut dyn FnMut(Pixels,usize,usize)->Result<(),String>,
)->Result<Pixels,String>{
    let mut buffer=ProgressivePdok::new(view,previous,cancel)?;
    let mut update=|p:crate::pdok_wms::Progress<'_>|{
        buffer.progress(p,cancel,publish)
    };
    let image=crate::pdok_wms::request_tiled_with_progress(
        crate::pdok_wms::Source::Aerial,view.bounds,view.size[0],view.size[1],
        cancel,Some(&mut update))?;
    Ok(Pixels{bounds:view.bounds,width:image.width(),height:image.height(),
        rgba:image.into_raw(),stale_cache:false})
}
// Preparing the parcel overlay in parallel with the aerial/OSM base avoids
// waiting for TWO sequential network phases on ordinary screens. The PDOK
// client still enforces its existing global four-request limit. Very large
// canvases use the old sequential path to avoid doubling peak RGBA memory.
const PARALLEL_PARCEL_MAX_PIXELS: u64 = 12_000_000;
fn parallel_parcel_overlay(view: Viewport) -> bool {
    u64::from(view.size[0]) * u64::from(view.size[1]) <= PARALLEL_PARCEL_MAX_PIXELS
}
fn parcel_overlay(view: Viewport, cancel: &AtomicBool) -> Result<image::RgbaImage, String> {
    let mut check = |_: crate::pdok_wms::Progress<'_>| crate::input::check(cancel);
    crate::pdok_wms::request_tiled_with_progress(
        crate::pdok_wms::Source::CadastralOverlay,
        view.bounds, view.size[0], view.size[1], cancel, Some(&mut check))
}

impl Worker {
    pub fn new(osm_cache: PathBuf) -> Result<Self, String> {
        let mut osm = crate::osm_tiles::ViewportRenderer::default();
        let mut aerial = crate::pdok_aerial_wmts::ViewportRenderer::default();
        let mut wmts:Option<crate::cadastral_tiles::Client> = None;
        let mut last_pdok:Option<(Provider, Pixels)> = None;
        let mut worker=Self::with_fetcher(move |request, cancel, preview| {
            let v = request.view;
            let result=match v.source {
                Provider::Osm | Provider::OsmCadastral => {
                    std::thread::scope(|scope| -> Result<Pixels, String> {
                        // Start collecting cadastral linework while OSM tiles
                        // are fetched and progressively painted. Both sources
                        // use their own existing network and cache limits.
                        let is_combined = v.source == Provider::OsmCadastral;
                        let overlay = (is_combined && parallel_parcel_overlay(v))
                            .then(|| scope.spawn(|| parcel_overlay(v, cancel)));
                        let mut update = |p: crate::osm_tiles::ViewportProgress<'_>| {
                            preview(Pixels {
                                bounds: p.bounds,
                                width: p.width,
                                height: p.height,
                                rgba: p.rgba.to_vec(),
                                stale_cache: p.stale_cache,
                            }, p.completed, p.total)
                        };
                        let raster = osm.request(
                            v.bounds, v.size[0], v.size[1], &osm_cache, cancel, &mut update,
                        )?;
                        if is_combined {
                            // The complete base stays visible while waiting
                            // for parcels; no partial transparent overlay is
                            // ever placed on top of the road map.
                            preview(Pixels {
                                bounds: raster.bounds,
                                width: raster.width,
                                height: raster.height,
                                rgba: raster.rgba.clone(),
                                stale_cache: raster.stale_cache,
                            }, 1, 1)?;
                            let mut image = image::RgbaImage::from_raw(
                                raster.width, raster.height, raster.rgba,
                            ).ok_or("OpenStreetMap gaf ongeldige pixels terug")?;
                            let layer = if let Some(job) = overlay {
                                job.join().map_err(|_| "Perceelwerker is gestopt")??
                            } else { parcel_overlay(v, cancel)? };
                            crate::pdok_wms::composite_rgba(&mut image, &layer, cancel)?;
                            Ok(Pixels {
                                bounds: raster.bounds,
                                width: image.width(),
                                height: image.height(),
                                rgba: image.into_raw(),
                                stale_cache: raster.stale_cache,
                            })
                        } else {
                            Ok(Pixels {
                                bounds: raster.bounds,
                                width: raster.width,
                                height: raster.height,
                                rgba: raster.rgba,
                                stale_cache: raster.stale_cache,
                            })
                        }
                    })
                }
                Provider::AerialCadastral => {
                    std::thread::scope(|scope| -> Result<Pixels, String> {
                        let overlay = parallel_parcel_overlay(v)
                            .then(|| scope.spawn(|| parcel_overlay(v, cancel)));
                        let old=last_pdok.as_ref().filter(|(kind,_)|*kind==v.source)
                            .map(|(_,image)|image);
                        let base=aerial_wmts(&mut aerial,v,cancel,preview)
                            .or_else(|_|aerial_wms_fallback(v,old,cancel,preview))?;
                        // Publish a complete first aerial base before the
                        // vector/raster parcel overlay has finished.
                        preview(base.clone(), 1, 2)?;
                        let mut image=image::RgbaImage::from_raw(
                            base.width,base.height,base.rgba)
                            .ok_or("PDOK-luchtfoto gaf ongeldige pixels terug")?;
                        let layer = if let Some(job) = overlay {
                            job.join().map_err(|_| "Perceelwerker is gestopt")??
                        } else { parcel_overlay(v, cancel)? };
                        crate::pdok_wms::composite_rgba(&mut image, &layer, cancel)?;
                        Ok(Pixels {
                            bounds: v.bounds,
                            width: image.width(),
                            height: image.height(),
                            rgba: image.into_raw(),
                            stale_cache: base.stale_cache,
                        })
                    })
                }
                Provider::Cadastral => {
                    // At broad zooms use the existing multiscale Kadaster WMTS
                    // tile client instead of enlarging a single WMS PNG.
                    // Close-up backgrounds switch to RD vector shapes in UI.
                    if wmts.is_none() {
                        wmts=Some(crate::cadastral_tiles::Client::new(
                            crate::project::app_data_directory().join("cadastral-wmts"))?);
                    }
                    let image=wmts.as_ref().unwrap().request(v.bounds,v.size,cancel)?;
                    Ok(Pixels{
                        bounds:v.bounds,width:image.width(),height:image.height(),
                        rgba:image.into_raw(),stale_cache:false,
                    })
                }
                Provider::Aerial => {
                    let old=last_pdok.as_ref().filter(|(kind,_)|*kind==v.source)
                        .map(|(_,image)|image);
                    aerial_wmts(&mut aerial,v,cancel,preview)
                        .or_else(|_|aerial_wms_fallback(v,old,cancel,preview))
                }
            };
            if matches!(v.source,Provider::Aerial|Provider::AerialCadastral)
                && u64::from(v.size[0])*u64::from(v.size[1])<=12_000_000
            {
                if let Ok(image)=&result {
                    last_pdok=Some((v.source,image.clone()));
                }
            }
            result
        })?;
        worker.progressive_pdok=true;
        Ok(worker)
    }
    pub fn with_fetcher(
        mut fetch: impl FnMut(
                Request,
                &AtomicBool,
                &mut dyn FnMut(Pixels, usize, usize) -> Result<(), String>,
            ) -> Result<Pixels, String>
            + Send
            + 'static,
    ) -> Result<Self, String> {
        let (tx, input) = mpsc::sync_channel::<Task>(1);
        let (output, rx) = mpsc::sync_channel(1);
        let preview: Arc<Mutex<Preview>> = Arc::new(Mutex::new(None));
        let slot = preview.clone();
        std::thread::Builder::new()
            .name("sleufbase-map-view".into())
            .spawn(move || {
                while let Ok(task) = input.recv() {
                    let mut publish = |pixels, completed, total| {
                        crate::input::check(&task.cancel)?;
                        let mut latest =
                            slot.lock().map_err(|_| "Kaartvoorbeeld niet beschikbaar")?;
                        crate::input::check(&task.cancel)?;
                        *latest = Some((task.epoch, task.request, pixels, completed, total));
                        Ok(())
                    };
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        crate::input::check(&task.cancel)?;
                        fetch(task.request, &task.cancel, &mut publish)
                    }))
                    .unwrap_or_else(|_| Err("Kaarttaak is onverwacht gestopt".into()));
                    let mut message = (task.epoch, task.request, result);
                    loop {
                        if task.cancel.load(Ordering::Relaxed) {
                            break;
                        }
                        match output.try_send(message) {
                            Ok(()) => break,
                            Err(mpsc::TrySendError::Full(value)) => {
                                message = value;
                                std::thread::sleep(Duration::from_millis(5));
                            }
                            Err(mpsc::TrySendError::Disconnected(_)) => return,
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tx,
            rx,
            preview,
            cancel: Arc::new(AtomicBool::new(false)),
            epoch: 0,
            pending: None,
            busy: false,
            progressive_pdok: false,
        })
    }
    pub fn start(&mut self, request: Request) {
        self.cancel();
        self.cancel = Arc::new(AtomicBool::new(false));
        self.pending = Some(Task {
            epoch: self.epoch,
            request,
            cancel: self.cancel.clone(),
        });
        self.busy = true;
        self.pump();
    }
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.pending = None;
        self.busy = false;
        self.epoch = self.epoch.wrapping_add(1);
        if let Ok(mut p) = self.preview.lock() {
            *p = None;
        }
    }
    fn pump(&mut self) {
        if let Some(task) = self.pending.take() {
            match self.tx.try_send(task) {
                Ok(()) => {}
                Err(mpsc::TrySendError::Full(task)) => self.pending = Some(task),
                Err(mpsc::TrySendError::Disconnected(_)) => self.busy = false,
            }
        }
    }
    pub fn poll(&mut self) -> Option<Event> {
        let mut complete = None;
        while let Ok((epoch, request, result)) = self.rx.try_recv() {
            if epoch == self.epoch {
                self.busy = false;
                complete = Some(Event::Complete { request, result });
                break;
            }
        }
        self.pump();
        if complete.is_some() {
            if let Ok(mut p) = self.preview.lock() {
                *p = None;
            }
            return complete;
        }
        self.preview
            .lock()
            .ok()
            .and_then(|mut p| p.take())
            .and_then(|(epoch, request, pixels, completed, total)| {
                (epoch == self.epoch).then_some(Event::Preview {
                    request,
                    pixels,
                    completed,
                    total,
                })
            })
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    /// True only for the real renderer: all partial PDOK frames are composed
    /// over its last visible picture, unlike synthetic preview test workers.
    pub fn progressive_pdok(&self) -> bool { self.progressive_pdok }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn view() -> Viewport {
        Viewport {
            source: Provider::Aerial,
            bounds: Bounds {
                min_x: 0.,
                min_y: 0.,
                max_x: 100.,
                max_y: 80.,
            },
            size: [100, 80],
            logical_meters_per_pixel: None,
        }
    }
    #[test]
    fn original_coverage_resolution_oracle() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/map-request/reference.json"))
                .unwrap();
        for row in data["coverage"].as_array().unwrap() {
            let cache: Viewport = serde_json::from_value(row["cached"].clone()).unwrap();
            let current: Viewport = serde_json::from_value(row["view"].clone()).unwrap();
            assert_eq!(
                cache.covers(current),
                row["expected"].as_bool().unwrap(),
                "{row}"
            );
        }
    }
    #[test]
    fn baseline_scheduling_decisions_now_allow_loading_during_drag() {
        let data: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/map-request/reference.json"))
                .unwrap();
        let rows = data["scheduling"].as_array().unwrap();
        assert_eq!(rows.len(), 5);
        for row in rows {
            let mut v = view();
            v.size = serde_json::from_value(row["size"].clone()).unwrap();
            if row["notice"].as_bool().unwrap() {
                v.source = Provider::Cadastral;
                v.logical_meters_per_pixel = Some(2.);
            }
            let mut scheduler = Scheduler::default();
            if row["cached"].as_bool().unwrap() {
                let mut previous = v;
                previous.size = [1, 1];
                scheduler.desired = Some(previous);
                scheduler.cached = Some(v);
            }
            scheduler
                .update(Some(v), row["drag"].as_bool().unwrap(), Duration::ZERO)
                .unwrap();
            // Preserve the original disabled/cache/size decisions, but a drag
            // now starts loading before the mouse button is released.
            let started = scheduler.due(Duration::from_millis(45)).is_some();
            if row["drag"].as_bool().unwrap() {
                assert!(started, "{row}");
                assert!(scheduler.next_deadline().is_none());
                continue;
            }
            assert_eq!(
                usize::from(started),
                row["started"].as_u64().unwrap() as usize,
                "{row}"
            );
            let timers: Vec<u64> = serde_json::from_value(row["timers"].clone()).unwrap();
            assert_eq!(
                scheduler.next_deadline().map(|d| d.as_millis() as u64 - 45),
                timers.first().copied(),
                "{row}"
            );
        }
    }
    #[test]
    fn camera_debounce_drag_zoom_resize_and_provider_stale_gates() {
        let mut s = Scheduler::default();
        let base = view();
        assert!(s.update(Some(base), false, Duration::ZERO).unwrap());
        assert!(s.due(Duration::from_millis(44)).is_none());
        let first = s.due(Duration::from_millis(45)).unwrap();
        assert!(s.complete(first, true));
        assert!(!s
            .update(Some(base), false, Duration::from_millis(50))
            .unwrap());
        let mut zoom = base;
        zoom.size = [200, 160];
        assert!(s
            .update(Some(zoom), true, Duration::from_millis(60))
            .unwrap());
        assert!(s.due(Duration::from_millis(105)).is_none());
        assert_eq!(s.next_deadline(), Some(Duration::from_millis(205)));
        s.update(Some(zoom), false, Duration::from_millis(110))
            .unwrap();
        let second = s.due(Duration::from_millis(155)).unwrap();
        assert!(!s.accepts(first));
        assert!(s.accepts(second));
        let mut provider = zoom;
        provider.source = Provider::Osm;
        s.update(Some(provider), false, Duration::from_millis(160))
            .unwrap();
        assert!(!s.accepts(second));
        let third = s.due(Duration::from_millis(205)).unwrap();
        s.invalidate();
        assert!(!s.complete(third, true));
        let mut cadastral = view();
        cadastral.source = Provider::Cadastral;
        cadastral.size = [200, 160];
        cadastral.logical_meters_per_pixel = Some(2.);
        s.update(Some(cadastral), false, Duration::ZERO).unwrap();
        assert!(s.suppressed());
        assert!(s.due(Duration::from_secs(1)).is_none());
    }
    #[test]
    fn identical_physical_view_threshold_transition_invalidates_suppressed_cache() {
        let mut v = view();
        v.source = Provider::Cadastral;
        v.logical_meters_per_pixel = Some(1.9);
        let mut s = Scheduler::default();
        s.update(Some(v), false, Duration::ZERO).unwrap();
        let first = s.due(Duration::from_millis(45)).unwrap();
        assert!(s.complete(first, true));
        v.logical_meters_per_pixel = Some(2.);
        assert!(s.update(Some(v), false, Duration::from_millis(50)).unwrap());
        assert!(s.suppressed());
        assert!(s.due(Duration::from_secs(1)).is_none());
        v.logical_meters_per_pixel = Some(1.9);
        assert!(s.update(Some(v), false, Duration::from_secs(2)).unwrap());
        assert!(s.due(Duration::from_millis(2045)).is_some());
        v.logical_meters_per_pixel = Some(f64::NAN);
        assert!(s.update(Some(v), false, Duration::ZERO).is_err());
    }
    #[test]
    fn latest_worker_replaces_preview_and_cancels_stale_project() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut worker = Worker::with_fetcher(move |request, cancel, publish| {
            if request.generation == 1 {
                for value in 0..20 {
                    publish(
                        Pixels {
                            bounds: request.view.bounds,
                            width: 1,
                            height: 1,
                            rgba: vec![value, 0, 0, 255],
                            stale_cache: false,
                        },
                        value as usize,
                        20,
                    )?;
                }
                tx.send(()).unwrap();
                while !cancel.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(1));
                }
                return Err("cancelled".into());
            }
            Ok(Pixels {
                bounds: request.view.bounds,
                width: 1,
                height: 1,
                rgba: vec![2, 0, 0, 255],
                stale_cache: false,
            })
        })
        .unwrap();
        let first = Request {
            generation: 1,
            view: view(),
        };
        worker.start(first);
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        match worker.poll().unwrap() {
            Event::Preview { pixels, .. } => assert_eq!(pixels.rgba[0], 19),
            _ => panic!("preview expected"),
        };
        worker.start(Request {
            generation: 2,
            view: view(),
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(event) = worker.poll() {
                match event {
                    Event::Complete { request, result } => {
                        assert_eq!(request.generation, 2);
                        assert_eq!(result.unwrap().rgba[0], 2);
                        break;
                    }
                    _ => panic!("stale preview escaped"),
                }
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!worker.busy());
    }
    #[test]
    fn buffered_pan_keeps_active_work_and_completed_cache_while_zoom_and_sources_refresh() {
        let mut scheduler=Scheduler::default();let base=view();
        scheduler.update(Some(base),false,Duration::ZERO).unwrap();
        let request=scheduler.due(Duration::from_millis(45)).unwrap();
        assert!(request.view.covers(base));
        let mut moved=base;moved.bounds.min_x+=100.;moved.bounds.max_x+=100.;
        assert!(!scheduler.update(Some(moved),true,Duration::from_millis(60)).unwrap());
        assert!(scheduler.accepts(request));
        assert!(scheduler.complete(request,true));
        assert!(!scheduler.update(Some(moved),false,Duration::from_millis(70)).unwrap());
        assert!(scheduler.due(Duration::from_secs(1)).is_none());
        let mut zoom=moved;zoom.size=[400,320];
        assert!(scheduler.update(Some(zoom),false,Duration::from_secs(2)).unwrap());
        let sharp=scheduler.due(Duration::from_millis(2045)).unwrap();
        assert!(sharp.view.covers(zoom));
        zoom.source=Provider::Osm;
        assert!(scheduler.update(Some(zoom),false,Duration::from_secs(3)).unwrap());
        assert!(!scheduler.accepts(sharp));
    }
    #[test]
    fn world_grid_tiles_are_stable_across_pans_and_requests_remain_bounded() {
        let base=Viewport{bounds:Bounds{min_x:155000.,min_y:463000.,max_x:156024.,max_y:463512.},
            size:[1024,512],..view()};
        let a=base.buffered();let mut next=base;
        next.bounds.min_x+=512.;next.bounds.max_x+=512.;
        let b=next.buffered();
        assert_eq!(a.size,b.size);
        assert_eq!(b.bounds.min_x-a.bounds.min_x,512.);
        assert!(a.covers(base)&&b.covers(next));
        let huge=Viewport{size:[10000,5000],..base};
        huge.buffered().validate().unwrap();
        assert!(huge.buffered().covers(huge));
    }

    #[test]
    fn continuous_drag_keeps_the_first_deadline_and_starts_the_latest_view() {
        for source in [Provider::Aerial, Provider::Osm] {
            let mut scheduler = Scheduler::default();
            let mut latest = Viewport { source, ..view() };
            scheduler.update(Some(latest), true, Duration::ZERO).unwrap();
            for millis in (5..=40).step_by(5) {
                latest.bounds.min_x += 2.;
                latest.bounds.max_x += 2.;
                scheduler.update(Some(latest), true, Duration::from_millis(millis)).unwrap();
                assert_eq!(scheduler.next_deadline(), Some(Duration::from_millis(45)));
                assert!(scheduler.due(Duration::from_millis(millis)).is_none());
            }
            let request = scheduler.due(Duration::from_millis(45)).unwrap();
            assert!(request.view.covers(latest));
            assert!(scheduler.dragging, "loading must not require pointer release");
        }
    }

    #[test]
    fn dragging_allows_useful_tiles_to_finish_then_loads_the_latest_edge() {
        let mut scheduler = Scheduler::default();
        let mut latest = Viewport { source: Provider::Osm, ..view() };
        scheduler.update(Some(latest), true, Duration::ZERO).unwrap();
        let first = scheduler.due(Duration::from_millis(45)).unwrap();
        for millis in (60..=100).step_by(10) {
            latest.bounds.min_x += 5.;
            latest.bounds.max_x += 5.;
            assert!(!scheduler.update(Some(latest), true, Duration::from_millis(millis)).unwrap());
            assert!(scheduler.accepts(first), "a pan must not starve the active tile fetch");
            assert!(scheduler.due(Duration::from_millis(millis)).is_none());
            assert!(scheduler.next_deadline().is_none(), "active work owns its repaint cadence");
        }
        assert!(scheduler.preview(first));
        assert!(scheduler.complete(first, true));
        assert!(scheduler.due(Duration::from_millis(204)).is_none());
        let second = scheduler.due(Duration::from_millis(205)).unwrap();
        assert_eq!(second.view, latest);
        assert!(!scheduler.complete(first, true), "late completion cannot replace the new request");
        assert!(scheduler.complete(second, true));
        assert!(scheduler.due(Duration::from_secs(1)).is_none());
    }

    #[test]
    fn dragging_replaces_obsolete_extents_without_starting_a_request_per_frame() {
        let mut scheduler = Scheduler::default();
        let mut latest = Viewport { source: Provider::Osm, ..view() };
        scheduler.update(Some(latest), true, Duration::ZERO).unwrap();
        let first = scheduler.due(Duration::from_millis(45)).unwrap();
        for millis in (60..=200).step_by(20) {
            latest.bounds.min_x += 200.;
            latest.bounds.max_x += 200.;
            assert!(scheduler.update(Some(latest), true, Duration::from_millis(millis)).unwrap());
            assert!(!scheduler.accepts(first));
            assert!(scheduler.due(Duration::from_millis(millis)).is_none());
            assert_eq!(scheduler.next_deadline(), Some(Duration::from_millis(205)));
        }
        let current = scheduler.due(Duration::from_millis(205)).unwrap();
        assert_eq!(current.view, latest);
        latest.source = Provider::Aerial;
        assert!(scheduler.update(Some(latest), true, Duration::from_millis(210)).unwrap());
        assert!(!scheduler.accepts(current));
        assert!(scheduler.due(Duration::from_millis(254)).is_none());
        assert_eq!(scheduler.due(Duration::from_millis(255)).unwrap().view.source, Provider::Aerial);
        scheduler.invalidate();
        assert!(scheduler.due(Duration::from_secs(1)).is_none());
    }

}
