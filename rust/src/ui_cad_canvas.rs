//! Work-map CAD drawing: spatial culling and reusable screen-space mesh batches.
//! Source records, inspection indices and export geometry remain unchanged.
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use sleufbase_rust::{Bounds, Vertex, cad_scene::Scene, dxf_read::DxfOverlay};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::{Arc, Weak},
};

const CACHE_ID: &str = "cad-work-map-meshes";
const LARGE_FEATURE_COUNT: usize = 1024;
const BATCH_VERTICES: usize = 32_768;
// This limits retained copies, never the amount of geometry that is drawn.
// If a whole drawing exceeds it, batches continue directly to the painter.
const MAX_CACHED_BYTES: usize = 128 * 1024 * 1024;

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
    meshes: Vec<Arc<egui::Mesh>>,
}

impl CachedDrawing {
    fn matches(&self, camera: Camera, overlays: &[DxfOverlay]) -> bool {
        self.camera == camera
            && self.overlays.len() == overlays.iter().filter(|overlay| overlay.visible).count()
            && self.overlays.iter().zip(overlays.iter().filter(|overlay| overlay.visible))
                .all(|(stamp, overlay)| stamp.matches(overlay))
    }
}

fn is_large(overlay: &DxfOverlay) -> bool {
    overlay.features.len() + overlay.expanded.features.len() >= LARGE_FEATURE_COUNT
        || overlay.features.iter().any(|feature| feature.points.len() >= 4096)
        || overlay.expanded.features.iter().any(|feature| feature.points.len() >= 4096)
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
        painter.extend(cached.meshes.iter().cloned().map(egui::Shape::mesh));
        return;
    }
    // Drop obsolete meshes before rebuilding, including when a project or a
    // layer is replaced. Weak stamps never keep an old CAD source alive.
    drop(previous);
    painter.ctx().data_mut(|data| data.remove::<Arc<CachedDrawing>>(cache_id));

    let mut batches = Batches::new(painter, camera);
    for overlay in overlays.iter().filter(|overlay| overlay.visible) {
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
                    batches.point(camera.point(*point), Color32::from_rgb(r, g, b));
                }
            }
            // Preserve disconnected contours and individual MTEXT paint runs.
            for (path, [r, g, b]) in feature.painted_paths() {
                let stroke = Stroke::new(1.6_f32, Color32::from_rgb(r, g, b));
                for pair in path.windows(2) {
                    batches.line(camera.point(pair[0]), camera.point(pair[1]), stroke);
                }
            }
        }
    }
    if let Some(meshes) = batches.finish() {
        let cached = Arc::new(CachedDrawing {
            camera,
            overlays: overlays.iter().filter(|overlay| overlay.visible)
                .map(OverlayStamp::capture).collect(),
            meshes,
        });
        painter.ctx().data_mut(|data| data.insert_temp(cache_id, cached));
    }
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

struct Batches<'a> {
    painter: &'a egui::Painter,
    tessellator: egui::epaint::Tessellator,
    clip: Rect,
    current: egui::Mesh,
    cached: Option<Vec<Arc<egui::Mesh>>>,
    bytes: usize,
}

impl<'a> Batches<'a> {
    fn new(painter: &'a egui::Painter, camera: Camera) -> Self {
        // This renderer has no glyph shapes: CAD text already consists of
        // prepared contours. The atlas dimensions are unused; empty discs are
        // explicitly supported by egui and keep these cached meshes atlas-free.
        let mut tessellator = egui::epaint::Tessellator::new(
            camera.pixels_per_point, camera.tessellation, [1, 1], Vec::new(),
        );
        tessellator.set_clip_rect(camera.clip);
        Self {
            painter, tessellator, clip: camera.clip.expand(4.),
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
        self.painter.add(egui::Shape::mesh(mesh));
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
