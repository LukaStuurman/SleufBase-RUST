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
    hash::{DefaultHasher, Hasher},
    io::Write,
    sync::Arc,
};

const KEYS: [&str; 3] = ["start", "middle", "end"];
const LABELS: [&str; 3] = ["Beginpunt", "Middenpunt", "Eindpunt"];
const META: &str = "template_maaiveld_segments";
const DEFAULT_COLOR: [u8; 3] = [128; 3];
const RADIUS: f32 = 9.;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp {
    source: Source,
    generation: u64,
    dataset: Option<usize>,
    trench: Option<usize>,
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
struct Markers {
    stamp: Stamp,
    points: [[f64; 2]; 3],
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
}
impl State {
    pub(super) fn captures(&self) -> bool {
        self.active
    }
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
    fn maaiveld_selected(&self) -> Option<Source> {
        self.virtual_map
            .selected
            .filter(|&i| i < self.virtual_trenches.len())
            .map(Source::Virtual)
            .or_else(|| {
                self.raster_points
                    .selected
                    .as_ref()
                    .and_then(|path| self.geotiffs.iter().position(|r| r.path == *path))
                    .map(Source::Raster)
            })
            .or_else(|| (!self.geotiffs.is_empty()).then_some(Source::Raster(0)))
            .or_else(|| (!self.virtual_trenches.is_empty()).then_some(Source::Virtual(0)))
    }
    fn maaiveld_metadata(&self, source: Source) -> Result<&serde_json::Value, String> {
        match source {
            Source::Raster(i) => self.geotiffs.get(i).map(|r| &r.metadata),
            Source::Virtual(i) => self.virtual_trenches.get(i).map(|t| &t.metadata),
        }
        .ok_or("De gekozen proefsleuf is niet meer beschikbaar".into())
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
        serde_json::to_writer(
            &mut writer,
            self.project_settings
                .as_ref()
                .unwrap_or(&self.migrated_settings),
        )
        .map_err(|e| e.to_string())?;
        Ok(Stamp {
            source,
            generation: self.project_generation,
            dataset,
            trench,
            digest: writer.0.finish(),
        })
    }
    fn maaiveld_unchanged(&self, stamp: &Stamp) -> bool {
        self.maaiveld_selected() == Some(stamp.source)
            && self
                .maaiveld_stamp(stamp.source, stamp.dataset, stamp.trench)
                .as_ref()
                == Ok(stamp)
    }
    // Clone only the selected layer and its uniquely associated profile input;
    // unrelated project allocations and account/session state remain untouched.
    fn maaiveld_input(&self, source: Source) -> Result<(NativeProject, Source, Stamp), String> {
        let mut project = NativeProject::default();
        let (mut dataset, mut trench) = (None, None);
        let local_source = match source {
            Source::Virtual(i) => {
                project.virtual_trenches.push(
                    self.virtual_trenches
                        .get(i)
                        .ok_or("Proefsleuf ontbreekt")?
                        .clone(),
                );
                Source::Virtual(0)
            }
            Source::Raster(i) => {
                let raster = self.geotiffs.get(i).ok_or("TIFF ontbreekt")?;
                project.rasters.push(raster.clone());
                let local = raster
                    .metadata
                    .get("kickthemap_job_features_path")
                    .and_then(serde_json::Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(|s| {
                        let path = PathBuf::from(s);
                        if path.is_absolute() {
                            path
                        } else {
                            raster
                                .path
                                .parent()
                                .unwrap_or(std::path::Path::new("."))
                                .join(path)
                        }
                    })
                    .is_some_and(|p| p.is_file());
                if !local {
                    let jobs: Vec<_> = self
                        .datasets
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| {
                            raster
                                .kickthemap_job_id
                                .is_some_and(|id| id > 0 && d.job_id == id)
                        })
                        .map(|(j, _)| j)
                        .collect();
                    if jobs.len() > 1 {
                        return Err("De TIFF-jobkoppeling is niet eenduidig".into());
                    }
                    if let Some(&j) = jobs.first() {
                        dataset = Some(j);
                    } else {
                        let choices: Vec<_> = sleufbase_rust::template_project::suggest(
                            &self.datasets,
                            &self.geotiffs,
                            &self.virtual_trenches,
                        )
                        .into_iter()
                        .filter(|c| c.raster == Some(i))
                        .collect();
                        if choices.len() != 1 {
                            return Err(
                                "Koppel deze TIFF eerst eenduidig aan lokale profielobjecten"
                                    .into(),
                            );
                        }
                        match choices[0].source {
                            sleufbase_rust::template_project::Source::Job(j) => dataset = Some(j),
                            sleufbase_rust::template_project::Source::Virtual(j) => trench = Some(j),
                            _ => return Err("Geen lokale profielobjecten beschikbaar; laad eerst de gekoppelde KickTheMap-jobdata".into()),
                        }
                    }
                    if let Some(j) = dataset {
                        project.datasets.push(self.datasets[j].clone());
                    }
                    if let Some(j) = trench {
                        project
                            .virtual_trenches
                            .push(self.virtual_trenches[j].clone());
                    }
                }
                Source::Raster(0)
            }
        };
        Ok((
            project,
            local_source,
            self.maaiveld_stamp(source, dataset, trench)?,
        ))
    }
    pub(super) fn close_maaiveld_mode(&mut self) {
        self.maaiveld_ui = State::default();
    }
    pub(super) fn toggle_maaiveld_mode(&mut self) {
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
        self.maaiveld_ui.selected = self.maaiveld_selected();
        self.maaiveld_ui.job = None;
        self.maaiveld_ui.markers = None;
        self.maaiveld_ui.draft = None;
        let result = (|| {
            let source = self
                .maaiveld_selected()
                .ok_or("Kies eerst een proefsleuf om maaiveldpunten te tonen.")?;
            let (project, local_source, stamp) = self.maaiveld_input(source)?;
            let settings = self
                .project_settings
                .as_ref()
                .unwrap_or(&self.migrated_settings)
                .clone();
            let control = Arc::new(Control::default());
            let task_control = control.clone();
            let (tx, rx) = mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("manual-maaiveld-profile".into())
                .spawn(move || {
                    let result =
                        maaiveld_profile::prepare(&project, local_source, &settings, &task_control);
                    let _ = tx.send(result);
                })
                .map_err(|e| e.to_string())?;
            Ok::<_, String>(Job { stamp, control, rx })
        })();
        match result {
            Ok(job) => {
                self.maaiveld_ui.job = Some(job);
                self.message = "Maaiveldpunten van het dwarsprofiel voorbereiden…".into();
            }
            Err(error) => self.message = error,
        }
    }
    pub(super) fn poll_manual_maaiveld(&mut self, ctx: &egui::Context) {
        if !self.maaiveld_ui.active {
            return;
        }
        if self.maaiveld_ui.draft.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.close_maaiveld_mode();
            return;
        }
        if !self
            .project_settings
            .as_ref()
            .unwrap_or(&self.migrated_settings)
            .export_preferences
            .template_enable_maaiveld_edit_points
            || self.project_sources_owned()
        {
            self.close_maaiveld_mode();
            return;
        }
        let selected = self.maaiveld_selected();
        if self.maaiveld_ui.selected != selected {
            self.refresh_manual_maaiveld();
        }
        let Some(job) = &self.maaiveld_ui.job else {
            return;
        };
        let result = match job.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("Voorbereiden maaiveldpunten is afgebroken".into())
            }
        };
        let job = self.maaiveld_ui.job.take().unwrap();
        if !self.maaiveld_unchanged(&job.stamp) {
            self.message = "De proefsleuf is gewijzigd; maak de maaiveldpunten opnieuw aan.".into();
            return;
        }
        match result {
            Ok(prepared) if prepared.points.len() == 3 => {
                self.maaiveld_ui.markers = Some(Markers {
                    stamp: job.stamp.clone(),
                    points: std::array::from_fn(|i| [prepared.points[i].x, prepared.points[i].y]),
                });
                self.message = "Klik op begin, midden of eind van het dwarsprofiel om maaiveldtekst en kleur te kiezen.".into();
                if !prepared.context_failures.is_empty() {
                    self.message.push_str(" Weg- of terreincontext kon niet volledig worden opgehaald; lokale profielgeometrie is gebruikt.");
                }
            }
            Ok(_) => self.message = "Geen drie geldige maaiveldpunten beschikbaar.".into(),
            Err(error) => self.message = error,
        }
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
        self.message = format!("{} bijgewerkt.", LABELS[draft.segment]);
        Ok(())
    }
    pub(super) fn show_manual_maaiveld(&mut self, ctx: &egui::Context) {
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
