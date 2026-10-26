//! Resume only original auto/pending heights after project open or import.
//! Requests carry small point snapshots; project geometry is never cloned into a worker.
use crate::{
    model::Vertex,
    virtual_map::{self, MapPoint},
    virtual_trench::VirtualTrench,
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
#[derive(Clone, Debug)]
pub struct Request {
    pub generation: u64,
    pub index: usize,
    pub target: MapPoint,
    pub position: Vertex,
    pub metadata: Value,
    source: Arc<Source>,
    identity: Option<(String, String, String, [String; 3])>,
}
#[derive(Clone, Debug, PartialEq)]
struct Source {
    label: String,
    path: Option<PathBuf>,
    job_id: u64,
    axis: [[f64; 2]; 2],
}
fn source(t: &VirtualTrench) -> Source {
    Source {
        label: t.label.clone(),
        path: t.source_path.clone(),
        job_id: t.job_id,
        axis: [[t.start.x, t.start.y], [t.end.x, t.end.y]],
    }
}
impl Source {
    fn matches(&self, t: &VirtualTrench) -> bool {
        self.label == t.label
            && self.path == t.source_path
            && self.job_id == t.job_id
            && self.axis == [[t.start.x, t.start.y], [t.end.x, t.end.y]]
    }
}
fn metadata(t: &VirtualTrench, target: MapPoint) -> Option<&Value> {
    match target {
        MapPoint::Start => Some(&t.start_metadata),
        MapPoint::End => Some(&t.end_metadata),
        MapPoint::Object(i) => t.objects.get(i).map(|o| &o.metadata),
    }
}
fn identity(t: &VirtualTrench, target: MapPoint) -> Option<(String, String, String, [String; 3])> {
    if let MapPoint::Object(i) = target {
        t.objects.get(i).map(|o| {
            (
                o.role.clone(),
                o.object_name.clone(),
                o.source_name.clone(),
                o.attributes.clone(),
            )
        })
    } else {
        None
    }
}
fn pending(meta: &Value) -> bool {
    virtual_map::point_surface_mode(meta) == "auto"
        && crate::python_values::string(
            meta.get("surface_status")
                .filter(|v| crate::python_values::truth(v)),
        )
        .unwrap_or_default()
        .to_lowercase()
            == "pending"
}
pub fn plan(trenches: &[VirtualTrench], generation: u64) -> Vec<Request> {
    trenches
        .iter()
        .enumerate()
        .filter(|(_, t)| !crate::virtual_trench_editor::is_measured(t))
        .flat_map(|(index, t)| {
            let shared_source = Arc::new(source(t));
            [MapPoint::Start, MapPoint::End]
                .into_iter()
                .chain((0..t.objects.len()).map(MapPoint::Object))
                .filter_map(move |target| {
                    let meta = metadata(t, target)?;
                    let position = virtual_map::position(t, target)?;
                    (pending(meta) && position.x.is_finite() && position.y.is_finite()).then(|| {
                        Request {
                            generation,
                            index,
                            target,
                            position,
                            metadata: meta.clone(),
                            source: Arc::clone(&shared_source),
                            identity: identity(t, target),
                        }
                    })
                })
        })
        .collect()
}
pub struct Response {
    pub request: Request,
    pub height: Option<f64>,
    pub error: Option<String>,
}
/// Network errors become unavailable just as the original background queue did;
/// cancellation returns no partial batch for application to a new project.
pub fn resolve_with(
    requests: Vec<Request>,
    cancel: &AtomicBool,
    mut fetch: impl FnMut(f64, f64, &AtomicBool) -> Result<Option<f64>, String>,
) -> Result<Vec<Response>, String> {
    let mut responses = Vec::with_capacity(requests.len());
    for request in requests {
        if cancel.load(Ordering::Relaxed) {
            return Err("AHN-aanvulling geannuleerd".into());
        }
        let result = fetch(request.position.x, request.position.y, cancel);
        let (height, error) = match result {
            Ok(height) => (height, None),
            Err(e) => (None, Some(e)),
        };
        responses.push(Response {
            request,
            height,
            error,
        });
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("AHN-aanvulling geannuleerd".into());
    }
    Ok(responses)
}
pub fn resolve(requests: Vec<Request>, cancel: &AtomicBool) -> Result<Vec<Response>, String> {
    let client = crate::ahn::AhnClient::new()?;
    resolve_with(requests, cancel, |x, y, c| client.ground_level(x, y, c))
}
/// Exact source/point/generation matching rejects replies after movement, manual
/// edits, deletion, reordering or project replacement. Each accepted mutation is
/// validated before publication and respects the original metadata budget.
pub fn apply(
    trenches: &mut [VirtualTrench],
    generation: u64,
    response: Response,
) -> Result<bool, String> {
    let r = response.request;
    if r.generation != generation {
        return Ok(false);
    }
    let Some(t) = trenches.get_mut(r.index) else {
        return Ok(false);
    };
    if crate::virtual_trench_editor::is_measured(t)
        || !r.source.matches(t)
        || identity(t, r.target) != r.identity
        || virtual_map::position(t, r.target) != Some(r.position)
        || metadata(t, r.target) != Some(&r.metadata)
        || !pending(&r.metadata)
    {
        return Ok(false);
    }
    if !matches!(r.target, MapPoint::Object(_)) {
        return virtual_map::apply_surface_height(t, r.target, response.height);
    }
    let MapPoint::Object(index) = r.target else {
        unreachable!()
    };
    if response
        .height
        .is_some_and(|v| !v.is_finite() || v.abs() > 1e6)
    {
        return Err("Ongeldige AHN-hoogte".into());
    }
    crate::virtual_trench_editor::check(t)?;
    let object = &t.objects[index];
    let depth = virtual_map::point_depth(&object.metadata, object.position.z);
    let mut meta = object.metadata.as_object().cloned().unwrap_or_default();
    meta.insert(
        "surface_status".into(),
        json!(if response.height.is_some() {
            "resolved"
        } else {
            "unavailable"
        }),
    );
    if let Some(height) = response.height {
        meta.insert("surface_z".into(), json!(height));
    }
    let next_meta = Value::Object(meta);
    let next_z = response.height.map(|h| h - depth).or(object.position.z);
    let old_bytes = crate::raster_points::validate_metadata(&object.metadata)?;
    let new_bytes = crate::raster_points::validate_metadata(&next_meta)?;
    if t.point_metadata_bytes()?
        .saturating_sub(old_bytes)
        .saturating_add(new_bytes)
        > 2 * 1024 * 1024
    {
        return Err("AHN-puntmetadata overschrijdt het laagbudget".into());
    }
    if next_z.is_some_and(|v| !v.is_finite() || v.abs() > 1e6) {
        return Err("Ongeldige AHN-objecthoogte".into());
    }
    let changed = next_z != object.position.z || next_meta != object.metadata;
    t.objects[index].position.z = next_z;
    t.objects[index].metadata = next_meta;
    Ok(changed)
}

struct Task {
    revision: u64,
    requests: Vec<Request>,
    cancel: std::sync::Arc<AtomicBool>,
}
/// One long-lived bounded worker. Replacing a project cancels its work and
/// retains at most one latest unsent batch; late generations never escape poll.
pub struct Worker {
    tx: std::sync::mpsc::SyncSender<Task>,
    rx: std::sync::mpsc::Receiver<(u64, bool, Result<Vec<Response>, String>)>,
    revision: u64,
    cancel: std::sync::Arc<AtomicBool>,
    pending: Option<Task>,
    busy: bool,
}
impl Worker {
    pub fn new() -> Result<Self, String> {
        let mut client = None;
        Self::with_streaming_fetcher(move |x, y, cancel| {
            if client.is_none() {
                client = Some(crate::ahn::AhnClient::new()?);
            }
            client.as_ref().unwrap().ground_level(x, y, cancel)
        })
    }
    /// Production and fake providers publish each resolved/unavailable point
    /// before requesting the next one. The AHN client's original three search
    /// windows and negative-result LRU stay shared across project batches.
    pub fn with_streaming_fetcher(
        mut fetch: impl FnMut(f64, f64, &AtomicBool) -> Result<Option<f64>, String> + Send + 'static,
    ) -> Result<Self, String> {
        Self::with_runner(move |requests, cancel, publish| {
            for request in requests {
                crate::input::check(cancel)?;
                let result = fetch(request.position.x, request.position.y, cancel);
                crate::input::check(cancel)?;
                let (height, error) = match result {
                    Ok(h) => (h, None),
                    Err(e) => (None, Some(e)),
                };
                publish(vec![Response {
                    request,
                    height,
                    error,
                }])?;
            }
            Ok(Vec::new())
        })
    }
    /// Batch-injection compatibility. This constructor publishes its returned
    /// batch once; use with_streaming_fetcher to exercise actual point progress.
    pub fn with_processor(
        mut processor: impl FnMut(Vec<Request>, &AtomicBool) -> Result<Vec<Response>, String>
            + Send
            + 'static,
    ) -> Result<Self, String> {
        Self::with_runner(move |requests, cancel, _| processor(requests, cancel))
    }
    fn with_runner(
        mut processor: impl FnMut(
                Vec<Request>,
                &AtomicBool,
                &mut dyn FnMut(Vec<Response>) -> Result<(), String>,
            ) -> Result<Vec<Response>, String>
            + Send
            + 'static,
    ) -> Result<Self, String> {
        let (tx, input) = std::sync::mpsc::sync_channel::<Task>(1);
        let (output, rx) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("sleufbase-pending-ahn".into())
            .spawn(move || {
                while let Ok(task) = input.recv() {
                    let send = |terminal, event| {
                        let mut message = (task.revision, terminal, event);
                        loop {
                            crate::input::check(&task.cancel)?;
                            match output.try_send(message) {
                                Ok(()) => return Ok(()),
                                Err(std::sync::mpsc::TrySendError::Full(value)) => {
                                    message = value;
                                    std::thread::sleep(std::time::Duration::from_millis(5));
                                }
                                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                                    return Err("AHN-resultaatontvanger is gesloten".into())
                                }
                            }
                        }
                    };
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        processor(task.requests, &task.cancel, &mut |rows| {
                            send(false, Ok(rows))
                        })
                    }))
                    .unwrap_or_else(|_| Err("AHN-taak is onverwacht gestopt".into()));
                    if send(true, result).is_err() && !task.cancel.load(Ordering::Relaxed) {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            tx,
            rx,
            revision: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            pending: None,
            busy: false,
        })
    }
    pub fn start(&mut self, requests: Vec<Request>) {
        self.cancel();
        if requests.is_empty() {
            return;
        }
        self.cancel = std::sync::Arc::new(AtomicBool::new(false));
        self.pending = Some(Task {
            revision: self.revision,
            requests,
            cancel: self.cancel.clone(),
        });
        self.busy = true;
        self.pump();
    }
    pub fn cancel(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.pending = None;
        self.busy = false;
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    fn pump(&mut self) {
        if let Some(task) = self.pending.take() {
            match self.tx.try_send(task) {
                Ok(()) => {}
                Err(std::sync::mpsc::TrySendError::Full(task)) => self.pending = Some(task),
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    self.busy = false;
                }
            }
        }
    }
    /// Nonterminal point batches leave busy() true; terminal success is an
    /// empty batch. Returning one bounded message avoids losing earlier points
    /// when a later request finishes or fails during the same UI frame.
    pub fn poll(&mut self) -> Option<Result<Vec<Response>, String>> {
        let mut result = None;
        loop {
            match self.rx.try_recv() {
                Ok((revision, terminal, event)) if revision == self.revision => {
                    if terminal {
                        self.busy = false;
                    }
                    result = Some(event);
                    break;
                }
                Ok(_) => continue,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if self.busy {
                        self.busy = false;
                        result = Some(Err("AHN-taak is afgebroken".into()));
                    }
                    break;
                }
            }
        }
        self.pump();
        result
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
    fn next(worker: &mut Worker) -> Result<Vec<Response>, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(result) = worker.poll() {
                return result;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    #[test]
    fn streaming_publishes_before_later_stall_and_cancel_discards_old_terminal() {
        let mut t = trench();
        t.end_metadata = t.start_metadata.clone();
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let mut calls = 0;
        let mut worker = Worker::with_streaming_fetcher(move |_, _, cancel| {
            calls += 1;
            if calls == 2 {
                started_tx.send(()).unwrap();
                while !cancel.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                return Err("cancelled".into());
            }
            Ok(Some(4.))
        })
        .unwrap();
        worker.start(plan(&[t.clone()], 1));
        started_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let rows = next(&mut worker).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].request.target, MapPoint::Start);
        assert_eq!(rows[0].height, Some(4.));
        assert!(worker.busy());
        worker.start(plan(&[t], 2));
        let first = next(&mut worker).unwrap();
        assert_eq!(first[0].request.generation, 2);
        let second = next(&mut worker).unwrap();
        assert_eq!(second[0].request.target, MapPoint::End);
        assert!(next(&mut worker).unwrap().is_empty());
        assert!(!worker.busy());
    }
    #[test]
    fn streaming_unavailable_and_later_panic_do_not_hide_prior_results() {
        let mut t = trench();
        t.end_metadata = t.start_metadata.clone();
        let mut calls = 0;
        let mut worker = Worker::with_streaming_fetcher(move |_, _, _| {
            calls += 1;
            if calls == 1 {
                Err("provider offline".into())
            } else {
                panic!("injected later failure")
            }
        })
        .unwrap();
        worker.start(plan(&[t], 1));
        let rows = next(&mut worker).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].height.is_none());
        assert_eq!(rows[0].error.as_deref(), Some("provider offline"));
        assert!(worker.busy());
        assert!(next(&mut worker).is_err());
        assert!(!worker.busy());
    }
    #[test]
    fn cancellation_unblocks_full_result_channel_without_another_fetch() {
        let mut t = trench();
        t.end_metadata = t.start_metadata.clone();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = counter.clone();
        let mut worker = Worker::with_streaming_fetcher(move |_, _, _| {
            count.fetch_add(1, Ordering::Relaxed);
            Ok(Some(1.))
        })
        .unwrap();
        worker.start(plan(&[t], 1));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while counter.load(Ordering::Relaxed) < 2 {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        worker.cancel();
        assert!(worker.poll().is_none());
        assert!(!worker.busy());
        drop(worker);
    }
    #[test]
    fn pending_points_share_one_source_snapshot() {
        let mut t = trench();
        t.label = "S".repeat(256);
        t.source_path = Some(PathBuf::from(
            "source-".to_owned() + &"x".repeat(200) + ".tiff",
        ));
        let point = crate::virtual_trench::TrenchPoint {
            role: "object".into(),
            object_name: "LS".into(),
            source_name: "LS".into(),
            position: t.start,
            metadata: t.start_metadata.clone(),
            attributes: Default::default(),
        };
        t.objects = vec![point; 10_000];
        let requests = plan(&[t], 7);
        assert_eq!(requests.len(), 10_001);
        assert!(requests
            .windows(2)
            .all(|pair| Arc::ptr_eq(&pair[0].source, &pair[1].source)));
        assert_eq!(Arc::strong_count(&requests[0].source), requests.len());
    }
    fn trench() -> VirtualTrench {
        let mut t = VirtualTrench::new(
            "pending".into(),
            Vertex {
                x: 155000.,
                y: 463000.,
                z: None,
            },
            Vertex {
                x: 155010.,
                y: 463000.,
                z: None,
            },
            0.6,
        )
        .unwrap();
        t.start_metadata =
            json!({"surface_mode":"auto","surface_status":"pending","sleufbase_depth_meters":0.4});
        t
    }
    #[test]
    fn original_pending_queue_oracle() {
        let data: Value =
            serde_json::from_str(include_str!("../tests/fixtures/ahn-pending/reference.json"))
                .unwrap();
        for case in data["cases"].as_array().unwrap() {
            let mut t = trench();
            for (i, p) in case["points"].as_array().unwrap().iter().enumerate() {
                let mut meta = p.clone();
                let point = Vertex {
                    x: p["x"].as_f64().unwrap(),
                    y: p["y"].as_f64().unwrap(),
                    z: p["z"].as_f64(),
                };
                for key in ["role", "x", "y", "z"] {
                    meta.as_object_mut().unwrap().remove(key);
                }
                match i {
                    0 => {
                        t.start = point;
                        t.start_metadata = meta
                    }
                    1 => {
                        t.end = point;
                        t.end_metadata = meta
                    }
                    _ => t.objects.push(crate::virtual_trench::TrenchPoint {
                        role: "object".into(),
                        object_name: "LS".into(),
                        source_name: "LS".into(),
                        position: point,
                        metadata: meta,
                        attributes: Default::default(),
                    }),
                }
            }
            let got = plan(&[t], 7)
                .into_iter()
                .map(|r| r.metadata["point_id"].clone())
                .collect::<Vec<_>>();
            assert_eq!(got, *case["queued"].as_array().unwrap());
        }
    }
    #[test]
    fn stale_manual_measured_and_cancel_protect_sources() {
        let t = trench();
        let mut ts = vec![t.clone()];
        let make = || {
            resolve_with(plan(&[t.clone()], 7), &AtomicBool::new(false), |_, _, _| {
                Ok(Some(4.))
            })
            .unwrap()
            .remove(0)
        };
        assert!(!apply(&mut ts, 8, make()).unwrap());
        ts[0].start_metadata["surface_mode"] = json!("manual");
        assert!(!apply(&mut ts, 7, make()).unwrap());
        ts[0] = t.clone();
        ts[0].start.x += 1.;
        assert!(!apply(&mut ts, 7, make()).unwrap());
        ts[0] = t.clone();
        assert!(apply(&mut ts, 7, make()).unwrap());
        assert_eq!(ts[0].start.z, Some(3.6));
        let mut measured = trench();
        measured.boundary_3d = vec![
            measured.start,
            measured.end,
            Vertex {
                x: 155005.,
                y: 463001.,
                z: Some(2.),
            },
        ];
        assert!(plan(&[measured], 7).is_empty());
        assert!(
            resolve_with(plan(&ts, 9), &AtomicBool::new(true), |_, _, _| panic!(
                "cancelled request must not fetch"
            ))
            .is_err()
        );
    }
    #[test]
    fn bounded_worker_replaces_stalled_batch_and_drop_cancels() {
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let (drop_tx, drop_rx) = std::sync::mpsc::sync_channel(1);
        let mut worker = Worker::with_processor(move |requests, cancel| {
            let generation = requests[0].generation;
            started_tx.send(generation).unwrap();
            if generation == 1 || generation == 3 {
                while !cancel.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                if generation == 3 {
                    drop_tx.send(()).unwrap();
                }
                return Err("cancelled".into());
            }
            resolve_with(requests, cancel, |_, _, _| Ok(Some(2.)))
        })
        .unwrap();
        let t = trench();
        worker.start(plan(&[t.clone()], 1));
        assert_eq!(
            started_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            1
        );
        worker.start(plan(&[t.clone()], 2));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let result = loop {
            if let Some(r) = worker.poll() {
                break r.unwrap();
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        assert_eq!(result[0].request.generation, 2);
        assert!(!worker.busy());
        assert_eq!(started_rx.recv().unwrap(), 2);
        worker.start(plan(&[t], 3));
        assert_eq!(
            started_rx
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap(),
            3
        );
        drop(worker);
        drop_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
    }
}
