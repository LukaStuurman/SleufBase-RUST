//! Manual terrain-segment text/color editing, matching the validated original
//! app's maaiveld mode. Profile preparation supplies the marker geometry;
//! editing terrain descriptions leaves measured elevations unchanged.
use super::*;
use sleufbase_rust::{
    maaiveld_profile::{self, Source},
    export_control::Control,
    raster_points,
};
use std::{
    collections::VecDeque,
    hash::{DefaultHasher, Hasher},
    io::Write,
    sync::Arc,
};

const KEYS: [&str; 3] = ["start", "middle", "end"];
const LABELS: [&str; 3] = ["Beginpunt", "Middenpunt", "Eindpunt"];
const META: &str = "template_maaiveld_segments";
const DEFAULT_COLOR: [u8; 3] = [128; 3];
const RADIUS: f32 = 9.;
// Only three coordinates and their validation stamp are retained per source.
// Even unusually large projects cannot grow the preparation cache unbounded.
const MAX_CACHED_SOURCES: usize = 4096;
const MAX_FAILURE_DETAILS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    source: Source,
    generation: u64,
    dataset: Option<usize>,
    trench: Option<usize>,
    settings_digest: u64,
    local_profile: Option<(PathBuf, u64, Option<std::time::SystemTime>)>,
    digest: u64,
}
struct HashWriter(DefaultHasher);
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct Job {
    stamp: Stamp,
    control: Arc<Control>,
    rx: Receiver<Result<maaiveld_profile::Prepared, String>>,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.control.cancel();
    }
}
#[derive(Clone)]
struct Markers {
    stamp: Stamp,
    points: [[f64; 2]; 3],
    partial_context: bool,
}
struct Preload {
    generation: u64,
    rasters: usize,
    trenches: usize,
    next: usize,
    completed: usize,
    prepared: usize,
    reused: usize,
    skipped: usize,
    partial_context: usize,
    failures: Vec<String>,
    current: String,
    cancelled: bool,
}
impl Preload {
    fn total(&self) -> usize { self.rasters + self.trenches }
    fn skip(&mut self, message: String) {
        self.completed += 1;
        self.skipped += 1;
        if self.failures.len() < MAX_FAILURE_DETAILS {
            self.failures.push(message.chars().take(240).collect());
        }
    }
}
struct Draft {
    stamp: Stamp,
    segment: usize,
    point: [f64; 2],
    text: String,
    color: [u8; 3],
    error: String,
}
#[derive(Default)]
pub(super) struct State {
    pub(super) active: bool,
    selected: Option<Source>,
    job: Option<Job>,
    markers: Option<Markers>,
    draft: Option<Draft>,
    cache: VecDeque<Markers>,
    generation: Option<u64>,
    preload: Option<Preload>,
    settings_checked: Option<Instant>,
}
impl State {
    pub(super) fn captures(&self) -> bool { self.active }
    pub(super) fn has_draft(&self) -> bool { self.draft.is_some() }
    pub(super) fn preloading(&self) -> bool { self.preload.is_some() }
    fn cached(&mut self, stamp: &Stamp) -> Option<Markers> {
        let index = self.cache.iter().position(|entry| entry.stamp.source == stamp.source)?;
        let entry = self.cache.remove(index)?;
        if entry.stamp != *stamp { return None; }
        self.cache.push_back(entry.clone());
        Some(entry)
    }
    fn remember(&mut self, markers: Markers) {
        self.cache.retain(|entry| entry.stamp.source != markers.stamp.source);
        while self.cache.len() >= MAX_CACHED_SOURCES { self.cache.pop_front(); }
        self.cache.push_back(markers);
    }
}

fn markers_from_prepared(stamp: Stamp, prepared: maaiveld_profile::Prepared) -> Result<Markers, String> {
    if prepared.points.len() != 3 || prepared.points.iter().any(|point| !point.x.is_finite() || !point.y.is_finite()) {
        return Err("Geen drie geldige maaiveldpunten beschikbaar.".into());
    }
    Ok(Markers {
        stamp,
        points: std::array::from_fn(|index| [prepared.points[index].x, prepared.points[index].y]),
        partial_context: !prepared.context_failures.is_empty(),
    })
}

fn edited_metadata(
    original: &serde_json::Value,
    segment: usize,
    text: &str,
    rgb: [u8; 3],
) -> Result<serde_json::Value, String> {
    let key = KEYS.get(segment).ok_or("Onbekend maaiveldsegment")?;
    raster_points::validate_metadata(original)?;
    let text = text.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'));
    if text.len() > 4096 || text.chars().any(char::is_control) {
        return Err("Maaiveldtekst is te lang of bevat ongeldige tekens".into());
    }
    let mut value = original.as_object().cloned().unwrap_or_default();
    let segments = value.entry(META).or_insert_with(|| serde_json::json!({}));
    if !segments.is_object() {
        *segments = serde_json::json!({});
    }
    // Original replaces this segment and preserves all other segment metadata.
    segments.as_object_mut().unwrap().insert(
        (*key).into(),
        serde_json::json!({
            "text": if text.is_empty() { "INVULLEN" } else { text },
            "color": format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
        }),
    );
    let value = serde_json::Value::Object(value);
    raster_points::validate_metadata(&value)?;
    Ok(value)
}
fn nearest(points: &[[f64; 2]; 3], view: &raster_point_ui::View, pointer: Pos2) -> Option<usize> {
    nearest_screen(&points.map(|point| view.project(point)), pointer)
}
fn nearest_screen(points: &[Pos2; 3], pointer: Pos2) -> Option<usize> {
    let mut result = None;
    let mut distance = f32::INFINITY;
    for (index, point) in points.iter().enumerate() {
        let d = point.distance_sq(pointer);
        if d <= (RADIUS + 5.).powi(2) && d < distance {
            result = Some(index);
            distance = d;
        }
    }
    result
}
impl SleufBaseApp {
    fn maaiveld_source_visible(&self, source: Source) -> bool {
        match source {
            Source::Raster(i)=>self.geotiffs.get(i).is_some_and(layer_visibility_ui::raster_is_visible),
            Source::Virtual(i)=>self.virtual_trenches.get(i).is_some_and(layer_visibility_ui::trench_is_visible),
        }
    }
    fn maaiveld_selected(&self) -> Option<Source> {
        // A deliberately selected hidden row must not redirect edits to another layer.
        if let Some(index)=self.virtual_map.selected.filter(|&i|i<self.virtual_trenches.len()) {
            return self.maaiveld_source_visible(Source::Virtual(index)).then_some(Source::Virtual(index));
        }
        if let Some(index)=self.raster_points.selected.as_ref()
            .and_then(|path|self.geotiffs.iter().position(|r|r.path==*path)) {
            return self.maaiveld_source_visible(Source::Raster(index)).then_some(Source::Raster(index));
        }
        self.geotiffs.iter().position(layer_visibility_ui::raster_is_visible).map(Source::Raster)
            .or_else(||self.virtual_trenches.iter().position(layer_visibility_ui::trench_is_visible).map(Source::Virtual))
    }
    fn maaiveld_metadata(&self, source: Source) -> Result<&serde_json::Value, String> {
        match source {
            Source::Raster(i) => self.geotiffs.get(i).map(|r| &r.metadata),
            Source::Virtual(i) => self.virtual_trenches.get(i).map(|t| &t.metadata),
        }
        .ok_or("De gekozen proefsleuf is niet meer beschikbaar".into())
    }
    fn maaiveld_settings_digest(&self) -> Result<u64, String> {
        // Preparation reads only these two settings. Map visibility, labels,
        // contact details and other export options must not invalidate geometry.
        let imported = if self.project_settings.is_none() && self.legacy_archive.as_ref()
            .is_some_and(|archive| archive.original.get("template_export_settings").is_some())
        { Some(self.project_export_settings()?) } else { None };
        let settings = imported.as_ref().or(self.project_settings.as_ref())
            .unwrap_or(&self.migrated_settings);
        let mut writer = HashWriter(DefaultHasher::new());
        serde_json::to_writer(&mut writer, &(&settings.object_rules,
            settings.export_preferences.template_cross_section_marker_diameter))
            .map_err(|error| error.to_string())?;
        Ok(writer.0.finish())
    }
    fn maaiveld_local_profile_path(&self, source: Source) -> Option<PathBuf> {
        let Source::Raster(index) = source else { return None; };
        let raster = self.geotiffs.get(index)?;
        let path = raster.metadata.get("kickthemap_job_features_path")?
            .as_str().filter(|path| !path.is_empty())?;
        let path = PathBuf::from(path);
        Some(if path.is_absolute() { path } else {
            raster.path.parent().unwrap_or(std::path::Path::new(".")).join(path)
        })
    }
    fn maaiveld_stamp(
        &self,
        source: Source,
        dataset: Option<usize>,
        trench: Option<usize>,
    ) -> Result<Stamp, String> {
        let mut writer = HashWriter(DefaultHasher::new());
        match source {
            Source::Raster(i) => serde_json::to_writer(
                &mut writer,
                self.geotiffs.get(i).ok_or("TIFF niet beschikbaar")?,
            ),
            Source::Virtual(i) => serde_json::to_writer(
                &mut writer,
                self.virtual_trenches
                    .get(i)
                    .ok_or("Proefsleuf niet beschikbaar")?,
            ),
        }
        .map_err(|e| e.to_string())?;
        if let Some(i) = dataset {
            serde_json::to_writer(
                &mut writer,
                self.datasets.get(i).ok_or("Profieljob niet beschikbaar")?,
            )
            .map_err(|e| e.to_string())?;
        }
        if let Some(i) = trench {
            serde_json::to_writer(
                &mut writer,
                self.virtual_trenches
                    .get(i)
                    .ok_or("Profielsleuf niet beschikbaar")?,
            )
            .map_err(|e| e.to_string())?;
        }
        let settings_digest = self.maaiveld_settings_digest()?;
        writer.0.write_u64(settings_digest);
        let local_profile = self.maaiveld_local_profile_path(source).and_then(|path| {
            let metadata = std::fs::metadata(&path).ok().filter(|metadata| metadata.is_file())?;
            Some((path, metadata.len(), metadata.modified().ok()))
        });
        Ok(Stamp {
            source,
            generation: self.project_generation,
            dataset,
            trench,
            settings_digest,
            local_profile,
            digest: writer.0.finish(),
        })
    }
    fn maaiveld_valid(&self, stamp: &Stamp) -> bool {
        stamp.generation == self.project_generation
            && self.maaiveld_association(stamp.source).ok() == Some((stamp.dataset, stamp.trench))
            && self.maaiveld_stamp(stamp.source, stamp.dataset, stamp.trench).as_ref() == Ok(stamp)
    }
    fn maaiveld_unchanged(&self, stamp: &Stamp) -> bool {
        self.maaiveld_selected() == Some(stamp.source) && self.maaiveld_valid(stamp)
    }
    // Resolve associations before a cache lookup without cloning any datasets.
    fn maaiveld_association(&self, source: Source) -> Result<(Option<usize>, Option<usize>), String> {
        let Source::Raster(index) = source else {
            if let Source::Virtual(index) = source {
                self.virtual_trenches.get(index).ok_or("Proefsleuf ontbreekt")?;
            }
            return Ok((None, None));
        };
        let raster = self.geotiffs.get(index).ok_or("TIFF ontbreekt")?;
        if self.maaiveld_local_profile_path(source).is_some_and(|path| path.is_file()) {
            return Ok((None, None));
        }
        let mut jobs = self.datasets.iter().enumerate().filter(|(_, dataset)|
            raster.kickthemap_job_id.is_some_and(|id| id > 0 && dataset.job_id == id));
        let first = jobs.next().map(|(index, _)| index);
        if jobs.next().is_some() { return Err("De TIFF-jobkoppeling is niet eenduidig".into()); }
        if let Some(index) = first { return Ok((Some(index), None)); }
        let choices = sleufbase_rust::template_project::suggest(
            &self.datasets, &self.geotiffs, &self.virtual_trenches);
        let mut matches = choices.iter().filter(|choice| choice.raster == Some(index));
        let choice = matches.next().ok_or("Koppel deze TIFF eerst eenduidig aan lokale profielobjecten")?;
        if matches.next().is_some() {
            return Err("Koppel deze TIFF eerst eenduidig aan lokale profielobjecten".into());
        }
        match choice.source {
            sleufbase_rust::template_project::Source::Job(index) => Ok((Some(index), None)),
            sleufbase_rust::template_project::Source::Virtual(index) => Ok((None, Some(index))),
            _ => Err("Geen lokale profielobjecten beschikbaar; laad eerst de gekoppelde KickTheMap-jobdata".into()),
        }
    }
    fn maaiveld_current_stamp(&self, source: Source) -> Result<Stamp, String> {
        let (dataset, trench) = self.maaiveld_association(source)?;
        self.maaiveld_stamp(source, dataset, trench)
    }
    // Clone only the chosen layer and its uniquely associated profile input.
    fn maaiveld_input(&self, stamp: &Stamp) -> Result<(NativeProject, Source), String> {
        let mut project = NativeProject::default();
        let source = match stamp.source {
            Source::Virtual(index) => {
                project.virtual_trenches.push(self.virtual_trenches.get(index)
                    .ok_or("Proefsleuf ontbreekt")?.clone());
                Source::Virtual(0)
            }
            Source::Raster(index) => {
                project.rasters.push(self.geotiffs.get(index).ok_or("TIFF ontbreekt")?.clone());
                if let Some(index) = stamp.dataset {
                    project.datasets.push(self.datasets.get(index).ok_or("Profieljob ontbreekt")?.clone());
                }
                if let Some(index) = stamp.trench {
                    project.virtual_trenches.push(self.virtual_trenches.get(index)
                        .ok_or("Profielsleuf ontbreekt")?.clone());
                }
                Source::Raster(0)
            }
        };
        Ok((project, source))
    }
    fn start_maaiveld_preparation(&mut self, stamp: Stamp) -> Result<(), String> {
        let (project, source) = self.maaiveld_input(&stamp)?;
        let settings = self.project_export_settings()?;
        let control = Arc::new(Control::default());
        let task_control = control.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::Builder::new().name("manual-maaiveld-profile".into()).spawn(move || {
            let result = maaiveld_profile::prepare(&project, source, &settings, &task_control);
            let _ = tx.send(result);
        }).map_err(|error| error.to_string())?;
        self.maaiveld_ui.job = Some(Job { stamp, control, rx });
        Ok(())
    }
    fn ensure_maaiveld_generation(&mut self) {
        if self.maaiveld_ui.generation.is_some_and(|generation| generation != self.project_generation) {
            self.reset_maaiveld_state();
        }
        self.maaiveld_ui.generation = Some(self.project_generation);
    }
    pub(super) fn reset_maaiveld_state(&mut self) {
        self.maaiveld_ui = State::default();
    }
    pub(super) fn close_maaiveld_mode(&mut self) {
        self.maaiveld_ui.active = false;
        self.maaiveld_ui.selected = None;
        self.maaiveld_ui.markers = None;
        self.maaiveld_ui.draft = None;
        if !self.maaiveld_ui.preloading() { self.maaiveld_ui.job = None; }
    }
    pub(super) fn maaiveld_selection_changed(&mut self) {
        if self.maaiveld_ui.active && !self.maaiveld_ui.has_draft()
            && self.maaiveld_ui.selected != self.maaiveld_selected()
        {
            self.refresh_manual_maaiveld();
        }
    }
    pub(super) fn manual_maaiveld_captures_click(&self, view: &raster_point_ui::View) -> bool {
        self.maaiveld_ui.active && (self.maaiveld_ui.has_draft()
            || view.clicked.is_some_and(|click| self.maaiveld_ui.markers.as_ref().is_some_and(|markers|
                self.maaiveld_source_visible(markers.stamp.source)
                    && self.maaiveld_selected() == Some(markers.stamp.source)
                    && nearest(&markers.points, view, click).is_some())))
    }
    pub(super) fn toggle_maaiveld_mode(&mut self) {
        if self.maaiveld_ui.has_draft() {
            self.message = "Sla de maaiveldtekst op of sluit het tekstvenster eerst.".into();
            return;
        }
        self.ensure_maaiveld_generation();
        if self.maaiveld_ui.active {
            self.close_maaiveld_mode();
            self.message = "Maaiveld-invulmodus uitgeschakeld.".into();
            return;
        }
        if !self
            .project_settings
            .as_ref()
            .unwrap_or(&self.migrated_settings)
            .export_preferences
            .template_enable_maaiveld_edit_points
        {
            self.message = "Zet deze functie eerst aan in Instellingen > Proefsleuven.".into();
            return;
        }
        if self.project_sources_owned()
            || self.trench_editor.is_some()
            || self.file_intent.is_some()
            || self.export_intent.is_some()
            || self.template_job.is_some()
        {
            self.message = "Rond de lopende taak of het sleufconcept eerst af".into();
            return;
        }
        self.cancel_virtual_map();
        self.stop_raster_rotation();
        self.raster_points.mode = None;
        self.comment_tools = Default::default();
        self.maaiveld_ui.active = true;
        self.refresh_manual_maaiveld();
    }
    pub(super) fn refresh_manual_maaiveld(&mut self) {
        if self.maaiveld_ui.preloading() || self.maaiveld_ui.has_draft() { return; }
        self.ensure_maaiveld_generation();
        self.maaiveld_ui.selected = self.maaiveld_selected();
        self.maaiveld_ui.job = None;
        self.maaiveld_ui.markers = None;
        let result = (|| {
            let source = self.maaiveld_ui.selected
                .ok_or("Kies eerst een proefsleuf om maaiveldpunten te tonen.")?;
            let stamp = self.maaiveld_current_stamp(source)?;
            if let Some(markers) = self.maaiveld_ui.cached(&stamp) {
                self.activate_maaiveld_markers(markers);
            } else {
                self.start_maaiveld_preparation(stamp)?;
                self.message = "Maaiveldpunten van het dwarsprofiel voorbereiden…".into();
            }
            Ok::<_, String>(())
        })();
        if let Err(error) = result { self.message = error; }
    }
    fn activate_maaiveld_markers(&mut self, markers: Markers) {
        self.message = "Klik op begin, midden of eind van het dwarsprofiel om maaiveldtekst en kleur te kiezen.".into();
        if markers.partial_context {
            self.message.push_str(" Weg- of terreincontext kon niet volledig worden opgehaald; lokale profielgeometrie is gebruikt.");
        }
        self.maaiveld_ui.markers = Some(markers);
    }
    fn maaiveld_source_label(&self, source: Source) -> String {
        match source {
            Source::Raster(index) => self.geotiffs.get(index).map(|raster| {
                if !raster.kickthemap_job_title.is_empty() {
                    raster.kickthemap_job_title.chars().take(120).collect()
                } else {
                    raster.path.file_name().unwrap_or_default().to_string_lossy().chars().take(120).collect()
                }
            }),
            Source::Virtual(index) => self.virtual_trenches.get(index)
                .map(|trench| trench.label.chars().take(120).collect()),
        }.unwrap_or_else(|| "Onbeschikbare proefsleuf".into())
    }
    /// Prepare only marker geometry. Stored terrain descriptions and elevations
    /// are never edited by this operation, including when a source is skipped.
    pub(super) fn preload_all_maaiveld(&mut self) {
        if self.maaiveld_ui.preloading() { return; }
        if self.maaiveld_ui.has_draft() {
            self.message = "Sla de maaiveldtekst op of sluit het tekstvenster eerst.".into();
            return;
        }
        if self.project_sources_owned() || self.trench_editor.is_some()
            || self.file_intent.is_some() || self.export_intent.is_some()
            || self.template_job.is_some() || self.live_operation_busy()
        {
            self.message = "Rond de lopende taak of het sleufconcept eerst af.".into();
            return;
        }
        if self.geotiffs.is_empty() && self.virtual_trenches.is_empty() {
            self.message = "Laad eerst proefsleuven om maaiveldgegevens voor te bereiden.".into();
            return;
        }
        self.ensure_maaiveld_generation();
        self.maaiveld_ui.job = None;
        self.maaiveld_ui.preload = Some(Preload {
            generation: self.project_generation,
            rasters: self.geotiffs.len(), trenches: self.virtual_trenches.len(),
            next: 0, completed: 0, prepared: 0, reused: 0, skipped: 0,
            partial_context: 0, failures: Vec::new(), current: "Voorbereiding starten…".into(),
            cancelled: false,
        });
        self.message = "Alle maaiveldgegevens voorbereiden…".into();
        // Begin on the next frame so the progress window is painted first.
    }
    fn finish_maaiveld_preload(&mut self) {
        let Some(preload) = self.maaiveld_ui.preload.take() else { return; };
        self.maaiveld_ui.job = None;
        if self.maaiveld_ui.active {
            self.maaiveld_ui.selected = self.maaiveld_selected();
            self.maaiveld_ui.markers = None;
            if let Some(source) = self.maaiveld_ui.selected {
                if let Ok(stamp) = self.maaiveld_current_stamp(source) {
                    self.maaiveld_ui.markers = self.maaiveld_ui.cached(&stamp);
                }
            }
        }
        // Cancelling bulk loading can leave the selected source unprepared.
        // Do not retain an active tool that would consume clicks without markers.
        let reopen = self.maaiveld_ui.active && self.maaiveld_ui.markers.is_none();
        if reopen { self.close_maaiveld_mode(); }
        self.message = format!("Maaiveldgegevens {}: {} voorbereid, {} hergebruikt, {} overgeslagen.",
            if preload.cancelled { "laden geannuleerd" } else { "geladen" },
            preload.prepared, preload.reused, preload.skipped);
        if preload.partial_context > 0 {
            self.message.push_str(&format!(" Bij {} sleuven is lokale geometrie gebruikt omdat kaartcontext niet volledig beschikbaar was.", preload.partial_context));
        }
        if let Some(reason) = preload.failures.first() {
            self.message.push_str(&format!(" Eerste melding: {reason}"));
        }
        if preload.prepared + preload.reused > MAX_CACHED_SOURCES {
            self.message.push_str(&format!(" De {} laatst gebruikte sleuven blijven voorbereid.", MAX_CACHED_SOURCES));
        }
        if reopen {
            self.message.push_str(" Open Maaiveld invullen om de geselecteerde proefsleuf verder voor te bereiden.");
        }
    }
    fn advance_maaiveld_preload(&mut self, ctx: &egui::Context) {
        if self.maaiveld_ui.job.is_some() { return; }
        // Bound the UI work even when many sources are cached or unusable.
        for _ in 0..4 {
            let Some(preload) = self.maaiveld_ui.preload.as_mut() else { return; };
            if preload.cancelled || preload.next >= preload.total() {
                self.finish_maaiveld_preload();
                ctx.request_repaint();
                return;
            }
            let index = preload.next;
            preload.next += 1;
            let source = if index < preload.rasters { Source::Raster(index) }
                else { Source::Virtual(index - preload.rasters) };
            let label = self.maaiveld_source_label(source);
            self.maaiveld_ui.preload.as_mut().unwrap().current = format!("Voorbereiden: {label}");
            let result = (|| {
                let stamp = self.maaiveld_current_stamp(source)?;
                if let Some(markers) = self.maaiveld_ui.cached(&stamp) {
                    let preload = self.maaiveld_ui.preload.as_mut().unwrap();
                    preload.completed += 1;
                    preload.reused += 1;
                    preload.partial_context += usize::from(markers.partial_context);
                    return Ok(false);
                }
                self.start_maaiveld_preparation(stamp)?;
                Ok::<_, String>(true)
            })();
            match result {
                Ok(true) => break,
                Ok(false) => {},
                Err(error) => self.maaiveld_ui.preload.as_mut().unwrap().skip(format!("{label}: {error}")),
            }
        }
        ctx.request_repaint_after(Duration::from_millis(20));
    }
    pub(super) fn poll_manual_maaiveld(&mut self, ctx: &egui::Context) {
        self.ensure_maaiveld_generation();
        if self.maaiveld_ui.preloading() {
            let changed = self.maaiveld_ui.preload.as_ref().is_some_and(|preload|
                preload.generation != self.project_generation || preload.rasters != self.geotiffs.len()
                    || preload.trenches != self.virtual_trenches.len());
            if changed || self.project_sources_owned() {
                self.reset_maaiveld_state();
                self.message = "Maaiveldvoorbereiding gestopt omdat de projectbronnen zijn gewijzigd.".into();
                return;
            }
        } else {
            if !self.maaiveld_ui.active { return; }
            if !self.maaiveld_ui.has_draft() && ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
                self.close_maaiveld_mode();
                return;
            }
            if !self.project_settings.as_ref().unwrap_or(&self.migrated_settings)
                .export_preferences.template_enable_maaiveld_edit_points || self.project_sources_owned()
            {
                self.close_maaiveld_mode();
                return;
            }
            if self.maaiveld_ui.selected != self.maaiveld_selected() && !self.maaiveld_ui.has_draft() {
                self.refresh_manual_maaiveld();
            }
            // Geometry is fully revalidated on use. Check the small relevant
            // settings subset periodically so an open mode updates promptly.
            if !self.maaiveld_ui.has_draft() && self.maaiveld_ui.settings_checked
                .is_none_or(|time| time.elapsed() >= Duration::from_millis(500))
            {
                self.maaiveld_ui.settings_checked = Some(Instant::now());
                let expected = self.maaiveld_ui.markers.as_ref().map(|markers| markers.stamp.settings_digest)
                    .or_else(|| self.maaiveld_ui.job.as_ref().map(|job| job.stamp.settings_digest));
                if expected.is_some_and(|digest| self.maaiveld_settings_digest().ok() != Some(digest)) {
                    self.maaiveld_ui.cache.clear();
                    self.refresh_manual_maaiveld();
                }
            }
        }
        if let Some(job) = &self.maaiveld_ui.job {
            let result = match job.rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err("Voorbereiden maaiveldpunten is afgebroken".into())),
            };
            if let Some(result) = result {
                let job = self.maaiveld_ui.job.take().unwrap();
                let cancelled = self.maaiveld_ui.preload.as_ref().is_some_and(|preload| preload.cancelled);
                if !cancelled {
                    let result = if self.maaiveld_valid(&job.stamp) {
                        result.and_then(|prepared| markers_from_prepared(job.stamp.clone(), prepared))
                    } else { Err("De proefsleuf of profielinstellingen zijn gewijzigd; bereid de punten opnieuw voor.".into()) };
                    match result {
                        Ok(markers) => {
                            self.maaiveld_ui.remember(markers.clone());
                            if let Some(preload) = self.maaiveld_ui.preload.as_mut() {
                                preload.completed += 1;
                                preload.prepared += 1;
                                preload.partial_context += usize::from(markers.partial_context);
                            } else if self.maaiveld_ui.active && self.maaiveld_selected() == Some(markers.stamp.source) {
                                self.activate_maaiveld_markers(markers);
                            }
                        }
                        Err(error) => {
                            let label = self.maaiveld_source_label(job.stamp.source);
                            if let Some(preload) = self.maaiveld_ui.preload.as_mut() {
                                preload.skip(format!("{label}: {error}"));
                            } else { self.message = error; }
                        }
                    }
                }
            } else { ctx.request_repaint_after(Duration::from_millis(40)); }
        }
        if self.maaiveld_ui.preloading() { self.advance_maaiveld_preload(ctx); }
    }
    /// Returns true while this mode owns map clicks. Call before other handlers.
    pub(super) fn handle_manual_maaiveld(&mut self, view: &raster_point_ui::View) -> bool {
        if !self.maaiveld_ui.active {
            return false;
        }
        if self.virtual_map.captures()
            || self.raster_rotation.active()
            || self.raster_points.mode.is_some()
            || self.comment_tools.captures()
        {
            self.close_maaiveld_mode();
            return false;
        }
        if view
            .response
            .ctx
            .input(|i| i.key_pressed(egui::Key::Escape))
        {
            if self.maaiveld_ui.draft.is_none() {
                self.close_maaiveld_mode();
            }
            return true;
        }
        let Some(markers) = &self.maaiveld_ui.markers else {
            return true;
        };
        if !self.maaiveld_source_visible(markers.stamp.source) {
            self.close_maaiveld_mode();
            return false;
        }
        if self.maaiveld_ui.draft.is_some() {
            return true;
        }
        let stamp = markers.stamp.clone();
        let points = markers.points;
        let fills = match self.maaiveld_metadata(stamp.source).and_then(|m| {
            sleufbase_rust::profile_surface::resolve(m, &serde_json::Value::Null, true)
        }) {
            Ok(fills) => fills,
            Err(error) => {
                self.message = error;
                return true;
            }
        };
        for (i, point) in points.iter().enumerate() {
            let center = view.project(*point);
            if !view.rect.expand(RADIUS).contains(center) {
                continue;
            }
            let rgb = fills[i].rgb;
            view.painter.circle_filled(
                center,
                RADIUS,
                Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8),
            );
            view.painter
                .circle_stroke(center, RADIUS, egui::Stroke::new(1_f32, Color32::BLACK));
            view.painter.text(
                center + egui::vec2(12., -12.),
                egui::Align2::LEFT_BOTTOM,
                LABELS[i],
                egui::FontId::proportional(12.),
                Color32::BLACK,
            );
        }
        if let Some(click) = view.clicked {
            if let Some(segment) = nearest(&points, view, click) {
                if !self.maaiveld_unchanged(&stamp) {
                    self.refresh_manual_maaiveld();
                    return true;
                }
                let fill = &fills[segment];
                self.maaiveld_ui.draft = Some(Draft {
                    stamp,
                    segment,
                    point: points[segment],
                    text: fill.text.clone(),
                    color: [
                        (fill.rgb >> 16) as u8,
                        (fill.rgb >> 8) as u8,
                        fill.rgb as u8,
                    ],
                    error: String::new(),
                });
            }
        }
        true
    }
    fn save_manual_maaiveld(&mut self, draft: &Draft) -> Result<(), String> {
        if self.project_sources_owned()
            || self.trench_editor.is_some()
            || !self.maaiveld_unchanged(&draft.stamp)
        {
            return Err("De proefsleuf of profielinstellingen zijn gewijzigd. Sluit dit venster en maak nieuwe maaiveldpunten.".into());
        }
        let original = self.maaiveld_metadata(draft.stamp.source)?;
        let candidate = edited_metadata(original, draft.segment, &draft.text, draft.color)?;
        let old_bytes = raster_points::validate_metadata(original)?;
        let new_bytes = raster_points::validate_metadata(&candidate)?;
        let mut total = 0usize;
        for t in &self.virtual_trenches {
            total = total.saturating_add(t.point_metadata_bytes()?);
        }
        for r in &self.geotiffs {
            total = total.saturating_add(raster_points::validate_metadata(&r.metadata)?);
        }
        if total.saturating_sub(old_bytes).saturating_add(new_bytes) > 16 * 1024 * 1024 {
            return Err("De maaiveldtekst past niet binnen het projectbudget".into());
        }
        if let Source::Virtual(i) = draft.stamp.source {
            if self.virtual_trenches[i]
                .point_metadata_bytes()?
                .saturating_sub(old_bytes)
                .saturating_add(new_bytes)
                > 2 * 1024 * 1024
            {
                return Err("De maaiveldtekst past niet binnen de sleufmetadata".into());
            }
        }
        if candidate != *original {
            match draft.stamp.source {
                Source::Raster(i) => self.geotiffs[i].metadata = candidate,
                Source::Virtual(i) => self.virtual_trenches[i].metadata = candidate,
            }
            self.dirty = true;
        }
        // Text/color changes preserve the measured geometry; refresh the stamp
        // without repeating context requests for each edited segment.
        let stamp =
            self.maaiveld_stamp(draft.stamp.source, draft.stamp.dataset, draft.stamp.trench)?;
        if let Some(markers) = &mut self.maaiveld_ui.markers {
            markers.stamp = stamp;
        }
        if let Some(markers) = self.maaiveld_ui.markers.clone() {
            self.maaiveld_ui.remember(markers);
        }
        self.message = format!("{} bijgewerkt.", LABELS[draft.segment]);
        Ok(())
    }
    pub(super) fn show_manual_maaiveld(&mut self, ctx: &egui::Context) {
        if let Some(preload) = &self.maaiveld_ui.preload {
            let cancel = progress_ui::window(ctx, "maaiveld-preload-progress", "Maaiveldgegevens laden",
                &preload.current, Some((preload.completed, preload.total())), true, preload.cancelled,
                |ui| {
                    ui.label(format!("{} voorbereid · {} hergebruikt · {} overgeslagen",
                        preload.prepared, preload.reused, preload.skipped));
                    ui.small("Begin-, midden- en eindpunt worden voorbereid voor direct maaiveld bewerken.");
                    if preload.partial_context > 0 {
                        ui.small(format!("{} sleuven gebruiken lokale geometrie zonder volledige kaartcontext.", preload.partial_context));
                    }
                    for failure in &preload.failures { ui.small(failure); }
                });
            if cancel {
                if let Some(preload) = &mut self.maaiveld_ui.preload { preload.cancelled = true; }
                if let Some(job) = &self.maaiveld_ui.job { job.control.cancel(); }
            }
            return;
        }
        let Some(mut draft) = self.maaiveld_ui.draft.take() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        egui::Window::new(format!("Maaiveld invullen - {}", LABELS[draft.segment]))
            .id(egui::Id::new("manual-maaiveld"))
            .open(&mut open)
            .default_size([560., 340.])
            .resizable(true)
            .show(ctx, |ui| {
                ui.label(format!("RD {:.2} / {:.2}", draft.point[0], draft.point[1]));
                ui.label("Maaiveldtekst");
                ui.text_edit_singleline(&mut draft.text);
                ui.horizontal(|ui| {
                    ui.label("Lijnkleur");
                    ui.color_edit_button_srgb(&mut draft.color);
                    ui.label(format!(
                        "#{:02X}{:02X}{:02X}",
                        draft.color[0], draft.color[1], draft.color[2]
                    ));
                });
                if !draft.error.is_empty() {
                    ui.colored_label(Color32::DARK_RED, &draft.error);
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Herstel standaard").clicked() {
                        draft.text = "INVULLEN".into();
                        draft.color = DEFAULT_COLOR;
                    }
                    cancel = ui.button("Annuleren").clicked();
                    save = ui.button("Opslaan").clicked();
                });
            });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            cancel = true;
        }
        if !open || cancel {
            return;
        }
        if save {
            match self.save_manual_maaiveld(&draft) {
                Ok(()) => return,
                Err(error) => draft.error = error,
            }
        }
        self.maaiveld_ui.draft = Some(draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escape_in_text_dialog_discards_draft_and_keeps_the_marker_mode() {
        let trench = VirtualTrench::new(
            "PS1".into(),
            Vertex {
                x: 1.,
                y: 2.,
                z: None,
            },
            Vertex {
                x: 3.,
                y: 2.,
                z: None,
            },
            0.6,
        )
        .unwrap();
        let mut app = SleufBaseApp {
            virtual_trenches: vec![trench],
            ..Default::default()
        };
        app.virtual_map.selected = Some(0);
        let stamp = app.maaiveld_stamp(Source::Virtual(0), None, None).unwrap();
        app.maaiveld_ui = State {
            active: true,
            selected: Some(Source::Virtual(0)),
            draft: Some(Draft {
                stamp,
                segment: 0,
                point: [1., 2.],
                text: "discard".into(),
                color: [1, 2, 3],
                error: String::new(),
            }),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let _ = ctx.run(raw, |ctx| {
            app.poll_manual_maaiveld(ctx);
            app.show_manual_maaiveld(ctx);
        });
        assert!(app.maaiveld_ui.active);
        assert!(app.maaiveld_ui.draft.is_none());
        assert!(!app.dirty);
    }
    #[test]
    fn segment_changes_match_original_preserve_siblings_and_trim_defaults() {
        let old = serde_json::json!({"keep":{"nested":true},(META):{"start":{"text":"A","color":"#123456"},"middle":{"text":"B","extra":true}}});
        let next = edited_metadata(&old, 1, " \u{001c}\u{00a0} ", DEFAULT_COLOR).unwrap();
        assert_eq!(next["keep"], old["keep"]);
        assert_eq!(next[META]["start"], old[META]["start"]);
        assert_eq!(
            next[META]["middle"],
            serde_json::json!({"text":"INVULLEN","color":"#808080"})
        );
        assert_eq!(old[META]["middle"]["text"], "B");
    }
    #[test]
    fn hit_radius_matches_original_nearest_and_tie_order() {
        let points = [
            Pos2::new(100., 100.),
            Pos2::new(120., 100.),
            Pos2::new(300., 100.),
        ];
        assert_eq!(nearest_screen(&points, Pos2::new(100., 114.)), Some(0));
        assert_eq!(nearest_screen(&points, Pos2::new(100., 114.01)), None);
        assert_eq!(nearest_screen(&points, Pos2::new(110., 100.)), Some(0));
        assert_eq!(nearest_screen(&points, Pos2::new(111., 100.)), Some(1));
    }
    #[test]
    fn invalid_or_oversize_text_never_mutates_metadata() {
        let old = serde_json::json!({"keep":true});
        for text in ["x\nwrong".to_owned(), "x".repeat(4097)] {
            assert!(edited_metadata(&old, 0, &text, DEFAULT_COLOR).is_err());
        }
        assert!(edited_metadata(&old, 3, "x", DEFAULT_COLOR).is_err());
        assert_eq!(old, serde_json::json!({"keep":true}));
    }
    #[test]
    fn manual_edit_keeps_measured_elevations_and_rejects_stale_source() {
        let t = VirtualTrench::new(
            "Test".into(),
            Vertex {
                x: 155000.,
                y: 463000.,
                z: Some(2.),
            },
            Vertex {
                x: 155010.,
                y: 463000.,
                z: Some(3.),
            },
            0.6,
        )
        .unwrap();
        let mut app = SleufBaseApp {
            virtual_trenches: vec![t],
            ..Default::default()
        };
        app.virtual_map.selected = Some(0);
        let before = [app.virtual_trenches[0].start, app.virtual_trenches[0].end];
        let draft = Draft {
            stamp: app.maaiveld_stamp(Source::Virtual(0), None, None).unwrap(),
            segment: 2,
            point: [155010., 463000.],
            text: "  Gras  ".into(),
            color: [18, 52, 86],
            error: String::new(),
        };
        app.save_manual_maaiveld(&draft).unwrap();
        assert!(app.dirty);
        assert_eq!(
            [app.virtual_trenches[0].start, app.virtual_trenches[0].end],
            before
        );
        assert_eq!(
            app.virtual_trenches[0].metadata[META]["end"],
            serde_json::json!({"text":"Gras","color":"#123456"})
        );
        assert!(app.save_manual_maaiveld(&draft).is_err());
    }
}
