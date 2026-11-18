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

pub(super) fn show(ui: &mut egui::Ui, locations: &[Location], selected: &mut Option<String>,
    state: &mut State, size: egui::Vec2) -> Option<String> {
    let Some(fit) = all_bounds(locations) else {
        state.pause();
        ui.label("De kaart toont puntenwolken met RD-coördinaten. De broncoördinaten staan altijd in de tabel.");
        return None;
    };
    ui.horizontal_wrapped(|ui| {
        if ui.button("Alle locaties").clicked() { state.bounds = Some(fit); }
        let selection = locations.iter().find(|location| Some(&location.id) == selected.as_ref());
        if ui.add_enabled(selection.is_some(), egui::Button::new("Naar selectie")).clicked() {
            if let Some(location) = selection { state.bounds = Some(padded(location.bounds)); }
        }
        if ui.button("Kaart vernieuwen").clicked() { state.refresh = true; state.request_after = 0.; }
        ui.small(format!("{} RD-locaties", locations.len()));
    });
    ui.small("Klik: selecteren · Dubbelklik: bewerken · Muiswiel: zoomen · Rechts/midden slepen: verplaatsen");
    ui.add(egui::Label::new(egui::RichText::new(&state.status).small()).truncate()).on_hover_text(&state.status);
    let (rect, _) = ui.allocate_exact_size(size.max(egui::vec2(1., 1.)), egui::Sense::hover());
    let response = ui.interact(rect, ui.layer_id().id.with("pointcloud-catalog-map"), egui::Sense::click_and_drag());
    state.surface = Some((rect.intersect(ui.clip_rect()), ui.layer_id()));
    let mut base = *state.bounds.get_or_insert(fit);
    let mut view = viewport_bounds(base, rect.size());
    let panning = [egui::PointerButton::Secondary, egui::PointerButton::Middle].iter().any(|&button|
        response.dragged_by(button) || (response.drag_stopped() && ui.input(|input| input.pointer.button_released(button))));
    let wheel = ui.input(|input| if response.hovered() { input.raw_scroll_delta.y } else { 0. });
    if panning {
        let delta = ui.input(|input| input.pointer.delta());
        let dx = f64::from(delta.x / rect.width()) * (view.max_x - view.min_x);
        let dy = f64::from(delta.y / rect.height()) * (view.max_y - view.min_y);
        base.min_x -= dx; base.max_x -= dx; base.min_y += dy; base.max_y += dy;
    }
    if wheel.abs() > 0.1 {
        let anchor = response.hover_pos().unwrap_or(rect.center());
        let old_scale = ((base.max_x - base.min_x) / f64::from(rect.width()))
            .max((base.max_y - base.min_y) / f64::from(rect.height()));
        let scale = (old_scale * if wheel > 0. { 0.85 } else { 1.15 }).clamp(0.001, 10_000.);
        let factor = scale / old_scale;
        let offset = anchor - rect.center();
        let cx = (base.min_x + base.max_x) * 0.5 + f64::from(offset.x) * (old_scale - scale);
        let cy = (base.min_y + base.max_y) * 0.5 - f64::from(offset.y) * (old_scale - scale);
        let hx = (base.max_x - base.min_x) * factor * 0.5;
        let hy = (base.max_y - base.min_y) * factor * 0.5;
        base = Bounds { min_x: cx - hx, max_x: cx + hx, min_y: cy - hy, max_y: cy + hy };
    }
    if panning || wheel.abs() > 0.1 { state.bounds = Some(base); view = viewport_bounds(base, rect.size()); }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0., Color32::from_rgb(238, 242, 247));
    background(ui, state, rect, view);
    let mut nearest: Option<(&Location, f32)> = None;
    for location in locations {
        let point = screen(rect, view, [location.center[0], location.center[1]]);
        let active = Some(&location.id) == selected.as_ref();
        if active {
            let outline = egui::Rect::from_min_max(screen(rect, view, [location.bounds.min_x, location.bounds.max_y]),
                screen(rect, view, [location.bounds.max_x, location.bounds.min_y]));
            painter.rect_stroke(outline, 0., Stroke::new(1.5_f32, ORANGE), egui::StrokeKind::Inside);
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
        if !rect.contains(point) { continue; }
        let radius = if active { 7. } else { 5. };
        painter.circle_filled(point, radius, if active { ORANGE } else { Color32::from_rgb(35, 91, 167) });
        painter.circle_stroke(point, radius, Stroke::new(1_f32, Color32::WHITE));
        if let Some(pointer) = response.hover_pos() {
            let distance = pointer.distance(point);
            if distance < 16. && nearest.is_none_or(|(_, previous)| distance < previous) { nearest = Some((location, distance)); }
        }
    }
    let mut edit = None;
    if let Some((location, _)) = nearest {
        response.clone().on_hover_text(&location.title);
        if response.clicked_by(egui::PointerButton::Primary) || response.double_clicked() {
            *selected = Some(location.id.clone());
            if response.double_clicked() { edit = Some(location.id.clone()); }
        }
    }
    painter.text(rect.right_bottom() - egui::vec2(8., 6.), egui::Align2::RIGHT_BOTTOM,
        "© OpenStreetMap-bijdragers", egui::FontId::proportional(10.), INK);
    theme::finish_map_frame(ui, rect, 10., theme::APP_BG);
    edit
}
