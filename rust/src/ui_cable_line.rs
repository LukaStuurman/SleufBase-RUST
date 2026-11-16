//! Two-click cable selection. Only completed, current-source results replace
//! the existing candidates; preview and cancellation never edit CAD geometry.
use super::*;
use sleufbase_rust::{cad_scene::Scene, cable_line_selection::Query};
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, Weak},
};

#[derive(Default)]
pub(super) struct State {
    session: Option<Session>,
    consume_cancelled_click: bool,
}

struct Session {
    generation: u64,
    sources: Vec<SourceStamp>,
    start: Option<[f64; 2]>,
    end: Option<[f64; 2]>,
    preview: Option<[f64; 2]>,
    query: Option<Query>,
}

struct SourceStamp {
    scene: Weak<Scene>,
    visibility: Weak<Vec<bool>>,
    address: usize,
    count: usize,
    path: PathBuf,
    visible: bool,
    layers: u64,
}

fn layer_visibility(overlay: &DxfOverlay) -> u64 {
    let mut hash = DefaultHasher::new();
    for (name, layer) in &overlay.layers {
        name.hash(&mut hash);
        layer.visible.hash(&mut hash);
    }
    hash.finish()
}

impl SourceStamp {
    fn capture(overlay: &DxfOverlay) -> Self {
        Self {
            scene: Arc::downgrade(&overlay.expanded),
            visibility: Arc::downgrade(&overlay.block_visibility),
            address: overlay.features.as_ptr() as usize,
            count: overlay.features.len(),
            path: overlay.path.clone(),
            visible: overlay.visible,
            layers: layer_visibility(overlay),
        }
    }

    fn matches(&self, overlay: &DxfOverlay) -> bool {
        self.visible == overlay.visible && self.path == overlay.path
            && self.address == overlay.features.as_ptr() as usize
            && self.count == overlay.features.len()
            && self.scene.upgrade().is_some_and(|scene| Arc::ptr_eq(&scene, &overlay.expanded))
            && self.visibility.upgrade().is_some_and(|visible| Arc::ptr_eq(&visible, &overlay.block_visibility))
            && self.layers == layer_visibility(overlay)
    }
}

impl State {
    pub(super) fn active(&self) -> bool { self.session.is_some() }
    pub(super) fn busy(&self) -> bool {
        self.session.as_ref().is_some_and(|session| session.query.is_some())
    }
    pub(super) fn clear(&mut self) {
        self.session = None;
        self.consume_cancelled_click = false;
    }
}

impl SleufBaseApp {
    fn cable_line_blocked(&self) -> bool {
        self.browser_ui.web_visible || self.project_sources_owned()
            || self.file_intent.is_some() || self.export_intent.is_some()
            || self.template_job.is_some() || self.pending_project_action.is_some()
            || self.raster_points.pending_generation.is_some() || self.maaiveld_ui.preloading()
            || self.maaiveld_ui.has_draft() || self.comment_tools.has_draft()
            || self.virtual_map.has_dialog() || self.trench_editor.is_some()
            || self.show_trench_dialog || self.tiff_comment_editor.is_some()
            || self.settings_dialog.is_some() || self.shell.marxact_open
            || self.map_export_open || self.template_dialog.is_some()
            || self.template_order_dialog.is_some() || self.project_template_dialog.is_some()
    }

    pub(super) fn toggle_cable_line_selection(&mut self) {
        if self.cable_line.active() {
            self.cable_line.clear();
            self.message = "Lijnselectie geannuleerd.".into();
            return;
        }
        if self.cable_line_blocked() {
            self.message = "Rond de lopende taak af of sla het open concept op/sluit het venster voordat je kabels met een lijn selecteert.".into();
            return;
        }
        if !self.overlays.iter().any(|overlay| overlay.visible) {
            self.message = "Maak eerst een KLIC-overlay zichtbaar om kabels met een lijn te selecteren.".into();
            return;
        }
        self.close_maaiveld_mode();
        self.cancel_virtual_map();
        self.stop_raster_rotation();
        self.raster_points.mode = None;
        self.raster_points.rows.clear();
        self.comment_tools = Default::default();
        self.cable_line.consume_cancelled_click = false;
        self.cable_line.session = Some(Session {
            generation: self.project_generation,
            sources: self.overlays.iter().map(SourceStamp::capture).collect(),
            start: None, end: None, preview: None, query: None,
        });
        self.message = "Lijnselectie: klik het beginpunt en daarna het eindpunt. Esc of rechtsklik annuleert.".into();
    }

    /// Called before modal/worker early returns as well as immediately before
    /// map input, so another tool or changed drawing can never publish old hits.
    pub(super) fn poll_cable_line_selection(&mut self, ctx: &egui::Context) {
        if !self.cable_line.active() { return; }
        if self.cable_line_blocked() || self.maaiveld_ui.captures()
            || self.virtual_map.captures() || self.raster_rotation.active()
            || self.raster_points.mode.is_some() || self.comment_tools.captures()
        {
            self.cable_line.clear();
            ctx.request_repaint();
            return;
        }
        let current = self.cable_line.session.as_ref().is_some_and(|session|
            session.generation == self.project_generation
                && session.sources.len() == self.overlays.len()
                && session.sources.iter().zip(&self.overlays).all(|(stamp, overlay)| stamp.matches(overlay)));
        if !current {
            self.cable_line.clear();
            self.cable_line.consume_cancelled_click = true;
            self.message = "Lijnselectie geannuleerd: de kaartlagen of hun zichtbaarheid zijn gewijzigd.".into();
            ctx.request_repaint();
        }
    }

    pub(super) fn handle_cable_line_selection(&mut self, view: &raster_point_ui::View) -> bool {
        self.poll_cable_line_selection(view.painter.ctx());
        // A second endpoint click that coincides with a source refresh must
        // not fall through as an unrelated ordinary cable-picking click.
        if std::mem::take(&mut self.cable_line.consume_cancelled_click) { return true; }
        let Some(mut session) = self.cable_line.session.take() else { return false; };
        let ctx = view.painter.ctx();
        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) || view.response.secondary_clicked() {
            self.message = "Lijnselectie geannuleerd.".into();
            ctx.request_repaint();
            return true;
        }
        if view.response.hovered() {
            ctx.set_cursor_icon(if session.query.is_some() { egui::CursorIcon::Progress }
                else { egui::CursorIcon::Crosshair });
            if session.query.is_none() {
                session.preview = view.response.hover_pos().map(|point| view.world(point))
                    .filter(|point| point.iter().all(|coordinate| coordinate.is_finite()));
            }
        }
        if session.query.is_none() {
            if let Some(click) = view.clicked {
                let point = view.world(click);
                if point.iter().all(|coordinate| coordinate.is_finite()) {
                    if let Some(start) = session.start {
                        match Query::new(start, point) {
                            Ok(query) => {
                                session.end = Some(point);
                                session.query = Some(query);
                                self.message = "Zichtbare kabels langs de lijn zoeken…".into();
                            }
                            Err(error) => self.message = format!("Kies een ander eindpunt: {error}"),
                        }
                    } else {
                        session.start = Some(point);
                        session.preview = Some(point);
                        self.message = "Lijnselectie: klik nu het eindpunt om alle geraakte kabels te selecteren.".into();
                    }
                    ctx.request_repaint();
                }
            }
        }
        if let Some(query) = &mut session.query {
            if let Err(error) = query.advance(&self.overlays) {
                self.message = format!("Lijnselectie afgebroken: {error}");
                ctx.request_repaint();
                return true;
            }
            if query.finished() {
                let candidates = session.query.take().unwrap().into_results();
                self.found_layers = candidates;
                self.open_cable_candidate_list();
                if let Some((overlay, feature)) = self.found_layers.first().copied() {
                    self.select_dxf(overlay, feature);
                } else {
                    self.dxf_selection = None;
                    self.message = "Geen zichtbare kabels door deze lijn geraakt.".into();
                }
                ctx.request_repaint();
                return true;
            }
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        self.cable_line.session = Some(session);
        true
    }

    pub(super) fn paint_cable_line_selection(&self, view: &raster_point_ui::View) {
        if let Some(session) = &self.cable_line.session { paint(view, session); }
    }
}

fn screen(view: &raster_point_ui::View, point: [f64; 2]) -> [f64; 2] {
    [f64::from(view.center.x) + (point[0] - view.origin[0]) * view.scale,
        f64::from(view.center.y) - (point[1] - view.origin[1]) * view.scale]
}

// Clip in f64 before conversion to egui positions: a retained world-space
// start remains safe while the user pans or zooms far away from it.
fn clipped_line(a: [f64; 2], b: [f64; 2], rect: egui::Rect) -> Option<[Pos2; 2]> {
    if a.iter().chain(b.iter()).any(|value| !value.is_finite()) { return None; }
    let delta = [b[0] - a[0], b[1] - a[1]];
    if delta.iter().any(|value| !value.is_finite()) { return None; }
    let (mut first, mut last) = (0_f64, 1_f64);
    for (p, q) in [(-delta[0], a[0] - f64::from(rect.min.x)),
        (delta[0], f64::from(rect.max.x) - a[0]),
        (-delta[1], a[1] - f64::from(rect.min.y)),
        (delta[1], f64::from(rect.max.y) - a[1])]
    {
        if p == 0. {
            if q < 0. { return None; }
        } else {
            let fraction = q / p;
            if p < 0. { first = first.max(fraction); } else { last = last.min(fraction); }
            if first > last { return None; }
        }
    }
    Some([Pos2::new((a[0] + first * delta[0]) as f32, (a[1] + first * delta[1]) as f32),
        Pos2::new((a[0] + last * delta[0]) as f32, (a[1] + last * delta[1]) as f32)])
}

fn paint(view: &raster_point_ui::View, session: &Session) {
    let rect = view.rect.intersect(view.painter.clip_rect());
    if !rect.is_positive() { return; }
    let painter = view.painter.with_clip_rect(rect);
    if let Some(start) = session.start {
        let start = screen(view, start);
        if let Some(end) = session.end.or(session.preview) {
            if let Some(line) = clipped_line(start, screen(view, end), rect.expand(4.)) {
                painter.line_segment(line, Stroke::new(5_f32, Color32::WHITE));
                painter.line_segment(line, Stroke::new(2.5_f32, ORANGE));
            }
        }
        for (point, is_start) in std::iter::once((start, true))
            .chain(session.end.map(|point| (screen(view, point), false)))
        {
            if point.iter().all(|value| value.is_finite())
                && point[0] >= f64::from(rect.min.x) && point[0] <= f64::from(rect.max.x)
                && point[1] >= f64::from(rect.min.y) && point[1] <= f64::from(rect.max.y)
            {
                let point = Pos2::new(point[0] as f32, point[1] as f32);
                painter.circle_filled(point, 6., Color32::WHITE);
                painter.circle_stroke(point, 6., Stroke::new(2.5_f32, ORANGE));
                if is_start {
                    painter.text(point + egui::vec2(11., 9.), egui::Align2::LEFT_TOP,
                        "Start", egui::FontId::proportional(12.), INK);
                }
            }
        }
    }
    let instruction = if let Some(query) = &session.query {
        format!("Lijnselectie · Kabels zoeken… {} geraakt. Esc of rechtsklik annuleert.", query.matched_count())
    } else if session.start.is_some() {
        "Lijnselectie · Klik het eindpunt. Esc of rechtsklik annuleert.".into()
    } else {
        "Lijnselectie · Klik het beginpunt. Esc of rechtsklik annuleert.".into()
    };
    let width = (rect.width() - 44.).clamp(1., 560.);
    let text = painter.layout(instruction, egui::FontId::proportional(13.), INK, width);
    let position = rect.min + egui::vec2(22., 19.);
    let backing = egui::Rect::from_min_size(position, text.size()).expand2(egui::vec2(10., 7.));
    painter.rect_filled(backing, 8_u8, Color32::from_rgba_unmultiplied(255, 255, 255, 245));
    painter.rect_stroke(backing, 8_u8, Stroke::new(1_f32, ORANGE), egui::StrokeKind::Inside);
    painter.galley(position, text, INK);
}
