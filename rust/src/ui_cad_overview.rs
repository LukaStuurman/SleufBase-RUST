//! Bounded, transparent screen raster for dense CAD overviews.
//! The source drawing and all hit-test geometry remain in the parent renderer.
use eframe::egui::{self, Color32, Rect, Stroke};

const MAX_SIDE: usize = 4096;
const MAX_PIXELS: usize = 8_388_608;

pub(super) struct Overview {
    clip: Rect,
    size: [usize; 2],
    scale: [f64; 2],
    pixels_per_point: f64,
    // Color32 stores premultiplied RGBA in exactly four bytes per pixel.
    pixels: Vec<Color32>,
    color: Color32,
    coverage_colors: [Color32; 256],
}

impl Overview {
    pub(super) fn new(camera: super::Camera, max_texture_side: usize) -> Self {
        let width = f64::from(camera.clip.width()).max(f64::MIN_POSITIVE);
        let height = f64::from(camera.clip.height()).max(f64::MIN_POSITIVE);
        let side = max_texture_side.clamp(1, MAX_SIDE) as f64;
        let requested = f64::from(camera.pixels_per_point);
        let requested = if requested.is_finite() && requested > 0. { requested } else { 1. };
        let scale = requested.min(side / width).min(side / height)
            .min((MAX_PIXELS as f64 / (width * height)).sqrt());
        // Floor both dimensions so rounding cannot exceed the pixel budget.
        let size = [(width * scale).floor().max(1.) as usize,
            (height * scale).floor().max(1.) as usize];
        let actual = [size[0] as f64 / width, size[1] as f64 / height];
        Self {
            clip: camera.clip, size, scale: actual,
            pixels_per_point: actual[0].min(actual[1]),
            pixels: vec![Color32::TRANSPARENT; size[0] * size[1]],
            color: Color32::TRANSPARENT,
            coverage_colors: [Color32::TRANSPARENT; 256],
        }
    }

    pub(super) fn pixels_per_point(&self) -> f64 { self.pixels_per_point }

    fn pixel_point(&self, point: [f64; 2]) -> [f64; 2] {
        [(point[0] - f64::from(self.clip.min.x)) * self.scale[0],
            (point[1] - f64::from(self.clip.min.y)) * self.scale[1]]
    }

    fn set_color(&mut self, color: Color32) {
        if color != self.color {
            self.color = color;
            // Reuse one small coverage palette across consecutive source paths.
            // These are premultiplied values: never multiply alpha again at upload.
            for (alpha, value) in self.coverage_colors.iter_mut().enumerate() {
                *value = color.gamma_multiply_u8(alpha as u8);
            }
        }
    }

    fn blend(&mut self, index: usize, coverage: f64) {
        let alpha = (coverage.clamp(0., 1.) * 255.).round() as usize;
        if alpha == 0 { return; }
        let source = self.coverage_colors[alpha];
        let destination = self.pixels[index];
        if destination == source { return; }
        if destination == Color32::TRANSPARENT || source.a() == 255 {
            self.pixels[index] = source;
        } else if self.color.a() == 255
            && destination == self.coverage_colors[usize::from(destination.a())]
        {
            // Repeated equal-color contours share their maximum coverage instead
            // of darkening an anti-aliased edge every time a segment meets it.
            if source.a() > destination.a() { self.pixels[index] = source; }
        } else {
            // egui's premultiplied source-over operation preserves painter order.
            self.pixels[index] = destination.blend(source);
        }
    }

    pub(super) fn point(&mut self, point: [f64; 2], color: Color32, radius: f32) {
        if point.iter().any(|value| !value.is_finite()) || !radius.is_finite()
            || radius <= 0. || color == Color32::TRANSPARENT { return; }
        self.set_color(color);
        self.disc(self.pixel_point(point), f64::from(radius) * self.pixels_per_point);
    }

    pub(super) fn line(&mut self, a: [f64; 2], b: [f64; 2], stroke: Stroke) {
        if !stroke.width.is_finite() || stroke.width <= 0.
            || stroke.color == Color32::TRANSPARENT { return; }
        let radius = stroke.width * 0.5;
        // Include the stroke and its antialias fringe before clipping. Clipping
        // the centreline at the viewport itself would lose strokes at its edge.
        let margin = radius + (1. / self.pixels_per_point) as f32;
        let Some([a, b]) = super::clipped_line(a, b, self.clip.expand(margin)) else { return; };
        self.set_color(stroke.color);
        let a = self.pixel_point([f64::from(a.x), f64::from(a.y)]);
        let b = self.pixel_point([f64::from(b.x), f64::from(b.y)]);
        self.capsule(a, b, f64::from(radius) * self.pixels_per_point);
    }

    fn disc(&mut self, point: [f64; 2], radius: f64) {
        let outer = radius + 0.5;
        let Some(xs) = span(point[0] - outer, point[0] + outer, self.size[0]) else { return; };
        let Some(ys) = span(point[1] - outer, point[1] + outer, self.size[1]) else { return; };
        for y in ys {
            let dy = y as f64 + 0.5 - point[1];
            for x in xs.clone() {
                let dx = x as f64 + 0.5 - point[0];
                let coverage = coverage(dx * dx + dy * dy, radius);
                self.blend(y * self.size[0] + x, coverage);
            }
        }
    }

    fn capsule(&mut self, mut a: [f64; 2], mut b: [f64; 2], radius: f64) {
        let steep = (b[1] - a[1]).abs() > (b[0] - a[0]).abs();
        let dimensions = if steep {
            a.swap(0, 1); b.swap(0, 1); [self.size[1], self.size[0]]
        } else { self.size };
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length_squared = dx * dx + dy * dy;
        if length_squared < 1e-12 {
            if steep { a.swap(0, 1); }
            self.disc(a, radius);
            return;
        }
        let outer = radius + 0.5;
        let Some(major) = span(a[0].min(b[0]) - outer, a[0].max(b[0]) + outer, dimensions[0]) else { return; };
        let slope = dy / dx; // The major axis guarantees abs(slope) <= 1.
        let spread = outer * (1. + slope * slope).sqrt();
        for x in major {
            let px = x as f64 + 0.5;
            let centre = a[1] + (px - a[0]) * slope;
            let Some(minor) = span(centre - spread, centre + spread, dimensions[1]) else { continue; };
            for y in minor {
                let py = y as f64 + 0.5;
                let t = (((px - a[0]) * dx + (py - a[1]) * dy) / length_squared).clamp(0., 1.);
                let distance_x = px - (a[0] + t * dx);
                let distance_y = py - (a[1] + t * dy);
                let coverage = coverage(distance_x * distance_x + distance_y * distance_y, radius);
                let index = if steep { x * self.size[0] + y } else { y * self.size[0] + x };
                self.blend(index, coverage);
            }
        }
    }

    pub(super) fn finish(self, ctx: &egui::Context) -> egui::TextureHandle {
        // Move the four-byte pixels directly into egui; no full-size conversion
        // buffer or retained CPU raster accompanies the cached texture handle.
        ctx.load_texture("cad-work-map-overview", egui::ColorImage {
            size: self.size, pixels: self.pixels,
        }, egui::TextureOptions::LINEAR)
    }
}

/// Pixel centres within an interval, clamped before any integer arithmetic.
fn span(min: f64, max: f64, length: usize) -> Option<std::ops::RangeInclusive<usize>> {
    if !min.is_finite() || !max.is_finite() || max < 0.5 || min > length as f64 - 0.5 {
        return None;
    }
    let start = (min - 0.5).ceil().max(0.) as usize;
    let end = (max - 0.5).floor().min(length as f64 - 1.) as usize;
    (start <= end).then_some(start..=end)
}

fn coverage(distance_squared: f64, radius: f64) -> f64 {
    let outer = radius + 0.5;
    if distance_squared >= outer * outer { 0. }
    else if radius >= 0.5 && distance_squared <= (radius - 0.5) * (radius - 0.5) { 1. }
    else { (outer - distance_squared.sqrt()).clamp(0., 1.) }
}
