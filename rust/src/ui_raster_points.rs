//! Point modes use the exact current canvas transform, not a second fitted map.
use super::*;
use sleufbase_rust::{
    export_worker::{Exported, Kind, Request},
    raster_points::{self as points, Candidate},
};
use std::collections::HashSet;
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Mode {
    Reference,
    Start,
}
pub(super) struct State {
    pub online: bool,
    pub selected: Option<PathBuf>,
    pub mode: Option<Mode>,
    pub rows: Vec<Candidate>,
    pub pending_generation: Option<u64>,
    attempted: HashSet<u64>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            online: true,
            selected: None,
            mode: None,
            rows: Vec::new(),
            pending_generation: None,
            attempted: HashSet::new(),
        }
    }
}
pub(super) struct View {
    pub response:egui::Response,
    pub painter: egui::Painter,
    pub rect: egui::Rect,
    pub center: Pos2,
    pub origin: [f64; 2],
    pub scale: f64,
    pub clicked: Option<Pos2>,
}
impl View {
    pub fn project(&self, p: [f64; 2]) -> Pos2 {
        Pos2::new(
            self.center.x + ((p[0] - self.origin[0]) * self.scale) as f32,
            self.center.y - ((p[1] - self.origin[1]) * self.scale) as f32,
        )
    }
    pub fn world(&self, p: Pos2) -> [f64; 2] {
        [
            self.origin[0] + f64::from(p.x - self.center.x) / self.scale,
            self.origin[1] - f64::from(p.y - self.center.y) / self.scale,
        ]
    }
}
impl SleufBaseApp {
    pub(super) fn show_copy_trench_center(&mut self, ui: &mut egui::Ui, center: [f64; 2]) {
        if ui.add_enabled(center.iter().all(|value| value.is_finite()),
            egui::Button::new("Coördinaten midden kopiëren")).clicked()
        {
            let text = format!("{:.3}, {:.3}", center[0], center[1]);
            ui.ctx().copy_text(text.clone());
            self.message = format!("Midden van de proefsleuf gekopieerd: {text}");
            ui.close_menu();
        }
    }
    pub(super) fn raster_at_map_point(&self, world: [f64; 2]) -> Option<usize> {
        // The last raster is painted on top. Use its actual affine footprint,
        // including rotation, rather than an enlarged axis-aligned bounding box.
        self.geotiffs.iter().enumerate().rev().find_map(|(index, raster)| {
            if !layer_visibility_ui::raster_is_visible(raster) { return None; }
            let (column, row) = raster.transform.world_to_pixel(world[0], world[1]).ok()?;
            (column >= 0. && row >= 0.
                && column <= f64::from(raster.width) && row <= f64::from(raster.height))
                .then_some(index)
        })
    }
    /// Continue the chosen raster tool on the newly selected source. Candidate
    /// rows always belong to one TIFF and must never survive a source change.
    pub(super) fn resume_point_mode_after_selection(&mut self, previous_mode: Option<Mode>) {
        self.raster_points.mode = None;
        self.raster_points.rows.clear();
        let Some(mode) = previous_mode else { return; };
        if self.virtual_map.selected.is_some() {
            self.message = if mode == Mode::Start {
                "Beginpunt kiezen is beschikbaar voor TIFF-proefsleuven met profielpunten. Bij virtuele en MarXact-proefsleuven ligt het begin vast aan de meetrichting.".into()
            } else {
                "Selecteer Referentiepunt om een referentie bij deze virtuele proefsleuf te plaatsen.".into()
            };
            return;
        }
        let Some(path) = self.raster_points.selected.clone() else { return; };
        if !self.geotiffs.iter().any(|r| r.path == path && layer_visibility_ui::raster_is_visible(r)) {
            self.message = "Maak deze proefsleuf eerst zichtbaar om kaartpunten te kiezen.".into();
            return;
        }
        if mode == Mode::Start {
            self.start_point_task(Some(path));
        } else {
            self.raster_points.mode = Some(Mode::Reference);
            self.message = "Klik op de kaart om het referentiepunt van de geselecteerde proefsleuf te plaatsen.".into();
        }
    }
    fn raster_point_captures_click(&self, view: &View, click: Pos2) -> bool {
        let Some(raster) = self.raster_points.selected.as_ref().and_then(|path|
            self.geotiffs.iter().find(|r| r.path == *path && layer_visibility_ui::raster_is_visible(r)))
        else { return false; };
        match self.raster_points.mode {
            Some(Mode::Start) => {
                let mpp = 1. / view.scale;
                points::pick(&self.raster_points.rows, view.world(click), mpp,
                    (mpp * 6.).clamp(0.001, 3.)).is_some()
            }
            Some(Mode::Reference) => points::stored(raster, points::REFERENCE)
                .and_then(|point| points::display(raster, point).ok())
                .is_some_and(|point| view.project(point).distance_sq(click) <= 14_f32.powi(2)),
            None => false,
        }
    }
    /// A marker edits the current trench; another visible trench changes the
    /// target. Consume the selection click so it cannot also edit an old point.
    pub(super) fn handle_point_mode_selection(&mut self, view: &View) -> bool {
        let Some(click) = view.clicked else { return false; };
        if self.maaiveld_ui.has_draft() { return true; }
        if (self.raster_points.mode.is_none() && !self.maaiveld_ui.active)
            || self.virtual_map.captures() || self.raster_rotation.active()
            || self.comment_tools.captures() || !view.scale.is_finite() || view.scale <= 0.
        {
            return false;
        }
        if self.manual_maaiveld_captures_click(view) || self.raster_point_captures_click(view, click) {
            return false;
        }
        let world = view.world(click);
        let selected_virtual = virtual_map_ui::pick_visible_trench(&self.virtual_trenches, world,
            1. / view.scale, (6. / view.scale).clamp(0.001, 3.));
        let selected_raster = if selected_virtual.is_none() {
            self.raster_at_map_point(world).map(|index| self.geotiffs[index].path.clone())
        } else { None };
        if selected_virtual.is_none() && selected_raster.is_none() { return false; }
        if selected_virtual == self.virtual_map.selected && selected_raster == self.raster_points.selected {
            return false;
        }
        let previous_mode = self.raster_points.mode;
        let old_virtual = self.virtual_map.selected;
        let old_raster = self.raster_points.selected.clone();
        self.virtual_map.selected = selected_virtual;
        self.raster_points.selected = selected_raster;
        self.update_view_selection(old_virtual, old_raster.as_deref());
        self.resume_point_mode_after_selection(previous_mode);
        self.maaiveld_selection_changed();
        view.painter.ctx().request_repaint();
        true
    }
    pub(super) fn show_raster_trench_context(&mut self, ui: &mut egui::Ui, index: usize) {
        let Some(raster) = self.geotiffs.get(index) else { return; };
        let path = raster.path.clone();
        let center = points::center(raster);
        self.show_copy_trench_center(ui, center);
        ui.separator();
        if ui.button("Maak virtuele kopie...").clicked() {
            self.make_virtual_from_raster(index);
            ui.close_menu();
        }
        ui.separator();
        self.show_physical_rotation_menu(ui);
        ui.separator();
        if ui.button("Opmerking bewerken...").clicked() {
            self.open_tiff_comment(&path); ui.close_menu();
        }
        if ui.button("Opmerking wissen").clicked() {
            self.clear_tiff_comment(&path); ui.close_menu();
        }
    }
    pub(super) fn show_point_controls(&mut self, ui: &mut egui::Ui) {
        // Preparing candidates temporarily lends the sources to the worker.
        // Do not treat that empty source vector as a deleted selection.
        if self.export_intent.is_some() { return; }
        if self
            .raster_points
            .selected
            .as_ref()
            .is_none_or(|p| !self.geotiffs.iter().any(|r| r.path == *p))
        {
            self.raster_points.selected = self.geotiffs.iter().find(|r| layer_visibility_ui::raster_is_visible(r)).map(|r| r.path.clone());
            self.raster_points.mode = None;
            self.raster_points.rows.clear();
        }
        ui.collapsing("Begin- en referentiepunten",|ui|{
            ui.checkbox(&mut self.raster_points.online,"Weg- en terreincontext online ophalen");
            ui.small("Selecteer hierboven een GeoTIFF. Punten worden bij het project bewaard en in sjablonen gebruikt.");
            let target=self.raster_points.selected.as_ref().is_some_and(|path|
                self.geotiffs.iter().any(|r| r.path==*path && layer_visibility_ui::raster_is_visible(r)));
            if ui.add_enabled(target,egui::Button::new(if self.raster_points.mode==Some(Mode::Reference){"Referentie sluiten"}else{"Referentiepunt plaatsen"})).clicked(){
                self.cancel_virtual_map();
                self.stop_raster_rotation();
                self.raster_points.mode=if self.raster_points.mode==Some(Mode::Reference){None}else{Some(Mode::Reference)};
                self.message="Klik op de kaart om het referentiepunt te plaatsen of te verplaatsen.".into();
            }
            if ui.add_enabled(target,egui::Button::new(if self.raster_points.mode==Some(Mode::Start){"Beginpunt sluiten"}else{"Beginpunt kiezen"})).clicked(){
                self.cancel_virtual_map();
                self.stop_raster_rotation();
                if self.raster_points.mode==Some(Mode::Start){self.raster_points.mode=None;}else if let Some(path)=self.raster_points.selected.clone(){self.start_point_task(Some(path));}
            }
            if ui.button("Wis alle referentiepunten").clicked(){self.clear_raster_references();}
            if ui.add_enabled(!self.geotiffs.is_empty()&&self.raster_points.pending_generation.is_none(),egui::Button::new("Laad alle beginpunten")).clicked(){
                self.raster_points.pending_generation=Some(self.project_generation);self.raster_points.attempted.clear();self.poll_point_downloads();
            }
            if self.raster_points.pending_generation.is_some(){
                ui.label("Objectgegevens voor beginpunten worden voorbereid…");
                if ui.button("Beginpunten annuleren").clicked(){self.raster_points.pending_generation=None;self.browser_ui.cancel.store(true,std::sync::atomic::Ordering::Relaxed);self.message="Beginpunten laden geannuleerd".into();}
            }
        });
    }
    pub(super) fn load_all_shell_start_points(&mut self) {
        if self.raster_points.pending_generation.is_some() {return;}
        self.raster_points.pending_generation=Some(self.project_generation);
        self.raster_points.attempted.clear();
        self.poll_point_downloads();
    }
    pub(super) fn clear_raster_references(&mut self) {
        let virtual_removed=self.virtual_trenches.iter_mut().filter(|t|t.metadata.get(points::REFERENCE).is_some()).count();
        for t in &mut self.virtual_trenches{sleufbase_rust::virtual_map::clear_reference(t);}
        self.clear_virtual_editor_metadata(points::REFERENCE,None);
        if virtual_removed>0{self.trench_markers.clear();}
        let mut removed = self
            .geotiffs
            .iter()
            .filter(|r| r.metadata.get(points::REFERENCE).is_some())
            .map(|r| r.path.clone())
            .collect::<HashSet<_>>();
        points::clear_reference(&mut self.geotiffs);
        for state in self
            .template_state
            .iter_mut()
            .chain(self.template_order_dialog.iter_mut().map(|d| &mut d.state))
        {
            for row in &mut state.rows {
                if row
                    .metadata
                    .as_object_mut()
                    .and_then(|m| m.remove(points::REFERENCE))
                    .is_some()
                {
                    if let Some(path) = &row.raster {
                        removed.insert(path.clone());
                    }
                }
            }
        }
        self.dirty |= !removed.is_empty() || virtual_removed>0;
        self.message = if removed.is_empty() && virtual_removed==0 {
            "Geen referentiepunten om te wissen.".into()
        } else {
            format!("{} referentiepunt(en) gewist.", removed.len()+virtual_removed)
        };
    }
    pub(super) fn start_point_task(&mut self, target: Option<PathBuf>) -> bool {
        if target.as_ref().is_some_and(|path| !self.geotiffs.iter().any(|r|
            r.path==*path && layer_visibility_ui::raster_is_visible(r))) {
            self.message="Zet de proefsleuf eerst zichtbaar om een beginpunt op de kaart te kiezen.".into();
            return false;
        }
        let settings = match self.project_export_settings() {
            Ok(settings) => settings,
            Err(error) => { self.message = error; return false; }
        };
        self.raster_points.mode = None;
        self.raster_points.rows.clear();
        let kind = if target.is_some() {
            Kind::StartCandidates
        } else {
            Kind::StartPoints
        };
        let request = Request {
            kind,
            path: PathBuf::new(),
            selection: sleufbase_rust::object_selection::Selection::all(0, 0),
            rules: settings.object_rules,
            extra_layers: false,
            options: Default::default(),
            template: None,
            discipline_excel: false,
            maps: None,
            point_target: target,
            point_network: self.raster_points.online,
        };
        self.start_export(request, false)
    }
    pub(super) fn poll_point_downloads(&mut self) {
        let Some(generation) = self.raster_points.pending_generation else {
            return;
        };
        if generation != self.project_generation
            || self
                .browser_ui
                .cancel
                .load(std::sync::atomic::Ordering::Relaxed)
                && !self.raster_points.attempted.is_empty()
        {
            self.raster_points.pending_generation = None;
            return;
        }
        if self.live_busy
            || self.pending_project_action.is_some()
            || self.file_intent.is_some()
            || self.export_intent.is_some()
            || self.template_job.is_some()
            || self.browser_ui.batch_mode
            || !self.browser_ui.downloaded_json.is_empty()
            || !self.browser_ui.downloaded_tiffs.is_empty()
        {
            return;
        }
        let missing = self
            .geotiffs
            .iter()
            .filter(|r| points::local_dataset_path(r).is_none())
            .filter_map(|r| r.kickthemap_job_id)
            .filter(|id| {
                !self.datasets.iter().any(|d| d.job_id == *id)
                    && !self.raster_points.attempted.contains(id)
                    && self.live_jobs.iter().any(|j| j.job_id == *id)
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .take(500)
            .collect::<Vec<_>>();
        if !missing.is_empty() && self.live_session.is_some() {
            self.raster_points.attempted.extend(missing.iter().copied());
            self.queue_kickthemap_jobs(&missing, false, true);
            return;
        }
        self.raster_points.pending_generation = None;
        self.start_point_task(None);
    }
    pub(super) fn finish_point_task(&mut self, kind: Kind, exported: Exported) {
        if kind == Kind::StartCandidates {
            if let Some((path, rows)) = exported.point_candidates {
                // A completed preparation may never reactivate an older target.
                if self.virtual_map.selected.is_some() || self.raster_points.selected.as_ref() != Some(&path) {
                    return;
                }
                if !self.geotiffs.iter().any(|r| r.path==path && layer_visibility_ui::raster_is_visible(r)) {
                    self.raster_points.mode=None;
                    self.raster_points.rows.clear();
                    self.message="Beginpunt kiezen gesloten: de proefsleuf is niet zichtbaar.".into();
                    return;
                }
                self.raster_points.selected = Some(path);
                self.raster_points.mode = (rows.len() >= 2).then_some(Mode::Start);
                self.raster_points.rows = rows;
                self.message = if self.raster_points.mode.is_some() {
                    "Klik op een eindpuntmarker om het beginpunt van het dwarsprofiel te kiezen."
                        .into()
                } else {
                    "Geen beginpuntkandidaten beschikbaar voor deze proefsleuf.".into()
                };
                for warning in exported.warnings.iter().take(15) {
                    self.message.push('\n');
                    self.message.push_str(warning);
                }
            }
        } else {
            let count = exported.point_updates.len();
            match points::apply(&mut self.geotiffs, &exported.point_updates) {
                Ok(changed) => {
                    self.dirty |= changed > 0;
                    self.message = if count == 0 {
                        "Er konden geen beginpunten worden geladen.".into()
                    } else {
                        format!(
                            "Beginpunten geladen voor {count} KickTheMap-proefsleuf/proefsleuven."
                        )
                    };
                }
                Err(e) => self.message = format!("Beginpunten niet gewijzigd: {e}"),
            }
            for warning in exported.warnings.iter().take(15) {
                self.message.push('\n');
                self.message.push_str(warning);
            }
        }
    }
    pub(super) fn handle_raster_points(&mut self, view: &View) {
        let Some(mode) = self.raster_points.mode else {
            return;
        };
        let Some(index) = self
            .raster_points
            .selected
            .as_ref()
            .and_then(|p| self.geotiffs.iter().position(|r| r.path == *p && layer_visibility_ui::raster_is_visible(r)))
        else {
            self.raster_points.mode=None;
            self.raster_points.rows.clear();
            return;
        };
        if mode == Mode::Reference {
            if let Some(raw) = points::stored(&self.geotiffs[index], points::REFERENCE) {
                if let Ok(p) = points::display(&self.geotiffs[index], raw) {
                    let p = view.project(p);
                    if view.rect.contains(p) {
                        view.painter.circle_filled(p, 9., Color32::WHITE);
                        view.painter
                            .circle_stroke(p, 9., Stroke::new(2.0_f32, Color32::BLACK));
                    }
                }
            }
        } else {
            for row in &self.raster_points.rows {
                let p = view.project(row.display);
                if view.rect.contains(p) {
                    view.painter.circle_filled(
                        p,
                        10.,
                        if row.selected { ORANGE } else { Color32::WHITE },
                    );
                    view.painter
                        .circle_stroke(p, 10., Stroke::new(2.0_f32, DARK));
                }
            }
        }
        if let Some(click) = view.clicked {
            let world = view.world(click);
            let result = if mode == Mode::Reference {
                points::set(&mut self.geotiffs[index], points::REFERENCE, world, true)
            } else {
                let mpp = 1. / view.scale;
                if let Some(i) = points::pick(
                    &self.raster_points.rows,
                    world,
                    mpp,
                    (mpp * 6.).clamp(0.001, 3.),
                ) {
                    let raw = self.raster_points.rows[i].raw;
                    for row in &mut self.raster_points.rows {
                        row.selected = (row.raw[0] - raw[0]).abs() <= 1e-6
                            && (row.raw[1] - raw[1]).abs() <= 1e-6;
                    }
                    points::set(&mut self.geotiffs[index], points::START, raw, false)
                } else {
                    self.message = "Klik op een eindpuntmarker om het beginpunt te kiezen.".into();
                    return;
                }
            };
            match result {
                Ok(changed) => {
                    self.dirty |= changed;
                    view.painter.ctx().request_repaint();
                    self.message = if mode == Mode::Reference {
                        "Referentiepunt bijgewerkt.".into()
                    } else {
                        "Beginpunt bijgewerkt.".into()
                    };
                }
                Err(e) => self.message = e,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn app() -> SleufBaseApp {
        let data: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/raster-points/reference.json"
        ))
        .unwrap();
        let c = &data["cases"][1];
        let transform =
            serde_json::from_value::<sleufbase_rust::geo::GeoTransform>(c["transform"].clone())
                .unwrap()
                .rotate_degrees(37.5, 20, 20)
                .unwrap();
        let r = GeoTiffInfo {
            path: "PS-2.tif".into(),
            width: 20,
            height: 20,
            transform,
            bounds: transform.bounds(20, 20).unwrap(),
            epsg: Some(28992),
            kickthemap_job_id: Some(2),
            kickthemap_job_title: "PS2".into(),
            kickthemap_job_sidecar: None,
            metadata: c["metadata"].clone(),
        };
        let mut app = SleufBaseApp {
            geotiffs: vec![r],
            datasets: vec![serde_json::from_value(c["dataset"].clone()).unwrap()],
            ..Default::default()
        };
        app.raster_points.selected = Some("PS-2.tif".into());
        app.raster_points.online = false;
        app
    }
    fn frame(app: &mut SleufBaseApp, ctx: &egui::Context, events: Vec<egui::Event>) -> View {
        let mut result = None;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(900., 600.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let view = canvas(
                        ui,
                        &[],
                        &app.geotiffs,
                        &[],
                        &[],
                        &[],
                        &[],
                        &[],
                        None,
                        &[],
                        None,
                        None,
                        false,
                        &mut app.zoom,
                        &mut app.pan,
                        &app.migrated_settings.object_rules,
                        Some(&mut app.navigation),
                    )
                    .unwrap();
                    app.handle_raster_points(&view);
                    result = Some(view);
                });
            },
        );
        result.unwrap()
    }
    fn click(app: &mut SleufBaseApp, ctx: &egui::Context, p: Pos2) {
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(p),
                    egui::Event::PointerButton {
                        pos: p,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
    }
    #[test]
    fn real_canvas_pointer_click_stores_unrotated_reference_after_zoom_and_pan() {
        let ctx = egui::Context::default();
        let mut app = app();
        app.zoom = 1.7;
        app.pan = egui::vec2(23., -17.);
        app.raster_points.mode = Some(Mode::Reference);
        let c: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/raster-points/reference.json"
        ))
        .unwrap();
        let c = &c["cases"][1];
        let target: [f64; 2] = serde_json::from_value(c["reference_click"].clone()).unwrap();
        let view = frame(&mut app, &ctx, Vec::new());
        let p = view.project(target);
        assert!(view.rect.contains(p));
        click(&mut app, &ctx, p);
        let stored = points::stored(&app.geotiffs[0], points::REFERENCE).unwrap();
        let expected = points::point(&c["after_reference"][points::REFERENCE]).unwrap();
        assert!((stored[0] - expected[0]).hypot(stored[1] - expected[1]) < 1e-5);
        assert!(app.dirty);
        assert_eq!(app.geotiffs[0].metadata["notes"], c["metadata"]["notes"]);
    }
    #[test]
    fn actual_candidate_worker_returns_sources_and_canvas_selection_updates_raw_start() {
        let ctx = egui::Context::default();
        let mut app = app();
        let source_pointer = app.geotiffs.as_ptr();
        let feature_pointer = app.datasets[0].features.as_ptr();
        assert!(app.start_point_task(Some("PS-2.tif".into())));
        assert!(app.geotiffs.is_empty());
        let until = Instant::now() + Duration::from_secs(10);
        while app.export_intent.is_some() {
            app.poll_export_jobs(&ctx);
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(app.geotiffs.as_ptr(), source_pointer);
        assert_eq!(app.datasets[0].features.as_ptr(), feature_pointer);
        assert!(!app.dirty);
        assert!(app.raster_points.mode == Some(Mode::Start));
        let row = app.raster_points.rows.last().unwrap().clone();
        let view = frame(&mut app, &ctx, Vec::new());
        click(&mut app, &ctx, view.project(row.display));
        assert_eq!(
            points::stored(&app.geotiffs[0], points::START),
            Some(row.raw)
        );
        assert!(app.dirty);
    }
    #[test]
    fn clear_all_invalidates_older_slot_snapshots_and_no_op_does_not_dirty() {
        let mut app = app();
        app.clear_raster_references();
        assert!(!app.dirty);
        assert_eq!(app.message, "Geen referentiepunten om te wissen.");
        app.geotiffs[0].metadata[points::REFERENCE] = json!({"x":1,"y":2});
        let mut state = sleufbase_rust::template_order::State::default();
        state.sync(&app.geotiffs).unwrap();
        state.rows[0].metadata =
            json!({"template_reference_point":{"x":1,"y":2},"keep":"old slot option"});
        app.template_state = Some(state);
        app.clear_raster_references();
        assert_eq!(app.message, "1 referentiepunt(en) gewist.");
        assert!(app.dirty);
        assert!(points::stored(&app.geotiffs[0], points::REFERENCE).is_none());
        assert_eq!(
            app.template_state.as_ref().unwrap().rows[0].metadata["keep"],
            "old slot option"
        );
        assert!(app.template_state.as_ref().unwrap().rows[0]
            .metadata
            .get(points::REFERENCE)
            .is_none());
        app.dirty = false;
        app.clear_raster_references();
        assert!(!app.dirty);
    }
    #[test]
    fn pending_bulk_points_wait_for_download_loading_and_cancel_on_project_switch() {
        let mut app = app();
        let (started, rx) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        app.exporter = Some(
            sleufbase_rust::export_worker::Exporter::with_processor(move |_, request, control| {
                assert_eq!(request.kind, Kind::StartPoints);
                started.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                assert!(control.is_cancelled());
                Ok(Exported {
                    point_updates: Vec::new(),
                    point_candidates: None,
                    skipped: Vec::new(),
                    companion: None,
                    warnings: Vec::new(),
                })
            })
            .unwrap(),
        );
        app.raster_points.pending_generation = Some(app.project_generation);
        app.browser_ui.batch_mode = true;
        app.poll_point_downloads();
        assert!(app.export_intent.is_none());
        assert!(app.raster_points.pending_generation.is_some());
        app.browser_ui.batch_mode = false;
        app.poll_point_downloads();
        assert!(app.export_intent.is_some());
        rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let ctx = egui::Context::default();
        app.request_project_action(ProjectAction::New, &ctx);
        release.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(10);
        while app.export_intent.is_some() {
            app.poll_export_jobs(&ctx);
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(app.geotiffs.is_empty());
        assert!(app.raster_points.pending_generation.is_none());
        assert!(app.raster_points.mode.is_none());
    }
}
