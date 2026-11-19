//! Independent RD overview map for the persistent point-cloud catalog.
//! Raster extents stay in world coordinates when the window is resized or panned.
use super::*;
use sleufbase_rust::osm_tiles::{OsmRaster, ViewportRenderer};

pub(super) struct Location {
    pub id: String,
    pub title: String,
    pub center: [f64; 3],
    pub bounds: Bounds,
    pub axis: Option<[Vertex; 2]>,
}

#[derive(Default)]
pub(super) struct State {
    bounds: Option<Bounds>,
    texture: Option<(Bounds, egui::TextureHandle)>,
    loading: Option<Background>,
    request_after: f64,
    refresh: bool,
    status: String,
    surface: Option<(egui::Rect, egui::LayerId)>,
    selection: Option<SelectionDrag>,
}

struct Background {
    bounds: Bounds,
    size: [u32; 2],
    cancel: Arc<AtomicBool>,
    receiver: Receiver<Result<OsmRaster, String>>,
}
impl Drop for Background {
    fn drop(&mut self) { self.cancel.store(true, Ordering::Release); }
}

impl State {
    pub(super) fn pause(&mut self) {
        self.loading = None;
        self.surface = None;
        self.selection = None;
    }
    pub(super) fn fit_selection(&mut self, source: &PointCloudSource) {
        if source.crs == CloudCrs::RdNew {
            self.bounds = Some(padded(Bounds {
                min_x: source.bounds_min[0], max_x: source.bounds_max[0],
                min_y: source.bounds_min[1], max_y: source.bounds_max[1],
            }));
        }
    }
}

pub(super) fn window_movable(ctx: &egui::Context, state: &State) -> bool {
    if state.selection.is_some() { return false; }
    let Some((rect, layer)) = state.surface else { return true; };
    let origin = ctx.input(|input| input.pointer.any_down().then(|| input.pointer.press_origin()).flatten());
    !origin.is_some_and(|point| rect.contains(point) && ctx.layer_id_at(point) == Some(layer))
}

fn padded(bounds: Bounds) -> Bounds {
    let pad = 80_f64.max((bounds.max_x - bounds.min_x) * 0.18)
        .max((bounds.max_y - bounds.min_y) * 0.18);
    Bounds { min_x: bounds.min_x - pad, max_x: bounds.max_x + pad,
        min_y: bounds.min_y - pad, max_y: bounds.max_y + pad }
}

fn all_bounds(locations: &[Location]) -> Option<Bounds> {
    let first = locations.first()?;
    let mut bounds = first.bounds;
    for point in locations.iter().skip(1) {
        bounds.min_x = bounds.min_x.min(point.bounds.min_x);
        bounds.max_x = bounds.max_x.max(point.bounds.max_x);
        bounds.min_y = bounds.min_y.min(point.bounds.min_y);
        bounds.max_y = bounds.max_y.max(point.bounds.max_y);
    }
    Some(padded(bounds))
}

fn viewport_bounds(base: Bounds, size: egui::Vec2) -> Bounds {
    let scale = ((base.max_x - base.min_x) / f64::from(size.x.max(1.)))
        .max((base.max_y - base.min_y) / f64::from(size.y.max(1.)));
    let cx = (base.min_x + base.max_x) * 0.5;
    let cy = (base.min_y + base.max_y) * 0.5;
    let hx = f64::from(size.x.max(1.)) * scale * 0.5;
    let hy = f64::from(size.y.max(1.)) * scale * 0.5;
    Bounds { min_x: cx - hx, max_x: cx + hx, min_y: cy - hy, max_y: cy + hy }
}

fn screen(rect: egui::Rect, bounds: Bounds, point: [f64; 2]) -> Pos2 {
    egui::pos2(rect.left() + ((point[0] - bounds.min_x) / (bounds.max_x - bounds.min_x)) as f32 * rect.width(),
        rect.bottom() - ((point[1] - bounds.min_y) / (bounds.max_y - bounds.min_y)) as f32 * rect.height())
}

fn overlap(a: Bounds, b: Bounds) -> Option<Bounds> {
    let shared = Bounds { min_x: a.min_x.max(b.min_x), max_x: a.max_x.min(b.max_x),
        min_y: a.min_y.max(b.min_y), max_y: a.max_y.min(b.max_y) };
    (shared.min_x < shared.max_x && shared.min_y < shared.max_y).then_some(shared)
}

fn covered_area(image: Bounds, view: Bounds) -> f64 {
    overlap(image, view).map_or(0., |b| (b.max_x - b.min_x) * (b.max_y - b.min_y))
}

fn raster_rects(rect: egui::Rect, view: Bounds, image: Bounds) -> Option<(egui::Rect, egui::Rect)> {
    let shared = overlap(view, image)?;
    let destination = egui::Rect::from_min_max(screen(rect, view, [shared.min_x, shared.max_y]),
        screen(rect, view, [shared.max_x, shared.min_y]));
    let w = image.max_x - image.min_x;
    let h = image.max_y - image.min_y;
    let uv = egui::Rect::from_min_max(
        egui::pos2(((shared.min_x - image.min_x) / w) as f32, ((image.max_y - shared.max_y) / h) as f32),
        egui::pos2(((shared.max_x - image.min_x) / w) as f32, ((image.max_y - shared.min_y) / h) as f32));
    Some((destination, uv))
}

fn raster_size(size: egui::Vec2, dpi: f32) -> [u32; 2] {
    let width = f64::from((size.x * dpi.max(2.)).max(1.));
    let height = f64::from((size.y * dpi.max(2.)).max(1.));
    let scale = 1_f64.min(4096. / width).min(4096. / height)
        .min((8_388_608. / (width * height)).sqrt());
    [(width * scale).floor().max(1.) as u32, (height * scale).floor().max(1.) as u32]
}

fn background(ui: &egui::Ui, state: &mut State, rect: egui::Rect, view: Bounds) {
    let now = ui.input(|input| input.time);
    let size = raster_size(rect.size(), ui.ctx().pixels_per_point());
    if let Some(request) = &state.loading {
        if overlap(request.bounds, view).is_none() { request.cancel.store(true, Ordering::Release); }
    }
    let reply = state.loading.as_ref().and_then(|request| match request.receiver.try_recv() {
        Ok(result) => Some(result),
        Err(TryRecvError::Disconnected) => Some(Err("Kaartdownload onderbroken".into())),
        Err(TryRecvError::Empty) => None,
    });
    if let Some(reply) = reply {
        if let Some(request) = state.loading.take() {
            if !request.cancel.load(Ordering::Acquire) && overlap(request.bounds, view).is_some() {
                match reply {
                    Ok(raster) if raster.bounds == request.bounds && [raster.width, raster.height] == request.size => {
                        let keep = state.texture.as_ref().is_some_and(|(cached, _)|
                            covered_area(*cached, view) > covered_area(raster.bounds, view));
                        if !keep {
                            let image = egui::ColorImage::from_rgba_unmultiplied(
                                [raster.width as usize, raster.height as usize], &raster.rgba);
                            let texture = ui.ctx().load_texture("pointcloud-catalog-osm", image, egui::TextureOptions::LINEAR);
                            state.texture = Some((raster.bounds, texture));
                        }
                        state.status = if raster.stale_cache { "OpenStreetMap uit tegelcache" } else { "OpenStreetMap" }.into();
                    }
                    result => {
                        state.status = format!("Kaartachtergrond niet beschikbaar: {}",
                            result.err().unwrap_or_else(|| "Onverwachte kaartafmetingen".into()));
                        if request.bounds == view && request.size == size { state.request_after = now + 3.; }
                    }
                }
            }
        }
    }
    if let Some((cached, texture)) = &state.texture {
        if let Some((destination, uv)) = raster_rects(rect, view, *cached) {
            ui.painter_at(rect).image(texture.id(), destination, uv, Color32::WHITE);
        }
    }
    let matches = state.texture.as_ref().is_some_and(|(cached, texture)|
        *cached == view && texture.size() == [size[0] as usize, size[1] as usize]);
    if state.loading.is_none() && (!matches || state.refresh) {
        if now >= state.request_after {
            let (tx, receiver) = mpsc::channel();
            let cancel = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&cancel);
            let ctx = ui.ctx().clone();
            let cache = project::app_data_directory().join("osm_pointclouds");
            state.loading = Some(Background { bounds: view, size, cancel, receiver });
            state.request_after = now + 0.25;
            state.refresh = false;
            state.status = "Kaarttegels ophalen…".into();
            std::thread::spawn(move || {
                let result = ViewportRenderer::default().request(view, size[0], size[1], &cache, &flag, &mut |_| Ok(()));
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        } else { ui.ctx().request_repaint_after(Duration::from_secs_f64(state.request_after - now)); }
    }
    if state.loading.is_some() { ui.ctx().request_repaint_after(Duration::from_millis(80)); }
}

// Match KickTheMap's left-drag selection semantics while retaining cloud-specific
// bounds, trench axes and double-click-to-edit actions.
type MapPoint = (String, [f64; 2]);
const MIN_METERS_PER_POINT: f64 = 0.001;
const MAX_METERS_PER_POINT: f64 = 10_000.;

struct SelectionDrag {
    start: Pos2,
    end: Pos2,
    viewport: egui::Rect,
    bounds: Bounds,
    points: Vec<MapPoint>,
    additive: bool,
    dragged: bool,
}
impl SelectionDrag {
    fn rect(&self) -> egui::Rect {
        egui::Rect::from_two_pos(self.start, self.end).intersect(self.viewport)
    }
}

fn additive(modifiers: egui::Modifiers) -> bool {
    modifiers.ctrl || modifiers.command || modifiers.shift
}
fn selected_bounds(locations: &[Location], selection: &super::Selection) -> Option<Bounds> {
    let mut bounds = locations.iter().filter(|loc| selection.ids.contains(&loc.id))
        .map(|loc| loc.bounds);
    let mut fit = bounds.next()?;
    for bounds in bounds {
        fit.min_x = fit.min_x.min(bounds.min_x);
        fit.max_x = fit.max_x.max(bounds.max_x);
        fit.min_y = fit.min_y.min(bounds.min_y);
        fit.max_y = fit.max_y.max(bounds.max_y);
    }
    Some(padded(fit))
}
fn pan_bounds(base: Bounds, view: Bounds, rect: egui::Rect, delta: egui::Vec2) -> Bounds {
    let dx = f64::from(delta.x / rect.width()) * (view.max_x - view.min_x);
    let dy = f64::from(delta.y / rect.height()) * (view.max_y - view.min_y);
    Bounds { min_x: base.min_x - dx, max_x: base.max_x - dx,
        min_y: base.min_y + dy, max_y: base.max_y + dy }
}
fn zoom_bounds(base: Bounds, factor: f64) -> Bounds {
    let cx = (base.min_x + base.max_x) * 0.5;
    let cy = (base.min_y + base.max_y) * 0.5;
    let hx = (base.max_x - base.min_x) * factor * 0.5;
    let hy = (base.max_y - base.min_y) * factor * 0.5;
    Bounds { min_x: cx - hx, max_x: cx + hx, min_y: cy - hy, max_y: cy + hy }
}
fn zoom_bounds_at(base: Bounds, rect: egui::Rect, anchor: Pos2, factor: f64) -> Bounds {
    let width = f64::from(rect.width());
    let height = f64::from(rect.height());
    let span_x = base.max_x - base.min_x;
    let span_y = base.max_y - base.min_y;
    if width < 1. || height < 1. || span_x <= 0. || span_y <= 0. || factor <= 0.
        || ![width, height, span_x, span_y, factor, f64::from(anchor.x), f64::from(anchor.y)]
            .iter().all(|value| value.is_finite()) { return base; }
    let old_scale = (span_x / width).max(span_y / height);
    let scale = (old_scale * factor).clamp(MIN_METERS_PER_POINT, MAX_METERS_PER_POINT);
    if scale == old_scale { return base; }
    let next = zoom_bounds(base, scale / old_scale);
    let offset = anchor - rect.center();
    let dx = f64::from(offset.x) * (old_scale - scale);
    let dy = -f64::from(offset.y) * (old_scale - scale);
    let next = Bounds { min_x: next.min_x + dx, max_x: next.max_x + dx,
        min_y: next.min_y + dy, max_y: next.max_y + dy };
    if [next.min_x, next.max_x, next.min_y, next.max_y].iter().all(|value| value.is_finite())
        && next.min_x < next.max_x && next.min_y < next.max_y { next } else { base }
}
fn ids_in_rectangle(points: &[MapPoint], viewport: egui::Rect, bounds: Bounds, selection: egui::Rect) -> Vec<String> {
    points.iter().filter_map(|(id, point)| {
        let position = screen(viewport, bounds, *point);
        (viewport.contains(position) && selection.contains(position)).then(|| id.clone())
    }).collect()
}
fn select_with_pointer(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect,
    bounds: Bounds, points: &[MapPoint], selection: &mut super::Selection, state: &mut State) -> bool {
    let (abort, pressed, pointer, down, released) = ui.input(|input| (
        input.key_pressed(egui::Key::Escape) || !input.raw.focused
            || input.events.iter().any(|event| matches!(event, egui::Event::PointerGone)),
        input.events.iter().find_map(|event| match event {
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary,
                pressed: true, modifiers } => Some((*pos, *modifiers)),
            _ => None,
        }),
        input.pointer.interact_pos(),
        input.pointer.primary_down(),
        input.pointer.button_released(egui::PointerButton::Primary),
    ));
    let changed = state.selection.as_ref().is_some_and(|drag|
        drag.viewport != rect || drag.bounds != bounds || drag.points != points);
    if abort || changed {
        state.selection = None;
        if ui.ctx().dragged_id() == Some(response.id) { ui.ctx().stop_dragging(); }
        return true;
    }
    if let Some((start, modifiers)) = pressed {
        if response.enabled() && rect.intersect(ui.clip_rect()).contains(start)
            && ui.ctx().layer_id_at(start) == Some(ui.layer_id()) {
            state.selection = Some(SelectionDrag { start, end: pointer.unwrap_or(start),
                viewport: rect, bounds, points: points.to_vec(),
                additive: additive(modifiers), dragged: false });
        }
    }
    if let Some(drag) = state.selection.as_mut() {
        if let Some(pointer) = pointer { drag.end = pointer; }
        drag.dragged |= drag.start.distance(drag.end) >= 4.;
        if drag.dragged && down { ui.ctx().set_dragged_id(response.id); }
    }
    if released {
        if let Some(drag) = state.selection.take() {
            if !drag.dragged { return false; }
            selection.apply(ids_in_rectangle(points, rect, bounds, drag.rect()), drag.additive);
            return true;
        }
    } else if !down {
        state.selection = None;
    }
    false
}

pub(super) fn show(ui: &mut egui::Ui, locations: &[Location], selection: &mut super::Selection,
    state: &mut State, size: egui::Vec2) -> Option<String> {
    let Some(fit) = all_bounds(locations) else {
        state.pause();
        ui.label("De kaart toont puntenwolken met RD-coördinaten. De broncoördinaten staan altijd in de tabel.");
        return None;
    };
    ui.horizontal_wrapped(|ui| {
        if ui.button("Passend").clicked() {
            state.bounds = Some(fit);
            state.selection = None;
        }
        let selected = selected_bounds(locations, selection);
        if ui.add_enabled(selected.is_some(), egui::Button::new("Zoom naar selectie")).clicked() {
            state.bounds = selected;
            state.selection = None;
        }
        if ui.button("Vernieuwen").clicked() {
            state.refresh = true;
            state.request_after = 0.;
        }
        ui.small(format!("{} RD-locaties · {} geselecteerd", locations.len(), selection.ids.len()));
    });
    ui.small("Klik op een punt of sleep met links een selectievak. Ctrl/Shift voegt toe. Dubbelklik bewerkt een scan; het muiswiel zoomt; midden/rechts slepen verplaatst. Esc annuleert het selectievak.");
    ui.add(egui::Label::new(egui::RichText::new(&state.status).small()).truncate()).on_hover_text(&state.status);

    let (rect, _) = ui.allocate_exact_size(size.max(egui::vec2(1., 1.)), egui::Sense::hover());
    let response = ui.interact(rect, ui.layer_id().id.with("pointcloud-catalog-map"),
        egui::Sense::click_and_drag());
    state.surface = Some((rect.intersect(ui.clip_rect()), ui.layer_id()));
    let mut base = *state.bounds.get_or_insert(fit);
    let mut view = viewport_bounds(base, rect.size());
    let points = locations.iter().map(|loc| (loc.id.clone(), [loc.center[0], loc.center[1]]))
        .collect::<Vec<_>>();
    let panning = [egui::PointerButton::Secondary, egui::PointerButton::Middle].iter().any(|&button|
        response.dragged_by(button) ||
        (response.drag_stopped() && ui.input(|input| input.pointer.button_released(button))));
    let wheel = ui.input(|input| if response.hovered() { input.raw_scroll_delta.y } else { 0. });
    if panning { base = pan_bounds(base, view, rect, ui.input(|input| input.pointer.delta())); }
    if wheel.abs() > 0.1 {
        base = zoom_bounds_at(base, rect, response.hover_pos().unwrap_or(rect.center()),
            if wheel > 0. { 0.85 } else { 1.15 });
    }
    let selection_consumed = if panning || wheel.abs() > 0.1 {
        state.selection = None;
        state.bounds = Some(base);
        view = viewport_bounds(base, rect.size());
        true
    } else {
        select_with_pointer(ui, &response, rect, view, &points, selection, state)
    };

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0., Color32::from_rgb(238, 242, 247));
    background(ui, state, rect, view);
    let mut nearest: Option<(&Location, f32)> = None;
    for location in locations {
        let point = screen(rect, view, [location.center[0], location.center[1]]);
        let active = selection.ids.contains(&location.id);
        if active {
            let outline = egui::Rect::from_min_max(
                screen(rect, view, [location.bounds.min_x, location.bounds.max_y]),
                screen(rect, view, [location.bounds.max_x, location.bounds.min_y]));
            painter.rect_stroke(outline, 0., Stroke::new(1.5_f32, ORANGE), egui::StrokeKind::Inside);
            if selection.focused.as_deref() == Some(location.id.as_str()) {
                if let Some([start, end]) = location.axis {
                    let a = screen(rect, view, [start.x, start.y]);
                    let b = screen(rect, view, [end.x, end.y]);
                    painter.line_segment([a, b], Stroke::new(3_f32, Color32::from_rgb(22, 135, 115)));
                    for (position, text) in [(a, "Begin"), (b, "Eind")] {
                        painter.circle_filled(position, 4., Color32::from_rgb(22, 135, 115));
                        painter.text(position + egui::vec2(7., -8.), egui::Align2::LEFT_BOTTOM,
                            text, egui::FontId::proportional(11.), INK);
                    }
                }
            }
        }
        if !rect.contains(point) { continue; }
        let radius = if active { 7. } else { 5. };
        painter.circle_filled(point, radius, if active { ORANGE } else { Color32::from_rgb(35, 91, 167) });
        painter.circle_stroke(point, radius, Stroke::new(1_f32, Color32::WHITE));
        if let Some(pointer) = response.hover_pos().or_else(|| response.interact_pointer_pos()) {
            let distance = pointer.distance(point);
            if distance < 16. && nearest.is_none_or(|(_, previous)| distance < previous) {
                nearest = Some((location, distance));
            }
        }
    }
    let mut edit = None;
    if let Some((location, _)) = nearest {
        response.clone().on_hover_text(&location.title);
        if !selection_consumed && (response.clicked_by(egui::PointerButton::Primary) || response.double_clicked()) {
            selection.apply([location.id.clone()], ui.input(|input| additive(input.modifiers)));
            if response.double_clicked() { edit = Some(location.id.clone()); }
        }
    }
    if let Some(drag) = state.selection.as_ref().filter(|drag| drag.dragged) {
        painter.rect_filled(drag.rect(), 0., Color32::from_rgba_unmultiplied(249, 115, 22, 28));
        painter.rect_stroke(drag.rect(), 0., Stroke::new(1.5_f32, ORANGE), egui::StrokeKind::Inside);
    }
    painter.text(rect.right_bottom() - egui::vec2(8., 6.), egui::Align2::RIGHT_BOTTOM,
        "© OpenStreetMap-bijdragers", egui::FontId::proportional(10.), INK);
    theme::finish_map_frame(ui, rect, 10., theme::APP_BG);
    edit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn location(id: &str, x: f64, y: f64) -> Location {
        Location { id: id.to_owned(), title: id.to_owned(),
            center: [x, y, 0.], bounds: Bounds {
                min_x: x - 1., max_x: x + 1., min_y: y - 1., max_y: y + 1.,
            }, axis: None }
    }

    #[test]
    fn rectangle_only_selects_markers_inside_visible_viewport() {
        let viewport = egui::Rect::from_min_size(egui::pos2(0., 0.), egui::vec2(100., 100.));
        let world = Bounds { min_x: 0., max_x: 100., min_y: 0., max_y: 100. };
        let points = [("inside".into(), [25., 75.]), ("outside".into(), [80., 20.]),
            ("offscreen".into(), [150., 150.])];
        let rect = egui::Rect::from_min_max(egui::pos2(10., 10.), egui::pos2(50., 50.));
        assert_eq!(ids_in_rectangle(&points, viewport, world, rect), vec!["inside"]);
    }

    #[test]
    fn zoom_to_selection_contains_all_selected_cloud_bounds() {
        let locations = [location("first", 100., 100.), location("second", 300., 250.),
            location("unselected", 800., 800.)];
        let mut selection = super::super::Selection::default();
        selection.apply(["first".into(), "second".into()], false);
        let fit = selected_bounds(&locations, &selection).unwrap();
        assert!(fit.min_x < 99. && fit.max_x > 301.);
        assert!(fit.min_y < 99. && fit.max_y > 251.);
        assert!(fit.max_x < 800.);
    }
}
