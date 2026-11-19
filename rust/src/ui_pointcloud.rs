//! Persistent LAS/LAZ overview, editor ownership and deliberate project loading.
use super::*;
use sleufbase_rust::pointcloud::{self, AnnotationKind, AnnotationPurpose, CloudCrs, PointCloudSource};
use sleufbase_rust::pointcloud_catalog::{self as catalog, Catalog};
use sleufbase_rust::pointcloud_server::Session;
use sleufbase_rust::pointcloud_trench::Prepared;
use std::{collections::{HashMap, HashSet, VecDeque}, process::Child,
    sync::{Arc, atomic::{AtomicBool, Ordering}}};
#[cfg(windows)]
use std::process::{Command, Stdio};

#[path="ui_pointcloud_map.rs"]
mod map;

// Used by both the list and RD map; map selection must be visible in the list.
#[derive(Default)]
pub(super) struct Selection {
    ids: HashSet<String>,
    focused: Option<String>,
    anchor: Option<String>,
}
impl Selection {
    fn clear(&mut self) {
        self.ids.clear();
        self.focused = None;
        self.anchor = None;
    }
    fn apply(&mut self, ids: impl IntoIterator<Item = String>, extend: bool) {
        if !extend { self.clear(); }
        for id in ids {
            self.ids.insert(id.clone());
            self.focused = Some(id.clone());
            self.anchor = Some(id);
        }
    }
    fn select_row(&mut self, ordered: &[String], index: usize, extend: bool) {
        let id = ordered[index].clone();
        if extend {
            if let Some(anchor) = self.anchor.as_ref()
                .filter(|id| self.ids.contains(*id))
                .and_then(|id| ordered.iter().position(|candidate| candidate == id))
            {
                self.ids.extend(ordered[anchor.min(index)..=anchor.max(index)].iter().cloned());
                self.focused = Some(id);
                return;
            }
        }
        self.apply([id], extend);
    }
    fn reconcile(&mut self) {
        if self.focused.as_ref().is_some_and(|id| !self.ids.contains(id)) {
            self.focused = self.ids.iter().next().cloned();
        }
        if self.anchor.as_ref().is_some_and(|id| !self.ids.contains(id)) {
            self.anchor = self.focused.clone();
        }
    }
    fn set_checked(&mut self, id: &str, checked: bool) {
        if checked { self.apply([id.to_owned()], true); }
        else { self.ids.remove(id); self.reconcile(); }
    }
}

// The persistent catalog supports thousands of scans. One converter runs at a
// time, so selecting many LAS/LAZ files does not create as many large decoders.
const MAX_LAS_FILES_PER_SELECTION: usize = 2048;
const MAX_PENDING_LAS_FILES: usize = 4096;
const MAX_VISIBLE_IMPORT_ERRORS: usize = 100;

#[derive(Default)]
struct ImportBatch {
    total: usize,
    completed: usize,
    skipped: usize,
    failed: usize,
    stopped: bool,
    errors: Vec<String>,
}
impl ImportBatch {
    fn record_error(&mut self, name: &std::path::Path, error: &str) {
        self.failed += 1;
        if self.errors.len() < MAX_VISIBLE_IMPORT_ERRORS {
            self.errors.push(format!("{}: {error}", name.display()));
        }
    }
    fn description(&self) -> String {
        format!("Import: {} van {} verwerkt · {} overgeslagen · {} fouten",
            self.completed, self.total, self.skipped, self.failed)
    }
}

/// Deduplicate a large Explorer/file-dialog selection before it enters the
/// bounded queue. Normalized paths prevent repeated conversion of one file.
fn plan_las_queue(
    pending: &VecDeque<PathBuf>,
    active: Option<&std::path::Path>,
    selected: Vec<PathBuf>,
) -> Result<Vec<PathBuf>, String> {
    if selected.len() > MAX_LAS_FILES_PER_SELECTION {
        return Err(format!("Selecteer maximaal {MAX_LAS_FILES_PER_SELECTION} LAS/LAZ-bestanden per keer."));
    }
    let mut seen = pending.iter().cloned().collect::<HashSet<_>>();
    if let Some(path) = active { seen.insert(path.to_path_buf()); }
    let mut added = Vec::new();
    for path in selected {
        let valid = path.extension().and_then(|s|s.to_str())
            .is_some_and(|ext|ext.eq_ignore_ascii_case("las")||ext.eq_ignore_ascii_case("laz"));
        if !valid { return Err("Selecteer alleen .las- en .laz-bestanden.".into()); }
        let normalized = path.canonicalize().unwrap_or(path);
        if seen.insert(normalized.clone()) { added.push(normalized); }
    }
    if pending.len().saturating_add(usize::from(active.is_some()))
        .saturating_add(added.len()) > MAX_PENDING_LAS_FILES {
        return Err(format!("De importwachtrij kan maximaal {MAX_PENDING_LAS_FILES} bestanden bevatten. Verwerk eerst een deel."));
    }
    Ok(added)
}

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    queue: VecDeque<PathBuf>,
    batch: ImportBatch,
    form: Option<ImportForm>,
    worker: Option<ImportWorker>,
    ortho: Option<OrthoWorker>,
    editor: Option<Editor>,
    trench: Option<TrenchForm>,
    catalog: Option<Catalog>,
    catalog_attempted: bool,
    catalog_error: Option<String>,
    remembered_generation: Option<u64>,
    pending_catalog: Option<PendingCatalog>,
    titles: HashMap<String, String>,
    selection: Selection,
    search: String,
    filters: [String; 7],
    only_loaded: bool,
    only_selected: bool,
    sort_column: usize,
    descending: bool,
    map_open: bool,
    map: map::State,
    status: String,
}
impl State {
    pub fn busy(&self) -> bool { self.worker.is_some() || self.editor.is_some() }
    pub fn importing(&self) -> bool { self.worker.is_some() }
    pub fn reset(&mut self) {
        // The catalog and overview belong to the application, not one project.
        self.queue.clear(); self.batch = ImportBatch::default(); self.form = None; self.worker = None;
        self.editor = None; self.trench = None;
        self.ortho = None;
        self.remembered_generation = None;
        self.map.pause(); self.status.clear();
    }
    fn sources(&self) -> &[PointCloudSource] {
        self.catalog.as_ref().map_or(&[], |catalog| catalog.sources.as_slice())
    }
    fn source(&self, id: &str) -> Option<&PointCloudSource> {
        self.sources().iter().find(|source| source.id == id)
    }
    fn remember_memory(&mut self, source: PointCloudSource) {
        let sources = &mut self.catalog.get_or_insert_with(Catalog::default).sources;
        if let Some(old) = sources.iter_mut().find(|old| old.id == source.id) { *old = source; }
        else { sources.push(source); }
    }
}
struct ImportForm { path: PathBuf, title: String, crs: CloudCrs }
struct PendingCatalog { source: PointCloudSource, expected: Option<PointCloudSource>, edit: bool }
enum ImportEvent { Progress(String), Done(Result<PointCloudSource, String>) }
struct ImportWorker {
    rx: Receiver<ImportEvent>, cancel: Arc<AtomicBool>,
    generation: u64, expected: Option<PointCloudSource>,
    path: PathBuf, queued: bool,
}
impl Drop for ImportWorker { fn drop(&mut self) { self.cancel.store(true, Ordering::Release); } }
struct OrthoWorker { rx: Receiver<Result<PathBuf,String>>, cancel: Arc<AtomicBool>,
    generation:u64, source_id:String }
impl Drop for OrthoWorker { fn drop(&mut self) { self.cancel.store(true,Ordering::Release); } }
struct Editor { session: Session, child: Child, source_id: String,
    revision: u64, _log: tempfile::NamedTempFile }
impl Drop for Editor { fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); } }

#[derive(Clone, PartialEq)]
enum LoadMode { New, Update(String) }
struct TrenchForm {
    source: PointCloudSource,
    generation: u64,
    axis_id: String,
    selected_ids: HashSet<String>,
    label: String,
    width: f64,
    mode: LoadMode,
    preview: Option<Result<Prepared, String>>,
    error: Option<String>,
}

#[derive(Clone)]
struct Row {
    id: String, title: String, path: String, crs: CloudCrs,
    point_count: u64, object_count: usize, axis: String, axis_ready: bool,
    center: [f64; 3], loaded: bool,
}
impl Row {
    fn new(source: &PointCloudSource, loaded: bool) -> Self {
        let (axis, axis_ready) = axis_status(source);
        Self { id: source.id.clone(), title: source.title.clone(), path: source.source_path.display().to_string(),
            crs: source.crs, point_count: source.point_count,
            object_count: source.annotations.iter().filter(|item| item.is_object()).count(),
            axis, axis_ready, center: center(source), loaded }
    }
}

fn center(source: &PointCloudSource) -> [f64; 3] {
    std::array::from_fn(|index| source.bounds_min[index] + (source.bounds_max[index] - source.bounds_min[index]) * 0.5)
}
fn legacy_axes(source: &PointCloudSource) -> impl Iterator<Item=&pointcloud::Annotation> {
    source.annotations.iter().filter(|item| item.is_object() && item.kind == AnnotationKind::Polyline && item.vertices.len() == 2)
}
fn endpoint_count(source: &PointCloudSource) -> (usize, usize) {
    (source.annotations.iter().filter(|item| item.purpose == AnnotationPurpose::TrenchStart).count(),
     source.annotations.iter().filter(|item| item.purpose == AnnotationPurpose::TrenchEnd).count())
}
fn axis_status(source: &PointCloudSource) -> (String, bool) {
    match endpoint_count(source) {
        (1, 1) => ("Begin en eind gereed".into(), true),
        (1, 0) => ("Eindpunt ontbreekt".into(), false),
        (0, 1) => ("Beginpunt ontbreekt".into(), false),
        (0, 0) if legacy_axes(source).count() == 1 => ("Oude aslijn beschikbaar".into(), false),
        _ => ("Begin/eind instellen".into(), false),
    }
}
fn count(value: u64) -> String {
    let digits = value.to_string();
    let mut output = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 { output.push('.'); }
        output.push(digit);
    }
    output
}
fn crs_label(crs: &CloudCrs) -> &'static str {
    match crs { CloudCrs::Unknown => "Onbekend / uit bestand lezen", CloudCrs::RdNew => "RD New (EPSG:28992), meters",
        CloudCrs::Wgs84 => "WGS 84 (EPSG:4326 / 4979), graden",
        CloudCrs::Other => "Ander / lokaal coördinatenstelsel" }
}
fn crs_short(crs: CloudCrs) -> &'static str {
    match crs { CloudCrs::RdNew => "RD New · m", CloudCrs::Wgs84 => "WGS 84 · °",
        CloudCrs::Unknown => "Onbekend", CloudCrs::Other => "Ander / lokaal" }
}
fn crs_combo(ui: &mut egui::Ui, id: impl std::hash::Hash, crs: &mut CloudCrs) -> bool {
    let before = *crs;
    egui::ComboBox::from_id_salt(id).selected_text(crs_label(crs)).show_ui(ui, |ui| {
        for value in [CloudCrs::Unknown, CloudCrs::RdNew, CloudCrs::Wgs84, CloudCrs::Other] {
            if ui.selectable_label(*crs == value, crs_label(&value)).clicked() { *crs = value; }
        }
    });
    before != *crs
}

impl SleufBaseApp {
    fn ensure_pointcloud_catalog(&mut self) {
        if !self.pointcloud_ui.catalog_attempted {
            self.pointcloud_ui.catalog_attempted = true;
            match catalog::load() {
                Ok(catalog) => { self.pointcloud_ui.catalog = Some(catalog); self.pointcloud_ui.catalog_error = None; }
                Err(error) => { self.pointcloud_ui.catalog_error = Some(error); return; }
            }
        }
        if self.pointcloud_ui.catalog.is_none() || self.pointcloud_ui.remembered_generation == Some(self.project_generation) { return; }
        self.pointcloud_ui.remembered_generation = Some(self.project_generation);
        // Opening an old project must never overwrite newer, globally saved edits.
        for source in &self.point_clouds {
            if self.pointcloud_ui.source(&source.id).is_some() { continue; }
            let result = catalog::remember_if_missing(source).and_then(|()| catalog::get(&source.id));
            match result {
                Ok(Some(saved)) => self.pointcloud_ui.remember_memory(saved),
                Ok(None) => self.pointcloud_ui.catalog_error = Some("Een bron kon niet in het puntenwolkoverzicht worden teruggelezen.".into()),
                Err(error) => self.pointcloud_ui.catalog_error = Some(error),
            }
        }
    }

    pub(super) fn queue_pointcloud_files(&mut self, files: Vec<PathBuf>) {
        if files.is_empty() { return; }
        if self.trench_editor.is_some() || self.pointcloud_ui.editor.is_some() {
            self.message = "Sluit eerst het openstaande sleuf- of puntenwolkbewerkscherm.".into();
            return;
        }
        if self.pointcloud_ui.pending_catalog.is_some() {
            self.message = "Bewaar eerst de eerder geïmporteerde puntenwolk met Opnieuw bewaren.".into();
            return;
        }
        self.pointcloud_ui.open = true;
        self.ensure_pointcloud_catalog();
        if self.pointcloud_ui.catalog.is_none() {
            self.message = self.pointcloud_ui.catalog_error.clone()
                .unwrap_or_else(|| "Puntenwolkoverzicht is niet beschikbaar.".into());
            return;
        }
        let active = self.pointcloud_ui.worker.as_ref()
            .filter(|worker| worker.queued).map(|worker| worker.path.as_path());
        let added = match plan_las_queue(&self.pointcloud_ui.queue, active, files) {
            Ok(paths) => paths,
            Err(error) => { self.message = error.clone(); self.pointcloud_ui.status = error; return; }
        };
        if added.is_empty() {
            self.pointcloud_ui.status = "Deze bestanden staan al in de importwachtrij.".into();
            return;
        }
        // The global catalog is separate from the project's 512-source limit.
        // Never let a successful batch silently exceed the catalog read limit.
        let known = self.pointcloud_ui.sources().iter().map(|source| source.source_path.clone())
            .collect::<HashSet<_>>();
        let mut pending = self.pointcloud_ui.queue.iter().cloned().collect::<HashSet<_>>();
        if let Some(active) = active { pending.insert(active.to_path_buf()); }
        pending.extend(added.iter().cloned());
        let unique_new = pending.iter().filter(|path| !known.contains(*path)).count();
        let total = known.len().saturating_add(unique_new);
        if total > catalog::MAX_CATALOG_SOURCES {
            let available = catalog::MAX_CATALOG_SOURCES.saturating_sub(known.len());
            self.message = format!("Er is ruimte voor maximaal {available} nieuwe scans in het puntenwolkoverzicht. Selecteer minder bestanden.");
            self.pointcloud_ui.status = self.message.clone();
            return;
        }
        if self.pointcloud_ui.queue.is_empty() && active.is_none() {
            self.pointcloud_ui.batch = ImportBatch::default();
        }
        self.pointcloud_ui.batch.total += added.len();
        let count = added.len();
        self.pointcloud_ui.queue.extend(added);
        self.pointcloud_ui.status = format!("{count} LAS/LAZ-bestanden toegevoegd. {}", self.pointcloud_ui.batch.description());
    }
    pub(super) fn choose_pointcloud_files(&mut self) {
        if let Some(files) = rfd::FileDialog::new().set_title("Puntenwolken toevoegen")
            .add_filter("LAS / LAZ puntenwolk", &["las", "laz"]).pick_files() { self.queue_pointcloud_files(files); }
    }
    pub(super) fn pointcloud_blocks_action(&mut self) -> bool {
        self.collect_pointcloud_edits();
        if self.pointcloud_ui.busy() {
            self.pointcloud_ui.open = true;
            self.message = "Sluit eerst de puntenwolkeditor of rond de import af. Afgeronde meetobjecten blijven in het puntenwolkoverzicht bewaard.".into();
            true
        } else { false }
    }
    fn collect_pointcloud_edits(&mut self) {
        let mut updates = Vec::new();
        if let Some(editor) = self.pointcloud_ui.editor.as_mut() {
            while let Some(update) = editor.session.poll() {
                if update.source_id != editor.source_id || update.revision <= editor.revision { continue; }
                editor.revision = update.revision;
                updates.push(update);
            }
        }
        for update in updates {
            // The server has already saved this snapshot to the durable catalog.
            if let Some(source) = self.pointcloud_ui.catalog.as_mut()
                .and_then(|catalog| catalog.sources.iter_mut().find(|source| source.id == update.source_id)) {
                source.annotations = update.annotations.clone();
            }
        }
    }
    pub(super) fn sync_pointcloud_datasets(&mut self) {
        let selected = self.selected.and_then(|index| self.datasets.get(index)).map(|dataset| dataset.source_path.clone());
        self.datasets.retain(|dataset| !pointcloud::is_cloud_dataset(dataset));
        for source in &self.point_clouds {
            if source.annotations.iter().any(|item| item.is_object()) {
                if let Ok(dataset) = source.to_dataset() { self.datasets.push(dataset); }
            }
        }
        self.selected = selected.and_then(|path| self.datasets.iter().position(|dataset| dataset.source_path == path));
    }
    fn accept_catalog_source(&mut self, source: PointCloudSource, edit: bool, expected: Option<PointCloudSource>) {
        let id = source.id.clone();
        match catalog::save_if_current(&source, expected.as_ref()) {
            Ok(()) => {
                self.pointcloud_ui.pending_catalog = None;
                self.pointcloud_ui.catalog_error = None;
                self.pointcloud_ui.selection.apply([id.clone()], false);
                self.pointcloud_ui.titles.remove(&id);
                self.pointcloud_ui.status = format!("{} staat in het puntenwolkoverzicht. Kies Inladen om de sleuf in dit project te gebruiken.", source.title);
                self.pointcloud_ui.remember_memory(source);
                if edit { self.open_pointcloud_editor(&id); }
            }
            Err(error) => {
                self.pointcloud_ui.remember_memory(source.clone());
                self.pointcloud_ui.selection.apply([id], false);
                self.pointcloud_ui.pending_catalog = Some(PendingCatalog { source, expected, edit });
                self.pointcloud_ui.status = format!("De scan is voorbereid, maar het overzicht kon niet worden bewaard: {error}. Kies Opnieuw bewaren.");
            }
        }
    }
    pub(super) fn poll_pointcloud(&mut self, ctx: &egui::Context) {
        if self.pointcloud_ui.open || !self.point_clouds.is_empty() || !self.pointcloud_ui.queue.is_empty() {
            self.ensure_pointcloud_catalog();
        }
        self.collect_pointcloud_edits();
        let ended = self.pointcloud_ui.editor.as_mut().and_then(|editor| editor.child.try_wait().ok().flatten());
        if let Some(status) = ended {
            if let Some(editor) = self.pointcloud_ui.editor.as_mut() { editor.session.close(); }
            self.collect_pointcloud_edits();
            self.pointcloud_ui.editor = None;
            self.pointcloud_ui.status = if status.success() {
                "Bewerken afgesloten. De scan en meetobjecten blijven hier bewaard. Kies Inladen om de proefsleuf te maken of bij te werken.".into()
            } else { "De editor is gestopt. Alle bevestigde wijzigingen blijven in het puntenwolkoverzicht bewaard.".into() };
        }
        let mut done = None;
        if let Some(worker) = &self.pointcloud_ui.worker {
            loop { match worker.rx.try_recv() {
                Ok(ImportEvent::Progress(message)) => self.pointcloud_ui.status = message,
                Ok(ImportEvent::Done(result)) => {
                    done = Some((worker.generation, worker.expected.clone(),
                        worker.path.clone(), worker.queued, result));
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    done = Some((worker.generation, worker.expected.clone(),
                        worker.path.clone(), worker.queued,
                        Err("De puntenwolkimport is onverwacht gestopt. Het bronbestand is behouden.".into())));
                    break;
                }
            } }
        }
        if let Some((generation, expected, path, queued, result)) = done {
            self.pointcloud_ui.worker = None;
            if generation == self.project_generation {
                if queued {
                    self.pointcloud_ui.batch.completed += 1;
                    if let Err(error) = &result {
                        self.pointcloud_ui.batch.record_error(&path, error);
                    }
                }
                match result {
                    Ok(source) => {
                        self.accept_catalog_source(source, false, expected);
                        if queued && self.pointcloud_ui.pending_catalog.is_some() {
                            self.pointcloud_ui.batch.record_error(&path, "Bronrecord nog niet bewaard");
                        }
                    }
                    Err(error) => {
                        self.pointcloud_ui.status = error.clone();
                        if !queued { self.message = error; }
                    }
                }
                if queued && self.pointcloud_ui.queue.is_empty()
                    && self.pointcloud_ui.pending_catalog.is_none()
                    && self.pointcloud_ui.batch.completed == self.pointcloud_ui.batch.total
                {
                    let batch = &self.pointcloud_ui.batch;
                    self.pointcloud_ui.status = format!("{}: {} gereed{}.",
                        if batch.stopped { "Import gestopt" } else { "Import afgerond" },
                        batch.description(),
                        if batch.failed > 0 { " (bekijk Importfouten)" } else { "" });
                }
            }
        }
        let completed_ortho=self.pointcloud_ui.ortho.as_ref().and_then(|worker| match worker.rx.try_recv() {
            Ok(result)=>Some((worker.generation,worker.source_id.clone(),result)),
            Err(TryRecvError::Disconnected)=>Some((worker.generation,worker.source_id.clone(),
                Err("De GeoTIFF-verwerking is onverwacht gestopt.".into()))),
            Err(TryRecvError::Empty)=>None,
        });
        if let Some((generation,id,result))=completed_ortho {
            self.pointcloud_ui.ortho=None;
            if generation==self.project_generation && self.point_clouds.iter().any(|source|source.id==id) {
                match result {
                    Ok(_) => {
                        if let Some(source)=self.point_clouds.iter().find(|source|source.id==id).cloned() {
                            if let Err(error)=self.register_cloud_geotiff(&source) {
                                self.pointcloud_ui.status=format!("Proefsleuf geladen, maar de GeoTIFF kon niet worden toegevoegd: {error}");
                            }else{
                                self.pointcloud_ui.status="Bovenaanzicht georiënteerd op begin → eind, als GeoTIFF toegevoegd zonder tekenobjecten.".into();
                            }
                        }
                    }
                    Err(error)=>self.pointcloud_ui.status=format!("Proefsleuf is geladen; GeoTIFF maken mislukt: {error}"),
                }
            }
        }
        if self.pointcloud_ui.busy()||self.pointcloud_ui.ortho.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if self.pointcloud_ui.form.is_none() && !self.pointcloud_ui.busy()
            && self.pointcloud_ui.pending_catalog.is_none() && self.file_intent.is_none() {
            if let Some(path) = self.pointcloud_ui.queue.pop_front() {
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                if let Some(source) = self.pointcloud_ui.sources().iter()
                    .find(|source| source.source_path == canonical && source.validate_cache().is_ok()) {
                    let id = source.id.clone(); self.pointcloud_ui.selection.apply([id], false);
                    self.pointcloud_ui.batch.completed += 1;
                    self.pointcloud_ui.batch.skipped += 1;
                    self.pointcloud_ui.status = format!("Scan al bewaard. {}", self.pointcloud_ui.batch.description());
                } else {
                    let title = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    // Every queued LAS/LAZ is prepared and remembered without
                    // opening Potree. The user can edit any scan later from
                    // the catalog, even when several files were selected.
                    self.start_pointcloud_import(ImportForm { path, title, crs: CloudCrs::Unknown }, None, ctx, true);
                }
            }
        }
    }
    fn pointcloud_task_allowed(&self) -> bool {
        !self.pointcloud_ui.busy() && !self.project_sources_owned() && self.file_intent.is_none()
            && self.export_intent.is_none() && self.template_job.is_none() && self.trench_editor.is_none()
    }
    fn pointcloud_unavailable_message(&self) -> &'static str {
        if self.trench_editor.is_some() {
            "Sluit eerst de gewone sleufbewerker en bewaar of annuleer het openstaande concept."
        } else { "Rond de lopende taak af en sluit een eventueel geopend puntenwolkvenster." }
    }
    fn start_pointcloud_import(&mut self, form: ImportForm, existing: Option<PointCloudSource>, ctx: &egui::Context, queued: bool) {
        if !self.pointcloud_task_allowed() {
            self.message = self.pointcloud_unavailable_message().into(); self.pointcloud_ui.form = Some(form); return;
        }
        let (tx, rx) = mpsc::channel(); let cancel = Arc::new(AtomicBool::new(false)); let flag = Arc::clone(&cancel);
        let repaint = ctx.clone();
        self.pointcloud_ui.status = "Puntenwolk voorbereiden…".into();
        self.pointcloud_ui.worker = Some(ImportWorker {
            rx, cancel, generation: self.project_generation, expected: existing.clone(),
            path: form.path.clone(), queued,
        });
        std::thread::spawn(move || {
            let progress = |message| { let _ = tx.send(ImportEvent::Progress(message)); repaint.request_repaint(); };
            let result = if let Some(source) = existing { pointcloud::import_existing(&source, &flag, progress) }
                else { pointcloud::import(&form.path, &form.title, form.crs, &flag, progress) };
            let _ = tx.send(ImportEvent::Done(result)); repaint.request_repaint();
        });
    }
    fn relink_pointcloud(&mut self, id: &str, path: PathBuf, ctx: &egui::Context) {
        if !self.pointcloud_task_allowed() { self.message = self.pointcloud_unavailable_message().into(); return; }
        let Some(mut source) = self.pointcloud_ui.source(id).cloned() else { return; };
        let (tx, rx) = mpsc::channel(); let cancel = Arc::new(AtomicBool::new(false)); let flag = Arc::clone(&cancel);
        let repaint = ctx.clone();
        self.pointcloud_ui.status = "Het gekozen bestand vergelijken met de oorspronkelijke puntenwolk…".into();
        self.pointcloud_ui.worker = Some(ImportWorker {
            rx, cancel, generation: self.project_generation, expected: Some(source.clone()),
            path: path.clone(), queued: false,
        });
        std::thread::spawn(move || {
            let result = source.relink_with_cancel(&path, &flag).map(|()| source);
            let _ = tx.send(ImportEvent::Done(result)); repaint.request_repaint();
        });
    }
    fn open_pointcloud_editor(&mut self, id: &str) {
        if !self.pointcloud_task_allowed() {
            self.message = self.pointcloud_unavailable_message().into(); return;
        }
        let source = match catalog::get(id) {
            Ok(Some(source)) => source,
            Ok(None) => { self.pointcloud_ui.status = "Deze scan is nog niet bewaard. Kies eerst Opnieuw bewaren.".into(); return; }
            Err(error) => { self.pointcloud_ui.status = error; return; }
        };
        self.pointcloud_ui.titles.remove(id);
        self.pointcloud_ui.remember_memory(source.clone());
        self.pointcloud_ui.selection.apply([id.into()], false);
        #[cfg(not(windows))]
        { let _ = source; self.pointcloud_ui.status = "De ingebouwde puntenwolkeditor vereist Windows en WebView2.".into(); }
        #[cfg(windows)]
        {
            let result = (|| {
                if self.browser_ui.host == 0 { return Err("Het hoofdvenster is nog niet gereed.".to_string()); }
                let options = self.pointcloud_editor_options()?;
                let session = Session::start_with_options(source, options)?;
                let log = tempfile::NamedTempFile::new().map_err(|error| error.to_string())?;
                let stdout = log.reopen().map_err(|error| error.to_string())?;
                let stderr = log.reopen().map_err(|error| error.to_string())?;
                use std::os::windows::process::CommandExt;
                let child = Command::new(std::env::current_exe().map_err(|error| error.to_string())?)
                    .arg("--pointcloud-webview").arg(&session.url).arg(self.browser_ui.host.to_string())
                    .stdin(Stdio::null()).stdout(stdout).stderr(stderr).creation_flags(0x08000000)
                    .spawn().map_err(|error| format!("Puntenwolkeditor openen mislukt: {error}"))?;
                Ok(Editor { session, child, source_id: id.into(), revision: 0, _log: log })
            })();
            match result {
                Ok(editor) => { self.pointcloud_ui.editor = Some(editor); self.pointcloud_ui.status = "Bewerken geopend. Plaats begin/eind en meetobjecten; sluit daarna de editor en kies Inladen.".into(); }
                Err(error) => { self.pointcloud_ui.status = error.clone(); self.message = error; }
            }
        }
    }
    fn export_pointcloud_objects(&mut self, id: &str) {
        let Some(source) = self.pointcloud_ui.source(id) else { return; };
        let features = source.annotations.iter().filter_map(|annotation| annotation.to_feature()).collect::<Vec<_>>();
        if features.is_empty() { self.pointcloud_ui.status = "Teken eerst meetobjecten in de editor.".into(); return; }
        let Some(path) = rfd::FileDialog::new().set_title("Meetobjecten exporteren")
            .add_filter("DXF", &["dxf"]).set_file_name(format!("{}-objecten.dxf", source.title)).save_file() else { return; };
        let dataset = Dataset { job_id: 0, title: source.title.clone(), source_path: source.source_path.clone(), features };
        let result = self.project_export_settings().and_then(|settings| sleufbase_rust::object_dxf::write_with_layers(
            &path, &[dataset], &[], None, &[], &settings.object_rules, settings.object_dxf_options()));
        self.pointcloud_ui.status = match result { Ok(_) => format!("Meetobjecten geëxporteerd met originele XYZ: {}", path.display()), Err(error) => error };
    }
    fn save_pointcloud_details(&mut self, id: &str, title: String, crs: CloudCrs) -> bool {
        let Some(previous) = self.pointcloud_ui.source(id).cloned() else { return false; };
        let mut source = previous.clone();
        source.title = title; source.crs = crs;
        let result = source.validate().and_then(|()| catalog::save_if_current(&source, Some(&previous)));
        match result {
            Ok(()) => {
                self.pointcloud_ui.remember_memory(source);
                self.pointcloud_ui.status = "Brongegevens bewaard in het puntenwolkoverzicht.".into();
                true
            }
            Err(error) => { self.pointcloud_ui.titles.remove(id); self.pointcloud_ui.status = error; false }
        }
    }

    pub(super) fn show_pointcloud_manager(&mut self, ctx: &egui::Context) {
        if !self.pointcloud_ui.open { return; }
        self.ensure_pointcloud_catalog();
        let can_work = self.pointcloud_task_allowed();
        let loaded = self.point_clouds.iter().map(|source| source.id.clone()).collect::<HashSet<_>>();
        let mut state = std::mem::take(&mut self.pointcloud_ui);
        let enabled = can_work && state.pending_catalog.is_none();
        let query = state.search.trim().to_lowercase();
        let mut rows = state.sources().iter().map(|source| Row::new(source, loaded.contains(&source.id)))
            .filter(|row| !state.only_loaded || row.loaded)
            .filter(|row| !state.only_selected || state.selection.ids.contains(&row.id))
            .filter(|row| query.is_empty() ||
                format!("{} {} {} {} {}", row.title, row.path, row.axis, crs_short(row.crs),
                    row.center.iter().map(|v| format!("{v:.3}")).collect::<Vec<_>>().join(" "))
                    .to_lowercase().contains(&query))
            .filter(|row| state.filters.iter().enumerate().all(|(column, filter)| {
                let filter = filter.trim().to_lowercase();
                filter.is_empty() || cloud_column_value(row, column).to_lowercase().contains(&filter)
            }))
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            let order = match state.sort_column {
                1 => a.path.to_lowercase().cmp(&b.path.to_lowercase()),
                2 => a.point_count.cmp(&b.point_count),
                3 => a.object_count.cmp(&b.object_count),
                4 => a.axis_ready.cmp(&b.axis_ready).then_with(|| a.axis.cmp(&b.axis)),
                5 => a.center[0].total_cmp(&b.center[0]).then_with(|| a.center[1].total_cmp(&b.center[1])),
                6 => crs_short(a.crs).cmp(crs_short(b.crs)),
                _ => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
            };
            (if state.descending { order.reverse() } else { order }).then_with(|| a.id.cmp(&b.id))
        });
        // Do not reselect the first scan after the user explicitly clears selection.
        let visible = rows.iter().map(|row| row.id.as_str()).collect::<HashSet<_>>();
        let locations = state.sources().iter().filter(|source| source.crs == CloudCrs::RdNew && visible.contains(source.id.as_str()))
            .map(|source| map::Location { id: source.id.clone(), title: source.title.clone(), center: center(source),
                bounds: Bounds { min_x: source.bounds_min[0], max_x: source.bounds_max[0], min_y: source.bounds_min[1], max_y: source.bounds_max[1] },
                axis: source.trench_endpoints().ok().map(|(start, end)| [start.vertices[0], end.vertices[0]]) })
            .collect::<Vec<_>>();
        let mut open = true;
        let mut action: Option<(&'static str, String)> = None;
        let mut details = None;
        let movable = !state.map_open || map::window_movable(ctx, &state.map);
        egui::Window::new("Puntenwolken · LAS / LAZ").id(egui::Id::new("pointcloud-manager"))
            .open(&mut open).default_width((ctx.screen_rect().width() - 48.).clamp(340., 1120.))
            .default_height((ctx.screen_rect().height() - 50.).clamp(260., 760.))
            .min_width(340.).min_height(240.).constrain_to(ctx.screen_rect().shrink(12.))
            .movable(movable).resizable(true).show(ctx, |ui| theme::bounded_window_body(ui, |ui| {
                settings_ui::control_style(ui);
                ui.set_max_width(ui.available_width());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.spacing_mut().item_spacing = egui::vec2(6., 4.);
                ui.spacing_mut().button_padding = egui::vec2(9., 5.);
                let visuals = ui.visuals_mut();
                for widget in [&mut visuals.widgets.inactive, &mut visuals.widgets.hovered, &mut visuals.widgets.active] {
                    widget.corner_radius = 8.into();
                }
                egui::TopBottomPanel::bottom("pointcloud-source-details")
                    .frame(theme::card(10, 8, 6)).show_inside(ui, |ui| {
                        if let Some(id) = state.selection.focused.clone() {
                            if let Some(source) = state.catalog.as_ref().and_then(|catalog| catalog.sources.iter().find(|source| source.id == id)) {
                                let title = state.titles.entry(id.clone()).or_insert_with(|| source.title.clone());
                                show_source_details(ui, source, title, loaded.contains(&id), enabled, &mut details, &mut action);
                            }
                        } else { ui.label("Selecteer een scan voor de brongegevens en coördinaten."); }
                        if !state.status.is_empty() {
                            ui.separator();
                            ui.add(egui::Label::new(egui::RichText::new(&state.status).small()).wrap());
                        }
                        if let Some(error) = &state.catalog_error { ui.colored_label(Color32::from_rgb(160, 60, 30), error); }
                        if let Some(catalog) = &state.catalog {
                            if !catalog.errors.is_empty() {
                                ui.collapsing(format!("{} bronrecords konden niet worden gelezen", catalog.errors.len()), |ui| {
                                    egui::ScrollArea::vertical().max_height(100.).show(ui, |ui| {
                                        for error in &catalog.errors { ui.small(error); }
                                    });
                                });
                            }
                        }
                        if state.pending_catalog.is_some() && ui.add_enabled(can_work, egui::Button::new("Opnieuw bewaren")).clicked() {
                            action = Some(("retry-save", String::new()));
                        }
                        if state.pending_catalog.as_ref().is_some_and(|pending| pending.expected.is_some())
                            && ui.add_enabled(can_work, egui::Button::new("Gebruik de nieuwste bewaarde versie")).clicked() {
                            action = Some(("reload-saved", String::new()));
                        }
                    });
                // Like SleufBase Jobs, keep the list/map tabs and filtering in
                // a compact toolbar, leaving the rest to virtualized rows.
                let active_filters = state.filters.iter().filter(|value| !value.trim().is_empty()).count()
                    + usize::from(state.only_loaded) + usize::from(state.only_selected)
                    + usize::from(!state.search.is_empty());
                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(!state.map_open, "Lijst").clicked() {
                        state.map_open = false; state.map.pause();
                    }
                    if ui.selectable_label(state.map_open, "Kaart").clicked() { state.map_open = true; }
                    ui.separator();
                    ui.menu_button(if active_filters == 0 { "Filter / selectie".into() }
                        else { format!("Filters ({active_filters}) / selectie") }, |ui| {
                        ui.set_max_width((ctx.screen_rect().width() - 50.).min(320.));
                        egui::ScrollArea::vertical().max_height((ctx.screen_rect().height() - 100.).max(100.)).show(ui, |ui| {
                            ui.checkbox(&mut state.only_loaded, "Alleen ingeladen scans");
                            ui.checkbox(&mut state.only_selected, "Alleen geselecteerd");
                            if ui.button("Alle zichtbare scans selecteren").clicked() {
                                state.selection.apply(rows.iter().map(|row| row.id.clone()), true);
                            }
                            if ui.button("Selectie wissen").clicked() { state.selection.clear(); }
                            ui.separator();
                            egui::ComboBox::from_id_salt("cloud-sort-column")
                                .selected_text(CLOUD_COLUMNS[state.sort_column]).show_ui(ui, |ui| {
                                for (column, name) in CLOUD_COLUMNS.iter().enumerate() {
                                    if ui.selectable_label(state.sort_column == column, *name).clicked() {
                                        state.sort_column = column; state.descending = matches!(column, 2 | 3);
                                    }
                                }
                            });
                            ui.checkbox(&mut state.descending, "Aflopend sorteren");
                            ui.separator();
                            for (column, name) in CLOUD_COLUMNS.iter().enumerate() {
                                ui.label(*name);
                                ui.add(egui::TextEdit::singleline(&mut state.filters[column])
                                    .desired_width(230.).hint_text("Filter…"));
                            }
                            if ui.button("Alle filters wissen").clicked() {
                                state.filters = Default::default();
                                state.search.clear();
                                state.only_loaded = false;
                                state.only_selected = false;
                            }
                        });
                    });
                    if ui.add_enabled(enabled, egui::Button::new("Vernieuwen")).clicked() {
                        action = Some(("refresh", String::new()));
                    }
                    if cloud_action_button(ui, "LAS/LAZ toevoegen…", enabled, true).clicked() {
                        action = Some(("choose", String::new()));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut state.search).hint_text("Zoek scans…")
                        .desired_width(ui.available_width().min(310.)));
                    if !state.search.is_empty() && ui.small_button("Wissen").clicked() { state.search.clear(); }
                    ui.label(egui::RichText::new(format!("{} / {} scans · {} geselecteerd · {} in project",
                        rows.len(), state.sources().len(), state.selection.ids.len(), loaded.len())).small().color(theme::MUTED));
                });
                if state.batch.total > 0 {
                    ui.horizontal_wrapped(|ui| {
                        ui.small(state.batch.description());
                        let running = !state.queue.is_empty() ||
                            state.worker.as_ref().is_some_and(|worker| worker.queued);
                        if running && ui.button("Wachtrij stoppen")
                            .on_hover_text("Annuleer de huidige conversie en verwijder de resterende bestanden uit de wachtrij; al bewaarde scans blijven behouden.")
                            .clicked()
                        {
                            state.queue.clear();
                            if let Some(worker) = state.worker.as_ref().filter(|worker| worker.queued) {
                                worker.cancel.store(true, Ordering::Release);
                            }
                            state.batch.total = state.batch.completed
                                + usize::from(state.worker.as_ref().is_some_and(|worker| worker.queued));
                            state.batch.stopped = true;
                            state.status = "De importwachtrij wordt gestopt. Reeds bewaarde puntenwolken blijven beschikbaar.".into();
                        }
                    });
                    if state.batch.failed > 0 {
                        ui.collapsing(format!("Importfouten ({})", state.batch.failed), |ui| {
                            egui::ScrollArea::vertical().max_height(110.).show(ui, |ui| {
                                for error in &state.batch.errors { ui.small(error); }
                                if state.batch.failed > state.batch.errors.len() {
                                    ui.small(format!("{} extra fouten niet in dit overzicht getoond.",
                                        state.batch.failed - state.batch.errors.len()));
                                }
                            });
                        });
                    }
                }
                if let Some(worker) = &state.worker {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        if worker.queued {
                            ui.label(format!("LAS/LAZ converteren · {} wachten", state.queue.len()));
                        } else {
                            ui.label("Puntenwolk voorbereiden…");
                            if ui.button("Annuleren").clicked() {
                                worker.cancel.store(true, Ordering::Release);
                            }
                        }
                    });
                } else if state.editor.is_some() {
                    ui.small("De editor staat open. Sluit hem om een scan in te laden of brongegevens te wijzigen.");
                } else if self.trench_editor.is_some() {
                    ui.small("Sluit eerst de gewone sleufbewerker en bewaar of annuleer het openstaande concept.");
                }
                ui.small("Bewerken: scan en meetobjecten aanpassen · Inladen: proefsleuf en objecten overnemen in het huidige project.");
                ui.add_space(3.);
                if state.map_open {
                    let size = egui::vec2(ui.available_width().max(1.), ui.available_height().max(80.));
                    if let Some(id) = map::show(ui, &locations, &mut state.selection, &mut state.map, size) {
                        if enabled { action = Some(("edit", id)); }
                    }
                } else if rows.is_empty() {
                    ui.add_space(24.);
                    ui.heading(if state.sources().is_empty() { "Voeg je eerste puntenwolk toe" } else { "Geen scans voor dit filter" });
                    ui.label(if state.sources().is_empty() {
                        "Scans en tekenobjecten blijven in dit overzicht beschikbaar, ook wanneer je een ander project opent."
                    } else { "Pas de zoektekst of het projectfilter aan." });
                } else { show_source_table(ui, &rows, &mut state, enabled, &mut action); }
            }));
        state.open = open;
        if !open { state.map.pause(); }
        self.pointcloud_ui = state;
        if let Some((id, title, crs)) = details {
            if !self.save_pointcloud_details(&id, title, crs) { action = None; }
        }
        if let Some((action, id)) = action {
            match action {
                "choose" => self.choose_pointcloud_files(),
                "refresh" => {
                    self.pointcloud_ui.catalog_attempted = false;
                    self.pointcloud_ui.remembered_generation = None;
                    self.pointcloud_ui.titles.clear();
                    self.ensure_pointcloud_catalog();
                }
                "retry-save" => {
                    if let Some(pending) = self.pointcloud_ui.pending_catalog.take() {
                        self.accept_catalog_source(pending.source, pending.edit, pending.expected);
                    }
                }
                "reload-saved" => {
                    if let Some(pending) = &self.pointcloud_ui.pending_catalog {
                        match catalog::get(&pending.source.id) {
                            Ok(Some(source)) => {
                                self.pointcloud_ui.pending_catalog = None;
                                self.pointcloud_ui.titles.remove(&source.id);
                                self.pointcloud_ui.remember_memory(source);
                                self.pointcloud_ui.status = "De nieuwste bewaarde versie is geladen. De niet-bewaarde lokale bronwijziging is vervallen; het bronbestand is behouden.".into();
                            }
                            Ok(None) => self.pointcloud_ui.status = "Er is nog geen bewaarde versie beschikbaar. Kies Opnieuw bewaren.".into(),
                            Err(error) => self.pointcloud_ui.status = error,
                        }
                    }
                }
                "edit" => self.open_pointcloud_editor(&id),
                "load" => self.start_pointcloud_load(&id, false),
                "load-options" => self.start_pointcloud_load(&id, true),
                "export" => self.export_pointcloud_objects(&id),
                "locate" => {
                    if let Some(source) = self.pointcloud_ui.source(&id).cloned() {
                        self.pointcloud_ui.map.fit_selection(&source); self.pointcloud_ui.map_open = true;
                    }
                }
                "remove-loaded" => {
                    self.point_clouds.retain(|source| source.id != id);
                    self.sync_pointcloud_datasets(); self.dirty = true;
                    self.pointcloud_ui.status = "Bronobjecten uit het huidige project verwijderd. De scan, meetobjecten en bestaande proefsleuven blijven bewaard.".into();
                }
                "relink" => {
                    if let Some(path) = rfd::FileDialog::new().set_title("Hetzelfde LAS/LAZ-bronbestand aanwijzen")
                        .add_filter("Puntenwolk", &["las", "laz"]).pick_file() { self.relink_pointcloud(&id, path, ctx); }
                }
                "rebuild" => {
                    if let Some(source) = self.pointcloud_ui.source(&id).cloned() {
                        self.start_pointcloud_import(ImportForm { path: source.source_path.clone(), title: source.title.clone(), crs: source.crs }, Some(source), ctx, false);
                    }
                }
                _ => {}
            }
        }
        self.show_pointcloud_import(ctx);
        self.show_pointcloud_trench(ctx);
    }

    fn show_pointcloud_import(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.pointcloud_ui.form.take() else { return; };
        let mut create = false; let mut cancel = false;
        let response = egui::Modal::new(egui::Id::new("pointcloud-import")).show(ctx, |ui| {
            ui.heading("Puntenwolk toevoegen"); ui.label(form.path.display().to_string());
            ui.label("Naam"); ui.add(egui::TextEdit::singleline(&mut form.title).char_limit(1024));
            ui.label("Coördinatenstelsel"); crs_combo(ui, "import-crs", &mut form.crs);
            ui.label("RD New gebruikt meters; WGS 84 gebruikt geografische lengte-/breedtegraad. SleufBase herprojecteert de originele XYZ niet. Alleen RD New kan op de Nederlandse kaart worden ingeladen.");
            ui.small("De scan wordt opgeslagen in het puntenwolkoverzicht. Kies later Bewerken om punten of lijnen te tekenen; met Inladen neem je de proefsleuf over in een project.");
            let valid = !form.title.trim().is_empty() && form.title.len() <= 1024 && !form.title.chars().any(char::is_control);
            ui.horizontal(|ui| {
                if ui.add_enabled(valid, egui::Button::new("Puntenwolk toevoegen").fill(ORANGE)).clicked() { create = true; }
                if ui.button("Annuleren").clicked() { cancel = true; }
            });
        });
        if create { self.start_pointcloud_import(form, None, ctx, false); }
        else if !cancel && !response.should_close() { self.pointcloud_ui.form = Some(form); }
    }

    fn start_pointcloud_load(&mut self, id: &str, show_options: bool) {
        if !self.pointcloud_task_allowed() { self.message = self.pointcloud_unavailable_message().into(); return; }
        let source = match catalog::get(id) {
            Ok(Some(source)) => source,
            Ok(None) => { self.pointcloud_ui.status = "Bewaar de scan eerst in het puntenwolkoverzicht.".into(); return; }
            Err(error) => { self.pointcloud_ui.status = error; return; }
        };
        self.pointcloud_ui.titles.remove(id);
        self.pointcloud_ui.remember_memory(source.clone());
        let existing = self.virtual_trenches.iter().find(|trench| same_source(trench, id));
        let axis_id = if endpoint_count(&source) == (0, 0) {
            // A line is preselected only when an earlier explicit load recorded
            // that exact annotation as the axis. A lone cable is never guessed.
            existing.and_then(|trench| trench.metadata.get("pointcloud_axis_id")).and_then(serde_json::Value::as_str)
                .filter(|id| legacy_axes(&source).any(|axis| axis.id == *id)).unwrap_or_default().to_owned()
        } else { String::new() };
        let selected_ids = source.annotations.iter().filter(|item| item.is_object() && item.id != axis_id)
            .map(|item| item.id.clone()).collect();
        let (mode, label, width) = if let Some(trench) = existing {
            (LoadMode::Update(trench.label.clone()), trench.label.clone(), trench.width_meters)
        } else { (LoadMode::New, unique_label(&source.title, &self.virtual_trenches), 0.6) };
        let mut form = TrenchForm { source, generation: self.project_generation, axis_id,
            selected_ids, label, width, mode, preview: None, error: None };
        let duplicates = self.virtual_trenches.iter().filter(|trench| same_source(trench, id)).count();
        if !show_options && form.source.trench_endpoints().is_ok() && duplicates <= 1 {
            let ids = form.selected_ids.iter().cloned().collect::<Vec<_>>();
            match sleufbase_rust::pointcloud_trench::prepare(
                &form.source, &form.axis_id, &ids, form.label.clone(), form.width) {
                Ok(prepared) => match self.commit_pointcloud_trench(&form, prepared) {
                    Ok(()) => return,
                    Err(error) => form.error = Some(error),
                },
                Err(error) => form.error = Some(error),
            }
        }
        // Invalid/missing endpoints or ambiguous duplicates still use the
        // options dialog, where the user can edit the missing geometry.
        self.pointcloud_ui.trench = Some(form);
    }


    /// Finalize a Potree-drawn trench directly on the SleufBase map. The
    /// regular virtual editor is still available explicitly from the map.
    fn commit_pointcloud_trench(&mut self, form:&TrenchForm,
        prepared:sleufbase_rust::pointcloud_trench::Prepared)->Result<(),String> {
        if !self.pointcloud_task_allowed() || form.generation!=self.project_generation {
            return Err("Het project is intussen gewijzigd. Open Inladen opnieuw.".into());
        }
        let (index, trench) = match &form.mode {
            LoadMode::New => (self.virtual_trenches.len(), prepared.trench),
            LoadMode::Update(label) => {
                let index = self.virtual_trenches.iter().position(|trench| &trench.label == label && same_source(trench, &form.source.id))
                    .ok_or("De bestaande proefsleuf is intussen gewijzigd. Open Inladen opnieuw.")?;
                (index, merge_trench(&self.virtual_trenches[index], prepared.trench)?)
            }
        };
        if self.virtual_trenches.iter().enumerate().any(|(other, old)| other != index && old.label == trench.label) {
            return Err("Er bestaat al een proefsleuf met deze naam.".into());
        }
        if !self.point_clouds.iter().any(|source| source.id == form.source.id) && self.point_clouds.len() >= pointcloud::MAX_SOURCES {
            return Err("Dit project bevat het maximumaantal puntenwolken. Gebruik een ander project.".into());
        }
        if index == self.virtual_trenches.len() { self.virtual_trenches.push(trench); } else { self.virtual_trenches[index] = trench; }
        if let Some(source) = self.point_clouds.iter_mut().find(|source| source.id == form.source.id) { *source = form.source.clone(); }
        else { self.point_clouds.push(form.source.clone()); }
        self.sync_pointcloud_datasets();
        self.trench_markers = self.virtual_trenches.iter().map(sleufbase_rust::cross_section::markers).collect();
        // Keep the source trench for cross-sections/DXF but never select its
        // synthetic virtual outline: the orthophoto is the one map layer.
        self.refresh_virtual_references(); self.virtual_map.selected = Some(index); self.dirty = true;
        // The cloud editor already supplied a complete measured trench. Centre
        // the real map and leave the separate virtual editor closed.
        let placed=&self.virtual_trenches[index];
        self.navigation.fit_bounds=Some(Bounds {
            min_x:placed.start.x.min(placed.end.x)-2., max_x:placed.start.x.max(placed.end.x)+2.,
            min_y:placed.start.y.min(placed.end.y)-2., max_y:placed.start.y.max(placed.end.y)+2.,
        });
        self.pointcloud_ui.status = "Proefsleuf direct op de kaart geplaatst. Sla het project op.".into();
        self.pointcloud_ui.open = false; self.pointcloud_ui.map.pause();
        if let Err(error)=self.register_cloud_geotiff(&form.source) {
            self.pointcloud_ui.status=format!("Proefsleuf geplaatst, maar GeoTIFF laden mislukt: {error}");
        }
        if !sleufbase_rust::pointcloud_ortho::is_current(&form.source) {
            self.start_ortho_worker(form.source.clone());
        }

        Ok(())
    }

    /// Register a generated, annotation-free overview raster alongside the
    /// placed trench. The original LAS/LAZ is never modified.
    fn register_cloud_geotiff(&mut self, source:&PointCloudSource)->Result<bool,String> {
        if source.crs!=CloudCrs::RdNew {return Ok(false);}
        let path=sleufbase_rust::pointcloud_ortho::path_for(source)?;
        if !sleufbase_rust::pointcloud_ortho::is_current(source) {return Ok(false);}
        let mut raster=sleufbase_rust::geotiff::read_geotiff_metadata(&path)?;
        if raster.epsg!=Some(28992) {return Err("GeoTIFF heeft geen RD New-coördinaten.".into());}
        raster.metadata=serde_json::json!({
            "pointcloud_source_id":source.id,
            "pointcloud_title":source.title,
            "pointcloud_generated_orthoview":true
        });
        // Keep a single physical GeoTIFF per pointcloud source, even if an
        // older project still refers to the generic "orthoview.tif".
        let old_virtual=self.virtual_map.selected;
        let old_raster=self.raster_points.selected.clone();
        let cloud_selected=old_virtual.and_then(|i|self.virtual_trenches.get(i))
            .is_some_and(|trench|same_source(trench,&source.id));
        let same_cloud=|candidate:&sleufbase_rust::geotiff::GeoTiffInfo| {
            candidate.path==path || candidate.metadata.get("pointcloud_source_id")
                .and_then(serde_json::Value::as_str)==Some(source.id.as_str())
        };
        if let Some(index)=self.geotiffs.iter().position(same_cloud) {
            let previous_path=self.geotiffs[index].path.clone();
            self.geotiffs[index]=raster;
            if let Some(texture)=self.raster_textures.get_mut(index) {*texture=None;}
            if self.raster_points.selected.as_ref()==Some(&previous_path) {
                self.raster_points.selected=Some(path.clone());
            }
            // A prior save could contain two physical entries for one scan.
            for other in (0..self.geotiffs.len()).rev() {
                if other!=index && same_cloud(&self.geotiffs[other]) {
                    self.geotiffs.remove(other);
                    if other<self.raster_textures.len(){self.raster_textures.remove(other);}
                }
            }
        } else {
            self.geotiffs.push(raster);
            self.raster_textures.push(None);
        }
        // Display the orthophoto immediately after generation. Reset both
        // preview pipelines, otherwise a new/updated TIFF is listed but the
        // map keeps showing its previous or empty cached texture.
        self.navigation.raster_crops.clear();
        if let Some(loader)=&mut self.viewport_loader {loader.update(&[]);}
        self.schedule_raster_previews();
        if cloud_selected || old_raster.as_ref()==Some(&path) {
            self.virtual_map.selected=None;
            self.raster_points.selected=Some(path);
            self.update_view_selection(old_virtual,old_raster.as_deref());
        }
        self.dirty=true;
        Ok(true)
    }
    fn start_ortho_worker(&mut self, source:PointCloudSource) {
        if self.pointcloud_ui.ortho.is_some() || source.crs!=CloudCrs::RdNew {return;}
        let (tx,rx)=mpsc::channel();
        let cancel=Arc::new(AtomicBool::new(false));
        let flag=Arc::clone(&cancel);
        let id=source.id.clone();
        self.pointcloud_ui.ortho=Some(OrthoWorker{
            rx,cancel,generation:self.project_generation,source_id:id,
        });
        self.pointcloud_ui.status="Proefsleuf geplaatst; GeoTIFF wordt op de begin-/eindrichting georiënteerd…".into();
        std::thread::spawn(move || {
            let result=source.validate_cache().and_then(|path|
                sleufbase_rust::pointcloud_ortho::generate(&path,&source,&flag));
            let _=tx.send(result);
        });
    }

    fn show_pointcloud_trench(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.pointcloud_ui.trench.take() else { return; };
        if form.generation != self.project_generation { return; }
        let targets = self.virtual_trenches.iter().filter(|trench| same_source(trench, &form.source.id))
            .map(|trench| (trench.label.clone(), trench.width_meters,
                trench.objects.iter().filter(|object| object.metadata.get("pointcloud_annotation_id").is_none()).count(),
                trench.alignment.is_some())).collect::<Vec<_>>();
        let mut create = false; let mut cancel = false; let mut edit = false;
        let mut changed = false;
        let response = egui::Modal::new(egui::Id::new("pointcloud-load")).show(ctx, |ui| {
            ui.set_max_width(680.);
            ui.heading("Puntenwolk inladen als proefsleuf");
            ui.label(egui::RichText::new(&form.source.title).strong());
            ui.small("De begin- en eindhoogte vormen het maaiveld. Meetobjecten behouden hun broncoördinaten en gekozen objecttype.");
            ui.separator();
            match form.source.trench_endpoints() {
                Ok((start, end)) => {
                    ui.colored_label(Color32::from_rgb(25, 120, 85), "Begin- en eindpunt gevonden — sleufas wordt automatisch gebruikt.");
                    for (label, point) in [("Begin", start.vertices[0]), ("Eind", end.vertices[0])] {
                        ui.monospace(format!("{label:5} X {:.3}   Y {:.3}   Z {:.3}", point.x, point.y, point.z.unwrap_or(0.)));
                    }
                }
                Err(error) => {
                    let axes = if endpoint_count(&form.source) == (0, 0) { legacy_axes(&form.source).collect::<Vec<_>>() } else { Vec::new() };
                    if axes.len() == 1 {
                        ui.label("Deze tekening bevat geen expliciet begin/eind. Je kunt een bestaande rechte lijn als sleufas aanwijzen.");
                        let mut use_line = form.axis_id == axes[0].id;
                        if ui.checkbox(&mut use_line, format!("Gebruik '{}' als bestaande sleufas", axes[0].name)).changed() {
                            form.axis_id = if use_line { axes[0].id.clone() } else { String::new() }; changed = true;
                        }
                        ui.small("Kies dit alleen als deze lijn de sleufas is. Gebruik anders Beginpunt plaatsen en Eindpunt plaatsen in Bewerken.");
                    } else if !axes.is_empty() {
                        ui.label("Deze oudere tekening heeft meerdere rechte lijnen. Kies de bestaande sleufas, of gebruik Beginpunt plaatsen en Eindpunt plaatsen in Bewerken.");
                        egui::ScrollArea::vertical().id_salt("legacy-cloud-axis").max_height(100.).show(ui, |ui| {
                            for axis in axes {
                                changed |= ui.radio_value(&mut form.axis_id, axis.id.clone(), &axis.name).changed();
                            }
                        });
                    } else {
                        ui.colored_label(Color32::from_rgb(160, 60, 30), error);
                        ui.label("Open Bewerken, kies Beginpunt plaatsen en klik op het begin. Kies daarna Eindpunt plaatsen en klik op het eind. Sla op en sluit; Inladen herkent deze punten vanzelf.");
                    }
                }
            }
            if ui.add_enabled(self.pointcloud_task_allowed(), egui::Button::new("Begin/eind of objecten aanpassen in Bewerken…")).clicked() { edit = true; }
            if !targets.is_empty() {
                ui.separator();
                ui.label("Deze scan is al in dit project gebruikt.");
                let updating = matches!(form.mode, LoadMode::Update(_));
                if ui.radio(updating, "Bestaande proefsleuf bijwerken").clicked() && !updating {
                    form.mode = LoadMode::Update(targets[0].0.clone()); form.label = targets[0].0.clone(); form.width = targets[0].1; changed = true;
                }
                if let LoadMode::Update(target) = &mut form.mode {
                    if targets.len() > 1 {
                        egui::ComboBox::from_id_salt("pointcloud-update-target").selected_text(target.as_str()).show_ui(ui, |ui| {
                            for (label, width, _, _) in &targets {
                                if ui.selectable_value(target, label.clone(), label).changed() {
                                    form.label = label.clone(); form.width = *width; changed = true;
                                }
                            }
                        });
                    }
                    let manual = targets.iter().find(|item| &item.0 == target).map_or(0, |item| item.2);
                    ui.small(format!("Begin/eind, bronposities, objecttypen en materiaalgegevens worden vernieuwd. Oude brongebonden diepte-, kleur- en aantalaanpassingen vervallen. {manual} handmatig toegevoegde objecten, dekbanden en vrije metadata blijven bewaard."));
                    if targets.iter().any(|item| &item.0 == target && item.3) {
                        ui.small("Een eerdere handmatige asverplaatsing vervalt als de bronas is gewijzigd. Kies een nieuwe kopie om de bestaande sleuf volledig te behouden.");
                    }
                }
                if ui.radio(matches!(form.mode, LoadMode::New), "Nieuwe kopie toevoegen").clicked() && !matches!(form.mode, LoadMode::New) {
                    form.mode = LoadMode::New; form.label = unique_label(&form.source.title, &self.virtual_trenches); changed = true;
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Naam proefsleuf");
                changed |= ui.add(egui::TextEdit::singleline(&mut form.label).char_limit(256).desired_width(280.)).changed();
                ui.label("Breedte (m)"); changed |= ui.add(egui::DragValue::new(&mut form.width).range(0.1..=100.).speed(0.05)).changed();
            });
            ui.horizontal(|ui| {
                ui.label("Objecten in de doorsnede");
                if ui.small_button("Alles").clicked() {
                    form.selected_ids = form.source.annotations.iter().filter(|item| item.is_object() && item.id != form.axis_id).map(|item| item.id.clone()).collect(); changed = true;
                }
                if ui.small_button("Geen").clicked() { form.selected_ids.clear(); changed = true; }
            });
            let objects = form.source.annotations.iter().filter(|item| item.is_object() && item.id != form.axis_id).collect::<Vec<_>>();
            if objects.is_empty() { ui.small("Nog geen meetobjecten. Je kunt wel de sleufas inladen en later meetobjecten toevoegen."); }
            egui::ScrollArea::vertical().id_salt("cloud-load-objects").max_height(160.).show_rows(ui, 23., objects.len(), |ui, range| {
                for index in range {
                    let item = objects[index];
                    let mut selected = form.selected_ids.contains(&item.id);
                    let label = if item.display_name() == item.name { format!("{} · {}", item.name, if item.kind == AnnotationKind::Point { "punt" } else { "lijn" }) }
                        else { format!("{} · {}", item.display_name(), item.name) };
                    if ui.checkbox(&mut selected, label).changed() {
                        if selected { form.selected_ids.insert(item.id.clone()); } else { form.selected_ids.remove(&item.id); }
                        changed = true;
                    }
                }
            });
            if changed { form.preview = None; form.error = None; }
            if form.preview.is_none() {
                let ids = form.selected_ids.iter().cloned().collect::<Vec<_>>();
                form.preview = Some(sleufbase_rust::pointcloud_trench::prepare(&form.source, &form.axis_id, &ids, form.label.clone(), form.width));
            }
            let duplicate = self.virtual_trenches.iter().any(|trench| trench.label == form.label.trim()
                && !matches!(&form.mode, LoadMode::Update(target) if &trench.label == target && same_source(trench, &form.source.id)));
            match form.preview.as_ref() {
                Some(Ok(prepared)) => {
                    ui.label(format!("{} kabel-/leidingpunten worden ingeladen; {} geselecteerde objecten liggen buiten de sleuf of kruisen de as niet.",
                        prepared.trench.objects.len(), prepared.skipped));
                }
                Some(Err(error)) => { ui.colored_label(Color32::from_rgb(160, 60, 30), error); }
                None => {}
            }
            if duplicate { ui.colored_label(Color32::from_rgb(160, 60, 30), "Er bestaat al een andere proefsleuf met deze naam. Kies een andere naam."); }
            if let Some(error) = &form.error { ui.colored_label(Color32::from_rgb(160, 60, 30), error); }
            ui.small("De vinkjes bepalen alleen welke objecten in het profiel komen. De volledige tekening wordt als bronlaag in het project opgeslagen en blijft ook in het overzicht bewaard. Van lijnen gebruikt het profiel de echte askruisingen met geïnterpoleerde hoogte.");
            ui.horizontal(|ui| {
                let ready = form.preview.as_ref().is_some_and(Result::is_ok) && !duplicate && self.pointcloud_task_allowed();
                let label = if matches!(form.mode, LoadMode::Update(_)) { "Bijwerken en inladen" } else { "Inladen in project" };
                if ui.add_enabled(ready, egui::Button::new(label).fill(ORANGE)).clicked() { create = true; }
                if ui.button("Annuleren").clicked() { cancel = true; }
            });
        });
        if edit { self.open_pointcloud_editor(&form.source.id); return; }
        if create {
            let result = form.preview.take()
                .unwrap_or_else(|| Err("De inlaadvoorvertoning ontbreekt".into()));
            match result.and_then(|prepared| self.commit_pointcloud_trench(&form, prepared)) {
                Ok(()) => {}
                Err(error) => { form.error = Some(error); form.preview = None; self.pointcloud_ui.trench = Some(form); }
            }
        } else if !cancel && !response.should_close() { self.pointcloud_ui.trench = Some(form); }
    }
}

fn show_source_details(ui: &mut egui::Ui, source: &PointCloudSource, title: &mut String,
    loaded: bool, enabled: bool, details: &mut Option<(String, String, CloudCrs)>, action: &mut Option<(&'static str, String)>) {
    ui.horizontal_wrapped(|ui| {
        ui.strong("Geselecteerde scan");
        cloud_badge(ui, if loaded { "In dit project" } else { "Bewaard" }, loaded);
        if cloud_action_button(ui, "Inladen", enabled, true).clicked() { *action = Some(("load", source.id.clone())); }
        if cloud_action_button(ui, "Bewerken", enabled, false).clicked() { *action = Some(("edit", source.id.clone())); }
        ui.add_enabled_ui(enabled, |ui| {
            ui.menu_button("Meer", |ui| {
                for (label, key) in [("Meetobjecten naar DXF…", "export"), ("Bronbestand opnieuw kiezen…", "relink"), ("Weergave opnieuw opbouwen", "rebuild")] {
                    if ui.button(label).clicked() { *action = Some((key, source.id.clone())); ui.close_menu(); }
                }
                if ui.add_enabled(enabled, egui::Button::new("Inladen met opties…"))
                    .on_hover_text("Kies zelf de sleufbreedte, objecten en bijwerken of kopiëren.")
                    .clicked() { *action = Some(("load-options", source.id.clone())); ui.close_menu(); }
                if ui.add_enabled(source.crs == CloudCrs::RdNew, egui::Button::new("Op kaart tonen")).clicked() { *action = Some(("locate", source.id.clone())); ui.close_menu(); }
                if loaded {
                    ui.separator();
                    if ui.button("Bronobjecten uit huidig project verwijderen")
                        .on_hover_text("De scan en tekenobjecten blijven in het overzicht. Bestaande proefsleuven blijven in dit project.").clicked() {
                        *action = Some(("remove-loaded", source.id.clone())); ui.close_menu();
                    }
                }
            });
        });
    });
    let mut crs = source.crs;
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label("Naam");
            let response = ui.add(egui::TextEdit::singleline(title).char_limit(1024).desired_width(280.));
            let valid = !title.trim().is_empty() && title.len() <= 1024 && !title.chars().any(char::is_control);
            let commit = response.lost_focus() || (response.has_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
            if !valid {
                ui.small("De vorige geldige naam blijft bewaard.");
                if response.lost_focus() { *title = source.title.clone(); }
            } else if commit && *title != source.title { *details = Some((source.id.clone(), title.clone(), crs)); }
            if crs_combo(ui, "selected-cloud-crs", &mut crs) {
                *details = Some((source.id.clone(), if valid { title.clone() } else { source.title.clone() }, crs));
            }
        });
    });
    let position = center(source);
    ui.horizontal_wrapped(|ui| {
        ui.small(format!("Centrum X {:.3} · Y {:.3} · Z {:.3}", position[0], position[1], position[2]));
        ui.small(format!("{} scanpunten · {} meetobjecten · {}", count(source.point_count), source.annotations.iter().filter(|item| item.is_object()).count(), axis_status(source).0));
    });
    let path = source.source_path.display().to_string();
    ui.add(egui::Label::new(egui::RichText::new(&path).small()).truncate()).on_hover_text(&path);
    if source.crs != CloudCrs::RdNew {
        ui.small(if source.crs == CloudCrs::Wgs84 {
            "WGS 84: X/Y zijn geografische graden. Bekijken, tekenen en object-DXF behouden de broncoördinaten. Inladen op de RD-kaart vereist omzetting naar RD New buiten deze keuze."
        } else {
            "Bewerken en DXF-export behouden de broncoördinaten. Inladen en de kaart vereisen RD New in meters."
        });
    }
}

// The cloud catalog shares the compact job-list visual grammar: responsive
// columns, a single fixed header and virtualized rounded rows with an orange
// selection rail. It intentionally does not require a horizontally scrolling
// 1100-pixel table on smaller displays.
const CLOUD_COLUMNS: [&str; 7] = [
    "Naam", "Bronbestand", "Scanpunten", "Objecten", "Sleufstatus", "Coördinaten", "CRS",
];
const CLOUD_ROW_HEIGHT: f32 = 36.;
const CLOUD_COLUMN_GAP: f32 = 6.;

fn cloud_columns_for_width(width: f32) -> Vec<(usize, f32)> {
    let visible = |column| match column {
        0 | 4 => true,
        1 => width >= 490.,
        5 => width >= 700.,
        2 => width >= 800.,
        3 => width >= 1040.,
        6 => width >= 1090.,
        _ => false,
    };
    let mut columns = (0..CLOUD_COLUMNS.len()).filter(|&column| visible(column))
        .map(|column| (column, match column {
            2 => 82., 3 => 72., 4 => if width < 540. { 114. } else { 138. },
            5 => 188., 6 => 94., _ => 0.,
        })).collect::<Vec<_>>();
    let free = (width - 44. - CLOUD_COLUMN_GAP * (columns.len() - 1) as f32
        - columns.iter().map(|(_, value)| *value).sum::<f32>()).max(60.);
    let has_path = columns.iter().any(|(column, _)| *column == 1);
    for (column, column_width) in &mut columns {
        match *column {
            0 => *column_width = if has_path { free * 0.55 } else { free },
            1 => *column_width = free * 0.45,
            _ => {}
        }
    }
    columns
}
fn cloud_column_value(item: &Row, column: usize) -> String {
    match column {
        0 => item.title.clone(),
        1 => std::path::Path::new(&item.path).file_name()
            .map(|value| value.to_string_lossy().into_owned()).unwrap_or_else(|| item.path.clone()),
        2 => count(item.point_count),
        3 => count(item.object_count as u64),
        4 => item.axis.clone(),
        5 => format!("{:.1} · {:.1} · {:.1}", item.center[0], item.center[1], item.center[2]),
        6 => crs_short(item.crs).into(),
        _ => String::new(),
    }
}
fn cloud_badge(ui: &mut egui::Ui, text: &str, loaded: bool) {
    let background = if loaded { Color32::from_rgb(232, 246, 238) } else { Color32::from_rgb(238, 242, 247) };
    let foreground = if loaded { Color32::from_rgb(35, 114, 74) } else { theme::MUTED };
    egui::Frame::new().fill(background).corner_radius(7)
        .inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(text).size(12.).color(foreground)).selectable(false));
        });
}
fn cloud_action_button(ui: &mut egui::Ui, label: &str, enabled: bool, primary: bool) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(egui::RichText::new(label).strong()
        .color(if primary { Color32::WHITE } else { INK }))
        .fill(if primary { ORANGE } else { Color32::WHITE })
        .stroke(Stroke::new(1., if primary { ORANGE } else { theme::BORDER }))
        .corner_radius(8).min_size(egui::vec2(0., 30.)))
}
fn show_cloud_header(ui: &mut egui::Ui, width: f32, columns: &[(usize, f32)],
    state: &mut State, visible_ids: &[String], inline_filters: bool) {
    let height = if inline_filters { 56. } else { 28. };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    ui.painter_at(rect).rect_filled(rect, 6., Color32::WHITE);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt("cloud-column-headings"));
    let mut all = !visible_ids.is_empty() && visible_ids.iter().all(|id| state.selection.ids.contains(id));
    if child.put(egui::Rect::from_min_size(rect.min + egui::vec2(8., 3.), egui::vec2(22., 22.)),
        egui::Checkbox::without_text(&mut all))
        .on_hover_text("Alle zichtbare scans selecteren of deselecteren").changed() {
        if all { state.selection.apply(visible_ids.iter().cloned(), true); }
        else {
            for id in visible_ids { state.selection.ids.remove(id); }
            state.selection.reconcile();
        }
    }
    let mut x = rect.left() + 36.;
    for &(column, column_width) in columns {
        let arrow = if state.sort_column == column {
            if state.descending { " ▼" } else { " ▲" }
        } else { "" };
        let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(column_width, 27.));
        let text = egui::RichText::new(format!("{}{arrow}", CLOUD_COLUMNS[column]))
            .strong().size(12.).color(if state.sort_column == column { ORANGE } else { INK });
        if child.put(cell, egui::Button::new(text).frame(false)
            .wrap_mode(egui::TextWrapMode::Truncate))
            .on_hover_text(format!("Sorteren op {}", CLOUD_COLUMNS[column])).clicked() {
            if state.sort_column == column { state.descending = !state.descending; }
            else { state.sort_column = column; state.descending = matches!(column, 2 | 3); }
        }
        if inline_filters {
            let filter = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 29.), egui::vec2(column_width, 24.));
            child.put(filter, egui::TextEdit::singleline(&mut state.filters[column])
                .id(egui::Id::new(("pointcloud-filter", column)))
                .desired_width(column_width).hint_text("Filter…").margin(egui::vec2(5., 3.)));
        }
        x += column_width + CLOUD_COLUMN_GAP;
    }
}
fn show_cloud_card(ui: &mut egui::Ui, item: &Row, selected: bool,
    striped: bool, width: f32, columns: &[(usize, f32)]) -> (egui::Response, Option<bool>) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, CLOUD_ROW_HEIGHT), egui::Sense::hover());
    let response = ui.interact(rect, ui.make_persistent_id(("cloud-card", item.id.as_str())), egui::Sense::click());
    let fill = if selected { Color32::from_rgb(255, 239, 226) }
        else if response.hovered() { Color32::from_rgb(242, 245, 250) }
        else if striped { theme::SURFACE_ALT } else { Color32::WHITE };
    let border = if selected { Color32::from_rgb(245, 178, 125) } else { theme::BORDER };
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6., fill);
    painter.rect_stroke(rect, 6., Stroke::new(1., border), egui::StrokeKind::Inside);
    if selected {
        painter.rect_filled(egui::Rect::from_min_size(rect.min + egui::vec2(2., 8.), egui::vec2(3., 20.)), 2., ORANGE);
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect)
        .id_salt(("cloud-card-content", item.id.as_str()))
        .layout(egui::Layout::left_to_right(egui::Align::Center)));
    let mut checked = selected;
    let check = child.put(egui::Rect::from_min_size(rect.min + egui::vec2(8., 7.), egui::vec2(22., 22.)),
        egui::Checkbox::without_text(&mut checked));
    let mut x = rect.left() + 36.;
    for &(column, column_width) in columns {
        let cell = egui::Rect::from_min_size(egui::pos2(x, rect.top() + 4.), egui::vec2(column_width, 28.));
        let value = cloud_column_value(item, column);
        if column == 0 {
            let text_rect = if item.loaded {
                painter.circle_filled(egui::pos2(cell.left() + 4., cell.center().y), 3., Color32::from_rgb(35, 114, 74));
                egui::Rect::from_min_max(cell.min + egui::vec2(12., 0.), cell.max)
            } else { cell };
            child.put(text_rect, egui::Label::new(egui::RichText::new(&value).strong().size(13.))
                .truncate().halign(egui::Align::Min).selectable(false)).on_hover_text(&value);
        } else if column == 4 {
            let (background, foreground) = if item.axis_ready {
                (Color32::from_rgb(232, 246, 238), Color32::from_rgb(35, 114, 74))
            } else {
                (Color32::from_rgb(255, 245, 225), Color32::from_rgb(149, 94, 24))
            };
            painter.rect_filled(cell.shrink2(egui::vec2(0., 2.)), 5., background);
            child.put(cell.shrink2(egui::vec2(6., 0.)),
                egui::Label::new(egui::RichText::new(&value).size(12.).color(foreground))
                    .truncate().halign(egui::Align::Min).selectable(false)).on_hover_text(&value);
        } else {
            child.put(cell, egui::Label::new(egui::RichText::new(if value.is_empty() { "—" } else { &value })
                .color(if column == 1 { INK } else { theme::MUTED }))
                .truncate().halign(egui::Align::Min).selectable(false)).on_hover_text(&value);
        }
        x += column_width + CLOUD_COLUMN_GAP;
    }
    (response.on_hover_text(format!("{}\nBestand: {}\nX {:.3} · Y {:.3} · Z {:.3}\n{}\n{}",
        item.title, item.path, item.center[0], item.center[1], item.center[2],
        item.axis, if item.loaded { "In dit project" } else { "Bewaard in overzicht" })), check.changed().then_some(checked))
}
fn show_source_table(ui: &mut egui::Ui, rows: &[Row], state: &mut State, enabled: bool,
    action: &mut Option<(&'static str, String)>) {
    let width = (ui.available_width() - ui.spacing().scroll.allocated_width()).max(1.);
    let columns = cloud_columns_for_width(width);
    let inline_filters = width >= 680. && ui.available_height() >= 240.;
    let ordered = rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>();
    show_cloud_header(ui, width, &columns, state, &ordered, inline_filters);
    ui.spacing_mut().item_spacing.y = 2.;
    egui::ScrollArea::vertical().id_salt("pointcloud-catalog-vertical")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .max_height(ui.available_height().max(1.)).auto_shrink([false, false])
        .show_rows(ui, CLOUD_ROW_HEIGHT, rows.len(), |ui, range| {
            for index in range {
                let item = &rows[index];
                let (response, checked) = show_cloud_card(ui, item,
                    state.selection.ids.contains(&item.id), index % 2 != 0, width, &columns);
                let extend = ui.input(|input| input.modifiers.ctrl || input.modifiers.command || input.modifiers.shift);
                if let Some(checked) = checked {
                    if checked && extend { state.selection.select_row(&ordered, index, true); }
                    else { state.selection.set_checked(&item.id, checked); }
                } else if response.clicked() { state.selection.select_row(&ordered, index, extend); }
                if enabled && response.double_clicked() {
                    state.selection.apply([item.id.clone()], false);
                    *action = Some(("edit", item.id.clone()));
                }
            }
        });
}

#[cfg(test)]
mod pointcloud_selection_tests {
    use super::Selection;
    #[test]
    fn map_and_list_selection_share_additive_and_range_rules() {
        let mut selection = Selection::default();
        let ordered = ["one", "two", "three", "four"].map(str::to_owned);
        selection.apply(["two".to_owned(), "three".to_owned()], false);
        assert_eq!(selection.ids.len(), 2);
        selection.apply(["four".to_owned()], true);
        assert_eq!(selection.ids.len(), 3);
        selection.select_row(&ordered, 0, false);
        assert_eq!(selection.ids.len(), 1);
        selection.select_row(&ordered, 2, true);
        assert!(["one", "two", "three"].iter().all(|id| selection.ids.contains(*id)));
        selection.set_checked("two", false);
        assert!(!selection.ids.contains("two"));
        selection.clear();
        assert!(selection.ids.is_empty());
        assert!(selection.focused.is_none());
    }
}

fn same_source(trench: &VirtualTrench, id: &str) -> bool {
    trench.metadata.get("pointcloud_source_id").and_then(serde_json::Value::as_str) == Some(id)
}
fn unique_label(title: &str, trenches: &[VirtualTrench]) -> String {
    let base = title.trim().chars().scan(0, |bytes, ch| {
        *bytes += ch.len_utf8(); (*bytes <= 220).then_some(ch)
    }).collect::<String>();
    let base = if base.is_empty() { "Proefsleuf" } else { &base };
    let names = trenches.iter().map(|trench| trench.label.as_str()).collect::<HashSet<_>>();
    if !names.contains(base) { return base.into(); }
    let mut suffix = 2;
    loop {
        let label = format!("{base} ({suffix})");
        if !names.contains(label.as_str()) { return label; }
        suffix += 1;
    }
}
fn merged_metadata(previous: &serde_json::Value, next: &serde_json::Value, renew_point: bool) -> serde_json::Value {
    let mut merged = previous.as_object().cloned().unwrap_or_default();
    merged.retain(|key, _| !key.starts_with("pointcloud_") && key != "crossing_angle_degrees"
        && !(renew_point && matches!(key.as_str(), "display_rgb" | "surface_mode" | "surface_z" | "surface_status"
            | "sleufbase_depth_meters" | "template_reference_point" | "quantity")));
    if let Some(values) = next.as_object() { merged.extend(values.clone()); }
    serde_json::Value::Object(merged)
}
fn merge_trench(previous: &VirtualTrench, mut next: VirtualTrench) -> Result<VirtualTrench, String> {
    next.metadata = merged_metadata(&previous.metadata, &next.metadata, false);
    next.start_metadata = merged_metadata(&previous.start_metadata, &next.start_metadata, true);
    next.end_metadata = merged_metadata(&previous.end_metadata, &next.end_metadata, true);
    next.job_id = previous.job_id;
    next.dekband_rows = previous.dekband_rows.clone();
    next.boundary_3d = previous.boundary_3d.clone();
    if previous.start == next.start && previous.end == next.end { next.alignment = previous.alignment.clone(); }
    let mut matched = HashSet::new();
    for object in &mut next.objects {
        let id = object.metadata.get("pointcloud_annotation_id").and_then(serde_json::Value::as_str);
        let closest = previous.objects.iter().enumerate().filter(|(index, old)| !matched.contains(index)
            && id.is_some() && old.metadata.get("pointcloud_annotation_id").and_then(serde_json::Value::as_str) == id)
            .min_by(|(_, a), (_, b)| {
                let distance = |old: &sleufbase_rust::virtual_trench::TrenchPoint| {
                    (old.position.x - object.position.x).hypot(old.position.y - object.position.y)
                };
                distance(a).total_cmp(&distance(b))
            });
        if let Some((index, old)) = closest {
            object.metadata = merged_metadata(&old.metadata, &object.metadata, true); matched.insert(index);
        }
    }
    next.objects.extend(previous.objects.iter().filter(|object| object.metadata.get("pointcloud_annotation_id").is_none()).cloned());
    next.validate().map_err(|error| format!("De bestaande aanvullende gegevens passen niet bij de vernieuwde sleuf: {error}. Kies een nieuwe kopie om de oude sleuf te behouden."))?;
    Ok(next)
}
