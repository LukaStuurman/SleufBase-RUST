use super::*;

fn base_bounds() -> Bounds {
    Bounds { min_x: 100_000., max_x: 101_000., min_y: 450_000., max_y: 451_000. }
}

struct MapHarness {
    ctx: egui::Context,
    browser: BrowserUi,
    points: Vec<JobPoint>,
    size: egui::Vec2,
    rect: egui::Rect,
    modifiers: egui::Modifiers,
    focused: bool,
    time: f64,
    action: Option<&'static str>,
}

impl MapHarness {
    fn new() -> Self {
        let mut harness = Self {
            ctx: egui::Context::default(),
            browser: BrowserUi { map_bounds: Some(base_bounds()), ..Default::default() },
            points: vec![(1, [100_100., 450_700.]), (2, [100_300., 450_600.]), (3, [100_850., 450_200.])],
            size: egui::vec2(600., 400.),
            rect: egui::Rect::NOTHING,
            modifiers: Default::default(), focused: true, time: 0., action: None,
        };
        harness.frame(vec![]);
        harness.frame(vec![]);
        harness
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        self.time += 0.03;
        let ctx = self.ctx.clone();
        ctx.run(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, self.size + egui::vec2(160., 160.))),
            time: Some(self.time), modifiers: self.modifiers, focused: self.focused,
            events, ..Default::default()
        }, |ctx| {
            // The application polls workers before rendering the Jobs window.
            poll_background(ctx, &mut self.browser);
            egui::CentralPanel::default().show(ctx, |ui| {
                self.rect = canvas(ui, &self.points, &mut self.browser, &mut self.action, self.size);
            });
        })
    }

    fn point(&self, id: u64) -> Pos2 {
        let point = self.points.iter().find(|(candidate, _)| *candidate == id).unwrap().1;
        map_point(self.rect, viewport_bounds(self.browser.map_bounds.unwrap(), self.rect.size()), point)
    }

    fn button(&mut self, pos: Pos2, button: egui::PointerButton, pressed: bool) {
        self.frame(vec![egui::Event::PointerButton { pos, button, pressed, modifiers: self.modifiers }]);
    }

    fn begin_drag(&mut self, start: Pos2, end: Pos2, button: egui::PointerButton) {
        self.frame(vec![egui::Event::PointerMoved(start)]);
        self.button(start, button, true);
        self.frame(vec![egui::Event::PointerMoved(end)]);
    }

    fn drag(&mut self, start: Pos2, end: Pos2) {
        self.begin_drag(start, end, egui::PointerButton::Primary);
        assert!(self.browser.map_interaction.selection.is_some(), "left drag should display the selection box");
        self.button(end, egui::PointerButton::Primary, false);
    }

    fn click(&mut self, pos: Pos2) {
        self.frame(vec![egui::Event::PointerMoved(pos)]);
        self.button(pos, egui::PointerButton::Primary, true);
        self.button(pos, egui::PointerButton::Primary, false);
    }

    fn first_pair_box(&self) -> (Pos2, Pos2) {
        let rect = egui::Rect::from_two_pos(self.point(1), self.point(2)).expand(20.);
        (rect.left_top(), rect.right_bottom())
    }
}

#[test]
fn jobs_map_keeps_equal_world_scale_at_every_viewport_aspect_without_resize_drift() {
    let base = Bounds { min_x: 99_000., max_x: 102_000., min_y: 450_000., max_y: 451_000. };
    let wide = viewport_bounds(base, egui::vec2(1000., 300.));
    for size in [egui::vec2(1000., 300.), egui::vec2(300., 900.), egui::vec2(340., 140.), egui::vec2(1000., 300.)] {
        let view = viewport_bounds(base, size);
        let rect = egui::Rect::from_min_size(egui::pos2(12., 23.), size);
        let center = map_point(rect, view, [100_500., 450_500.]);
        let horizontal = center.distance(map_point(rect, view, [100_600., 450_500.]));
        let vertical = center.distance(map_point(rect, view, [100_500., 450_600.]));
        assert!((horizontal - vertical).abs() < 0.0001, "RD metres must have equal horizontal/vertical scale");
        assert!(view.min_x <= base.min_x && view.max_x >= base.max_x);
        assert!(view.min_y <= base.min_y && view.max_y >= base.max_y);
    }
    assert_eq!(viewport_bounds(base, egui::vec2(1000., 300.)), wide);
}

#[test]
fn jobs_map_raster_follows_actual_viewport_and_stays_within_tile_preview_budget() {
    assert_eq!(raster_size(egui::vec2(600., 350.), 1.), [600, 350]);
    assert_eq!(raster_size(egui::vec2(600., 350.), 2.), [1200, 700]);
    for size in [egui::vec2(4000., 3000.), egui::vec2(300., 900.), egui::vec2(10_000., 200.)] {
        let [width, height] = raster_size(size, 2.);
        assert!(width > 0 && height > 0 && width <= JOBS_RASTER_MAX_SIDE && height <= JOBS_RASTER_MAX_SIDE);
        assert!(u64::from(width) * u64::from(height) <= JOBS_RASTER_MAX_PIXELS);
        assert!((f64::from(width) / f64::from(height) - f64::from(size.x / size.y)).abs()
            <= f64::from(size.x / size.y) * 0.03);
    }
}

#[test]
fn jobs_map_left_rectangle_selects_visible_points_and_updates_list_anchor() {
    let mut map = MapHarness::new();
    map.browser.selected.insert(90);
    let (start, end) = map.first_pair_box();
    map.begin_drag(start, end, egui::PointerButton::Primary);
    assert_eq!(map.browser.selected, BTreeSet::from([90]), "drag preview must not mutate the selection");
    assert!(map.browser.map_interaction.selection.is_some());
    map.button(end, egui::PointerButton::Primary, false);
    assert_eq!(map.browser.selected, BTreeSet::from([1, 2]));
    assert_eq!(map.browser.selection_anchor, Some(2));
    assert!(map.browser.map_interaction.selection.is_none());
    assert_eq!(map.browser.map_bounds, Some(base_bounds()), "selection must not pan the map");
    map.browser.select_row(&[1, 2, 3], 2, true);
    assert_eq!(map.browser.selected, BTreeSet::from([1, 2, 3]), "list range must continue from the last selected marker");
}

#[test]
fn jobs_map_modifier_rectangles_add_and_capture_the_modifier_at_drag_start() {
    for modifiers in [egui::Modifiers { ctrl: true, ..Default::default() },
        egui::Modifiers { shift: true, ..Default::default() },
        egui::Modifiers { command: true, ..Default::default() }] {
        let mut map = MapHarness::new();
        map.browser.selected.insert(3);
        map.modifiers = modifiers;
        let (start, end) = map.first_pair_box();
        map.begin_drag(start, end, egui::PointerButton::Primary);
        map.modifiers = Default::default();
        map.button(end, egui::PointerButton::Primary, false);
        assert_eq!(map.browser.selected, BTreeSet::from([1, 2, 3]));
    }
}

#[test]
fn jobs_map_click_double_click_and_empty_rectangle_remain_distinct() {
    let mut map = MapHarness::new();
    let first = map.point(1);
    map.click(first);
    assert_eq!(map.browser.selected, BTreeSet::from([1]));
    assert_eq!(map.browser.selection_anchor, Some(1));
    assert_eq!(map.action, None);
    map.click(first);
    assert_eq!(map.action, Some("open-job"));
    assert_eq!(map.browser.open_target, Some(1));
    map.modifiers.ctrl = true;
    map.click(map.point(3));
    assert_eq!(map.browser.selected, BTreeSet::from([1, 3]));
    map.modifiers = Default::default();
    let start = map.rect.left_top() + egui::vec2(15., 15.);
    map.drag(start, start + egui::vec2(30., 30.));
    assert!(map.browser.selected.is_empty());
    assert_eq!(map.browser.selection_anchor, None);
}

#[test]
fn jobs_map_rectangle_includes_only_current_filtered_and_on_screen_markers() {
    let mut map = MapHarness::new();
    map.points.retain(|(id, _)| *id != 2);
    map.points.push((9, [110_000., 450_700.]));
    map.frame(vec![]);
    let start = map.rect.left_top() + egui::vec2(3., 3.);
    let end = map.rect.right_bottom() + egui::vec2(70., 70.);
    map.drag(start, end);
    assert_eq!(map.browser.selected, BTreeSet::from([1, 3]), "release outside the map clips the rectangle, never selects hidden/filter-excluded jobs");
}

#[test]
fn jobs_map_filter_or_resize_changes_cancel_a_pending_rectangle() {
    for resize in [false, true] {
        let mut map = MapHarness::new();
        map.browser.selected.insert(3);
        let (start, end) = map.first_pair_box();
        map.begin_drag(start, end, egui::PointerButton::Primary);
        assert!(map.browser.map_interaction.selection.is_some());
        if resize { map.size = egui::vec2(400., 600.); }
        else { map.points.retain(|(id, _)| *id != 2); }
        map.frame(vec![]);
        assert!(map.browser.map_interaction.selection.is_none());
        map.button(end, egui::PointerButton::Primary, false);
        assert_eq!(map.browser.selected, BTreeSet::from([3]));
        assert_eq!(map.browser.map_bounds, Some(base_bounds()));
    }
}

#[test]
fn jobs_map_escape_focus_loss_and_pointer_loss_cancel_without_changing_selection() {
    for reason in 0..3 {
        let mut map = MapHarness::new();
        map.browser.selected.insert(3);
        let (start, end) = map.first_pair_box();
        map.begin_drag(start, end, egui::PointerButton::Primary);
        let events = match reason {
            0 => vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None,
                pressed: true, repeat: false, modifiers: Default::default() }],
            1 => { map.focused = false; vec![] },
            _ => vec![egui::Event::PointerGone],
        };
        map.frame(events);
        assert!(map.browser.map_interaction.selection.is_none());
        map.focused = true;
        map.button(end, egui::PointerButton::Primary, false);
        assert_eq!(map.browser.selected, BTreeSet::from([3]));
    }
}

#[test]
fn jobs_map_middle_and_right_pan_use_incremental_motion_without_selecting() {
    for button in [egui::PointerButton::Middle, egui::PointerButton::Secondary] {
        let mut map = MapHarness::new();
        map.browser.selected.insert(2);
        let start = map.rect.center();
        let step = egui::vec2(30., 20.);
        map.begin_drag(start, start + step, button);
        let once = map.browser.map_bounds.unwrap();
        map.frame(vec![egui::Event::PointerMoved(start + step * 2.)]);
        let twice = map.browser.map_bounds.unwrap();
        map.button(start + step * 2., button, false);
        assert_ne!(once, base_bounds(), "panning must move the extent");
        assert!(((twice.min_x - base_bounds().min_x) - 2. * (once.min_x - base_bounds().min_x)).abs() < 0.0001);
        assert!(((twice.min_y - base_bounds().min_y) - 2. * (once.min_y - base_bounds().min_y)).abs() < 0.0001);
        assert_eq!(map.browser.map_bounds, Some(twice), "release with no movement must not pan again");
        assert_eq!(map.browser.selected, BTreeSet::from([2]));
        assert!(map.browser.map_interaction.selection.is_none());
    }
}

fn assert_texture_tracks_world(map: &MapHarness, output: &egui::FullOutput,
    id: egui::TextureId, image: Bounds) {
    let shape = output.shapes.iter().find(|shape| {
        matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.texture_id == id)
    }).expect("overlapping map pixels must remain visible");
    let egui::Shape::Mesh(mesh) = &shape.shape else { unreachable!() };
    let view = viewport_bounds(map.browser.map_bounds.unwrap(), map.rect.size());
    for vertex in &mesh.vertices {
        let world = [image.min_x + f64::from(vertex.uv.x) * (image.max_x - image.min_x),
            image.max_y - f64::from(vertex.uv.y) * (image.max_y - image.min_y)];
        assert!(vertex.pos.distance(map_point(map.rect, view, world)) < 0.001,
            "cached pixels must follow RD coordinates, never stretch across the new viewport");
        assert!(map.rect.expand(0.001).contains(vertex.pos));
        assert!(egui::Rect::from_min_max(Pos2::ZERO, egui::pos2(1., 1.)).contains(vertex.uv));
    }
    assert_eq!(shape.clip_rect.intersect(map.rect), shape.clip_rect);
}

fn send_background(request: &BackgroundRequest) {
    request.reply.send(Ok(OsmRaster {
        bounds: request.bounds, width: request.size[0], height: request.size[1],
        rgba: vec![255; request.size[0] as usize * request.size[1] as usize * 4],
        stale_cache: false,
    })).unwrap();
}

#[test]
fn jobs_map_retains_georeferenced_pixels_during_pan_zoom_resize_and_dpi_changes() {
    let mut map = MapHarness::new();
    let texture = map.ctx.load_texture("old-jobs-map", egui::ColorImage::new([2, 2], Color32::GREEN), egui::TextureOptions::LINEAR);
    let id = texture.id();
    let image_bounds = viewport_bounds(base_bounds(), map.rect.size());
    map.browser.map_texture = Some((image_bounds, texture));
    let start = map.rect.center();
    map.begin_drag(start, start + egui::vec2(30., 20.), egui::PointerButton::Secondary);
    let output = map.frame(vec![egui::Event::PointerMoved(start + egui::vec2(60., 40.))]);
    assert_texture_tracks_world(&map, &output, id, image_bounds);
    map.button(start + egui::vec2(60., 40.), egui::PointerButton::Secondary, false);
    map.size = egui::vec2(300., 700.);
    let output = map.frame(vec![]);
    assert_texture_tracks_world(&map, &output, id, image_bounds);
    map.browser.map_bounds = Some(zoom_bounds(map.browser.map_bounds.unwrap(), 0.5));
    map.ctx.set_pixels_per_point(3.);
    let output = map.frame(vec![]);
    assert_texture_tracks_world(&map, &output, id, image_bounds);
    assert_eq!(map.browser.map_interaction.viewport.unwrap().1, raster_size(map.rect.size(), 3.));
    map.browser.map_bounds = Some(Bounds { min_x: 120_000., max_x: 121_000., ..base_bounds() });
    let output = map.frame(vec![]);
    assert!(!output.shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.texture_id == id)
    }), "a retained image outside the current world extent must not be shown");

    // Pan away and back while a neighboring request is still in flight. The
    // late partial coverage must not evict the fully covered original view.
    let mut map = MapHarness::new();
    send_background(map.browser.map_interaction.background.as_ref().unwrap());
    map.frame(vec![]);
    let retained = map.browser.map_texture.as_ref().unwrap().1.id();
    map.time += BACKGROUND_REQUEST_INTERVAL;
    let start = map.rect.center();
    map.begin_drag(start, start + egui::vec2(120., 0.), egui::PointerButton::Secondary);
    assert!(map.browser.map_interaction.background.is_some());
    send_background(map.browser.map_interaction.background.as_ref().unwrap());
    // B's result is already queued when global polling runs. Returning the
    // pointer to A occurs later in this same frame, inside canvas().
    let output = map.frame(vec![egui::Event::PointerMoved(start)]);
    assert_eq!(map.browser.map_texture.as_ref().unwrap().1.id(), retained,
        "a late overlapping image must not expose gray space that the retained image already covers");
    assert_texture_tracks_world(&map, &output, retained, viewport_bounds(base_bounds(), map.rect.size()));
    assert!(map.browser.map_loading.is_none(), "the restored full view needs no replacement request");
}

#[test]
fn jobs_map_requests_during_pan_are_throttled_without_cancelling_useful_work() {
    let mut map = MapHarness::new();
    let first = map.browser.map_interaction.background.as_ref().unwrap().cancel.clone();
    let start = map.rect.center();
    map.begin_drag(start, start + egui::vec2(20., 10.), egui::PointerButton::Secondary);
    assert!(!first.load(Ordering::Acquire), "an overlapping request must survive moving the pointer");
    send_background(map.browser.map_interaction.background.as_ref().unwrap());
    map.frame(vec![]);
    assert!(map.browser.map_texture.is_some(), "completed overlapping imagery is usable even after a pan");
    let deadline = map.browser.map_interaction.request_after;
    while map.time + 0.03 < deadline {
        map.frame(vec![]);
        assert!(map.browser.map_loading.is_none(), "do not request every moving frame");
    }
    map.frame(vec![]);
    let active = map.browser.map_interaction.background.as_ref().unwrap().cancel.clone();
    assert!(!Arc::ptr_eq(&first, &active), "new view loads while the mouse remains held");
    for step in 2..15 {
        map.frame(vec![egui::Event::PointerMoved(start + egui::vec2(20. + step as f32, 10.))]);
        assert!(Arc::ptr_eq(&active, &map.browser.map_interaction.background.as_ref().unwrap().cancel));
        assert!(!active.load(Ordering::Acquire));
    }
    let latest = map.browser.map_interaction.viewport.unwrap().0;
    send_background(map.browser.map_interaction.background.as_ref().unwrap());
    map.frame(vec![]);
    assert_eq!(map.browser.map_interaction.background.as_ref().unwrap().bounds, latest,
        "after completion request only the latest view, never queued intermediate positions");
}

#[test]
fn jobs_map_cancelled_replies_and_failed_refreshes_preserve_retained_pixels() {
    let mut map = MapHarness::new();
    let texture = map.ctx.load_texture("retained-jobs-map", egui::ColorImage::new([2, 2], Color32::GREEN), egui::TextureOptions::LINEAR);
    let retained = texture.id();
    map.browser.map_texture = Some((base_bounds(), texture));
    let old = map.browser.map_interaction.background.as_ref().unwrap().cancel.clone();
    map.browser.map_bounds = Some(Bounds { min_x: 120_000., max_x: 121_000., ..base_bounds() });
    map.frame(vec![]);
    assert!(old.load(Ordering::Acquire), "fully offscreen work is no longer useful");
    send_background(map.browser.map_interaction.background.as_ref().unwrap());
    map.frame(vec![]);
    assert_eq!(map.browser.map_texture.as_ref().unwrap().1.id(), retained,
        "a cancelled late result must never replace the retained image");
    map.time += BACKGROUND_REQUEST_INTERVAL;
    map.frame(vec![]);
    let task = map.browser.map_interaction.background.as_ref().unwrap();
    task.reply.send(Err("synthetic offline response".into())).unwrap();
    map.frame(vec![]);
    assert_eq!(map.browser.map_texture.as_ref().unwrap().1.id(), retained);
    map.frame(vec![]);
    assert!(map.browser.map_loading.is_none(), "network failure must use retry backoff");
    map.browser.map_bounds = Some(Bounds { min_x: 120_100., max_x: 121_100., ..base_bounds() });
    map.frame(vec![]);
    map.time += BACKGROUND_REQUEST_INTERVAL;
    map.frame(vec![]);
    assert!(map.browser.map_loading.is_some(), "a different viewport must not inherit the old failure's long backoff");
    let reply = map.browser.map_interaction.background.as_ref().unwrap().reply.clone();
    map.browser.map_interaction = Default::default();
    reply.send(Err("late reply after hiding the map".into())).unwrap();
    poll_background(&map.ctx, &mut map.browser);
    assert_eq!(map.browser.map_texture.as_ref().unwrap().1.id(), retained);
    assert!(map.browser.map_loading.is_none());
}
