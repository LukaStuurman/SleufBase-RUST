//! A derived, non-serialized index of prepared CAD geometry in world space.
//! Entries retain the public inspection index; source entities are never moved.
use crate::model::{Bounds, Vertex};
use std::sync::atomic::AtomicBool;

const LEAF_SIZE: usize = 16;

#[derive(Debug)]
struct Entry {
    index: usize,
    bounds: Bounds,
}

#[derive(Debug)]
struct Node {
    bounds: Bounds,
    start: usize,
    end: usize,
    children: Option<[usize; 2]>,
}

#[derive(Debug, Default)]
pub(crate) struct Index {
    entries: Vec<Entry>,
    nodes: Vec<Node>,
    positions: Vec<u32>,
    pub(crate) source_count: usize,
}

/// A resumable traversal: one step visits one node or one leaf entry. A long
/// diagonal selection need not collect every feature in its enclosing box.
#[derive(Debug)]
pub(crate) struct SegmentCursor {
    pending: Vec<usize>,
    leaf: std::ops::Range<usize>,
}

impl Default for SegmentCursor {
    fn default() -> Self {
        Self { pending: vec![0], leaf: 0..0 }
    }
}

pub(crate) enum SegmentStep {
    Pending,
    Candidate(usize),
    Done,
}

pub(crate) fn bounds(points: &[Vertex]) -> Option<Bounds> {
    let mut result: Option<Bounds> = None;
    for point in points {
        if !point.x.is_finite() || !point.y.is_finite() {
            continue;
        }
        let next = Bounds {
            min_x: point.x,
            min_y: point.y,
            max_x: point.x,
            max_y: point.y,
        };
        result = Some(result.map_or(next, |current| union(current, next)));
    }
    result
}

pub(crate) fn intersects(a: Bounds, b: Bounds) -> bool {
    a.min_x <= b.max_x && a.max_x >= b.min_x && a.min_y <= b.max_y && a.max_y >= b.min_y
}

fn union(a: Bounds, b: Bounds) -> Bounds {
    Bounds {
        min_x: a.min_x.min(b.min_x),
        min_y: a.min_y.min(b.min_y),
        max_x: a.max_x.max(b.max_x),
        max_y: a.max_y.max(b.max_y),
    }
}

fn segment_intersects_bounds(start: [f64; 2], end: [f64; 2], bounds: Bounds) -> bool {
    let mut low: f64 = 0.;
    let mut high: f64 = 1.;
    for (axis, min, max) in [(0, bounds.min_x, bounds.max_x), (1, bounds.min_y, bounds.max_y)] {
        // Only the broad phase is padded, by a few coordinate-scale rounding
        // units. Exact prepared segments decide the eventual selection.
        let magnitude = start[axis].abs().max(end[axis].abs()).max(min.abs()).max(max.abs()).max(1.);
        let padding = magnitude * (32. * f64::EPSILON);
        let min = min - padding;
        let max = max + padding;
        let direction = end[axis] - start[axis];
        if direction == 0. {
            if start[axis] < min || start[axis] > max { return false; }
        } else {
            let first = (min - start[axis]) / direction;
            let second = (max - start[axis]) / direction;
            low = low.max(first.min(second));
            high = high.min(first.max(second));
            if low > high { return false; }
        }
    }
    true
}

impl Index {
    pub(crate) fn build<'a>(
        source_count: usize,
        features: impl Iterator<Item = &'a [Vertex]>,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        let mut result = Self { source_count, ..Self::default() };
        for (index, points) in features.enumerate() {
            if index % 256 == 0 {
                crate::input::check(cancel)?;
            }
            if let Some(bounds) = bounds(points) {
                result.entries.push(Entry { index, bounds });
            }
            result.positions.push(u32::MAX);
        }
        if !result.entries.is_empty() {
            result.partition(0, result.entries.len(), cancel)?;
        }
        for (position, entry) in result.entries.iter().enumerate() {
            result.positions[entry.index] = position as u32;
        }
        crate::input::check(cancel)?;
        Ok(result)
    }

    fn partition(&mut self, start: usize, end: usize, cancel: &AtomicBool) -> Result<usize, String> {
        crate::input::check(cancel)?;
        let bounds = self.entries[start + 1..end].iter()
            .fold(self.entries[start].bounds, |a, b| union(a, b.bounds));
        let index = self.nodes.len();
        self.nodes.push(Node { bounds, start, end, children: None });
        if end - start > LEAF_SIZE {
            let x_axis = bounds.max_x - bounds.min_x >= bounds.max_y - bounds.min_y;
            let middle = start + (end - start) / 2;
            self.entries[start..end].select_nth_unstable_by(middle - start, |a, b| {
                let center = |entry: &Entry| if x_axis {
                    entry.bounds.min_x * 0.5 + entry.bounds.max_x * 0.5
                } else {
                    entry.bounds.min_y * 0.5 + entry.bounds.max_y * 0.5
                };
                center(a).total_cmp(&center(b)).then(a.index.cmp(&b.index))
            });
            let left = self.partition(start, middle, cancel)?;
            let right = self.partition(middle, end, cancel)?;
            self.nodes[index].children = Some([left, right]);
        }
        Ok(index)
    }

    /// Return source/instance order, so drawing overlap and inspection ties keep
    /// the same meaning as the original unindexed traversal.
    pub(crate) fn query(&self, viewport: Bounds) -> Vec<usize> {
        let mut found = Vec::new();
        if !self.nodes.is_empty() {
            self.visit(0, viewport, &mut found);
        }
        found.sort_unstable();
        found
    }

    pub(crate) fn feature_bounds(&self, index: usize) -> Option<Bounds> {
        self.positions.get(index).and_then(|&position| self.entries.get(position as usize))
            .map(|entry| entry.bounds)
    }

    pub(crate) fn segment_step(
        &self,
        cursor: &mut SegmentCursor,
        start: [f64; 2],
        end: [f64; 2],
    ) -> SegmentStep {
        if let Some(position) = cursor.leaf.next() {
            let entry = &self.entries[position];
            return if segment_intersects_bounds(start, end, entry.bounds) {
                SegmentStep::Candidate(entry.index)
            } else {
                SegmentStep::Pending
            };
        }
        let Some(index) = cursor.pending.pop() else { return SegmentStep::Done; };
        let Some(node) = self.nodes.get(index) else { return SegmentStep::Done; };
        if segment_intersects_bounds(start, end, node.bounds) {
            if let Some([left, right]) = node.children {
                cursor.pending.push(right);
                cursor.pending.push(left);
            } else {
                cursor.leaf = node.start..node.end;
            }
        }
        SegmentStep::Pending
    }

    fn visit(&self, index: usize, viewport: Bounds, found: &mut Vec<usize>) {
        let node = &self.nodes[index];
        if !intersects(node.bounds, viewport) {
            return;
        }
        if viewport.min_x <= node.bounds.min_x && viewport.max_x >= node.bounds.max_x
            && viewport.min_y <= node.bounds.min_y && viewport.max_y >= node.bounds.max_y
        {
            found.extend(self.entries[node.start..node.end].iter().map(|entry| entry.index));
        } else if let Some(children) = node.children {
            self.visit(children[0], viewport, found);
            self.visit(children[1], viewport, found);
        } else {
            found.extend(self.entries[node.start..node.end].iter()
                .filter(|entry| intersects(entry.bounds, viewport)).map(|entry| entry.index));
        }
    }
}
