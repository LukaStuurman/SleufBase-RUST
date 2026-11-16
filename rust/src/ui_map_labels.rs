//! Screen-space names for the working map only. Export renderers do not use this module.
use super::{Color32, GeoTiffInfo, Pos2, Stroke, VirtualTrench};
use eframe::egui::{self, Rect, Vec2};
use std::sync::Arc;

const IMAGE_CLEARANCE: f32 = 4.;
const LABEL_CLEARANCE: f32 = 3.;
const LABEL_GAP: f32 = 12.;
const MAX_TEXT_WIDTH: f32 = 200.;
const INK: Color32 = Color32::from_rgb(31, 41, 55);

struct Footprint {
    bounds: Rect,
    outline: Vec<Pos2>,
    name: String,
    accent: Color32,
}

impl Footprint {
    fn new(outline: Vec<Pos2>, name: String, accent: Color32) -> Option<Self> {
        if outline.len() < 3 || outline.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return None;
        }
        let mut bounds = Rect::from_min_max(outline[0], outline[0]);
        for point in &outline[1..] {
            bounds.min.x = bounds.min.x.min(point.x);
            bounds.min.y = bounds.min.y.min(point.y);
            bounds.max.x = bounds.max.x.max(point.x);
            bounds.max.y = bounds.max.y.max(point.y);
        }
        Some(Self { bounds, outline, name, accent })
    }
}

fn clean_name(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return fallback.to_owned();
    }
    // Bound layout work for imported job titles, without slicing through UTF-8.
    let mut chars = value.chars();
    let mut name: String = chars.by_ref().take(256)
        .map(|c| if c.is_control() { ' ' } else { c }).collect();
    if chars.next().is_some() {
        name.push('…');
    }
    name
}

fn raster_name(raster: &GeoTiffInfo) -> String {
    if !raster.kickthemap_job_title.trim().is_empty() {
        clean_name(&raster.kickthemap_job_title, "Proefsleuf")
    } else {
        clean_name(&raster.path.file_stem().unwrap_or_default().to_string_lossy(), "Proefsleuf")
    }
}

struct Blocker {
    bounds: Rect,
    owner: usize,
}

/// A small screen grid avoids scanning every trench for every label candidate.
/// Geometry is clipped before indexing, so offscreen and very large images do
/// not allocate an unbounded grid. Bounds are deliberately conservative: names
/// never cover a rotated image or an irregular measured boundary.
struct CollisionIndex {
    viewport: Rect,
    columns: usize,
    rows: usize,
    cells: Vec<Vec<usize>>,
    blockers: Vec<Blocker>,
}

impl CollisionIndex {
    fn new(viewport: Rect) -> Self {
        let columns = (viewport.width() / 96.).ceil().clamp(1., 32.) as usize;
        let rows = (viewport.height() / 96.).ceil().clamp(1., 32.) as usize;
        Self { viewport, columns, rows, cells: vec![Vec::new(); columns * rows], blockers: Vec::new() }
    }

    fn cell_range(&self, bounds: Rect) -> [usize; 4] {
        let col = |x: f32| (((x - self.viewport.left()) / self.viewport.width()
            * self.columns as f32).floor().max(0.) as usize).min(self.columns - 1);
        let row = |y: f32| (((y - self.viewport.top()) / self.viewport.height()
            * self.rows as f32).floor().max(0.) as usize).min(self.rows - 1);
        [col(bounds.left()), row(bounds.top()), col(bounds.right()), row(bounds.bottom())]
    }

    fn insert(&mut self, bounds: Rect, owner: usize) {
        let bounds = bounds.intersect(self.viewport);
        if !bounds.is_positive() { return; }
        let index = self.blockers.len();
        self.blockers.push(Blocker { bounds, owner });
        let [left, top, right, bottom] = self.cell_range(bounds);
        for row in top..=bottom {
            for col in left..=right {
                self.cells[row * self.columns + col].push(index);
            }
        }
    }

    fn any(&self, bounds: Rect, mut predicate: impl FnMut(&Blocker) -> bool) -> bool {
        let [left, top, right, bottom] = self.cell_range(bounds);
        for row in top..=bottom {
            for col in left..=right {
                for index in &self.cells[row * self.columns + col] {
                    let blocker = &self.blockers[*index];
                    if blocker.bounds.intersects(bounds) && predicate(blocker) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn occupied(&self, bounds: Rect) -> bool {
        self.any(bounds, |_| true)
    }

    fn blocks_leader(&self, segment: [Pos2; 2], owner: usize) -> bool {
        let bounds = Rect::from_two_pos(segment[0], segment[1]).expand(1.);
        self.any(bounds, |blocker| blocker.owner != owner && segment_intersects_rect(segment, blocker.bounds))
    }
}

fn label_position(footprint: Rect, size: Vec2, index: &CollisionIndex) -> Option<Rect> {
    // Include all shadow pixels in the viewport and collision checks.
    let safe = index.viewport.shrink(8. + LABEL_CLEARANCE);
    if size.x > safe.width() || size.y > safe.height() { return None; }
    let visible = footprint.intersect(index.viewport);
    if !visible.is_positive() { return None; }
    let center = visible.center();
    let half = size * 0.5;
    let top = footprint.top() - LABEL_GAP - half.y;
    let bottom = footprint.bottom() + LABEL_GAP + half.y;
    let left = footprint.left() - LABEL_GAP - half.x;
    let right = footprint.right() + LABEL_GAP + half.x;
    // Stable side order keeps names near their layer while panning/zooming.
    // Lateral alternatives let neighboring labels share the available margin.
    let mut candidates = vec![
        Pos2::new(center.x, top), Pos2::new(center.x, bottom),
        Pos2::new(right, center.y), Pos2::new(left, center.y),
    ];
    for offset in [-1., 1.] {
        let x = center.x + offset * (size.x + 8.);
        let y = center.y + offset * (size.y + 8.);
        candidates.extend([
            Pos2::new(x, top), Pos2::new(x, bottom),
            Pos2::new(right, y), Pos2::new(left, y),
        ]);
    }
    for x in [visible.left() + half.x, visible.right() - half.x] {
        candidates.extend([Pos2::new(x, top), Pos2::new(x, bottom)]);
    }
    for y in [visible.top() + half.y, visible.bottom() - half.y] {
        candidates.extend([Pos2::new(right, y), Pos2::new(left, y)]);
    }
    for center in candidates {
        let center = Pos2::new(
            center.x.clamp(safe.left() + half.x, safe.right() - half.x),
            center.y.clamp(safe.top() + half.y, safe.bottom() - half.y),
        );
        let badge = Rect::from_center_size(center, size);
        if !index.occupied(badge.expand(LABEL_CLEARANCE)) {
            return Some(badge);
        }
    }
    // A zoomed-in TIFF may fill the viewport. Hiding the name is preferable
    // to covering survey detail or forcing it onto an unrelated map location.
    None
}

fn segment_intersects_rect([a, b]: [Pos2; 2], bounds: Rect) -> bool {
    let delta = b - a;
    let mut near = 0.0_f32;
    let mut far = 1.0_f32;
    for (origin, direction, min, max) in [
        (a.x, delta.x, bounds.left(), bounds.right()),
        (a.y, delta.y, bounds.top(), bounds.bottom()),
    ] {
        if direction.abs() < f32::EPSILON {
            if origin < min || origin > max { return false; }
        } else {
            let first = (min - origin) / direction;
            let last = (max - origin) / direction;
            near = near.max(first.min(last));
            far = far.min(first.max(last));
            if near > far { return false; }
        }
    }
    true
}

fn leader(footprint: &Footprint, badge: Rect) -> Option<[Pos2; 2]> {
    let target = badge.center();
    let mut nearest = None;
    let mut distance = f32::INFINITY;
    for (a, b) in footprint.outline.iter().copied()
        .zip(footprint.outline.iter().copied().cycle().skip(1)) {
        let delta = b - a;
        let length = delta.length_sq();
        if !length.is_finite() || length <= f32::EPSILON { continue; }
        let t = ((target - a).dot(delta) / length).clamp(0., 1.);
        let point = a + delta * t;
        let candidate = point.distance_sq(target);
        if candidate < distance {
            distance = candidate;
            nearest = Some(point);
        }
    }
    let anchor = nearest?;
    let tip = Pos2::new(anchor.x.clamp(badge.left(), badge.right()), anchor.y.clamp(badge.top(), badge.bottom()));
    let length = anchor.distance(tip);
    // Long links on an extreme rotated/partly clipped footprint add clutter.
    (length >= 5. && length <= 96.).then_some([anchor, tip])
}

struct Badge {
    owner: usize,
    rect: Rect,
    galley: Arc<egui::Galley>,
}

pub(super) fn paint(
    painter: &egui::Painter,
    viewport: Rect,
    rasters: &[GeoTiffInfo],
    trenches: &[VirtualTrench],
    project: impl Fn(f64, f64) -> Pos2,
) {
    if !viewport.is_finite() || viewport.width() < 64. || viewport.height() < 48. { return; }
    let mut footprints = Vec::with_capacity(rasters.len() + trenches.len());
    for raster in rasters {
        if !super::layer_visibility_ui::raster_is_visible(raster) { continue; }
        if raster.width == 0 || raster.height == 0 { continue; }
        let outline = [(0., 0.), (f64::from(raster.width), 0.),
            (f64::from(raster.width), f64::from(raster.height)), (0., f64::from(raster.height))]
            .into_iter().map(|(col, row)| {
                let (x, y) = raster.transform.pixel_to_world(col, row);
                project(x, y)
            }).collect();
        let accent = if raster.kickthemap_job_id.is_some() {
            Color32::from_rgb(234, 111, 31)
        } else { Color32::from_rgb(100, 116, 139) };
        if let Some(footprint) = Footprint::new(outline, raster_name(raster), accent)
            .filter(|f| f.bounds.intersects(viewport)) {
            footprints.push(footprint);
        }
    }
    for trench in trenches {
        if !super::layer_visibility_ui::trench_is_visible(trench) { continue; }
        let measured = sleufbase_rust::virtual_trench_editor::is_measured(trench);
        let accent = if measured { Color32::from_rgb(59, 112, 180) }
            else { Color32::from_rgb(20, 133, 119) };
        let name = clean_name(&trench.label, if measured { "MarXact-proefsleuf" } else { "Virtuele proefsleuf" });
        let outline = trench.measured_boundary().into_iter().map(|(x, y)| project(x, y)).collect();
        if let Some(footprint) = Footprint::new(outline, name, accent)
            .filter(|f| f.bounds.intersects(viewport)) {
            footprints.push(footprint);
        }
    }
    let mut index = CollisionIndex::new(viewport);
    for (owner, footprint) in footprints.iter().enumerate() {
        index.insert(footprint.bounds.expand(IMAGE_CLEARANCE), owner);
    }
    let mut badges = Vec::new();
    for (owner, footprint) in footprints.iter().enumerate() {
        let mut job = egui::text::LayoutJob::simple_singleline(
            footprint.name.clone(), egui::FontId::proportional(12.5), INK,
        );
        job.wrap = egui::text::TextWrapping::truncate_at_width(MAX_TEXT_WIDTH.min(viewport.width() - 56.));
        let galley = painter.layout_job(job);
        let size = egui::vec2(galley.size().x + 28., galley.size().y.max(15.) + 12.);
        if let Some(rect) = label_position(footprint.bounds, size, &index) {
            index.insert(rect.expand(LABEL_CLEARANCE), owner);
            badges.push(Badge { owner, rect, galley });
        }
    }
    // Plan every name before painting links, so links cannot cross later labels.
    for badge in &badges {
        if let Some(segment) = leader(&footprints[badge.owner], badge.rect) {
            if !index.blocks_leader(segment, badge.owner) {
                painter.line_segment(segment, Stroke::new(1.0_f32, Color32::from_rgb(132, 144, 159)));
            }
        }
    }
    for badge in badges {
        let rect = badge.rect;
        painter.rect_filled(rect.translate(egui::vec2(0., 2.)), 7., Color32::from_black_alpha(20));
        painter.rect_filled(rect, 7., Color32::from_rgb(255, 255, 255));
        painter.rect_stroke(rect, 7., Stroke::new(1.0_f32, Color32::from_rgb(218, 224, 232)), egui::StrokeKind::Inside);
        painter.rect_filled(Rect::from_min_size(rect.left_top() + egui::vec2(7., 7.),
            egui::vec2(3., rect.height() - 14.)), 1.5, footprints[badge.owner].accent);
        let text_at = rect.left_top() + egui::vec2(17., (rect.height() - badge.galley.size().y) * 0.5);
        painter.galley(text_at, badge.galley, INK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Rect { Rect::from_min_size(Pos2::ZERO, egui::vec2(800., 600.)) }

    #[test]
    fn names_stay_clear_of_all_images_and_each_other_at_different_zoom_levels() {
        for zoom in [0.25, 1., 2., 4.] {
            let viewport = viewport();
            let mut index = CollisionIndex::new(viewport);
            let geometry = [
                Rect::from_center_size(egui::pos2(260., 290.), egui::vec2(120., 60.) * zoom),
                Rect::from_center_size(egui::pos2(430., 290.), egui::vec2(100., 60.) * zoom),
            ];
            for (owner, bounds) in geometry.iter().enumerate() {
                index.insert(bounds.expand(IMAGE_CLEARANCE), owner);
            }
            let mut placed: Vec<Rect> = Vec::new();
            for (owner, bounds) in geometry.iter().enumerate() {
                if let Some(label) = label_position(*bounds, egui::vec2(132., 27.), &index) {
                    assert!(viewport.contains_rect(label.expand(LABEL_CLEARANCE)));
                    assert!(geometry.iter().all(|image| !image.intersects(label.expand(LABEL_CLEARANCE))));
                    assert!(placed.iter().all(|other| !other.intersects(label.expand(LABEL_CLEARANCE))));
                    index.insert(label.expand(LABEL_CLEARANCE), owner);
                    placed.push(label);
                }
            }
            assert!(!placed.is_empty());
        }
    }

    #[test]
    fn fully_zoomed_image_hides_label_instead_of_covering_the_image() {
        let viewport = viewport();
        let image = viewport.expand(1_000_000.);
        let mut index = CollisionIndex::new(viewport);
        index.insert(image.expand(IMAGE_CLEARANCE), 0);
        assert!(label_position(image, egui::vec2(100., 27.), &index).is_none());
    }

    #[test]
    fn rotated_image_uses_every_transformed_corner_for_clearance() {
        let corners = vec![egui::pos2(400., 160.), egui::pos2(560., 300.),
            egui::pos2(400., 440.), egui::pos2(240., 300.)];
        let footprint = Footprint::new(corners, "PS 12".into(), Color32::BLACK).unwrap();
        let mut index = CollisionIndex::new(viewport());
        index.insert(footprint.bounds.expand(IMAGE_CLEARANCE), 0);
        let label = label_position(footprint.bounds, egui::vec2(100., 27.), &index).unwrap();
        assert!(!footprint.bounds.intersects(label.expand(LABEL_CLEARANCE)));
        let segment = leader(&footprint, label).unwrap();
        assert!(segment[0].distance(egui::pos2(400., 160.)) < 1.);
    }

    #[test]
    fn blocked_leaders_are_omitted_and_badge_shadows_share_the_clearance() {
        let mut index = CollisionIndex::new(viewport());
        let shadow = Rect::from_min_size(egui::pos2(200., 200.), egui::vec2(100., 27.)).expand(LABEL_CLEARANCE);
        index.insert(shadow, 2);
        assert!(index.blocks_leader([egui::pos2(250., 180.), egui::pos2(250., 240.)], 1));
        assert!(!index.blocks_leader([egui::pos2(100., 180.), egui::pos2(100., 240.)], 1));
        assert!(index.occupied(Rect::from_min_size(egui::pos2(250., 228.), egui::vec2(50., 20.))));
    }

    #[test]
    fn invalid_geometry_and_empty_or_multiline_names_are_handled() {
        assert!(Footprint::new(vec![Pos2::ZERO, egui::pos2(f32::NAN, 1.), egui::pos2(1., 2.)],
            String::new(), Color32::BLACK).is_none());
        assert_eq!(clean_name("  PS één\nNoord\t2  ", "Proefsleuf"), "PS één Noord 2");
        assert_eq!(clean_name(" \t ", "Proefsleuf"), "Proefsleuf");
        assert_eq!(clean_name(&"é".repeat(300), "Proefsleuf").chars().count(), 257);
    }
}
