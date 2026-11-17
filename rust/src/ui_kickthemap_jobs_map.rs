//! Jobs map geometry and pointer interaction. The stored bounds are independent
//! of the widget size: fitting a viewport must never change the user's zoom.
use super::*;

type JobPoint = (u64, [f64; 2]);
const JOBS_RASTER_SCALE: f32 = 2.;
const JOBS_RASTER_MAX_SIDE: u32 = 4096;
const JOBS_RASTER_MAX_PIXELS: u64 = 8_388_608;
const BACKGROUND_REQUEST_INTERVAL: f64 = 0.25;
const BACKGROUND_RETRY_DELAY: f64 = 3.;

#[derive(Default)]
pub(super) struct Interaction {
    selection: Option<SelectionDrag>,
    surface: Option<(egui::Rect, egui::LayerId)>,
    background: Option<BackgroundRequest>,
    completed: Option<Result<OsmRaster, String>>,
    viewport: Option<(Bounds, [u32; 2])>,
    request_after: f64,
    refresh: bool,
}

struct BackgroundRequest {
    bounds: Bounds,
    size: [u32; 2],
    cancel: Arc<AtomicBool>,
    #[cfg(test)]
    reply: mpsc::Sender<Result<OsmRaster, String>>,
}

impl Drop for BackgroundRequest {
    fn drop(&mut self) { self.cancel.store(true, Ordering::Release); }
}

struct SelectionDrag {
    start: Pos2,
    end: Pos2,
    viewport: egui::Rect,
    bounds: Bounds,
    points: Vec<JobPoint>,
    additive: bool,
    dragged: bool,
}

impl SelectionDrag {
    fn rect(&self) -> egui::Rect {
        egui::Rect::from_two_pos(self.start, self.end).intersect(self.viewport)
    }
}

/// Window::show handles its area-wide move response before calling its content
/// closure. Reserve a press that starts on the map before that response runs;
/// the title bar and other window space remain available for moving the window.
pub(super) fn window_movable(ctx: &egui::Context, interaction: &Interaction) -> bool {
    if interaction.selection.is_some() { return false; }
    let Some((rect, layer)) = interaction.surface else { return true; };
    let origin = ctx.input(|input| {
        input.pointer.any_down().then(|| input.pointer.press_origin()).flatten()
    });
    !origin.is_some_and(|point| rect.contains(point) && ctx.layer_id_at(point) == Some(layer))
}

fn jobs_bounds(points: &[JobPoint]) -> Option<Bounds> {
    if points.is_empty() { return None; }
    let min_x = points.iter().map(|(_, p)| p[0]).fold(f64::INFINITY, f64::min);
    let max_x = points.iter().map(|(_, p)| p[0]).fold(f64::NEG_INFINITY, f64::max);
    let min_y = points.iter().map(|(_, p)| p[1]).fold(f64::INFINITY, f64::min);
    let max_y = points.iter().map(|(_, p)| p[1]).fold(f64::NEG_INFINITY, f64::max);
    let padding = 500_f64.max((max_x - min_x) * 0.18).max((max_y - min_y) * 0.18);
    Some(Bounds { min_x: min_x - padding, max_x: max_x + padding,
        min_y: min_y - padding, max_y: max_y + padding })
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

fn map_point(rect: egui::Rect, bounds: Bounds, point: [f64; 2]) -> Pos2 {
    egui::pos2(
        rect.left() + ((point[0] - bounds.min_x) / (bounds.max_x - bounds.min_x)) as f32 * rect.width(),
        rect.bottom() - ((point[1] - bounds.min_y) / (bounds.max_y - bounds.min_y)) as f32 * rect.height(),
    )
}

fn raster_size(size: egui::Vec2, scale_factor: f32) -> [u32; 2] {
    let width = f64::from((size.x * scale_factor).max(1.));
    let height = f64::from((size.y * scale_factor).max(1.));
    let side = f64::from(JOBS_RASTER_MAX_SIDE);
    let scale = 1_f64.min(side / width).min(side / height)
        .min((JOBS_RASTER_MAX_PIXELS as f64 / (width * height)).sqrt());
    [(width * scale).floor().max(1.) as u32, (height * scale).floor().max(1.) as u32]
}

fn additive(modifiers: egui::Modifiers) -> bool {
    modifiers.ctrl || modifiers.command || modifiers.shift
}

fn select_points(browser: &mut BrowserUi, ids: impl IntoIterator<Item = u64>, extend: bool) {
    if !extend {
        browser.selected.clear();
        browser.selection_anchor = None;
    }
    for id in ids {
        browser.selected.insert(id);
        // Continue subsequent list range selection from this visible job.
        browser.selection_anchor = Some(id);
    }
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

fn overlap(a: Bounds, b: Bounds) -> Option<Bounds> {
    let shared = Bounds { min_x: a.min_x.max(b.min_x), max_x: a.max_x.min(b.max_x),
        min_y: a.min_y.max(b.min_y), max_y: a.max_y.min(b.max_y) };
    (shared.min_x < shared.max_x && shared.min_y < shared.max_y).then_some(shared)
}

fn covered_area(image: Bounds, view: Bounds) -> f64 {
    overlap(image, view).map_or(0., |shared|
        (shared.max_x - shared.min_x) * (shared.max_y - shared.min_y))
}

/// Crop in world coordinates before constructing a mesh. A retained image stays
/// attached to its original RD extent during pan, zoom and aspect-ratio changes.
fn raster_rects(rect: egui::Rect, view: Bounds, image: Bounds) -> Option<(egui::Rect, egui::Rect)> {
    let shared = overlap(view, image)?;
    let destination = egui::Rect::from_min_max(
        map_point(rect, view, [shared.min_x, shared.max_y]),
        map_point(rect, view, [shared.max_x, shared.min_y]),
    );
    let width = image.max_x - image.min_x;
    let height = image.max_y - image.min_y;
    let uv = egui::Rect::from_min_max(
        egui::pos2(((shared.min_x - image.min_x) / width) as f32,
            ((image.max_y - shared.max_y) / height) as f32),
        egui::pos2(((shared.max_x - image.min_x) / width) as f32,
            ((image.max_y - shared.min_y) / height) as f32),
    );
    Some((destination, uv))
}

fn background(ui: &egui::Ui, browser: &mut BrowserUi, rect: egui::Rect, bounds: Bounds) {
    // Always request extra native map detail, including on a 100% DPI screen.
    let size = raster_size(rect.size(), JOBS_RASTER_SCALE.max(ui.ctx().pixels_per_point()));
    let now = ui.input(|input| input.time);
    if browser.map_interaction.viewport != Some((bounds, size)) {
        // A failure's retry delay belongs to that old view. New navigation
        // retains only the ordinary throttle, without postponing on every move.
        browser.map_interaction.request_after = browser.map_interaction.request_after
            .min(now + BACKGROUND_REQUEST_INTERVAL);
    }
    browser.map_interaction.viewport = Some((bounds, size));
    if let Some(request) = &browser.map_interaction.background {
        // Let a still-useful request finish while the pointer moves. Cancelling
        // every different extent starves downloads throughout a continuous pan.
        if overlap(request.bounds, bounds).is_none() {
            request.cancel.store(true, Ordering::Release);
        }
    }
    // Global polling happens before this frame's pointer/resize handling. Only
    // now is it safe to compare a reply with the actual visible world extent.
    poll_background(ui.ctx(), browser);
    apply_background_reply(ui.ctx(), browser);
    if let Some((cached, texture)) = &browser.map_texture {
        if let Some((destination, uv)) = raster_rects(rect, bounds, *cached) {
            ui.painter_at(rect).image(texture.id(), destination, uv, Color32::WHITE);
        }
    }
    let resolution_matches = browser.map_texture.as_ref().is_some_and(|(cached, texture)| {
        *cached == bounds && texture.size() == [size[0] as usize, size[1] as usize]
    });
    if browser.map_loading.is_some() {
        ui.ctx().request_repaint_after(Duration::from_millis(60));
    } else if !resolution_matches || browser.map_interaction.refresh {
        if now >= browser.map_interaction.request_after {
            request_background(ui.ctx(), browser, bounds, size, now);
        } else {
            ui.ctx().request_repaint_after(Duration::from_secs_f64(
                browser.map_interaction.request_after - now));
        }
    }
}

fn request_background(ctx: &egui::Context, browser: &mut BrowserUi, bounds: Bounds, size: [u32; 2], now: f64) {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    browser.map_interaction.background = Some(BackgroundRequest { bounds, size, cancel: cancel.clone(),
        #[cfg(test)] reply: tx.clone(),
    });
    browser.map_interaction.request_after = now + BACKGROUND_REQUEST_INTERVAL;
    browser.map_interaction.refresh = false;
    browser.map_loading = Some(rx);
    browser.map_status = "OpenStreetMap-tegels ophalen…".into();
    #[cfg(not(test))]
    {
        let dir = kickthemap_cache_dir().join("osm_jobs");
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // Only visible tiles, at most 256 tiles and two global network
            // workers. There is one active request and no queue of old views.
            let mut renderer = sleufbase_rust::osm_tiles::ViewportRenderer::default();
            let result = renderer.request(bounds, size[0], size[1], &dir, &cancel, &mut |_| Ok(()));
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    // Headless tests keep the sender for deterministic replies, never network.
    #[cfg(test)] let _ = (ctx, tx);
}

pub(super) fn poll_background(ctx: &egui::Context, browser: &mut BrowserUi) {
    let result = match browser.map_loading.as_ref().map(|rx| rx.try_recv()) {
        Some(Ok(result)) => result,
        Some(Err(TryRecvError::Disconnected)) => Err("Kaartdownload onderbroken".into()),
        Some(Err(TryRecvError::Empty)) => {
            ctx.request_repaint_after(Duration::from_millis(60));
            return;
        }
        None => return,
    };
    browser.map_loading = None;
    if browser.map_interaction.background.as_ref().is_some_and(|request|
        !request.cancel.load(Ordering::Acquire)) {
        // Keep at most one completed raster until the canvas has updated its
        // view. This never replaces displayed pixels using last frame's bounds.
        browser.map_interaction.completed = Some(result);
    } else {
        browser.map_interaction.background = None;
        browser.map_interaction.completed = None;
    }
}

fn apply_background_reply(ctx: &egui::Context, browser: &mut BrowserUi) {
    let Some(result) = browser.map_interaction.completed.take() else { return; };
    let Some(request) = browser.map_interaction.background.take() else { return; };
    if request.cancel.load(Ordering::Acquire) { return; }
    let Some((view, size)) = browser.map_interaction.viewport else { return; };
    if overlap(request.bounds, view).is_none() { return; }
    match result {
        Ok(raster) if raster.bounds == request.bounds
            && [raster.width, raster.height] == request.size => {
            // A user may pan back while a nearby request finishes. Do not lose
            // already-covered pixels by adopting a less useful late image.
            let keep_existing = browser.map_texture.as_ref().is_some_and(|(cached, _)|
                covered_area(*cached, view) > covered_area(raster.bounds, view));
            if keep_existing {
                browser.map_status = "OpenStreetMap geladen".into();
                return;
            }
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [raster.width as usize, raster.height as usize], &raster.rgba);
            let texture = ctx.load_texture("kickthemap-jobs-osm", image, egui::TextureOptions::LINEAR);
            browser.map_texture = Some((raster.bounds, texture));
            browser.map_status = if raster.stale_cache {
                "OpenStreetMap geladen vanuit verouderde tegelcache".into()
            } else { "OpenStreetMap geladen".into() };
        }
        result => {
            // Retry a failed unchanged view at a bounded rate. A newer viewport
            // can proceed immediately once this single worker has finished.
            if request.bounds == view && request.size == size {
                browser.map_interaction.request_after = ctx.input(|input| input.time) + BACKGROUND_RETRY_DELAY;
                let error = result.err().unwrap_or_else(|| "Kaartafmetingen komen niet overeen".into());
                browser.map_status = format!("Kaartachtergrond niet beschikbaar: {error}");
            }
        }
    }
}

fn select_with_pointer(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect,
    bounds: Bounds, points: &[JobPoint], browser: &mut BrowserUi) -> bool {
    let (abort, pressed, pointer, down, released) = ui.input(|input| (
        input.key_pressed(egui::Key::Escape) || !input.raw.focused
            || input.events.iter().any(|event| matches!(event, egui::Event::PointerGone)),
        input.events.iter().find_map(|event| match event {
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary,
                pressed: true, modifiers } => Some((*pos, *modifiers)),
            _ => None,
        }),
        input.pointer.interact_pos(),
        input.pointer.primary_down(), input.pointer.button_released(egui::PointerButton::Primary),
    ));
    let changed = browser.map_interaction.selection.as_ref().is_some_and(|drag| {
        drag.viewport != rect || drag.bounds != bounds || drag.points != points
    });
    if abort || changed {
        browser.map_interaction.selection = None;
        if ui.ctx().dragged_id() == Some(response.id) { ui.ctx().stop_dragging(); }
        return true;
    }
    if let Some((start, modifiers)) = pressed {
        if response.enabled() && rect.intersect(ui.clip_rect()).contains(start)
            && ui.ctx().layer_id_at(start) == Some(ui.layer_id())
        {
            // Capture the physical press, rather than depending on a one-frame
            // drag-start event assigned by the enclosing movable Window.
            browser.map_interaction.selection = Some(SelectionDrag {
                start, end: pointer.unwrap_or(start), viewport: rect, bounds,
                points: points.to_vec(), additive: additive(modifiers), dragged: false,
            });
        }
    }
    if let Some(drag) = browser.map_interaction.selection.as_mut() {
        if let Some(pointer) = pointer { drag.end = pointer; }
        drag.dragged |= drag.start.distance(drag.end) >= 4.;
        if drag.dragged && down { ui.ctx().set_dragged_id(response.id); }
    }
    if released {
        if let Some(drag) = browser.map_interaction.selection.take() {
            if !drag.dragged { return false; }
            let selection = drag.rect();
            let ids = points.iter().filter_map(|(id, point)| {
                let position = map_point(rect, bounds, *point);
                (rect.contains(position) && selection.contains(position)).then_some(*id)
            });
            select_points(browser, ids, drag.additive);
            return true;
        }
    } else if !down {
        // Closing/hiding the map or losing the pointer must not leave a stale
        // drag that can be committed by a later, unrelated mouse release.
        browser.map_interaction.selection = None;
    }
    false
}

fn canvas(ui: &mut egui::Ui, points: &[JobPoint], browser: &mut BrowserUi,
    action: &mut Option<&'static str>, size: egui::Vec2) -> egui::Rect {
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    // Unlike an automatic layout ID, this survives changing status/toolbar
    // content in the real Jobs window throughout a held pointer gesture.
    let id = ui.layer_id().id.with("kickthemap-jobs-map-canvas");
    let response = ui.interact(rect, id, egui::Sense::click_and_drag());
    browser.map_interaction.surface = Some((rect.intersect(ui.clip_rect()), ui.layer_id()));
    let mut base = browser.map_bounds.expect("map bounds initialized before canvas");
    let mut bounds = viewport_bounds(base, rect.size());
    let panning = [egui::PointerButton::Middle, egui::PointerButton::Secondary].iter().any(|&button| {
        response.dragged_by(button)
            || (response.drag_stopped() && ui.input(|input| input.pointer.button_released(button)))
    });
    let scroll = ui.input(|input| if response.hovered() { input.raw_scroll_delta.y } else { 0. });
    if panning {
        base = pan_bounds(base, bounds, rect, ui.input(|input| input.pointer.delta()));
    }
    if scroll.abs() > 0.1 {
        base = zoom_bounds(base, if scroll > 0. { 0.85 } else { 1.15 });
    }
    let selection_consumed = if panning || scroll.abs() > 0.1 {
        browser.map_interaction.selection = None;
        browser.map_bounds = Some(base);
        bounds = viewport_bounds(base, rect.size());
        true
    } else {
        select_with_pointer(ui, &response, rect, bounds, points, browser)
    };

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0., Color32::from_rgb(238, 242, 247));
    background(ui, browser, rect, bounds);
    let mut nearest: Option<(u64, f32)> = None;
    for (id, point) in points {
        let position = map_point(rect, bounds, *point);
        if !rect.contains(position) { continue; }
        let selected = browser.selected.contains(id);
        let radius = if selected { 7. } else { 5. };
        painter.circle_filled(position, radius,
            if selected { Color32::from_rgb(249, 115, 22) } else { Color32::from_rgb(35, 91, 167) });
        painter.circle_stroke(position, radius, Stroke::new(1_f32, Color32::WHITE));
        if let Some(pointer) = response.interact_pointer_pos() {
            let distance = position.distance(pointer);
            if distance < 16. && nearest.is_none_or(|(_, previous)| distance < previous) {
                nearest = Some((*id, distance));
            }
        }
    }
    if !selection_consumed && response.clicked_by(egui::PointerButton::Primary) {
        if let Some((id, _)) = nearest {
            select_points(browser, [id], ui.input(|input| additive(input.modifiers)));
            if response.double_clicked() {
                browser.open_target = Some(id);
                *action = Some("open-job");
            }
        }
    }
    if let Some(drag) = browser.map_interaction.selection.as_ref().filter(|drag| drag.dragged) {
        painter.rect_filled(drag.rect(), 0., Color32::from_rgba_unmultiplied(249, 115, 22, 28));
        painter.rect_stroke(drag.rect(), 0., Stroke::new(1.5_f32, Color32::from_rgb(249, 115, 22)),
            egui::StrokeKind::Inside);
    }
    theme::finish_map_frame(ui, rect, 14., theme::APP_BG);
    rect
}

pub(super) fn show(ui: &mut egui::Ui, source: &[Job], indices: &[usize],
    browser: &mut BrowserUi, action: &mut Option<&'static str>) {
    let points = indices.iter().filter_map(|&index| {
        let job = &source[index];
        job_rd_coordinates(&job.coordinates).map(|point| (job.job_id, point))
    }).collect::<Vec<_>>();
    let Some(visible_bounds) = jobs_bounds(&points) else {
        browser.map_interaction = Default::default();
        ui.label("Geen jobs met geldige coördinaten gevonden.");
        return;
    };
    ui.horizontal_wrapped(|ui| {
        if ui.button("Passend").clicked() {
            browser.map_bounds = Some(visible_bounds);
            browser.map_interaction.selection = None;
        }
        if ui.button("Zoom naar selectie").clicked() {
            let selected = points.iter().filter(|(id, _)| browser.selected.contains(id)).copied().collect::<Vec<_>>();
            if let Some(bounds) = jobs_bounds(&selected) {
                browser.map_bounds = Some(bounds);
                browser.map_interaction.selection = None;
            }
        }
        if ui.button("Vernieuwen").clicked() {
            browser.map_interaction.refresh = true;
            browser.map_interaction.request_after = 0.;
        }
        ui.small(format!("{} jobs met coördinaten", points.len()));
    });
    ui.small("Klik op een punt of sleep met links een selectievak. Ctrl/Shift voegt toe. Dubbelklik opent een job; het muiswiel zoomt; midden/rechts slepen verplaatst. Esc annuleert het selectievak.");
    let status = if browser.map_status.is_empty() { "OpenStreetMap" } else { &browser.map_status };
    ui.add(egui::Label::new(egui::RichText::new(status).small()).truncate()).on_hover_text(status);
    browser.map_bounds.get_or_insert(visible_bounds);
    let size = ui.available_size().max(egui::vec2(1., 1.));
    canvas(ui, &points, browser, action, size);
}

#[cfg(test)]
#[path = "ui_kickthemap_jobs_map_tests.rs"]
mod tests;
