//! Incremental selection against a finite line in prepared world geometry.
//!
//! The caller must discard a query when project sources or layer visibility
//! change. No source data is cloned or changed; returned indices are the same
//! indices used by `DxfOverlay::inspect_feature` and cable information.
use crate::cad_scene::{DrawRef, Scene};
use crate::cad_spatial::{SegmentCursor, SegmentStep};
use crate::dxf_read::DxfOverlay;
use std::collections::BTreeSet;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

const MAX_WORK_UNITS: usize = 8_192;
const TIME_BUDGET: Duration = Duration::from_millis(3);
// The work map uses metres. Merge coincident contacts despite tiny source
// rounding differences, without making grouping depend on the current zoom.
const GROUP_DISTANCE_METRES: f64 = 0.01;

struct Budget {
    started: Instant,
    remaining: usize,
}

impl Budget {
    fn new() -> Self {
        Self { started: Instant::now(), remaining: MAX_WORK_UNITS }
    }

    fn take(&mut self) -> bool {
        if self.remaining == 0
            || (self.remaining % 64 == 0 && self.started.elapsed() >= TIME_BUDGET)
        {
            return false;
        }
        self.remaining -= 1;
        true
    }
}

#[derive(Debug)]
enum Search {
    Indexed(SegmentCursor),
    Sequential(usize),
}

#[derive(Debug)]
struct OverlayCursor {
    search: Search,
    source_address: usize,
    source_count: usize,
    scene: Weak<Scene>,
}

impl OverlayCursor {
    fn new(overlay: &DxfOverlay) -> Self {
        Self {
            search: if overlay.spatial_index().is_some() {
                Search::Indexed(SegmentCursor::default())
            } else {
                Search::Sequential(0)
            },
            source_address: overlay.features.as_ptr() as usize,
            source_count: overlay.features.len(),
            scene: Arc::downgrade(&overlay.expanded),
        }
    }

    fn matches(&self, overlay: &DxfOverlay) -> bool {
        self.source_address == overlay.features.as_ptr() as usize
            && self.source_count == overlay.features.len()
            && self.scene.as_ptr() == Arc::as_ptr(&overlay.expanded)
    }

    fn next(
        &mut self,
        overlay: &DxfOverlay,
        start: [f64; 2],
        end: [f64; 2],
    ) -> Result<SegmentStep, String> {
        match &mut self.search {
            Search::Indexed(cursor) => overlay.spatial_index()
                .map(|index| index.segment_step(cursor, start, end))
                .ok_or_else(changed_sources),
            Search::Sequential(next) => {
                // Old/manual overlays may have no derived index. Inspect their
                // geometry incrementally too, without first scanning all bounds.
                if *next < overlay.features.len() + overlay.expanded.features.len() {
                    let index = *next;
                    *next += 1;
                    Ok(SegmentStep::Candidate(index))
                } else {
                    Ok(SegmentStep::Done)
                }
            }
        }
    }
}

#[derive(Debug)]
struct FeatureCursor {
    index: usize,
    next_path: usize,
    previous_end: usize,
    edges: std::ops::Range<usize>,
    first_hit: Option<f64>,
}

impl FeatureCursor {
    fn new(index: usize) -> Self {
        Self { index, next_path: 0, previous_end: 0, edges: 0..0, first_hit: None }
    }

    fn advance(
        &mut self,
        drawing: &DrawRef<'_>,
        start: [f64; 2],
        end: [f64; 2],
        budget: &mut Budget,
    ) -> bool {
        while budget.take() {
            if let Some(index) = self.edges.next() {
                let a = drawing.points[index];
                let b = drawing.points[index + 1];
                if let Some(position) = intersection(start, end, [a.x, a.y], [b.x, b.y]) {
                    self.first_hit = Some(self.first_hit.map_or(position, |old| old.min(position)));
                    if position == 0. { return true; }
                }
            } else if self.next_path <= drawing.path_breaks.len() {
                let end = drawing.path_breaks.get(self.next_path).copied()
                    .unwrap_or(drawing.points.len());
                let start = self.previous_end;
                self.previous_end = end;
                self.next_path += 1;
                // Match DrawRef::paths exactly, including empty/invalid ranges.
                // Every path boundary consumes work; disconnected paths are
                // never joined, even when one feature has many small contours.
                self.edges = if drawing.points.get(start..end).is_some() {
                    start..end.saturating_sub(1)
                } else {
                    0..0
                };
            } else {
                return true;
            }
        }
        false
    }
}

/// An owned cursor. Call `advance` once per UI frame and request another frame
/// while unfinished. Dropping the query cancels it without changing the drawing.
#[derive(Debug)]
pub struct Query {
    start: [f64; 2],
    end: [f64; 2],
    overlay: usize,
    overlay_count: Option<usize>,
    traversal: Option<OverlayCursor>,
    feature: Option<FeatureCursor>,
    // Nonnegative finite f64 bit patterns sort in numerical order. Source IDs
    // provide deterministic ties without cloning layer names or other metadata.
    hits: BTreeSet<(u64, usize, usize)>,
    results: Vec<(usize, usize)>,
    groups: Vec<std::ops::Range<usize>>,
    group_anchor: Option<f64>,
    group_tolerance: f64,
    draining: bool,
    finished: bool,
}

impl Query {
    pub fn new(start: [f64; 2], end: [f64; 2]) -> Result<Self, String> {
        if !start.iter().chain(end.iter()).all(|value| value.is_finite())
            || !(end[0] - start[0]).is_finite()
            || !(end[1] - start[1]).is_finite()
        {
            return Err("Kies twee geldige punten op de kaart.".into());
        }
        if start == end {
            return Err("Kies voor het einde van de lijn een ander punt.".into());
        }
        let length = (end[0] - start[0]).hypot(end[1] - start[1]);
        Ok(Self {
            start, end, overlay: 0, overlay_count: None, traversal: None,
            feature: None, hits: BTreeSet::new(), results: Vec::new(),
            groups: Vec::new(), group_anchor: None,
            group_tolerance: if length.is_finite() { (GROUP_DISTANCE_METRES / length).min(1.) } else { 0. },
            draining: false, finished: false,
        })
    }

    /// Bound both geometric work and result collection, including individual
    /// long features. There is no final unbounded scene scan or result sort.
    /// Keep sources/visibility stable across calls, including after completion
    /// and before applying the returned indices to cable information.
    pub fn advance(&mut self, overlays: &[DxfOverlay]) -> Result<(), String> {
        if self.finished { return Ok(()); }
        match self.overlay_count {
            Some(count) if count != overlays.len() => return Err(changed_sources()),
            None => self.overlay_count = Some(overlays.len()),
            _ => {}
        }
        if let Some(cursor) = &self.traversal {
            if !overlays.get(self.overlay).is_some_and(|overlay| cursor.matches(overlay)) {
                return Err(changed_sources());
            }
        }
        let mut budget = Budget::new();
        while budget.take() {
            if self.draining {
                if let Some((position, overlay, feature)) = self.hits.pop_first() {
                    let position = f64::from_bits(position);
                    let next = self.results.len();
                    // Anchor every group to its first contact, not the previous
                    // cable: a chain of nearby hits must not join distant lines.
                    if self.group_anchor.is_some_and(|anchor| position - anchor <= self.group_tolerance) {
                        if let Some(group) = self.groups.last_mut() { group.end = next + 1; }
                    } else {
                        self.groups.push(next..next + 1);
                        self.group_anchor = Some(position);
                    }
                    self.results.push((overlay, feature));
                } else {
                    self.finished = true;
                    break;
                }
                continue;
            }
            let Some(overlay) = overlays.get(self.overlay) else {
                self.draining = true;
                // Reserve once so incremental pushes cannot copy an ever larger
                // result list in later frames. No geometry is copied here.
                self.results.reserve(self.hits.len());
                self.groups.reserve(self.hits.len());
                continue;
            };
            if !overlay.visible {
                self.next_overlay();
                continue;
            }
            if let Some(mut cursor) = self.feature.take() {
                let drawing = overlay.inspect_feature(cursor.index).ok_or_else(changed_sources)?;
                if !drawing.visible { continue; }
                if cursor.advance(&drawing, self.start, self.end, &mut budget) {
                    if let Some(position) = cursor.first_hit {
                        self.hits.insert((position.to_bits(), self.overlay, cursor.index));
                    }
                } else {
                    self.feature = Some(cursor);
                    break;
                }
                continue;
            }
            let traversal = self.traversal.get_or_insert_with(|| OverlayCursor::new(overlay));
            match traversal.next(overlay, self.start, self.end)? {
                SegmentStep::Pending => {}
                SegmentStep::Done => self.next_overlay(),
                SegmentStep::Candidate(index) => {
                    let drawing = overlay.inspect_feature(index).ok_or_else(changed_sources)?;
                    if drawing.visible && is_line(&drawing) {
                        self.feature = Some(FeatureCursor::new(index));
                    }
                }
            }
        }
        Ok(())
    }

    fn next_overlay(&mut self) {
        self.overlay += 1;
        self.traversal = None;
        self.feature = None;
    }

    pub fn finished(&self) -> bool { self.finished }

    pub fn matched_count(&self) -> usize { self.hits.len() + self.results.len() }

    /// Ordered by the first contact along the drawn line, then by stable source
    /// and feature order. The complete result is available only when finished.
    pub fn results(&self) -> &[(usize, usize)] { &self.results }

    pub fn into_results(self) -> Vec<(usize, usize)> { self.results }

    /// Groups are contiguous ranges in the ordered result. They are collected
    /// within the same per-frame budget, without another geometry scan. A source
    /// object still occurs only once, at its first contact along the line.
    pub fn into_grouped_results(self) -> (Vec<(usize, usize)>, Vec<std::ops::Range<usize>>) {
        (self.results, self.groups)
    }
}

fn changed_sources() -> String {
    "De tekening is gewijzigd. Teken de selectielijn opnieuw.".into()
}

fn is_line(drawing: &DrawRef<'_>) -> bool {
    matches!(drawing.source.entity_type.as_str(),
        "LINE" | "LWPOLYLINE" | "POLYLINE" | "ARC" | "CIRCLE" | "ELLIPSE" | "SPLINE"
    ) && !drawing.source.cad.as_deref().is_some_and(|cad| cad.mesh.is_some())
}

fn orient(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    robust::orient2d(
        robust::Coord { x: a[0], y: a[1] },
        robust::Coord { x: b[0], y: b[1] },
        robust::Coord { x: c[0], y: c[1] },
    )
}

fn opposite_or_zero(a: f64, b: f64) -> bool {
    a == 0. || b == 0. || (a < 0.) != (b < 0.)
}

/// Exact intersection of prepared segments, with no screen-space click radius.
/// Adaptive orientation handles endpoint touches and near-collinear geometry.
fn intersection(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> Option<f64> {
    if !c.iter().chain(d.iter()).all(|value| value.is_finite())
        || a[0].min(b[0]) > c[0].max(d[0]) || a[0].max(b[0]) < c[0].min(d[0])
        || a[1].min(b[1]) > c[1].max(d[1]) || a[1].max(b[1]) < c[1].min(d[1])
    {
        return None;
    }
    let c_side = orient(a, b, c);
    let d_side = orient(a, b, d);
    let a_side = orient(c, d, a);
    let b_side = orient(c, d, b);
    if ![c_side, d_side, a_side, b_side].iter().all(|value| value.is_finite())
        || !opposite_or_zero(c_side, d_side) || !opposite_or_zero(a_side, b_side)
    {
        return None;
    }
    let axis = usize::from((b[1] - a[1]).abs() > (b[0] - a[0]).abs());
    let position = |p: [f64; 2]| (p[axis] - a[axis]) / (b[axis] - a[axis]);
    let hit = if c_side == 0. && d_side == 0. {
        let first = position(c);
        let last = position(d);
        let low = first.min(last).max(0.);
        let high = first.max(last).min(1.);
        if low > high { return None; }
        low
    } else if c_side == 0. {
        position(c)
    } else if d_side == 0. {
        position(d)
    } else if a_side == 0. {
        0.
    } else if b_side == 0. {
        1.
    } else {
        // Opposite signs make this a ratio of distances from the source line.
        // Normalize before adding to avoid overflow for large determinants.
        let first = a_side.abs();
        let last = b_side.abs();
        if first <= last {
            let ratio = first / last;
            ratio / (1. + ratio)
        } else {
            1. / (1. + last / first)
        }
    };
    // Canonical positive zero is required for the bit-pattern ordering above.
    Some(if hit <= 0. { 0. } else { hit.min(1.) })
}
