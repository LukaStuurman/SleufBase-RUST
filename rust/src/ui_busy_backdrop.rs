//! Non-interactive last view behind compact progress windows.
//! Workers can keep exclusive ownership of project inputs while this is shown.
use super::*;

#[derive(Default)]
pub(super) struct State {
    shapes: Vec<egui::epaint::ClippedShape>,
    _textures: Vec<egui::TextureHandle>,
    screen: Option<egui::Rect>,
    captured: Option<Instant>,
    pixels_per_point: f32,
}

impl SleufBaseApp {
    pub(super) fn remember_busy_backdrop(&mut self, ctx: &egui::Context) {
        if self.project_sources_owned() || self.export_intent.is_some()
            || self.template_job.is_some() || self.file_intent.is_some()
            || self.browser_ui.web_visible
        {
            return;
        }
        // A short throttle avoids copying map primitives on every pan frame.
        // The capture is bounded and never includes interactive windows/popups.
        if self.busy_backdrop.captured.is_some_and(|time| time.elapsed() < Duration::from_millis(250)) {
            return;
        }
        let screen = ctx.screen_rect();
        let shapes = ctx.graphics(|graphics| {
            graphics.get(egui::LayerId::background()).map(|list| {
                list.all_entries().filter(|entry| {
                    let clip = entry.clip_rect.intersect(screen);
                    clip.is_positive() && entry.shape.visual_bounding_rect().intersects(clip)
                }).take(50_000).cloned().collect::<Vec<_>>()
            }).unwrap_or_default()
        });
        if shapes.is_empty() { return; }
        // Keep texture handles alive even if a loader replaces a preview while
        // this frozen view is displayed. This does not copy the image pixels.
        let textures = self.raster_textures.iter().filter_map(Clone::clone)
            .chain(self.pdok_texture.iter().cloned())
            .chain(self.osm_texture.iter().cloned())
            .chain(self.navigation.raster_crops.iter().filter_map(|crop| crop.as_ref().map(|crop| crop.texture.clone())))
            .collect();
        self.busy_backdrop = State {
            shapes, _textures: textures, screen: Some(screen), captured: Some(Instant::now()),
            pixels_per_point: ctx.pixels_per_point(),
        };
    }

    pub(super) fn paint_busy_backdrop(&self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::background());
        painter.rect_filled(screen, 0., theme::APP_BG);
        // A different monitor scale may rebuild the shared font atlas. Reuse
        // the snapshot only while its glyph coordinates still match that atlas.
        if let Some(old) = self.busy_backdrop.screen.filter(|rect| rect.is_positive()
            && (self.busy_backdrop.pixels_per_point - ctx.pixels_per_point()).abs() < 0.001) {
            let scaling = (screen.width() / old.width()).min(screen.height() / old.height());
            let transform = egui::emath::TSTransform::new(screen.center().to_vec2() - old.center().to_vec2() * scaling, scaling);
            for entry in &self.busy_backdrop.shapes {
                let clip = transform.mul_rect(entry.clip_rect).intersect(screen);
                let mut shape = entry.shape.clone();
                shape.transform(transform);
                painter.with_clip_rect(clip).add(shape);
            }
        }
        // This is paint only: the background has no live buttons or map tools.
        painter.rect_filled(screen, 0., Color32::from_white_alpha(115));
    }
}
