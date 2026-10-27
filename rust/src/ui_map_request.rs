//! Real-canvas background refresh. Central app wiring lives in ui.rs.
use super::*;
use sleufbase_rust::map_request::{self, Event, Provider, Viewport};
pub(super) struct State {
    scheduler: map_request::Scheduler,
    worker: Option<map_request::Worker>,
    clock: std::time::Instant,
    source: Option<Provider>,
    complete_image: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            scheduler: Default::default(),
            worker: None,
            clock: std::time::Instant::now(),
            source: None,
            complete_image: false,
        }
    }
}
impl State {
    pub(super) fn invalidate(&mut self) {
        self.scheduler.invalidate();
        if let Some(worker) = &mut self.worker {
            worker.cancel();
        }
        self.source = None;
        self.complete_image = false;
    }
    #[cfg(test)]
    pub(super) fn with_worker(worker: map_request::Worker) -> Self {
        Self {
            worker: Some(worker),
            ..Default::default()
        }
    }
    pub(super) fn busy(&self) -> bool {
        self.worker.as_ref().is_some_and(|w| w.busy())
    }
}
pub(super) fn viewport(view: &raster_point_ui::View, source: Provider) -> Viewport {
    let [left, top] = view.world(view.rect.left_top());
    let [right, bottom] = view.world(view.rect.right_bottom());
    let ratio = view.painter.ctx().pixels_per_point().max(0.1);
    let mut width = (view.rect.width() * ratio).ceil().max(1.) as u32;
    let mut height = (view.rect.height() * ratio).ceil().max(1.) as u32;
    // Keep full world coverage even on an unusually large/high-DPI display.
    let scale = (10_000. / width as f64)
        .min(10_000. / height as f64)
        .min((50_000_000. / (width as f64 * height as f64)).sqrt())
        .min(1.);
    width = (width as f64 * scale).floor().max(1.) as u32;
    height = (height as f64 * scale).floor().max(1.) as u32;
    Viewport {
        source,
        bounds: Bounds {
            min_x: left,
            min_y: bottom,
            max_x: right,
            max_y: top,
        },
        size: [width, height],
        logical_meters_per_pixel: Some(1. / view.scale),
    }
}
impl SleufBaseApp {
    /// Hook after canvas camera capture; it observes pan, zoom, resize and source
    /// changes. Call poll_map_background earlier in every update frame as well.
    pub(super) fn update_map_background(&mut self, view: &raster_point_ui::View) {
        use sleufbase_rust::project_view::BackgroundProvider as B;
        let source = if !self.project_view.use_background
            || self
                .geotiffs
                .iter()
                .any(|r| r.epsg.is_some_and(|e| e != 28992))
        {
            None
        } else {
            match self.project_view.provider() {
                B::PdokAerial => Some(Provider::Aerial),
                B::Cadastral => Some(Provider::Cadastral),
                B::OpenStreetMap => Some(Provider::Osm),
                _ => None,
            }
        };
        if source.is_none() || self.map_requests.source != source {
            self.pdok_texture = None;
            self.pdok_bounds = None;
            self.osm_texture = None;
            self.osm_bounds = None;
            self.map_requests.source = source;
            self.map_requests.complete_image = false;
        }
        let desired = source.map(|s| viewport(view, s));
        let dragging = view.response.dragged()
            && (!self.navigation.pointer_tool
                || view.response.dragged_by(egui::PointerButton::Middle));
        let now = self.map_requests.clock.elapsed();
        match self.map_requests.scheduler.update(desired, dragging, now) {
            Ok(true) => {
                if let Some(w) = &mut self.map_requests.worker {
                    w.cancel();
                }
            }
            Ok(false) => {}
            Err(error) => {
                self.message = error;
                return;
            }
        }
        if self.map_requests.scheduler.suppressed() {
            self.pdok_texture = None;
            self.pdok_bounds = None;
            self.message = "Zoom verder in om de kadastrale kaart te bekijken".into();
        }
        if let Some(request) = self.map_requests.scheduler.due(now) {
            #[cfg(test)]
            if self.map_requests.worker.is_none() {
                self.map_requests.scheduler.complete(request, false);
                return;
            }
            if self.map_requests.worker.is_none() {
                match map_request::Worker::new(project::app_data_directory().join("osm-tiles")) {
                    Ok(w) => self.map_requests.worker = Some(w),
                    Err(error) => {
                        self.message = error;
                        return;
                    }
                }
            }
            self.map_requests.worker.as_mut().unwrap().start(request);
        }
        self.pdok_busy = self.map_requests.busy() && source != Some(Provider::Osm);
        self.osm_busy = self.map_requests.busy() && source == Some(Provider::Osm);
        if let Some(deadline) = self.map_requests.scheduler.next_deadline() {
            view.painter
                .ctx()
                .request_repaint_after(deadline.saturating_sub(now).max(Duration::from_millis(1)));
        }
        if self.map_requests.busy() {
            view.painter
                .ctx()
                .request_repaint_after(Duration::from_millis(30));
        }
    }
    pub(super) fn poll_map_background(&mut self, ctx: &egui::Context) {
        let event = self.map_requests.worker.as_mut().and_then(|w| w.poll());
        let Some(event) = event else {
            return;
        };
        let (request, result, terminal, progress) = match event {
            Event::Preview {
                request,
                pixels,
                completed,
                total,
            } => (request, Ok(pixels), false, Some((completed, total))),
            Event::Complete { request, result } => (request, result, true, None),
        };
        if !self.map_requests.scheduler.accepts(request) {
            return;
        }
        if !terminal {
            // Keep the last complete image visible while missing edge tiles
            // arrive; replacing it with a partial frame makes every pan flash.
            if self.map_requests.complete_image { return; }
            self.map_requests.scheduler.preview(request);
        }
        if let Some((completed, total)) = progress {
            self.message = format!("Kaartachtergrond ophalen: {completed}/{total} tegels");
        }
        match result {
            Ok(pixels) => {
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [pixels.width as usize, pixels.height as usize],
                    &pixels.rgba,
                );
                if request.view.source == Provider::Osm {
                    if let Some(t) = &mut self.osm_texture {
                        t.set(image, egui::TextureOptions::LINEAR);
                    } else {
                        self.osm_texture = Some(ctx.load_texture(
                            "Kaartachtergrond OSM",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    self.osm_bounds = Some(pixels.bounds);
                    self.osm_visible = true;
                } else {
                    if let Some(t) = &mut self.pdok_texture {
                        t.set(image, egui::TextureOptions::LINEAR);
                    } else {
                        self.pdok_texture = Some(ctx.load_texture(
                            "Kaartachtergrond PDOK",
                            image,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                    self.pdok_bounds = Some(pixels.bounds);
                    self.osm_visible = false;
                    self.pdok_source = if request.view.source == Provider::Cadastral {
                        pdok_wms::Source::Cadastral
                    } else {
                        pdok_wms::Source::Aerial
                    };
                }
                if terminal {
                    self.map_requests.complete_image = true;
                    self.map_requests.scheduler.complete(request, true);
                    self.message = if pixels.stale_cache {
                        "Kaart geladen uit verouderde cache; netwerk niet beschikbaar".into()
                    } else {
                        "Kaartachtergrond geladen".into()
                    };
                }
            }
            Err(error) => {
                if terminal {
                    self.map_requests.scheduler.complete(request, false);
                }
                self.message = format!("Kaartachtergrond ophalen mislukt: {error}");
            }
        }
        self.pdok_busy = self.map_requests.busy() && request.view.source != Provider::Osm;
        self.osm_busy = self.map_requests.busy() && request.view.source == Provider::Osm;
        ctx.request_repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(
        ctx: &egui::Context,
        size: [f32; 2],
        source: Provider,
        inspect: impl FnMut(&raster_point_ui::View, Viewport),
    ) {
        frame_at_scale(ctx, size, source, 0.5, inspect);
    }
    fn frame_at_scale(
        ctx: &egui::Context,
        size: [f32; 2],
        source: Provider,
        scale: f64,
        mut inspect: impl FnMut(&raster_point_ui::View, Viewport),
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                Pos2::ZERO,
                egui::vec2(size[0], size[1]),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (response, painter) = ui.allocate_painter(
                    egui::vec2(size[0] - 20., size[1] - 20.),
                    egui::Sense::drag(),
                );
                let rect = response.rect;
                let view = raster_point_ui::View {
                    response,
                    painter,
                    rect,
                    center: rect.center(),
                    origin: [155000., 463000.],
                    scale,
                    clicked: None,
                };
                let v = viewport(&view, source);
                inspect(&view, v);
            });
        });
    }
    #[test]
    fn actual_canvas_dpi_zoom_resize_and_logical_cadastral_threshold() {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(2.);
        let mut scheduler = map_request::Scheduler::default();
        frame(&ctx, [180., 140.], Provider::Cadastral, |view, v| {
            assert_eq!(v.logical_meters_per_pixel, Some(2.));
            assert!(v.cadastral_too_far());
            assert_eq!(
                v.size[0],
                (view.rect.width() * ctx.pixels_per_point()).ceil() as u32
            );
            assert_eq!(v.bounds.min_x, view.world(view.rect.left_top())[0]);
        });
        frame(&ctx, [180., 140.], Provider::Aerial, |_, v| {
            scheduler.update(Some(v), false, Duration::ZERO).unwrap();
        });
        let first = scheduler.due(Duration::from_millis(45)).unwrap();
        scheduler.complete(first, true);
        frame(&ctx, [260., 200.], Provider::Aerial, |_, v| {
            assert!(!scheduler
                .update(Some(v), false, Duration::from_millis(50))
                .unwrap());
        });
        assert!(scheduler.due(Duration::from_millis(95)).is_none(),"buffer covers the resized viewport");
        frame_at_scale(&ctx, [260., 200.], Provider::Aerial, 1., |_, v| {
            assert_eq!(v.logical_meters_per_pixel, Some(1.));
            assert!(scheduler
                .update(Some(v), false, Duration::from_millis(100))
                .unwrap());
        });
        assert!(scheduler.due(Duration::from_millis(145)).is_some());
        frame_at_scale(
            &ctx,
            [180., 140.],
            Provider::Cadastral,
            1. / 1.99,
            |_, v| {
                assert!(!v.cadastral_too_far());
            },
        );
    }
    #[test]
    fn stale_fake_preview_after_project_and_disabled_source_cannot_publish_textures() {
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = map_request::Worker::with_fetcher(move |request, cancel, publish| {
            publish(
                map_request::Pixels {
                    bounds: request.view.bounds,
                    width: 1,
                    height: 1,
                    rgba: vec![12, 34, 56, 255],
                    stale_cache: false,
                },
                1,
                2,
            )?;
            tx.send(()).unwrap();
            while !cancel.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err("cancelled".into())
        })
        .unwrap();
        let mut app = SleufBaseApp {
            map_requests: State::with_worker(worker),
            ..Default::default()
        };
        app.project_view.use_background = true;
        app.project_view.background_source = "PDOK Luchtfoto NL actueel (8 cm, deels 5 cm)".into();
        let ctx = egui::Context::default();
        frame(&ctx, [180., 140.], Provider::Aerial, |view, _| {
            app.update_map_background(view)
        });
        app.map_requests.clock -= Duration::from_millis(100);
        frame(&ctx, [180., 140.], Provider::Aerial, |view, _| {
            app.update_map_background(view)
        });
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        app.map_requests.invalidate();
        app.poll_map_background(&ctx);
        assert!(app.pdok_texture.is_none());
        assert!(app.osm_texture.is_none());
        app.pdok_texture = Some(ctx.load_texture(
            "old",
            egui::ColorImage::new([1, 1], Color32::WHITE),
            egui::TextureOptions::LINEAR,
        ));
        app.project_view.use_background = false;
        frame(&ctx, [180., 140.], Provider::Aerial, |view, _| {
            app.update_map_background(view)
        });
        assert!(app.pdok_texture.is_none());
        assert!(app.osm_texture.is_none());
    }
    #[test]
    fn queued_aerial_preview_after_source_switch_cannot_publish() {
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = map_request::Worker::with_fetcher(move |request, cancel, publish| {
            publish(
                map_request::Pixels {
                    bounds: request.view.bounds,
                    width: 1,
                    height: 1,
                    rgba: vec![1, 2, 3, 255],
                    stale_cache: false,
                },
                1,
                2,
            )?;
            tx.send(()).unwrap();
            while !cancel.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err("cancelled".into())
        })
        .unwrap();
        let mut app = SleufBaseApp {
            map_requests: State::with_worker(worker),
            ..Default::default()
        };
        app.project_view.use_background = true;
        app.project_view.background_source = "PDOK luchtfoto".into();
        let ctx = egui::Context::default();
        frame(&ctx, [180., 140.], Provider::Aerial, |view, _| {
            app.update_map_background(view);
        });
        app.map_requests.clock -= Duration::from_millis(100);
        frame(&ctx, [180., 140.], Provider::Aerial, |view, _| {
            app.update_map_background(view);
        });
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        app.project_view.background_source = "OpenStreetMap".into();
        frame(&ctx, [180., 140.], Provider::Osm, |view, _| {
            app.update_map_background(view);
        });
        app.poll_map_background(&ctx);
        assert_eq!(app.map_requests.source, Some(Provider::Osm));
        assert!(app.pdok_texture.is_none());
        assert!(app.osm_texture.is_none());
        assert!(app.pdok_bounds.is_none());
        assert!(app.osm_bounds.is_none());
    }
    #[test]
    fn filling_missing_tiles_retains_the_previous_complete_texture() {
        let (sent,received)=mpsc::sync_channel(1);
        let worker=map_request::Worker::with_fetcher(move |request,cancel,publish| {
            publish(map_request::Pixels{bounds:request.view.bounds,width:1,height:1,
                rgba:vec![255,0,0,255],stale_cache:false},1,3)?;
            sent.send(()).unwrap();
            while !cancel.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err("cancelled".into())
        }).unwrap();
        let ctx=egui::Context::default();
        let mut app=SleufBaseApp {map_requests:State::with_worker(worker),..Default::default()};
        let old=Bounds{min_x:1.,min_y:2.,max_x:3.,max_y:4.};
        app.map_requests.source=Some(Provider::Aerial);app.map_requests.complete_image=true;
        app.pdok_bounds=Some(old);
        app.pdok_texture=Some(ctx.load_texture("complete",egui::ColorImage::new([1,1],Color32::BLUE),Default::default()));
        frame(&ctx,[180.,140.],Provider::Aerial,|v,_|app.update_map_background(v));
        app.map_requests.clock-=Duration::from_millis(100);
        frame(&ctx,[180.,140.],Provider::Aerial,|v,_|app.update_map_background(v));
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        app.poll_map_background(&ctx);
        assert_eq!(app.pdok_bounds,Some(old));
        assert!(app.map_requests.complete_image);
        app.map_requests.invalidate();
    }

}
