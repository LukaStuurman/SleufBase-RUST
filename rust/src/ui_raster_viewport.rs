//! Camera-dependent TIFF detail, with the original full-image preview as fallback.
use super::*;
use sleufbase_rust::raster_loader::{ViewportLoader, ViewportRequest};
pub(super) struct Crop {
    pub path: PathBuf,
    pub uv: [f64; 4],
    pub texture: egui::TextureHandle,
}

impl SleufBaseApp {
    pub(super) fn poll_viewport_previews(&mut self, ctx: &egui::Context) {
        let Some(loader) = self.viewport_loader.as_mut() else {
            return;
        };
        for ready in loader.poll() {
            if self
                .geotiffs
                .get(ready.id)
                .is_none_or(|r| r.path != ready.path)
            {
                continue;
            }
            if let Ok(window) = ready.result {
                self.navigation
                    .raster_crops
                    .resize_with(self.geotiffs.len(), || None);
                let image = egui::ColorImage::from_rgba_unmultiplied(
                    [
                        window.preview.width as usize,
                        window.preview.height as usize,
                    ],
                    &window.preview.rgba,
                );
                let texture = ctx.load_texture(
                    format!("detail:{}", ready.path.display()),
                    image,
                    egui::TextureOptions::LINEAR,
                );
                self.navigation.raster_crops[ready.id] = Some(Crop {
                    path: ready.path,
                    uv: window.uv,
                    texture,
                });
            }
            // Failed detail decodes retain the usable base preview. A new
            // camera generation can retry without an endless per-frame loop.
        }
        if loader.remaining() > 0 {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
    pub(super) fn request_viewport_previews(&mut self, view: &raster_point_ui::View) {
        if self.geotiffs.is_empty() {
            return;
        }
        let ratio = view.painter.ctx().pixels_per_point();
        let corners = [
            view.rect.left_top(),
            view.rect.right_top(),
            view.rect.right_bottom(),
            view.rect.left_bottom(),
        ]
        .map(|p| view.world(p));
        let requests = self
            .geotiffs
            .iter()
            .enumerate()
            .filter_map(|(id, r)| {
                if r.width == 0 || r.height == 0 {
                    return None;
                }
                let mut uv = [[0.; 2]; 4];
                for (i, [x, y]) in corners.into_iter().enumerate() {
                    let (col, row) = r.transform.world_to_pixel(x, y).ok()?;
                    uv[i] = [col / r.width as f64, row / r.height as f64];
                }
                // Match the original strict world-bounds intersection gate,
                // including TIFFs that merely touch the viewport edge.
                let min_x = corners.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
                let max_x = corners.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
                let min_y = corners.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
                let max_y = corners.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
                if r.bounds.max_x <= min_x || r.bounds.min_x >= max_x
                    || r.bounds.max_y <= min_y || r.bounds.min_y >= max_y { return None; }
                Some(ViewportRequest {
                    id,
                    path: r.path.clone(),
                    corners: uv,
                    screen_pixels: [
                        (view.rect.width() * ratio).ceil().max(1.) as u32,
                        (view.rect.height() * ratio).ceil().max(1.) as u32,
                    ],
                })
            })
            .collect::<Vec<_>>();
        self.apply_viewport_requests(&requests);
        if self.viewport_loader.as_ref().is_some_and(|loader| loader.remaining() > 0) {
            view.painter.ctx().request_repaint_after(Duration::from_millis(30));
        }
    }
    fn apply_viewport_requests(&mut self, requests: &[ViewportRequest]) {
        if self.viewport_loader.is_none() {
            match ViewportLoader::new() {
                Ok(loader) => self.viewport_loader = Some(loader),
                Err(e) => {
                    self.message = e;
                    return;
                }
            }
        }
        let loader = self.viewport_loader.as_mut().unwrap();
        loader.update(requests);
        let per_layer = loader.layer_pixel_budget();
        let mut retained = 0u64;
        for (id, crop) in self.navigation.raster_crops.iter_mut().enumerate() {
            let keep = crop.as_ref().is_some_and(|crop| {
                let size = crop.texture.size();
                let pixels = size[0] as u64 * size[1] as u64;
                let visible = requests.iter().any(|r| r.id == id && r.path == crop.path && r.visible());
                if visible && pixels <= per_layer && retained.saturating_add(pixels) <= loader.pixel_budget() {
                    retained += pixels;
                    true
                } else { false }
            });
            if !keep { *crop = None; }
        }
    }
}

/// Base and detail occupy disjoint regions. Painting the detail over the base
/// would incorrectly double translucent TIFF pixels and the opacity setting.
pub(super) fn paint(
    painter: &egui::Painter,
    raster: &GeoTiffInfo,
    base: Option<&egui::TextureHandle>,
    crop: Option<&Crop>,
    opacity: f32,
    project: impl Fn(f64, f64) -> Pos2,
) {
    let crop = crop.filter(|c| c.path == raster.path);
    let quad = |texture: &egui::TextureHandle, area: [f64; 4], uv: [f32; 4]| {
        if area[0] >= area[2] || area[1] >= area[3] {
            return;
        }
        let mut mesh = egui::Mesh::with_texture(texture.id());
        for (x, y, u, v) in [
            (area[0], area[1], uv[0], uv[1]),
            (area[2], area[1], uv[2], uv[1]),
            (area[2], area[3], uv[2], uv[3]),
            (area[0], area[3], uv[0], uv[3]),
        ] {
            let (x, y) = raster
                .transform
                .pixel_to_world(x * raster.width as f64, y * raster.height as f64);
            mesh.vertices.push(egui::epaint::Vertex {
                pos: project(x, y),
                uv: Pos2::new(u, v),
                color: Color32::from_white_alpha((opacity.clamp(0., 1.) * 255.).floor() as u8),
            });
        }
        mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
        painter.add(egui::Shape::mesh(mesh));
    };
    if let Some(base) = base {
        if let Some(crop) = crop {
            let [l, t, r, b] = crop.uv;
            for area in [
                [0., 0., 1., t],
                [0., b, 1., 1.],
                [0., t, l, b],
                [r, t, 1., b],
            ] {
                quad(base, area, area.map(|v| v as f32));
            }
        } else {
            quad(base, [0., 0., 1., 1.], [0., 0., 1., 1.]);
        }
    }
    if let Some(crop) = crop {
        quad(&crop.texture, crop.uv, [0., 0., 1., 1.]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn affine_detail_quad_and_fallback_partition_pixels_without_double_opacity() {
        let ctx = egui::Context::default();
        let raster = GeoTiffInfo {
            path: "rotated.tif".into(),
            width: 100,
            height: 60,
            transform: sleufbase_rust::geo::GeoTransform {
                a: 1.2,
                b: 0.4,
                c: 100.,
                d: 0.3,
                e: -1.1,
                f: 200.,
            },
            bounds: Bounds {
                min_x: 100.,
                min_y: 134.,
                max_x: 244.,
                max_y: 230.,
            },
            epsg: Some(28992),
            kickthemap_job_id: None,
            kickthemap_job_title: String::new(),
            kickthemap_job_sidecar: None,
            metadata: serde_json::Value::Null,
        };
        let base = ctx.load_texture(
            "base",
            egui::ColorImage::new([2, 2], Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let crop = Crop {
            path: raster.path.clone(),
            uv: [0.2, 0.3, 0.8, 0.7],
            texture: ctx.load_texture(
                "crop",
                egui::ColorImage::new([2, 2], Color32::WHITE),
                egui::TextureOptions::LINEAR,
            ),
        };
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                paint(
                    ui.painter(),
                    &raster,
                    Some(&base),
                    Some(&crop),
                    0.5,
                    |x, y| Pos2::new(x as f32, y as f32),
                )
            });
        });
        let meshes = output
            .shapes
            .iter()
            .filter_map(|s| {
                if let egui::Shape::Mesh(m) = &s.shape {
                    Some(m)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(meshes.len(), 5);
        let mut area = 0.;
        let mut detail = 0;
        for mesh in meshes {
            let raw = mesh
                .vertices
                .iter()
                .map(|v| {
                    assert_eq!(v.color.a(), 127);
                    let (x, y) = raster
                        .transform
                        .world_to_pixel(v.pos.x as f64, v.pos.y as f64)
                        .unwrap();
                    [x / 100., y / 60.]
                })
                .collect::<Vec<_>>();
            assert!(raw.iter().flatten().all(|v| (-1e-5..=1.00001).contains(v)));
            let size = (raw[1][0] - raw[0][0]) * (raw[3][1] - raw[0][1]);
            area += size;
            if mesh.texture_id == crop.texture.id() {
                detail += 1;
                assert!((size - 0.24).abs() < 1e-6);
                assert_eq!(mesh.vertices[0].uv, Pos2::ZERO);
                assert_eq!(mesh.vertices[2].uv, Pos2::new(1., 1.));
            }
        }
        assert_eq!(detail, 1);
        assert!((area - 1.).abs() < 1e-6);
    }
    fn pan_frame(app: &mut SleufBaseApp, ctx: &egui::Context, left: f64) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(300., 300.))),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (response, painter) = ui.allocate_painter(egui::vec2(100., 100.), egui::Sense::drag());
                let rect = response.rect;
                let view = raster_point_ui::View { response, painter, rect, center: rect.center(),
                    origin: [left + 1., 1.], scale: 50., clicked: None };
                app.request_viewport_previews(&view);
            });
        });
    }
    #[test]
    fn real_canvas_pan_edge_releases_detail_preserves_base_and_reentry_decodes() {
        let mut app = SleufBaseApp::default();
        let ctx = egui::Context::default();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rasters/orientation1.tiff");
        app.geotiffs.push(GeoTiffInfo {
            path: path.clone(), width: 3, height: 2,
            transform: sleufbase_rust::geo::GeoTransform { a: 1., b: 0., c: 0., d: 0., e: -1., f: 2. },
            bounds: Bounds { min_x: 0., min_y: 0., max_x: 3., max_y: 2. }, epsg: Some(28992),
            kickthemap_job_id: None, kickthemap_job_title: String::new(),
            kickthemap_job_sidecar: None, metadata: serde_json::Value::Null,
        });
        let base = ctx.load_texture("base-retained", egui::ColorImage::new([1,1], Color32::WHITE), egui::TextureOptions::LINEAR);
        let base_id = base.id();
        app.raster_textures.push(Some(base));
        let wait = |app: &mut SleufBaseApp| {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while app.viewport_loader.as_ref().unwrap().remaining() > 0 {
                app.poll_viewport_previews(&ctx);
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        pan_frame(&mut app, &ctx, 0.);
        wait(&mut app);
        assert!(app.navigation.raster_crops[0].is_some());
        // Original test_tiff_touching_viewport_edge_releases_its_cache.
        pan_frame(&mut app, &ctx, 3.);
        assert!(app.navigation.raster_crops[0].is_none());
        app.poll_viewport_previews(&ctx);
        assert!(app.navigation.raster_crops[0].is_none());
        assert_eq!(app.raster_textures[0].as_ref().unwrap().id(), base_id);
        pan_frame(&mut app, &ctx, 8.);
        assert!(app.navigation.raster_crops[0].is_none());
        pan_frame(&mut app, &ctx, 0.);
        wait(&mut app);
        assert!(app.navigation.raster_crops[0].is_some());
        assert_eq!(app.raster_textures[0].as_ref().unwrap().id(), base_id);
    }
    #[test]
    fn project_reset_releases_detail_textures_and_cancels_pending_decodes() {
        let mut app = SleufBaseApp::default();
        let ctx = egui::Context::default();
        app.navigation.raster_crops.push(Some(Crop {
            path: "old.tif".into(),
            uv: [0., 0., 1., 1.],
            texture: ctx.load_texture(
                "old",
                egui::ColorImage::new([2, 2], Color32::WHITE),
                egui::TextureOptions::LINEAR,
            ),
        }));
        let mut loader = ViewportLoader::new().unwrap();
        loader.update(&[ViewportRequest {
            id: 0,
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/rasters/large-striped.tiff"),
            corners: [[0., 0.], [1., 0.], [1., 1.], [0., 1.]],
            screen_pixels: [1000, 800],
        }]);
        app.viewport_loader = Some(loader);
        app.clear_project_requests();
        app.poll_viewport_previews(&ctx);
        assert!(app.navigation.raster_crops.is_empty());
        assert_eq!(app.viewport_loader.as_ref().unwrap().remaining(), 0);
    }
}
