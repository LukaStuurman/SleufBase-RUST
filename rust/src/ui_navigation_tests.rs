//! Exercise the real egui event/paint path, including the original shell.
use super::*;

fn frame(
    ctx: &egui::Context,
    app: &mut SleufBaseApp,
    size: [f32; 2],
    events: Vec<egui::Event>,
) -> raster_point_ui::View {
    frame_dpi(ctx, app, size, events, 1.)
}
fn frame_dpi(
    ctx: &egui::Context,
    app: &mut SleufBaseApp,
    size: [f32; 2],
    events: Vec<egui::Event>,
    dpi: f32,
) -> raster_point_ui::View {
    let mut view = None;
    let mut raw = egui::RawInput::default();
    raw.viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .native_pixels_per_point = Some(dpi);
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size.into())),
            events,
            predicted_dt: 0.2,
            ..raw
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                view = canvas(
                    ui,
                    &[],
                    &[],
                    &[],
                    &app.overlays,
                    &[],
                    &[],
                    &[],
                    None,
                    &[],
                    None,
                    app.pdok_bounds,
                    false,
                    &mut app.zoom,
                    &mut app.pan,
                    RULES,
                    Some(&mut app.navigation),
                );
            });
        },
    );
    view.unwrap()
}
fn wheel(pos: Pos2, y: f32) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0., y),
            modifiers: Default::default(),
        },
    ]
}
fn close(a: [f64; 2], b: [f64; 2]) {
    assert!(
        (a[0] - b[0]).abs() < 1e-4 && (a[1] - b[1]).abs() < 1e-4,
        "{a:?} != {b:?}"
    );
}

#[test]
fn navigation_zoom_anchors_the_world_under_the_pointer_at_all_sizes() {
    for size in [[800., 600.], [1200., 760.], [1500., 930.], [1920., 1080.]] {
        for fraction in [0.08, 0.5, 0.92] {
            let ctx = egui::Context::default();
            let mut app = SleufBaseApp::default();
            app.navigation.focus = Some(location_ui::Focus {
                center: [155000., 463000.],
                meters_per_pixel: 0.2,
            });
            let before = frame(&ctx, &mut app, size, vec![]);
            let p = before.rect.lerp_inside(egui::vec2(fraction, 1. - fraction));
            let world = before.world(p);
            let _ = frame(&ctx, &mut app, size, vec![egui::Event::PointerMoved(p)]);
            let after = frame(&ctx, &mut app, size, wheel(p, 120.));
            assert!(after.scale > before.scale);
            close(world, after.world(p));
        }
    }
}

#[test]
fn navigation_background_refresh_must_not_refit_the_camera() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp::default();
    let first = frame(&ctx, &mut app, [1200., 760.], vec![]);
    let original = project_view_ui::capture_camera(&first);
    for _ in 0..8 {
        app.pdok_bounds = app.navigation.viewport;
        let next = frame(&ctx, &mut app, [1200., 760.], vec![]);
        let actual = project_view_ui::capture_camera(&next);
        close(original.center, actual.center);
        assert!(
            (actual.meters_per_pixel - original.meters_per_pixel).abs() < 1e-9,
            "a fetched viewport changed the camera: {original:?} -> {actual:?}"
        );
    }
}

#[test]
fn navigation_shell_map_uses_the_visible_available_area() {
    for size in [[1200., 760.], [1500., 930.], [1920., 1080.]] {
        let ctx = egui::Context::default();
        let mut app = SleufBaseApp::default();
        for _ in 0..3 {
            let out = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, size.into())),
                    ..Default::default()
                },
                |ctx| app.render(ctx),
            );
            let map = out
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Rect(r)
                        if r.fill == Color32::WHITE
                            && r.corner_radius == egui::CornerRadius::same(18)
                            && r.rect.width() > 500.
                            && r.rect.height() > 100. =>
                    {
                        Some((r.rect, s.clip_rect))
                    }
                    _ => None,
                })
                .max_by(|a, b| a.0.area().total_cmp(&b.0.area()))
                .expect("map shape");
            assert!(
                map.0.width() > size[0] - 500. && map.0.height() > size[1] - 230.,
                "map too small: {map:?} at {size:?}"
            );
            assert!(map.1.contains_rect(map.0), "map clipped: {map:?}");
            assert!(map.0.bottom() < size[1] - 40., "map covers status: {map:?}");
        }
    }
}

fn button(pos: Pos2, button: egui::PointerButton, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: Default::default(),
        },
    ]
}

#[test]
fn navigation_pan_is_incremental_and_drawing_tools_keep_primary_drags() {
    for tool in [false, true] {
        for button_id in [
            egui::PointerButton::Primary,
            egui::PointerButton::Middle,
            egui::PointerButton::Secondary,
        ] {
            let ctx = egui::Context::default();
            let mut app = SleufBaseApp::default();
            app.navigation.pointer_tool = tool;
            let start = frame(&ctx, &mut app, [1000., 700.], vec![]).rect.center();
            let _ = frame(
                &ctx,
                &mut app,
                [1000., 700.],
                vec![egui::Event::PointerMoved(start)],
            );
            let _ = frame(
                &ctx,
                &mut app,
                [1000., 700.],
                button(start, button_id, true),
            );
            for delta in [egui::vec2(30., 20.), egui::vec2(70., -15.)] {
                let _ = frame(
                    &ctx,
                    &mut app,
                    [1000., 700.],
                    vec![egui::Event::PointerMoved(start + delta)],
                );
            }
            let moved = frame(
                &ctx,
                &mut app,
                [1000., 700.],
                button(start + egui::vec2(70., -15.), button_id, false),
            );
            let expected = if button_id == egui::PointerButton::Middle
                || (button_id == egui::PointerButton::Primary && !tool)
            {
                egui::vec2(70., -15.)
            } else {
                egui::Vec2::ZERO
            };
            assert_eq!(app.pan, expected, "tool={tool}, button={button_id:?}");
            close(
                moved.world(moved.rect.center() + expected),
                [155000., 463000.],
            );
            assert!(moved.clicked.is_none(), "drag must not create a map point");
        }
    }
}

#[test]
fn navigation_resize_and_background_visibility_preserve_world_center_and_scale() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp {
        pan: egui::vec2(110., -80.),
        zoom: 2.4,
        ..Default::default()
    };
    let first = frame(&ctx, &mut app, [1500., 930.], vec![]);
    let camera = project_view_ui::capture_camera(&first);
    for size in [[1200., 760.], [800., 600.], [1920., 1080.], [1500., 930.]] {
        app.navigation.show_background = Some(false);
        let view = frame(&ctx, &mut app, size, vec![]);
        close(camera.center, view.world(view.rect.center()));
        assert_eq!(camera.meters_per_pixel, 1. / view.scale);
        for p in [
            view.rect.left_top(),
            view.rect.right_bottom(),
            view.rect.center(),
        ] {
            assert!(view.project(view.world(p)).distance(p) < 0.001);
        }
        let b = app.navigation.viewport.unwrap();
        close([b.min_x, b.max_y], view.world(view.rect.left_top()));
        close([b.max_x, b.min_y], view.world(view.rect.right_bottom()));
    }
}

#[test]
fn navigation_wheel_is_consumed_once_and_pinch_is_anchored() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp::default();
    let first = frame(&ctx, &mut app, [1200., 760.], vec![]);
    let p = first.rect.center() + egui::vec2(170., 90.);
    let _ = frame(
        &ctx,
        &mut app,
        [1200., 760.],
        vec![egui::Event::PointerMoved(p)],
    );
    let next = frame(&ctx, &mut app, [1200., 760.], wheel(p, 120.));
    close(first.world(p), next.world(p));
    assert_eq!(ctx.input(|i| i.raw_scroll_delta), egui::Vec2::ZERO);
    assert_eq!(ctx.input(|i| i.smooth_scroll_delta), egui::Vec2::ZERO);
    let settled = frame(&ctx, &mut app, [1200., 760.], vec![]);
    assert_eq!(next.scale, settled.scale, "no synthetic wheel tail");
    let pinched = frame(&ctx, &mut app, [1200., 760.], vec![egui::Event::Zoom(1.5)]);
    close(next.world(p), pinched.world(p));
    assert!((pinched.scale / next.scale - 1.5).abs() < 1e-6);
    assert!(app.navigation.camera_changed);
}

#[test]
fn navigation_zoom_can_leave_a_small_tiff_and_clamps_in_world_units() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp::default();
    let first = frame(&ctx, &mut app, [1200., 760.], vec![]);
    let p = first.rect.center();
    let _ = frame(
        &ctx,
        &mut app,
        [1200., 760.],
        vec![egui::Event::PointerMoved(p)],
    );
    let far = frame(&ctx, &mut app, [1200., 760.], wheel(p, -10000.));
    assert_eq!(far.scale, 0.0001);
    let near = frame(&ctx, &mut app, [1200., 760.], wheel(p, 10000.));
    assert_eq!(near.scale, 1000.);
    assert!(near.scale.is_finite());
}

#[test]
fn navigation_dpi_changes_request_pixels_without_changing_world_coverage() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp::default();
    let first = frame(&ctx, &mut app, [1200., 760.], vec![]);
    let initial = map_request_ui::viewport(&first, sleufbase_rust::map_request::Provider::Aerial);
    for dpi in [1.25, 1.5, 2.] {
        let next = frame_dpi(&ctx, &mut app, [1200., 760.], vec![], dpi);
        let request =
            map_request_ui::viewport(&next, sleufbase_rust::map_request::Provider::Aerial);
        assert_eq!(request.bounds, initial.bounds);
        assert_eq!(
            request.size,
            [
                (next.rect.width() * dpi).ceil() as u32,
                (next.rect.height() * dpi).ceil() as u32
            ]
        );
    }
}

#[test]
fn navigation_progressive_background_finishes_instead_of_cancelling_after_first_tile() {
    use sleufbase_rust::map_request::{Pixels, Worker};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let count = Arc::new(AtomicUsize::new(0));
    let started = count.clone();
    let worker = Worker::with_fetcher(move |request, cancel, preview| {
        started.fetch_add(1, Ordering::Relaxed);
        let v = request.view;
        let pixels = |color: [u8; 4]| Pixels {
            bounds: v.bounds,
            width: v.size[0],
            height: v.size[1],
            rgba: color.repeat(v.size[0] as usize * v.size[1] as usize),
            stale_cache: false,
        };
        preview(pixels([243u8, 243, 243, 255]), 1, 6)?;
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(4));
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
        }
        Ok(pixels([20, 90, 120, 255]))
    })
    .unwrap();
    let mut app = SleufBaseApp {
        map_requests: map_request_ui::State::with_worker(worker),
        ..Default::default()
    };
    let ctx = egui::Context::default();
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.message != "Kaartachtergrond geladen" {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1200., 760.),
                )),
                ..Default::default()
            },
            |ctx| app.render(ctx),
        );
        assert!(
            Instant::now() < deadline,
            "progressive background never finishes: {}",
            app.message
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(count.load(Ordering::Relaxed), 1);
    assert!(app.pdok_texture.is_some());
}

#[test]
fn navigation_sidebar_and_location_overlay_do_not_zoom_the_map() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp::default();
    let shell = |app: &mut SleufBaseApp, events| {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1200., 760.),
                )),
                events,
                ..Default::default()
            },
            |ctx| app.render(ctx),
        )
    };
    for _ in 0..3 {
        let _ = shell(&mut app, vec![]);
    }
    let camera = app.project_view.camera.clone();
    let _ = shell(&mut app, wheel(egui::pos2(100., 300.), 120.));
    assert_eq!(app.project_view.camera, camera);
    let _ = shell(
        &mut app,
        vec![egui::Event::PointerMoved(egui::pos2(1000., 400.))],
    );
    assert_eq!(
        app.project_view.camera, camera,
        "list scroll must not leak into the map later"
    );
    let _ = shell(&mut app, wheel(egui::pos2(1000., 400.), 120.));
    assert_ne!(app.project_view.camera, camera);
    assert!(
        app.dirty,
        "wheel navigation must be saved in the project camera"
    );
    app.location.open = true;
    let mut out = shell(&mut app, vec![]);
    for _ in 0..3 {
        out = shell(&mut app, vec![]);
    }
    let entry = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text().contains("Adres, plaats, RD of GPS") => {
                Some(t.pos)
            }
            _ => None,
        })
        .expect("location entry")
        + egui::vec2(20., 8.);
    let camera = app.project_view.camera.clone();
    let _ = shell(&mut app, vec![egui::Event::PointerMoved(entry)]);
    let _ = shell(&mut app, wheel(entry, 120.));
    assert_eq!(
        app.project_view.camera, camera,
        "foreground input must shield the map"
    );
}

#[test]
fn navigation_plain_click_remains_available_after_zoom_and_pan() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp::default();
    let first = frame(&ctx, &mut app, [1000., 700.], vec![]);
    let p = first.rect.center();
    let _ = frame(
        &ctx,
        &mut app,
        [1000., 700.],
        vec![egui::Event::PointerMoved(p)],
    );
    let _ = frame(&ctx, &mut app, [1000., 700.], wheel(p, 120.));
    let _ = frame(
        &ctx,
        &mut app,
        [1000., 700.],
        button(p, egui::PointerButton::Primary, true),
    );
    let clicked = frame(
        &ctx,
        &mut app,
        [1000., 700.],
        button(p, egui::PointerButton::Primary, false),
    );
    assert_eq!(clicked.clicked, Some(p));
    close(clicked.world(p), [155000., 463000.]);
}

#[test]
fn navigation_layer_changes_do_not_implicitly_fit_or_move_the_camera() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp::default();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/cad-space/R2018-normal-ascii.dxf");
    app.overlays.push(load_dxf(&path).unwrap());
    let first = frame(&ctx, &mut app, [1200., 760.], vec![]);
    let camera = project_view_ui::capture_camera(&first);
    app.overlays[0].visible = false;
    app.pdok_bounds = Some(Bounds {
        min_x: -1e6,
        min_y: -1e6,
        max_x: 1e6,
        max_y: 1e6,
    });
    let after = frame(&ctx, &mut app, [1200., 760.], vec![]);
    assert_eq!(project_view_ui::capture_camera(&after), camera);
    app.overlays[0].visible = true;
    let after = frame(&ctx, &mut app, [1200., 760.], vec![]);
    assert_eq!(project_view_ui::capture_camera(&after), camera);
}

#[test]
fn navigation_extreme_off_center_zoom_preserves_rd_precision() {
    let ctx = egui::Context::default();
    let mut app = SleufBaseApp::default();
    let first = frame(&ctx, &mut app, [1200., 760.], vec![]);
    let p = first.rect.lerp_inside(egui::vec2(0.91, 0.07));
    let anchor = first.world(p);
    let _ = frame(
        &ctx,
        &mut app,
        [1200., 760.],
        vec![egui::Event::PointerMoved(p)],
    );
    for delta in [-10000., 10000., -10000., 10000.] {
        let view = frame(&ctx, &mut app, [1200., 760.], wheel(p, delta));
        close(view.world(p), anchor);
    }
}
