//! Work-map CAD drawing: spatial culling and reusable screen-space mesh batches.
//! Source records, inspection indices and export geometry remain unchanged.
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use sleufbase_rust::{Bounds, Vertex, cad_scene::Scene, dxf_read::DxfOverlay};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

#[path = "ui_cad_overview.rs"]
mod overview;

const CACHE_ID: &str = "cad-work-map-meshes";
const LARGE_FEATURE_COUNT: usize = 1024;
const BATCH_VERTICES: usize = 32_768;
// A dense view switches to a screen-sized image instead of repeatedly building
// an uncacheable mesh. No source features or export geometry are truncated.
const MAX_CACHED_BYTES: usize = 128 * 1024 * 1024;
const PREVIEW_DELAY: Duration = Duration::from_millis(70);

#[derive(Clone, Copy, PartialEq)]
struct Camera {
    clip: Rect,
    center: Pos2,
    origin: [f64; 2],
    scale: f64,
    pixels_per_point: f32,
    tessellation: egui::epaint::TessellationOptions,
}

impl Camera {
    fn point(self, point: Vertex) -> [f64; 2] {
        [
            f64::from(self.center.x) + (point.x - self.origin[0]) * self.scale,
            f64::from(self.center.y) - (point.y - self.origin[1]) * self.scale,
        ]
    }

    fn world_bounds(self) -> Bounds {
        let clip = self.clip.expand(4.);
        Bounds {
            min_x: self.origin[0] + f64::from(clip.min.x - self.center.x) / self.scale,
            max_x: self.origin[0] + f64::from(clip.max.x - self.center.x) / self.scale,
            min_y: self.origin[1] - f64::from(clip.max.y - self.center.y) / self.scale,
            max_y: self.origin[1] - f64::from(clip.min.y - self.center.y) / self.scale,
        }
    }
}

struct OverlayStamp {
    scene: Weak<Scene>,
    visibility: Weak<Vec<bool>>,
    source_address: usize,
    source_count: usize,
    layers: u64,
}

fn layer_signature(overlay: &DxfOverlay) -> u64 {
    let mut hash = DefaultHasher::new();
    for (name, layer) in &overlay.layers {
        name.hash(&mut hash);
        layer.visible.hash(&mut hash);
        layer.color_aci.hash(&mut hash);
        layer.true_color.hash(&mut hash);
    }
    hash.finish()
}

impl OverlayStamp {
    fn capture(overlay: &DxfOverlay) -> Self {
        Self {
            scene: Arc::downgrade(&overlay.expanded),
            visibility: Arc::downgrade(&overlay.block_visibility),
            source_address: overlay.features.as_ptr() as usize,
            source_count: overlay.features.len(),
            layers: layer_signature(overlay),
        }
    }

    fn matches(&self, overlay: &DxfOverlay) -> bool {
        self.source_address == overlay.features.as_ptr() as usize
            && self.source_count == overlay.features.len()
            && self.scene.upgrade().is_some_and(|scene| Arc::ptr_eq(&scene, &overlay.expanded))
            && self.visibility.upgrade().is_some_and(|visible| Arc::ptr_eq(&visible, &overlay.block_visibility))
            && self.layers == layer_signature(overlay)
    }
}

struct CachedDrawing {
    camera: Camera,
    overlays: Vec<OverlayStamp>,
    drawing: Drawing,
}

enum Drawing {
    Meshes(Vec<Arc<egui::Mesh>>),
    Overview(egui::TextureHandle),
}

#[derive(Clone, Copy)]
struct PendingPreview { camera: Camera, since: Instant }

impl CachedDrawing {
    fn matches(&self, camera: Camera, overlays: &[DxfOverlay]) -> bool {
        self.camera == camera && self.same_sources(overlays)
    }

    fn same_sources(&self, overlays: &[DxfOverlay]) -> bool {
        self.overlays.len() == overlays.iter().filter(|overlay| overlay.visible).count()
            && self.overlays.iter().zip(overlays.iter().filter(|overlay| overlay.visible))
                .all(|(stamp, overlay)| stamp.matches(overlay))
    }

    fn paint(&self, painter: &egui::Painter) {
        match &self.drawing {
            Drawing::Meshes(meshes) => painter.extend(meshes.iter().cloned().map(egui::Shape::mesh)),
            Drawing::Overview(texture) => {
                painter.with_clip_rect(self.camera.clip).image(texture.id(), self.camera.clip,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)), Color32::WHITE);
            }
        }
    }

    fn preview(&self, painter: &egui::Painter, camera: Camera) {
        let Drawing::Overview(texture) = &self.drawing else { return; };
        let ratio = camera.scale / self.camera.scale;
        let transform = |point: Pos2| Pos2::new(
            (f64::from(camera.center.x) + f64::from(point.x - self.camera.center.x) * ratio
                + (self.camera.origin[0] - camera.origin[0]) * camera.scale) as f32,
            (f64::from(camera.center.y) + f64::from(point.y - self.camera.center.y) * ratio
                - (self.camera.origin[1] - camera.origin[1]) * camera.scale) as f32,
        );
        let rect = Rect::from_min_max(transform(self.camera.clip.min), transform(self.camera.clip.max));
        if rect.is_finite() && rect.is_positive() {
            painter.with_clip_rect(camera.clip).image(texture.id(), rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)), Color32::WHITE);
        }
    }
}

fn is_large(overlay: &DxfOverlay) -> bool {
    if overlay.features.len() + overlay.expanded.features.len() >= LARGE_FEATURE_COUNT {
        return true;
    }
    // A few hundred medium-sized paths can also exceed the cost of a single
    // long path. Count their combined work before taking the uncached route.
    let mut points = 0usize;
    overlay.features.iter().map(|feature| feature.points.len())
        .chain(overlay.expanded.features.iter().map(|feature| feature.points.len()))
        .any(|count| {
            points = points.saturating_add(count);
            points >= 4096
        })
}

pub(super) fn paint(
    painter: &egui::Painter,
    overlays: &[DxfOverlay],
    rect: Rect,
    center: Pos2,
    origin: [f64; 2],
    scale: f64,
) {
    let cache_id = egui::Id::new(CACHE_ID);
    let clip = rect.intersect(painter.clip_rect());
    if !clip.is_positive() || !scale.is_finite() || scale <= 0. {
        painter.ctx().data_mut(|data| data.remove::<Arc<CachedDrawing>>(cache_id));
        return;
    }
    let camera = Camera {
        clip, center, origin, scale,
        pixels_per_point: painter.ctx().pixels_per_point(),
        tessellation: painter.ctx().tessellation_options(|options| *options),
    };
    let bounds = camera.world_bounds();
    if !overlays.iter().any(|overlay| overlay.visible && is_large(overlay)) {
        painter.ctx().data_mut(|data| data.remove::<Arc<CachedDrawing>>(cache_id));
        for overlay in overlays.iter().filter(|overlay| overlay.visible) {
            paint_small(painter, overlay, camera, bounds);
        }
        return;
    }

    let previous = painter.ctx().data(|data| data.get_temp::<Arc<CachedDrawing>>(cache_id));
    if let Some(cached) = previous.as_ref().filter(|cached| cached.matches(camera, overlays)) {
        cached.paint(painter);
        return;
    }
    // A wheel gesture can contain dozens of camera updates. Move the existing
    // overview texture immediately and refine once the camera has settled;
    // never label that temporary image as the final cache for the new camera.
    let pending_id = cache_id.with("camera-preview");
    if let Some(cached) = previous.as_ref().filter(|cached|
        matches!(&cached.drawing, Drawing::Overview(_)) && cached.same_sources(overlays)
            && cached.camera.pixels_per_point == camera.pixels_per_point
            && cached.camera.tessellation == camera.tessellation
    ) {
        let now = Instant::now();
        let pending = painter.ctx().data_mut(|data| {
            let pending = data.get_temp::<PendingPreview>(pending_id)
                .filter(|pending| pending.camera == camera)
                .unwrap_or(PendingPreview { camera, since: now });
            data.insert_temp(pending_id, pending);
            pending
        });
        let elapsed = now.saturating_duration_since(pending.since);
        if elapsed < PREVIEW_DELAY {
            cached.preview(painter, camera);
            painter.ctx().request_repaint_after(PREVIEW_DELAY - elapsed);
            return;
        }
    }
    painter.ctx().data_mut(|data| data.remove::<PendingPreview>(pending_id));
    // Drop obsolete meshes before rebuilding, including when a project or a
    // layer is replaced. Weak stamps never keep an old CAD source alive.
    drop(previous);
    painter.ctx().data_mut(|data| data.remove::<Arc<CachedDrawing>>(cache_id));

    let candidates = overlays.iter().filter(|overlay| overlay.visible)
        .map(|overlay| (overlay, overlay.feature_indices_in(bounds))).collect::<Vec<_>>();
    let physical_pixels = f64::from(clip.width()) * f64::from(clip.height())
        * f64::from(camera.pixels_per_point).powi(2);
    let point_threshold = (physical_pixels / 5.).clamp(40_000., 200_000.) as usize;
    let feature_threshold = (physical_pixels / 120.).max(4_000.) as usize;
    let mut point_count = 0usize;
    let mut feature_count = 0usize;
    for (overlay, indices) in &candidates {
        for &index in indices {
            if index < overlay.features.len() && overlay.features[index].entity_type == "INSERT" { continue; }
            feature_count = feature_count.saturating_add(1);
            point_count = point_count.saturating_add(if index < overlay.features.len() {
                overlay.features[index].points.len()
            } else {
                overlay.expanded.features[index - overlay.features.len()].points.len()
            });
        }
    }
    let drawing = if point_count > point_threshold || feature_count > feature_threshold {
        build_overview(painter, camera, &candidates)
    } else if let Some(meshes) = build_vectors(camera, &candidates) {
        Drawing::Meshes(meshes)
    } else {
        // Even an unusually expensive symbol must get a persistent result.
        // The old path discarded this cache and repeated all work every frame.
        build_overview(painter, camera, &candidates)
    };
    let cached = Arc::new(CachedDrawing {
        camera,
        overlays: overlays.iter().filter(|overlay| overlay.visible)
            .map(OverlayStamp::capture).collect(),
        drawing,
    });
    cached.paint(painter);
    painter.ctx().data_mut(|data| data.insert_temp(cache_id, cached));
}

fn build_vectors(camera: Camera, candidates: &[(&DxfOverlay, Vec<usize>)]) -> Option<Vec<Arc<egui::Mesh>>> {
    let mut batches = Batches::new(camera);
    for (overlay, indices) in candidates {
        for &index in indices {
            if index < overlay.features.len() && overlay.features[index].entity_type == "INSERT" {
                continue;
            }
            let Some(feature) = overlay.inspect_feature(index).filter(|feature| feature.visible) else {
                continue;
            };
            if matches!(feature.source.entity_type.as_str(), "POINT" | "INSERT") {
                if let Some(point) = feature.points.first() {
                    let [r, g, b] = feature.rgb;
                    batches.point(camera.point(*point), Color32::from_rgb(r, g, b));
                    if batches.cached.is_none() { return None; }
                }
            }
            // Preserve disconnected contours and individual MTEXT paint runs.
            for (path, [r, g, b]) in feature.painted_paths() {
                let stroke = Stroke::new(1.6_f32, Color32::from_rgb(r, g, b));
                for pair in path.windows(2) {
                    batches.line(camera.point(pair[0]), camera.point(pair[1]), stroke);
                    if batches.cached.is_none() { return None; }
                }
            }
        }
    }
    batches.finish()
}

fn build_overview(painter: &egui::Painter, camera: Camera, candidates: &[(&DxfOverlay, Vec<usize>)]) -> Drawing {
    let mut image = overview::Overview::new(camera, painter.ctx().input(|input| input.max_texture_side));
    let screen_scale = image.pixels_per_point();
    let world_pixel = 1. / (camera.scale * screen_scale);
    let tolerance = 0.4 * world_pixel;
    for (overlay, indices) in candidates {
        for &index in indices {
            if index < overlay.features.len() && overlay.features[index].entity_type == "INSERT" { continue; }
            let Some(feature) = overlay.inspect_feature(index).filter(|feature| feature.visible) else { continue; };
            let [r, g, b] = feature.rgb;
            let color = Color32::from_rgb(r, g, b);
            let is_text = matches!(feature.source.entity_type.as_str(), "TEXT" | "MTEXT" | "ATTRIB" | "ATTDEF");
            let is_marker = matches!(feature.source.entity_type.as_str(), "POINT" | "INSERT");
            if let Some(bounds) = overlay.feature_bounds(index) {
                let diameter = (bounds.max_x - bounds.min_x).max(bounds.max_y - bounds.min_y);
                if is_text && diameter < 3. * world_pixel { continue; }
                if !is_marker && diameter == 0. { continue; }
                if !is_text && diameter <= 0.7 * world_pixel
                    && (is_marker || feature.paths().any(|path| path.len() >= 2))
                {
                    // Subpixel geometry has no resolvable interior shape. Keep
                    // its presence/color as a dot, merging equal pixel coverage.
                    let point = Vertex {
                        x: bounds.min_x * 0.5 + bounds.max_x * 0.5,
                        y: bounds.min_y * 0.5 + bounds.max_y * 0.5, z: None,
                    };
                    let radius = if is_marker { 2.5 } else { 0.8 };
                    image.point(camera.point(point), color, radius);
                    continue;
                }
            }
            if matches!(feature.source.entity_type.as_str(), "POINT" | "INSERT") {
                if let Some(point) = feature.points.first() { image.point(camera.point(*point), color, 2.5); }
            }
            let lod = overlay.feature_lod(index);
            let mut start = 0;
            for (path, [r, g, b]) in feature.painted_paths() {
                let stroke = Stroke::new(1.6_f32, Color32::from_rgb(r, g, b));
                let mut draw = |a, b| image.line(camera.point(a), camera.point(b), stroke);
                if let Some(lod) = lod {
                    lod.for_each_segment(path, start, tolerance, &mut draw);
                } else {
                    for pair in path.windows(2) { draw(pair[0], pair[1]); }
                }
                start += path.len();
            }
        }
    }
    Drawing::Overview(image.finish(painter.ctx()))
}

fn paint_small(painter: &egui::Painter, overlay: &DxfOverlay, camera: Camera, bounds: Bounds) {
    let project = |point: Vertex| {
        let [x, y] = camera.point(point);
        Pos2::new(x as f32, y as f32)
    };
    for index in overlay.feature_indices_in(bounds) {
        if index < overlay.features.len() && overlay.features[index].entity_type == "INSERT" {
            continue;
        }
        let Some(feature) = overlay.inspect_feature(index).filter(|feature| feature.visible) else {
            continue;
        };
        if matches!(feature.source.entity_type.as_str(), "POINT" | "INSERT") {
            if let Some(point) = feature.points.first() {
                let [r, g, b] = feature.rgb;
                painter.circle_filled(project(*point), 2.5, Color32::from_rgb(r, g, b));
            }
        }
        for (path, [r, g, b]) in feature.painted_paths() {
            let stroke = Stroke::new(1.6_f32, Color32::from_rgb(r, g, b));
            for pair in path.windows(2) {
                painter.line_segment([project(pair[0]), project(pair[1])], stroke);
            }
        }
    }
}

struct Batches {
    tessellator: egui::epaint::Tessellator,
    clip: Rect,
    current: egui::Mesh,
    cached: Option<Vec<Arc<egui::Mesh>>>,
    bytes: usize,
}

impl Batches {
    fn new(camera: Camera) -> Self {
        // This renderer has no glyph shapes: CAD text already consists of
        // prepared contours. The atlas dimensions are unused; empty discs are
        // explicitly supported by egui and keep these cached meshes atlas-free.
        let mut tessellator = egui::epaint::Tessellator::new(
            camera.pixels_per_point, camera.tessellation, [1, 1], Vec::new(),
        );
        tessellator.set_clip_rect(camera.clip);
        Self {
            tessellator, clip: camera.clip.expand(4.),
            current: egui::Mesh::default(), cached: Some(Vec::new()), bytes: 0,
        }
    }

    fn point(&mut self, point: [f64; 2], color: Color32) {
        if point.iter().any(|value| !value.is_finite()) {
            return;
        }
        let point = Pos2::new(point[0] as f32, point[1] as f32);
        if self.clip.contains(point) {
            self.tessellator.tessellate_circle(
                egui::epaint::CircleShape::filled(point, 2.5, color), &mut self.current,
            );
            self.flush_if_full();
        }
    }

    fn line(&mut self, a: [f64; 2], b: [f64; 2], stroke: Stroke) {
        if let Some(line) = clipped_line(a, b, self.clip) {
            self.tessellator.tessellate_line_segment(line, stroke, &mut self.current);
            self.flush_if_full();
        }
    }

    fn flush_if_full(&mut self) {
        if self.current.vertices.len() >= BATCH_VERTICES {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if self.current.is_empty() {
            return;
        }
        let mesh = Arc::new(std::mem::take(&mut self.current));
        self.bytes = self.bytes.saturating_add(
            mesh.vertices.capacity() * std::mem::size_of::<egui::epaint::Vertex>()
                + mesh.indices.capacity() * std::mem::size_of::<u32>(),
        );
        if self.bytes > MAX_CACHED_BYTES {
            self.cached = None;
        }
        if let Some(cached) = &mut self.cached {
            cached.push(mesh.clone());
        }
    }

    fn finish(mut self) -> Option<Vec<Arc<egui::Mesh>>> {
        self.flush();
        self.cached
    }
}

// Clip in f64 before conversion to screen f32, including long lines whose
// endpoints both lie off screen. Do not merely test endpoint containment.
fn clipped_line(a: [f64; 2], b: [f64; 2], clip: Rect) -> Option<[Pos2; 2]> {
    if a.iter().chain(b.iter()).any(|value| !value.is_finite()) {
        return None;
    }
    let d = [b[0] - a[0], b[1] - a[1]];
    if d.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-d[0], a[0] - f64::from(clip.min.x)),
        (d[0], f64::from(clip.max.x) - a[0]),
        (-d[1], a[1] - f64::from(clip.min.y)),
        (d[1], f64::from(clip.max.y) - a[1]),
    ] {
        if p == 0. {
            if q < 0. { return None; }
        } else {
            let t = q / p;
            if p < 0. { lo = lo.max(t); } else { hi = hi.min(t); }
            if lo > hi { return None; }
        }
    }
    Some([
        Pos2::new((a[0] + lo * d[0]) as f32, (a[1] + lo * d[1]) as f32),
        Pos2::new((a[0] + hi * d[0]) as f32, (a[1] + hi * d[1]) as f32),
    ])
}
