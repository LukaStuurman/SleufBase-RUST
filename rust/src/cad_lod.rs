//! Work-map-only path simplification, prepared with the CAD load worker.
//! Balanced trees have linear construction work and logarithmic depth. Their
//! conservative errors bound every original vertex, including closed paths.
use crate::model::Vertex;
use std::sync::atomic::AtomicBool;

const LEAF_SEGMENTS: usize = 8;
const MIN_POINTS: usize = 17;
// These limits bound optimization metadata (under 46 MiB), never source data.
// Paths without a prepared tree retain their complete original geometry.
const MAX_NODES: usize = 1_500_000;
const MAX_PATHS: usize = 400_000;

#[derive(Debug)]
struct Node {
    first: u32,
    last: u32,
    children: [u32; 2],
    error: f64,
}

#[derive(Debug)]
struct Path {
    start: u32,
    node: u32,
}

#[derive(Debug)]
struct Feature {
    index: usize,
    start: u32,
    end: u32,
}

#[derive(Debug, Default)]
pub struct Cache {
    nodes: Vec<Node>,
    paths: Vec<Path>,
    features: Vec<Feature>,
    pub(crate) source_count: usize,
}

#[derive(Clone, Copy)]
pub struct FeatureLod<'a> {
    cache: &'a Cache,
    paths: &'a [Path],
}

fn distance(point: Vertex, a: Vertex, b: Vertex) -> f64 {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let length = dx.hypot(dy);
    if !length.is_finite() || length == 0. {
        return (point.x - a.x).hypot(point.y - a.y);
    }
    let along = ((point.x - a.x) * (dx / length) + (point.y - a.y) * (dy / length))
        .clamp(0., length);
    (point.x - a.x - along * (dx / length))
        .hypot(point.y - a.y - along * (dy / length))
}

impl Cache {
    pub(crate) fn build<'a>(
        source_count: usize,
        features: impl Clone + Iterator<Item = (&'a [Vertex], &'a [usize])>,
        cancel: &AtomicBool,
    ) -> Result<Self, String> {
        let total_points = features.clone().map(|(points, _)| points.len())
            .fold(0usize, usize::saturating_add);
        // Reserve once: no per-path allocation or quadratic realloc/copy loop.
        let mut result = Self {
            source_count,
            nodes: Vec::with_capacity(total_points.saturating_div(2).saturating_add(8).min(MAX_NODES)),
            paths: Vec::with_capacity((total_points / MIN_POINTS).min(MAX_PATHS)),
            features: Vec::with_capacity((total_points / MIN_POINTS).min(MAX_PATHS)),
        };
        for (index, (points, breaks)) in features.enumerate() {
            if index % 128 == 0 { crate::input::check(cancel)?; }
            let before = result.paths.len();
            let mut start = 0;
            for end in breaks.iter().copied().chain(std::iter::once(points.len())) {
                if end > points.len() || end < start { continue; }
                let length = end - start;
                // A balanced split has at most twice the next power-of-two
                // leaf count. Refusing extra metadata never hides a path.
                let leaf_count = length.saturating_sub(1).div_ceil(LEAF_SEGMENTS)
                    .checked_next_power_of_two().unwrap_or(usize::MAX);
                let maximum_nodes = leaf_count.saturating_mul(2).saturating_sub(1);
                if length >= MIN_POINTS && end <= u32::MAX as usize && result.paths.len() < MAX_PATHS
                    && result.nodes.len().saturating_add(maximum_nodes) <= MAX_NODES
                {
                    let node = result.build_node(points, start, end - 1, cancel)?;
                    result.paths.push(Path { start: start as u32, node });
                }
                start = end;
            }
            if result.paths.len() > before {
                result.features.push(Feature {
                    index, start: before as u32, end: result.paths.len() as u32,
                });
            }
        }
        crate::input::check(cancel)?;
        Ok(result)
    }

    fn build_node(&mut self, points: &[Vertex], first: usize, last: usize, cancel: &AtomicBool) -> Result<u32, String> {
        if self.nodes.len() % 256 == 0 { crate::input::check(cancel)?; }
        let at = self.nodes.len() as u32;
        self.nodes.push(Node { first: first as u32, last: last as u32, children: [0; 2], error: 0. });
        let a = points[first];
        let b = points[last];
        let error = if last - first > LEAF_SEGMENTS {
            let middle = first + (last - first) / 2;
            let left = self.build_node(points, first, middle, cancel)?;
            let right = self.build_node(points, middle, last, cancel)?;
            self.nodes[at as usize].children = [left, right];
            // Distance to a segment is convex. Each child's chord lies within
            // the middle-point deviation of the parent chord; add its own
            // already bounded error. This needs no rescan of ancestor points.
            self.nodes[left as usize].error.max(self.nodes[right as usize].error)
                + distance(points[middle], a, b)
        } else {
            points[first + 1..last].iter()
                .map(|&point| distance(point, a, b)).fold(0.0_f64, f64::max)
        };
        self.nodes[at as usize].error = if error.is_finite() { error } else { f64::INFINITY };
        Ok(at)
    }

    pub(crate) fn feature(&self, index: usize) -> Option<FeatureLod<'_>> {
        let at = self.features.binary_search_by_key(&index, |feature| feature.index).ok()?;
        let feature = &self.features[at];
        Some(FeatureLod { cache: self, paths: &self.paths[feature.start as usize..feature.end as usize] })
    }
}

impl FeatureLod<'_> {
    /// Visit only retained segments of this one source path. Separate contours
    /// and color runs are supplied separately by the caller, so no bridge can
    /// be introduced between disconnected parts or glyphs.
    pub fn for_each_segment(
        self, path: &[Vertex], start: usize, tolerance: f64,
        mut draw: impl FnMut(Vertex, Vertex),
    ) {
        let prepared = self.paths.binary_search_by_key(&start, |path| path.start as usize)
            .ok().map(|index| self.paths[index].node);
        if let Some(node) = prepared {
            self.visit(node, path, start, tolerance.max(0.), &mut draw);
        } else {
            for pair in path.windows(2) { draw(pair[0], pair[1]); }
        }
    }

    fn visit(self, node: u32, points: &[Vertex], start: usize, tolerance: f64, draw: &mut impl FnMut(Vertex, Vertex)) {
        let node = &self.cache.nodes[node as usize];
        let first = node.first as usize - start;
        let last = node.last as usize - start;
        if node.error <= tolerance {
            draw(points[first], points[last]);
        } else if node.children[0] != 0 {
            self.visit(node.children[0], points, start, tolerance, draw);
            self.visit(node.children[1], points, start, tolerance, draw);
        } else {
            for pair in points[first..=last].windows(2) { draw(pair[0], pair[1]); }
        }
    }
}
