//! Explicit object refreshes use fresh downloads and publish one complete,
//! owner-checked snapshot. Ordinary cached downloads remain resumable.
use super::*;
use sleufbase_rust::{jobs_cache::SnapshotOwner, kickthemap_browser::BrowserChannel};
use std::{
    collections::BTreeSet,
    io::Read,
    path::Path,
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
};

struct Snapshot {
    dataset: Dataset,
    // Keep the new files only after the current project accepts this snapshot.
    // Failure, cancellation and late results otherwise remove only this directory.
    directory: tempfile::TempDir,
}

struct Outcome {
    session: KickTheMapLiveClient,
    result: Result<Snapshot, String>,
}

#[derive(PartialEq, Eq)]
struct SourceStamp {
    path: PathBuf,
    address: usize,
    count: usize,
}

pub(super) struct JobReload {
    owner: SnapshotOwner,
    generation: u64,
    job_id: u64,
    sources: Vec<SourceStamp>,
    events: Receiver<Outcome>,
    cancel: Arc<AtomicBool>,
}

impl JobReload {
    pub(super) fn owner(&self) -> &SnapshotOwner {
        &self.owner
    }

    pub(super) fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
}

impl Drop for JobReload {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) { Err("Objecten opnieuw laden geannuleerd".into()) }
    else { Ok(()) }
}

fn read_object_bytes(path: &Path, cancel: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        check_cancel(cancel)?;
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 { return Ok(bytes); }
        if bytes.len().saturating_add(count) > 64 * 1024 * 1024 {
            return Err("Het KickTheMap-objectbestand is groter dan 64 MiB".into());
        }
        bytes.try_reserve(count).map_err(|_| "Onvoldoende geheugen voor de objectgegevens")?;
        bytes.extend_from_slice(&buffer[..count]);
    }
}

fn read_snapshot(path: &Path, job: &Job, cancel: &AtomicBool) -> Result<Dataset, String> {
    check_cancel(cancel)?;
    if sleufbase_rust::geotiff::job_sidecar(path).is_none_or(|identity| identity.0 != job.job_id) {
        return Err("De opgehaalde objectgegevens horen niet bij de gekozen KickTheMap-job".into());
    }
    let bytes = read_object_bytes(path, cancel)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "De objectgegevens bevatten geen geldige UTF-8")?;
    let mut dataset = match sleufbase_rust::parse::parse_job_features_with_cancel(
        text, job.job_id, &job.title, cancel,
    ) {
        Ok(dataset) => dataset,
        Err(error) => {
            check_cancel(cancel)?;
            // An explicit empty feature list is a valid online deletion of all
            // objects. Malformed/nonempty unusable responses retain old data.
            let empty = serde_json::from_str::<serde_json::Value>(text).ok()
                .is_some_and(|value| value.get("features").and_then(serde_json::Value::as_array)
                    .is_some_and(Vec::is_empty));
            if !empty { return Err(error); }
            Dataset { job_id: job.job_id, title: job.title.clone(), source_path: PathBuf::new(), features: Vec::new() }
        }
    };
    check_cancel(cancel)?;
    dataset.source_path = path.to_path_buf();
    Ok(dataset)
}

fn download_snapshot(
    session: &mut KickTheMapLiveClient, job: &Job, root: &Path,
    browser: Option<&BrowserChannel>, cancel: &AtomicBool,
) -> Result<Snapshot, String> {
    check_cancel(cancel)?;
    std::fs::create_dir_all(root).map_err(|error| error.to_string())?;
    let directory = tempfile::Builder::new().prefix("object_refresh_").tempdir_in(root)
        .map_err(|error| error.to_string())?;
    let path = session.download_job_with_browser(job, RemoteFileKind::JobFeatures,
        directory.path(), cancel, browser)?;
    let dataset = read_snapshot(&path, job, cancel)?;
    Ok(Snapshot { dataset, directory })
}

impl SleufBaseApp {
    fn kickthemap_reload_sources(&self, job_id: u64) -> Vec<SourceStamp> {
        self.datasets.iter().filter(|dataset| dataset.job_id == job_id).map(|dataset| SourceStamp {
            path: dataset.source_path.clone(), address: dataset.features.as_ptr() as usize,
            count: dataset.features.len(),
        }).collect()
    }

    fn kickthemap_reload_project_busy(&self) -> bool {
        self.project_sources_owned() || self.file_intent.is_some() || self.export_intent.is_some()
            || self.template_job.is_some() || self.trench_editor.is_some()
            || self.template_dialog.is_some() || self.project_template_dialog.is_some()
            || self.template_order_dialog.is_some() || self.export_dialog.is_some()
            || self.maaiveld_ui.has_draft() || self.maaiveld_ui.preloading()
    }

    pub(super) fn start_kickthemap_object_reload(&mut self, id: u64) {
        if self.live_operation_busy() || self.browser_ui.batch_mode
            || !self.browser_ui.queue.is_empty() || !self.browser_ui.downloaded_json.is_empty()
            || !self.browser_ui.downloaded_tiffs.is_empty() || self.kickthemap_reload_project_busy()
        {
            self.browser_ui.status = "Rond eerst de lopende taak af en sluit de bewerk- of exportvensters.".into();
            self.message = self.browser_ui.status.clone();
            return;
        }
        let Some(job) = self.live_jobs.iter().find(|job| id > 0 && job.job_id == id).cloned() else {
            self.browser_ui.status = "Vernieuw eerst de jobs; deze KickTheMap-job is niet beschikbaar.".into();
            self.message = self.browser_ui.status.clone();
            return;
        };
        let Some(session) = self.live_session.as_ref() else {
            self.browser_ui.status = "Meld je aan bij KickTheMap om de objecten opnieuw te laden.".into();
            self.message = self.browser_ui.status.clone();
            return;
        };
        if self.live_jobs_owner.as_ref() != Some(session.snapshot_owner()) {
            self.browser_ui.status = "Vernieuw de jobs van de huidige KickTheMap-sessie voordat je objecten opnieuw laadt.".into();
            self.message = self.browser_ui.status.clone();
            return;
        }
        let owner = session.snapshot_owner().clone();
        let sources = self.kickthemap_reload_sources(id);
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let browser = self.browser_ui.browser.as_ref().map(|browser| browser.channel());
        let (sender, events) = mpsc::channel();
        // Retain recoverable ownership if the operating system cannot start
        // the worker. No credentials are copied into another session.
        let session = Arc::new(Mutex::new(self.live_session.take()));
        let worker_session = Arc::clone(&session);
        let spawn = std::thread::Builder::new().name("kickthemap-object-refresh".into()).spawn(move || {
            let Some(mut session) = worker_session.lock().unwrap_or_else(|error| error.into_inner()).take() else { return; };
            let result = download_snapshot(&mut session, &job, &kickthemap_cache_dir().join("Rust"),
                browser.as_ref(), &worker_cancel);
            let _ = sender.send(Outcome { session, result });
        });
        if let Err(error) = spawn {
            self.live_session = session.lock().unwrap_or_else(|error| error.into_inner()).take();
            self.browser_ui.status = format!("Objecten niet opnieuw geladen; huidige gegevens behouden: {error}");
        } else {
            self.browser_ui.reloading = Some(JobReload {
                owner, generation: self.project_generation, job_id: id, sources, events, cancel,
            });
            self.browser_ui.status = format!("Objecten van KickTheMap-job {id} opnieuw ophalen…");
        }
        self.message = self.browser_ui.status.clone();
    }

    pub(super) fn poll_kickthemap_object_reload(&mut self, ctx: &egui::Context) {
        let Some(reload) = self.browser_ui.reloading.as_ref() else { return; };
        if reload.generation != self.project_generation || self.live_jobs_owner.as_ref() != Some(&reload.owner) {
            reload.cancel();
        }
        // A source-owning task can begin while the network worker runs. Wait
        // for its sources to return instead of mutating its temporary empty UI.
        if self.kickthemap_reload_project_busy() && !reload.cancel.load(Ordering::Acquire) {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        let event = match reload.events.try_recv() {
            Ok(outcome) => Ok(outcome),
            Err(TryRecvError::Empty) => { ctx.request_repaint_after(Duration::from_millis(100)); return; }
            Err(TryRecvError::Disconnected) => Err("De verbinding met de objectverversing is afgebroken".to_owned()),
        };
        let reload = self.browser_ui.reloading.take().unwrap();
        let status = match event {
            Err(error) => format!("Objecten niet opnieuw geladen; huidige gegevens behouden: {error}"),
            Ok(Outcome { session, result }) => {
                let same_owner = self.live_jobs_owner.as_ref() == Some(&reload.owner)
                    && session.snapshot_owner() == &reload.owner
                    && self.live_session.as_ref().is_none_or(|current| current.snapshot_owner() == &reload.owner);
                if same_owner && self.live_session.is_none() { self.live_session = Some(session); }
                if !same_owner {
                    "Objectverversing van een eerdere KickTheMap-sessie genegeerd; huidige gegevens behouden.".into()
                } else if reload.generation != self.project_generation {
                    "Project gewijzigd; de objectverversing is niet in het nieuwe project toegepast.".into()
                } else if reload.cancel.load(Ordering::Acquire) {
                    "Objecten opnieuw laden geannuleerd; huidige gegevens behouden.".into()
                } else if self.kickthemap_reload_sources(reload.job_id) != reload.sources {
                    "De lokale objectlagen zijn tijdens het ophalen gewijzigd. Laad de objecten opnieuw om die lagen te verversen.".into()
                } else {
                    match result {
                        Ok(snapshot) => match self.apply_kickthemap_object_snapshot(reload.job_id, snapshot) {
                            Ok(count) => format!("KickTheMap-job {} vernieuwd: {count} objecten. De kaart en gekoppelde profielen gebruiken de nieuwe gegevens.", reload.job_id),
                            Err(error) => format!("Objecten niet opnieuw geladen; huidige gegevens behouden: {error}"),
                        },
                        Err(error) => {
                            if kickthemap_login_required(&error) {
                                self.live_session = None;
                                self.hide_kickthemap_pane();
                                self.open_kickthemap_login();
                            }
                            format!("Objecten niet opnieuw geladen; huidige gegevens behouden: {error}")
                        }
                    }
                }
            }
        };
        self.browser_ui.status = status;
        self.message = self.browser_ui.status.clone();
        ctx.request_repaint();
    }

    fn apply_kickthemap_object_snapshot(&mut self, id: u64, snapshot: Snapshot) -> Result<usize, String> {
        if id == 0 || snapshot.dataset.job_id != id {
            return Err("De opgehaalde objectgegevens horen niet bij de gekozen job".into());
        }
        let Snapshot { dataset, directory } = snapshot;
        let count = dataset.features.len();
        let path = dataset.source_path.clone();
        let title = dataset.title.clone();
        let old_paths: BTreeSet<_> = self.datasets.iter().filter(|dataset| dataset.job_id == id)
            .map(|dataset| dataset.source_path.clone()).collect();
        let hidden = !old_paths.is_empty() && old_paths.iter().all(|path|
            self.project_view.hidden_object_layers.contains(path));
        let mut profile_paths = old_paths.clone();
        for raster in self.geotiffs.iter().filter(|raster| raster.kickthemap_job_id == Some(id)) {
            profile_paths.extend(sleufbase_rust::template_project::hydrate::local_path(raster));
            profile_paths.extend(sleufbase_rust::raster_points::local_dataset_path(raster));
        }
        let replacement = self.datasets.iter().position(|dataset| dataset.job_id == id).unwrap_or(self.datasets.len());
        let mut next = 0;
        let remap: Vec<_> = self.datasets.iter().enumerate().map(|(index, dataset)| {
            if dataset.job_id == id && index != replacement { replacement }
            else { let mapped = next; next += 1; mapped }
        }).collect();
        let preferences = self.template_preferences.as_ref().map(|preferences|
            (preferences.legacy_layout_only, preferences.restore(&self.datasets, &self.geotiffs, &self.virtual_trenches)));

        // All validation is complete. No fallible filesystem operations follow
        // this commit; the new directory and its JSON sidecar remain together.
        let _ = directory.keep();
        let mut fresh = Some(dataset);
        let previous = std::mem::take(&mut self.datasets);
        self.datasets = Vec::with_capacity(next + usize::from(replacement == previous.len()));
        for (index, dataset) in previous.into_iter().enumerate() {
            if index == replacement { self.datasets.push(fresh.take().unwrap()); }
            else if dataset.job_id != id { self.datasets.push(dataset); }
        }
        if let Some(dataset) = fresh { self.datasets.push(dataset); }
        self.selected = self.selected.and_then(|index| remap.get(index).copied());
        self.project_view.hidden_object_layers.retain(|old| !old_paths.contains(old)
            || self.datasets.iter().any(|dataset| dataset.source_path == *old));
        if hidden && !self.project_view.hidden_object_layers.contains(&path) {
            self.project_view.hidden_object_layers.push(path.clone());
        }
        self.navigation.hidden_object_layers = self.project_view.hidden_object_layers.clone();
        for raster in self.geotiffs.iter_mut().filter(|raster| raster.kickthemap_job_id == Some(id)) {
            if !raster.metadata.is_object() { raster.metadata = serde_json::json!({}); }
            raster.metadata["kickthemap_job_features_path"] = serde_json::json!(path.to_string_lossy());
            raster.metadata["kickthemap_job_title"] = serde_json::json!(title);
            raster.kickthemap_job_title = title.clone();
            // The TIFF .job.json sidecar identifies the TIFF, not the JSON.
            // Keep it intact; profile readers follow this project metadata.
        }
        if let Some(state) = self.template_state.as_mut() {
            for row in &mut state.rows {
                if row.dataset.as_ref().is_some_and(|old| profile_paths.contains(old)) {
                    row.dataset = Some(path.clone());
                }
            }
        }
        if let Some((legacy_layout_only, mut options)) = preferences {
            let mut included = false;
            options.choices.retain_mut(|choice| {
                let sleufbase_rust::template_project::Source::Job(old) = choice.source else { return true; };
                let Some(&index) = remap.get(old) else { return false; };
                choice.source = sleufbase_rust::template_project::Source::Job(index);
                if index == replacement { if included { return false; } included = true; }
                true
            });
            let mut preferences = sleufbase_rust::template_prefs::TemplatePreferences::capture(
                &options, &self.datasets, &self.geotiffs, &self.virtual_trenches);
            preferences.legacy_layout_only = legacy_layout_only;
            self.template_preferences = Some(preferences);
        }
        self.raster_points.mode = None;
        self.raster_points.rows.clear();
        self.raster_points.pending_generation = None;
        self.reset_maaiveld_state();
        self.dirty = true;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RAW: &str = r#"{"features":[{"geometry":{"type":"Point","coordinates":[155001,463002,1]},"properties":{"idName":"water","properties1":"PE","properties2":"110","properties3":"2"}}]}"#;

    fn job(id: u64) -> Job {
        Job { job_id: id, title: format!("PS{id}"), address: String::new(), municipality: String::new(),
            coordinates: String::new(), client_date: String::new(), status: 9, downloadable: true, prefix: String::new() }
    }
    fn dataset(id: u64, path: &str) -> Dataset {
        let mut dataset = sleufbase_rust::parse_job_features(RAW, id, &format!("PS{id}")).unwrap();
        dataset.source_path = path.into();
        dataset
    }
    fn snapshot(root: &Path, id: u64, raw: &str) -> Snapshot {
        let directory = tempfile::tempdir_in(root).unwrap();
        let path = directory.path().join(format!("PS{id}_{id}_jobFeatures.json"));
        std::fs::write(&path, raw).unwrap();
        std::fs::write(path.with_extension("json.job.json"), format!(r#"{{"job_id":{id},"title":"PS{id}"}}"#)).unwrap();
        let dataset = read_snapshot(&path, &job(id), &AtomicBool::new(false)).unwrap();
        Snapshot { dataset, directory }
    }
    fn raster(id: u64, old: &str) -> GeoTiffInfo {
        let transform = sleufbase_rust::geo::GeoTransform { a: 1., b: 0., c: 155000., d: 0., e: -1., f: 463010. };
        GeoTiffInfo { path: format!("PS{id}.tiff").into(), width: 10, height: 10,
            transform, bounds: transform.bounds(10, 10).unwrap(), epsg: Some(28992),
            kickthemap_job_id: Some(id), kickthemap_job_title: format!("old {id}"),
            kickthemap_job_sidecar: Some(format!("PS{id}.tiff.job.json").into()),
            metadata: serde_json::json!({"kickthemap_job_features_path":old,"rotation_degrees":12,"custom":"preserve"}) }
    }
    fn session() -> KickTheMapLiveClient {
        let cookie = sleufbase_rust::kickthemap_browser::BrowserCookie {
            name: "fixture".into(), value: "local-only".into(), domain: "my.kickthemap.com".into(), path: "/".into(), secure: true,
        };
        KickTheMapLiveClient::from_authenticated_browser(&[cookie], "fixture-csrf").unwrap()
    }
    fn pending(app: &mut SleufBaseApp, id: u64) -> (mpsc::Sender<Outcome>, KickTheMapLiveClient) {
        let session = session();
        let owner = session.snapshot_owner().clone();
        let (sender, events) = mpsc::channel();
        app.live_jobs_owner = Some(owner.clone());
        app.browser_ui.reloading = Some(JobReload { owner, generation: app.project_generation, job_id: id,
            sources: app.kickthemap_reload_sources(id), events, cancel: Arc::new(AtomicBool::new(false)) });
        (sender, session)
    }

    #[test]
    fn refresh_replaces_same_job_layers_and_relinks_profiles_without_moving_the_map() {
        let root = tempfile::tempdir().unwrap();
        let mut app = SleufBaseApp::default();
        app.datasets = vec![dataset(7, "keep-a.json"), dataset(42, "old-a.json"), dataset(42, "old-b.json"), dataset(8, "keep-b.json")];
        app.selected = Some(3);
        app.project_view.hidden_object_layers = vec!["old-a.json".into(), "old-b.json".into(), "keep-a.json".into()];
        app.geotiffs = vec![raster(42, "old-a.json"), raster(7, "keep-a.json")];
        app.zoom = 2.5; app.pan = egui::vec2(11., 19.);
        app.navigation.focus = Some(location_ui::Focus { center: [155010., 463020.], meters_per_pixel: 0.2 });
        app.template_state = Some(sleufbase_rust::template_order::State {
            rows: vec![sleufbase_rust::template_order::Row { dataset: Some("old-b.json".into()), comments: "keep".into(), ..Default::default() }],
            ..Default::default()
        });
        let snapshot = snapshot(root.path(), 42, RAW);
        let path = snapshot.dataset.source_path.clone();
        assert_eq!(app.apply_kickthemap_object_snapshot(42, snapshot).unwrap(), 1);
        assert_eq!(app.datasets.iter().map(|dataset| dataset.job_id).collect::<Vec<_>>(), [7, 42, 8]);
        assert_eq!(app.selected, Some(2));
        assert_eq!(app.datasets[1].source_path, path);
        assert!(path.is_file() && path.with_extension("json.job.json").is_file());
        assert_eq!(app.project_view.hidden_object_layers, vec![PathBuf::from("keep-a.json"), path.clone()]);
        assert_eq!(app.geotiffs[0].metadata["kickthemap_job_features_path"], path.to_string_lossy().as_ref());
        assert_eq!(app.geotiffs[0].metadata["rotation_degrees"], 12);
        assert_eq!(app.geotiffs[0].metadata["custom"], "preserve");
        assert_eq!(app.geotiffs[0].kickthemap_job_sidecar, Some("PS42.tiff.job.json".into()));
        assert_eq!(app.geotiffs[1].metadata["kickthemap_job_features_path"], "keep-a.json");
        assert_eq!(app.template_state.as_ref().unwrap().rows[0].dataset, Some(path));
        assert_eq!(app.template_state.as_ref().unwrap().rows[0].comments, "keep");
        assert_eq!((app.zoom, app.pan), (2.5, egui::vec2(11., 19.)));
        assert_eq!(app.navigation.focus.unwrap().center, [155010., 463020.]);
        assert!(app.dirty);
    }

    #[test]
    fn repeated_refresh_keeps_one_layer_its_selection_and_template_choices() {
        use sleufbase_rust::{template_project as template, template_prefs::TemplatePreferences};
        let root = tempfile::tempdir().unwrap();
        let mut app = SleufBaseApp::default();
        app.datasets = vec![dataset(42, "old.json")]; app.selected = Some(0);
        let mut options = template::Options { choices: template::suggest(&app.datasets, &[], &[]), ..Default::default() };
        options.choices[0].comments = "keep comments".into(); options.choices[0].label = Some("PS42B".into());
        options.choices[0].include = false;
        app.template_preferences = Some(TemplatePreferences::capture(&options, &app.datasets, &[], &[]));
        for _ in 0..2 {
            app.apply_kickthemap_object_snapshot(42, snapshot(root.path(), 42, RAW)).unwrap();
            assert_eq!(app.datasets.len(), 1); assert_eq!(app.selected, Some(0));
            assert!(app.project_view.hidden_object_layers.is_empty());
            let restored = app.template_preferences.as_ref().unwrap().restore(&app.datasets, &[], &[]);
            assert_eq!(restored.choices[0].comments, "keep comments");
            assert_eq!(restored.choices[0].label.as_deref(), Some("PS42B"));
            assert!(!restored.choices[0].include);
        }
    }

    #[test]
    fn cancellation_and_late_project_or_account_results_preserve_current_data() {
        let root = tempfile::tempdir().unwrap();
        for mode in 0..4 {
            let mut app = SleufBaseApp::default();
            app.datasets = vec![dataset(42, "old.json")];
            let (sender, old_session) = pending(&mut app, 42);
            let owner = old_session.snapshot_owner().clone();
            let snapshot = snapshot(root.path(), 42, RAW);
            let path = snapshot.dataset.source_path.clone();
            match mode {
                0 => app.browser_ui.reloading.as_ref().unwrap().cancel(),
                1 => app.project_generation += 1,
                2 => { let current = session(); app.live_jobs_owner = Some(current.snapshot_owner().clone()); app.live_session = Some(current); }
                _ => app.datasets[0].source_path = "changed-by-user.json".into(),
            }
            let before = serde_json::to_value(app.snapshot()).unwrap();
            assert!(sender.send(Outcome { session: old_session, result: Ok(snapshot) }).is_ok());
            app.poll_kickthemap_object_reload(&egui::Context::default());
            assert_eq!(serde_json::to_value(app.snapshot()).unwrap(), before);
            assert!(!path.exists()); assert!(!app.dirty); assert!(app.browser_ui.reloading.is_none());
            if mode != 2 { assert_eq!(app.live_session.as_ref().unwrap().snapshot_owner(), &owner); }
            else { assert_ne!(app.live_session.as_ref().unwrap().snapshot_owner(), &owner); }
        }
    }

    #[test]
    fn failed_or_wrong_job_refresh_keeps_data_and_returns_the_session() {
        let root = tempfile::tempdir().unwrap();
        for wrong_id in [false, true] {
            let mut app = SleufBaseApp::default(); app.datasets = vec![dataset(42, "old.json")];
            let (sender, session) = pending(&mut app, 42);
            let before = serde_json::to_value(app.snapshot()).unwrap();
            let result = if wrong_id { Ok(snapshot(root.path(), 99, RAW)) } else { Err("offline".into()) };
            assert!(sender.send(Outcome { session, result }).is_ok());
            app.poll_kickthemap_object_reload(&egui::Context::default());
            assert_eq!(serde_json::to_value(app.snapshot()).unwrap(), before);
            assert!(app.live_session.is_some()); assert!(!app.dirty);
            assert!(app.message.contains("huidige gegevens behouden"));
        }
    }

    #[test]
    fn explicit_empty_job_clears_objects_but_invalid_payload_or_identity_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let mut app = SleufBaseApp::default(); app.datasets = vec![dataset(42, "old.json")];
        let empty = snapshot(root.path(), 42, r#"{"features":[]}"#);
        let path = empty.dataset.source_path.clone();
        app.apply_kickthemap_object_snapshot(42, empty).unwrap();
        assert!(app.datasets[0].features.is_empty());
        assert_eq!(app.datasets.len(), 1);
        std::fs::write(&path, r#"{"features":[{"geometry":{"type":"Point","coordinates":["bad",2]}}]}"#).unwrap();
        assert!(read_snapshot(&path, &job(42), &AtomicBool::new(false)).is_err());
        std::fs::write(&path, RAW).unwrap();
        assert!(read_snapshot(&path, &job(99), &AtomicBool::new(false)).is_err());
        assert!(read_snapshot(&path, &job(42), &AtomicBool::new(true)).is_err());
    }
}
