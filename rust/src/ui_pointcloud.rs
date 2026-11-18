//! Persistent LAS/LAZ overview, editor ownership and deliberate project loading.
use super::*;
use egui_extras::{Column, TableBuilder};
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

#[derive(Default)]
pub(super) struct State {
    pub open: bool,
    queue: VecDeque<PathBuf>,
    form: Option<ImportForm>,
    worker: Option<ImportWorker>,
    editor: Option<Editor>,
    trench: Option<TrenchForm>,
    catalog: Option<Catalog>,
    catalog_attempted: bool,
    catalog_error: Option<String>,
    remembered_generation: Option<u64>,
    pending_catalog: Option<PendingCatalog>,
    titles: HashMap<String, String>,
    selected: Option<String>,
    search: String,
    only_loaded: bool,
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
        self.queue.clear(); self.form = None; self.worker = None;
        self.editor = None; self.trench = None;
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
struct ImportWorker { rx: Receiver<ImportEvent>, cancel: Arc<AtomicBool>, generation: u64, expected: Option<PointCloudSource> }
impl Drop for ImportWorker { fn drop(&mut self) { self.cancel.store(true, Ordering::Release); } }
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
        CloudCrs::Other => "Ander / lokaal coördinatenstelsel" }
}
fn crs_short(crs: CloudCrs) -> &'static str {
    match crs { CloudCrs::RdNew => "RD New · m", CloudCrs::Unknown => "Onbekend", CloudCrs::Other => "Ander / lokaal" }
}
fn crs_combo(ui: &mut egui::Ui, id: impl std::hash::Hash, crs: &mut CloudCrs) -> bool {
    let before = *crs;
    egui::ComboBox::from_id_salt(id).selected_text(crs_label(crs)).show_ui(ui, |ui| {
        for value in [CloudCrs::Unknown, CloudCrs::RdNew, CloudCrs::Other] {
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
        if files.len() > 32 { self.message = "Open maximaal 32 puntenwolken tegelijk.".into(); return; }
        if self.trench_editor.is_some() { self.message = self.pointcloud_unavailable_message().into(); return; }
        if self.pointcloud_ui.busy() { self.message = "Sluit eerst de puntenwolkeditor of rond de import af.".into(); return; }
        self.pointcloud_ui.open = true;
        self.ensure_pointcloud_catalog();
        for file in files { if !self.pointcloud_ui.queue.contains(&file) { self.pointcloud_ui.queue.push_back(file); } }
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
                self.pointcloud_ui.selected = Some(id.clone());
                self.pointcloud_ui.titles.remove(&id);
                self.pointcloud_ui.status = format!("{} staat in het puntenwolkoverzicht. Kies Inladen om de sleuf in dit project te gebruiken.", source.title);
                self.pointcloud_ui.remember_memory(source);
                if edit { self.open_pointcloud_editor(&id); }
            }
            Err(error) => {
                self.pointcloud_ui.remember_memory(source.clone());
                self.pointcloud_ui.selected = Some(id);
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
                Ok(ImportEvent::Done(result)) => { done = Some((worker.generation, worker.expected.clone(), result)); break; }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => { done = Some((worker.generation, None, Err("De puntenwolkimport is onverwacht gestopt. Het bronbestand is behouden.".into()))); break; }
            } }
        }
        if let Some((generation, expected, result)) = done {
            self.pointcloud_ui.worker = None;
            if generation == self.project_generation {
                match result {
                    Ok(source) => self.accept_catalog_source(source, true, expected),
                    Err(error) => { self.pointcloud_ui.status = error.clone(); self.message = error; }
                }
            }
        }
        if self.pointcloud_ui.busy() { ctx.request_repaint_after(Duration::from_millis(100)); }
        if self.pointcloud_ui.form.is_none() && !self.pointcloud_ui.busy()
            && self.pointcloud_ui.pending_catalog.is_none() && self.file_intent.is_none() {
            if let Some(path) = self.pointcloud_ui.queue.pop_front() {
                let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
                if let Some(source) = self.pointcloud_ui.sources().iter()
                    .find(|source| source.source_path == canonical && source.validate_cache().is_ok()) {
                    let id = source.id.clone(); self.pointcloud_ui.selected = Some(id.clone()); self.open_pointcloud_editor(&id);
                } else {
                    let title = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();
                    self.pointcloud_ui.form = Some(ImportForm { path, title, crs: CloudCrs::Unknown });
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
    fn start_pointcloud_import(&mut self, form: ImportForm, existing: Option<PointCloudSource>, ctx: &egui::Context) {
        if !self.pointcloud_task_allowed() {
            self.message = self.pointcloud_unavailable_message().into(); self.pointcloud_ui.form = Some(form); return;
        }
        let (tx, rx) = mpsc::channel(); let cancel = Arc::new(AtomicBool::new(false)); let flag = Arc::clone(&cancel);
        let repaint = ctx.clone();
        self.pointcloud_ui.status = "Puntenwolk voorbereiden…".into();
        self.pointcloud_ui.worker = Some(ImportWorker { rx, cancel, generation: self.project_generation, expected: existing.clone() });
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
        self.pointcloud_ui.worker = Some(ImportWorker { rx, cancel, generation: self.project_generation, expected: Some(source.clone()) });
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
        self.pointcloud_ui.selected = Some(id.into());
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
        let mut rows = state.sources().iter().map(|source| Row::new(source, loaded.contains(&source.id)))
            .filter(|row| !state.only_loaded || row.loaded)
            .filter(|row| {
                let query = state.search.trim().to_lowercase();
                query.is_empty() || format!("{} {} {} {}", row.title, row.path, row.axis, crs_short(row.crs)).to_lowercase().contains(&query)
            }).collect::<Vec<_>>();
        rows.sort_by(|a, b| {
            let order = match state.sort_column {
                1 => a.point_count.cmp(&b.point_count), 2 => a.object_count.cmp(&b.object_count),
                3 => a.axis_ready.cmp(&b.axis_ready).then_with(|| a.axis.cmp(&b.axis)),
                4..=6 => a.center[state.sort_column - 4].total_cmp(&b.center[state.sort_column - 4]),
                7 => a.path.to_lowercase().cmp(&b.path.to_lowercase()),
                _ => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
            };
            (if state.descending { order.reverse() } else { order }).then_with(|| a.id.cmp(&b.id))
        });
        if state.selected.is_none() { state.selected = rows.first().map(|row| row.id.clone()); }
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
            .open(&mut open).default_width((ctx.screen_rect().width() - 48.).clamp(540., 1320.))
            .default_height((ctx.screen_rect().height() - 50.).clamp(420., 820.))
            .min_width(540.).min_height(360.).constrain_to(ctx.screen_rect().shrink(12.))
            .movable(movable).resizable(true).show(ctx, |ui| theme::bounded_window_body(ui, |ui| {
                settings_ui::control_style(ui);
                ui.spacing_mut().item_spacing = egui::vec2(7., 5.);
                egui::TopBottomPanel::bottom("pointcloud-source-details")
                    .frame(theme::card(10, 8, 6)).show_inside(ui, |ui| {
                        if let Some(id) = state.selected.clone() {
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
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(enabled, egui::Button::new("LAS/LAZ toevoegen…").fill(ORANGE)).clicked() { action = Some(("choose", String::new())); }
                    if ui.add_enabled(enabled, egui::Button::new("Lijst vernieuwen")).clicked() { action = Some(("refresh", String::new())); }
                    ui.separator();
                    if ui.selectable_label(!state.map_open, "Lijst").clicked() { state.map_open = false; state.map.pause(); }
                    if ui.selectable_label(state.map_open, "Kaart").clicked() { state.map_open = true; }
                    ui.label(egui::RichText::new(format!("{} / {} scans · {} in dit project", rows.len(), state.sources().len(), loaded.len())).small().color(theme::MUTED));
                });
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut state.search).hint_text("Zoek op naam, bronbestand of status…").desired_width(310.));
                    ui.checkbox(&mut state.only_loaded, "Alleen in dit project");
                    if !state.search.is_empty() && ui.small_button("Wissen").clicked() { state.search.clear(); }
                });
                if state.worker.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner(); ui.label("Puntenwolk voorbereiden…");
                        if ui.button("Annuleren").clicked() {
                            if let Some(worker) = &state.worker { worker.cancel.store(true, Ordering::Release); }
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
                    let size = egui::vec2(ui.available_width().max(1.), (ui.available_height() - 75.).max(80.));
                    if let Some(id) = map::show(ui, &locations, &mut state.selected, &mut state.map, size) {
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
                "load" => self.start_pointcloud_load(&id),
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
                        self.start_pointcloud_import(ImportForm { path: source.source_path.clone(), title: source.title.clone(), crs: source.crs }, Some(source), ctx);
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
            ui.label("Kies RD alleen als de bron daadwerkelijk RD New in meters gebruikt. De originele XYZ-coördinaten blijven behouden.");
            ui.small("De scan wordt bewaard in het puntenwolkoverzicht en opent daarna in de editor. Met Inladen neem je hem over in een project.");
            let valid = !form.title.trim().is_empty() && form.title.len() <= 1024 && !form.title.chars().any(char::is_control);
            ui.horizontal(|ui| {
                if ui.add_enabled(valid, egui::Button::new("Toevoegen en bewerken").fill(ORANGE)).clicked() { create = true; }
                if ui.button("Annuleren").clicked() { cancel = true; }
            });
        });
        if create { self.start_pointcloud_import(form, None, ctx); }
        else if !cancel && !response.should_close() { self.pointcloud_ui.form = Some(form); }
    }

    fn start_pointcloud_load(&mut self, id: &str) {
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
        self.pointcloud_ui.trench = Some(TrenchForm { source, generation: self.project_generation, axis_id,
            selected_ids, label, width, mode, preview: None, error: None });
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
            let result = (|| {
                if !self.pointcloud_task_allowed() || form.generation != self.project_generation { return Err("Het project is intussen gewijzigd. Open Inladen opnieuw.".into()); }
                let prepared = form.preview.take().ok_or("De inlaadvoorvertoning ontbreekt")??;
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
                self.refresh_virtual_references(); self.virtual_map.selected = Some(index); self.dirty = true;
                self.pointcloud_ui.status = "Proefsleuf ingeladen. Controleer objectgegevens en maaiveldhoogtes en sla het project op.".into();
                self.pointcloud_ui.open = false; self.pointcloud_ui.map.pause(); self.open_virtual_editor(index);
                Ok::<(), String>(())
            })();
            if let Err(error) = result { form.error = Some(error); form.preview = None; self.pointcloud_ui.trench = Some(form); }
        } else if !cancel && !response.should_close() { self.pointcloud_ui.trench = Some(form); }
    }
}

fn show_source_details(ui: &mut egui::Ui, source: &PointCloudSource, title: &mut String,
    loaded: bool, enabled: bool, details: &mut Option<(String, String, CloudCrs)>, action: &mut Option<(&'static str, String)>) {
    ui.horizontal_wrapped(|ui| {
        ui.strong("Geselecteerde scan");
        ui.small(if loaded { "Meetobjecten in huidig project" } else { "Bewaard in overzicht · nog niet ingeladen" });
        if ui.add_enabled(enabled, egui::Button::new("Bewerken")).clicked() { *action = Some(("edit", source.id.clone())); }
        if ui.add_enabled(enabled, egui::Button::new("Inladen").fill(ORANGE)).clicked() { *action = Some(("load", source.id.clone())); }
        ui.add_enabled_ui(enabled, |ui| {
            ui.menu_button("Meer", |ui| {
                for (label, key) in [("Meetobjecten naar DXF…", "export"), ("Bronbestand opnieuw kiezen…", "relink"), ("Weergave opnieuw opbouwen", "rebuild")] {
                    if ui.button(label).clicked() { *action = Some((key, source.id.clone())); ui.close_menu(); }
                }
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
    if source.crs != CloudCrs::RdNew { ui.small("Bewerken en DXF-export behouden de broncoördinaten. Inladen en de kaart vereisen RD New in meters."); }
}

fn show_source_table(ui: &mut egui::Ui, rows: &[Row], state: &mut State, enabled: bool,
    action: &mut Option<(&'static str, String)>) {
    // Reserve the header and horizontal scrollbar before sizing the table body.
    let height = (ui.available_height() - 48.).max(0.);
    egui::ScrollArea::horizontal().id_salt("pointcloud-table-horizontal").show(ui, |ui| {
        ui.set_min_width(1110.);
        TableBuilder::new(ui).id_salt("pointcloud-catalog-table").striped(true).resizable(true)
            .sense(egui::Sense::click()).cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .max_scroll_height(height).min_scrolled_height(0.).auto_shrink([false, false])
            .column(Column::initial(195.).at_least(130.).clip(true))
            .column(Column::initial(100.).at_least(75.).clip(true))
            .column(Column::initial(82.).at_least(60.).clip(true))
            .column(Column::initial(150.).at_least(110.).clip(true))
            .columns(Column::initial(100.).at_least(75.).clip(true), 3)
            .column(Column::initial(155.).at_least(110.).clip(true))
            .column(Column::exact(158.))
            .header(28., |mut header| {
                for (index, label) in ["Naam", "Scanpunten", "Objecten", "Begin / eind", "Centrum X", "Centrum Y", "Centrum Z", "Bron / CRS"].iter().enumerate() {
                    header.col(|ui| {
                        let suffix = if state.sort_column == index { if state.descending { " ↓" } else { " ↑" } } else { "" };
                        if ui.selectable_label(state.sort_column == index, format!("{label}{suffix}")).clicked() {
                            if state.sort_column == index { state.descending = !state.descending; }
                            else { state.sort_column = index; state.descending = matches!(index, 1 | 2); }
                        }
                    });
                }
                header.col(|ui| { ui.strong("Acties"); });
            })
            .body(|body| {
                body.rows(42., rows.len(), |mut row| {
                    let item = &rows[row.index()];
                    row.set_selected(state.selected.as_deref() == Some(item.id.as_str()));
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            ui.add(egui::Label::new(egui::RichText::new(&item.title).strong()).truncate()).on_hover_text(&item.title);
                            if item.loaded { ui.small(egui::RichText::new("In huidig project").color(Color32::from_rgb(25, 120, 85))); }
                        });
                    });
                    row.col(|ui| { ui.monospace(count(item.point_count)); });
                    row.col(|ui| { ui.monospace(count(item.object_count as u64)); });
                    row.col(|ui| {
                        ui.add(egui::Label::new(egui::RichText::new(&item.axis).small()
                            .color(if item.axis_ready { Color32::from_rgb(25, 120, 85) } else { Color32::from_rgb(149, 94, 24) })).truncate()).on_hover_text(&item.axis);
                    });
                    for coordinate in item.center { row.col(|ui| { ui.monospace(format!("{coordinate:.3}")); }); }
                    row.col(|ui| {
                        ui.vertical(|ui| {
                            let file = std::path::Path::new(&item.path).file_name().unwrap_or_default().to_string_lossy();
                            ui.add(egui::Label::new(egui::RichText::new(file).small()).truncate()).on_hover_text(&item.path);
                            ui.small(crs_short(item.crs));
                        });
                    });
                    row.col(|ui| {
                        if ui.add_enabled(enabled, egui::Button::new("Bewerken")).clicked() { state.selected = Some(item.id.clone()); *action = Some(("edit", item.id.clone())); }
                        if ui.add_enabled(enabled, egui::Button::new("Inladen").fill(ORANGE)).clicked() { state.selected = Some(item.id.clone()); *action = Some(("load", item.id.clone())); }
                    });
                    let response = row.response();
                    if response.clicked() { state.selected = Some(item.id.clone()); }
                    if enabled && response.double_clicked() && action.is_none() { *action = Some(("edit", item.id.clone())); }
                });
            });
    });
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
