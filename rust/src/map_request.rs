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
    pub fn cadastral_too_far(self) -> bool {
        self.source == Provider::Cadastral
            && self
                .logical_meters_per_pixel
                .unwrap_or((self.bounds.max_x - self.bounds.min_x) / self.size[0].max(1) as f64)
                >= 2.
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
        self.generation = self.generation.wrapping_add(1);
        self.active = None;
        if provider_changed || view.is_none() || view.is_some_and(Viewport::cadastral_too_far) {
            self.cached = None;
        }
        self.deadline = view
            .filter(|v| {
                v.size[0] >= 20
                    && v.size[1] >= 20
                    && !v.cadastral_too_far()
                    && !self
                        .cached
                        .is_some_and(|c| c.covers(*v) || c.key() == v.key())
            })
            .map(|_| now + Duration::from_millis(45));
        Ok(true)
    }
    pub fn due(&mut self, now: Duration) -> Option<Request> {
        if now < self.deadline? {
            return None;
        }
        if self.dragging {
            self.deadline = Some(now + Duration::from_millis(160));
            return None;
        }
        let request = Request {
            generation: self.generation,
            view: self.desired?,
        };
        self.deadline = None;
        self.active = Some(request);
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
        }
        true
    }
    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.desired = None;
        self.cached = None;
        self.active = None;
        self.deadline = None;
    }
    pub fn next_deadline(&self) -> Option<Duration> {
        self.deadline
    }
    pub fn suppressed(&self) -> bool {
        self.desired.is_some_and(Viewport::cadastral_too_far)
    }
}
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
}
impl Worker {
    pub fn new(osm_cache: PathBuf) -> Result<Self, String> {
        Self::with_fetcher(move |request, cancel, preview| {
            let v = request.view;
            match v.source {
                Provider::Osm => {
                    let raster = crate::osm_tiles::request_export_map_with_cancel(
                        v.bounds, v.size[0], v.size[1], &osm_cache, cancel,
                    )?;
                    Ok(Pixels {
                        bounds: raster.bounds,
                        width: raster.width,
                        height: raster.height,
                        rgba: raster.rgba,
                        stale_cache: raster.stale_cache,
                    })
                }
                _ => {
                    let source = if v.source == Provider::Cadastral {
                        crate::pdok_wms::Source::Cadastral
                    } else {
                        crate::pdok_wms::Source::Aerial
                    };
                    let mut update = |p: crate::pdok_wms::Progress<'_>| {
                        crate::input::check(cancel)?;
                        if p.completed < p.total {
                            preview(
                                Pixels {
                                    bounds: v.bounds,
                                    width: v.size[0],
                                    height: v.size[1],
                                    rgba: p.image.as_raw().clone(),
                                    stale_cache: false,
                                },
                                p.completed,
                                p.total,
                            )?;
                        }
                        Ok(())
                    };
                    let image = crate::pdok_wms::request_tiled_with_progress(
                        source,
                        v.bounds,
                        v.size[0],
                        v.size[1],
                        cancel,
                        Some(&mut update),
                    )?;
                    Ok(Pixels {
                        bounds: v.bounds,
                        width: image.width(),
                        height: image.height(),
                        rgba: image.into_raw(),
                        stale_cache: false,
                    })
                }
            }
        })
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
    fn original_five_scheduling_decisions_after_canvas_debounce() {
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
            // The UI adds its 45 ms debounce before the original start decision.
            let started = scheduler.due(Duration::from_millis(45)).is_some();
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
        assert_eq!(s.next_deadline(), Some(Duration::from_millis(265)));
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
}
