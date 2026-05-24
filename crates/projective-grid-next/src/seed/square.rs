//! Square seed-quad finder for `(LatticeKind::Square, Evidence::Oriented2)`.
//!
//! The detector's first non-local commitment is a *seed quad*: four
//! [`OrientedFeature`]s at grid cells `(0, 0), (1, 0), (0, 1), (1, 1)` whose
//! four edge lengths are pairwise self-consistent (within
//! [`SeedParams::edge_ratio_tol`]) and whose chord directions align with
//! the corner axes (within [`SeedParams::axis_tol_rad`]).
//!
//! Cell size is derived from the seed's own four edges — never passed in
//! ahead of time. This matches the CLAUDE.md "Cell-size estimation gotcha"
//! discipline.
//!
//! The seed finder is `(Square, Oriented2)`-specific: it consults each
//! feature's two local axes to classify chord directions as "+u" vs "+v",
//! and only commits a quad whose `D` corner (parallelogram completion) is
//! within [`SeedParams::close_tol_rel`] of the prediction.

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};

use crate::feature::OrientedFeature;
use crate::float::{lit, Float};

/// Indices of the four seed corners at lattice cells `(0, 0)`, `(1, 0)`,
/// `(0, 1)`, `(1, 1)` (the canonical 2×2 seed layout).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Seed {
    /// Feature index at lattice cell `(0, 0)`.
    pub a: usize,
    /// Feature index at lattice cell `(1, 0)`.
    pub b: usize,
    /// Feature index at lattice cell `(0, 1)`.
    pub c: usize,
    /// Feature index at lattice cell `(1, 1)`.
    pub d: usize,
}

impl Seed {
    /// Construct a seed quad from four feature indices in canonical
    /// `(0, 0), (1, 0), (0, 1), (1, 1)` layout order.
    pub const fn new(a: usize, b: usize, c: usize, d: usize) -> Self {
        Self { a, b, c, d }
    }
}

/// A seed quad bundled with the cell size derived from its own four edges.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct SeedSearchOutput<F: Float> {
    /// The four corners in canonical layout.
    pub seed: Seed,
    /// Mean of the four seed-edge lengths in pixels.
    pub cell_size: F,
}

impl<F: Float> SeedSearchOutput<F> {
    /// Construct a seed-output bundle.
    pub const fn new(seed: Seed, cell_size: F) -> Self {
        Self { seed, cell_size }
    }
}

/// Tuning knobs for the square seed-quad finder.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SeedParams<F: Float> {
    /// Angular tolerance (radians) for classifying a chord against the
    /// corner's `axes[0]` vs `axes[1]`. Chord directions farther than
    /// `axis_tol_rad` from both axes are dropped.
    pub axis_tol_rad: F,
    /// Edge-length ratio tolerance: `min_edge / max_edge >= 1 - edge_ratio_tol`
    /// for all four seed edges.
    pub edge_ratio_tol: F,
    /// Search radius for the `D` corner around the parallelogram prediction,
    /// expressed as a fraction of the seed's mean `(|AB| + |AC|) / 2`.
    pub close_tol_rel: F,
    /// `K` in the KD-tree query for B/C neighbours of each A candidate.
    pub k_neighbours: usize,
    /// Per-axis cap on enumerated B / C candidates when running the inner
    /// pair search.
    pub top_per_axis: usize,
    /// Optional per-feature parity tag (length must equal `features.len()`).
    /// Tag values: `0` for the "A/D" pool (typically the canonical-cluster
    /// corners of a chessboard) and `1` for the "B/C" pool (typically the
    /// swapped-cluster corners). When set, the seed finder enforces the
    /// canonical 2×2 chess pattern: `A.tag == 0`, `B.tag == 1`,
    /// `C.tag == 1`, `D.tag == 0`. This blocks seed quads at 2×-spacing
    /// or with marker-internal corners that have the wrong parity. When
    /// `None`, no parity-pattern constraint is enforced (default).
    pub candidate_pool_split: Option<Vec<u8>>,
}

impl<F: Float> Default for SeedParams<F> {
    fn default() -> Self {
        Self {
            // 15° expressed via radian conversion of an f32 literal.
            axis_tol_rad: lit::<F>(15.0_f32) * F::pi() / lit::<F>(180.0_f32),
            edge_ratio_tol: lit::<F>(0.30_f32),
            close_tol_rel: lit::<F>(0.30_f32),
            k_neighbours: 32,
            top_per_axis: 6,
            candidate_pool_split: None,
        }
    }
}

impl<F: Float> SeedParams<F> {
    /// Construct seed params from the three primary tolerances. Other knobs
    /// take their defaults.
    pub fn new(axis_tol_rad: F, edge_ratio_tol: F, close_tol_rel: F) -> Self {
        Self {
            axis_tol_rad,
            edge_ratio_tol,
            close_tol_rel,
            ..Self::default()
        }
    }

    /// Builder-style override: supply the per-feature parity tags.
    /// See [`Self::candidate_pool_split`].
    pub fn with_candidate_pool_split(mut self, tags: Vec<u8>) -> Self {
        self.candidate_pool_split = Some(tags);
        self
    }
}

/// Search a slice of [`OrientedFeature`]s for the first seed quad whose
/// edges and axes are self-consistent under [`SeedParams`].
///
/// Returns `None` when no quad satisfies every gate.
///
/// When [`SeedParams::candidate_pool_split`] is `Some`, the tags slice
/// must have the same length as `features`. The
/// [`crate::detect::detect_grid_all`] entry point validates this and
/// returns `GridError::InconsistentInput` on a length mismatch. Calling
/// `find_quad` directly with a length-mismatched tags vector falls back
/// to the no-parity behaviour (the gate is silently disabled).
pub fn find_quad<F>(
    features: &[OrientedFeature<F, 2>],
    params: &SeedParams<F>,
) -> Option<SeedSearchOutput<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    if features.len() < 4 {
        return None;
    }

    let positions: Vec<Point2<F>> = features.iter().map(|f| f.point.position).collect();
    let tree = build_tree(&positions);
    let ratio_floor = ratio_floor(params.edge_ratio_tol);
    let tags = params
        .candidate_pool_split
        .as_deref()
        .filter(|t| t.len() == features.len());

    for a_idx in 0..features.len() {
        if !tag_matches(tags, a_idx, 0) {
            continue;
        }
        if let Some(out) = try_from_anchor(
            a_idx,
            features,
            &positions,
            &tree,
            params,
            ratio_floor,
            tags,
        ) {
            return Some(out);
        }
    }
    None
}

/// Return `true` when either no parity tags are supplied or `tags[idx]
/// == expected`. The caller (`find_quad`) only passes `Some(tags)` after
/// confirming `tags.len() == features.len()`, so the bounds check is
/// guaranteed safe; a guarded `get` keeps the helper itself panic-free
/// if it is ever reused from a less defensive caller.
#[inline]
fn tag_matches(tags: Option<&[u8]>, idx: usize, expected: u8) -> bool {
    match tags {
        None => true,
        Some(t) => t.get(idx) == Some(&expected),
    }
}

// ----------------------------- internals -----------------------------------

fn build_tree<F>(positions: &[Point2<F>]) -> KdTree<F, 2>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let mut tree: KdTree<F, 2> = KdTree::new();
    for (idx, p) in positions.iter().enumerate() {
        tree.add(&[p.x, p.y], idx as u64);
    }
    tree
}

fn ratio_floor<F: Float>(edge_ratio_tol: F) -> F {
    let one = F::one();
    let min_ratio = one - edge_ratio_tol;
    let max_ratio = one + edge_ratio_tol;
    min_ratio / max_ratio
}

type NeighbourRow<F> = (usize, F, Vector2<F>);

fn try_from_anchor<F>(
    a_idx: usize,
    features: &[OrientedFeature<F, 2>],
    positions: &[Point2<F>],
    tree: &KdTree<F, 2>,
    params: &SeedParams<F>,
    ratio_floor: F,
    tags: Option<&[u8]>,
) -> Option<SeedSearchOutput<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let a_axes = &features[a_idx].axes;
    let a_axis0 = wrap_pi(a_axes[0].angle_rad);
    let a_axis1 = wrap_pi(a_axes[1].angle_rad);
    let a_pos = positions[a_idx];

    let neighbours = collect_neighbours(a_idx, a_pos, tree, positions, params.k_neighbours);
    if neighbours.len() < 2 {
        return None;
    }

    let (b_cands, c_cands) =
        split_neighbours_by_axis(&neighbours, a_axis0, a_axis1, params.axis_tol_rad);
    if b_cands.is_empty() || c_cands.is_empty() {
        return None;
    }

    for (b_idx, b_dist, b_off) in b_cands.iter().take(params.top_per_axis) {
        if !tag_matches(tags, *b_idx, 1) {
            continue;
        }
        for (c_idx, c_dist, c_off) in c_cands.iter().take(params.top_per_axis) {
            if b_idx == c_idx {
                continue;
            }
            if !tag_matches(tags, *c_idx, 1) {
                continue;
            }
            if let Some(out) = try_complete_quad(QuadCompleteArgs {
                a_idx,
                a_pos,
                b_idx: *b_idx,
                b_dist: *b_dist,
                b_off: *b_off,
                c_idx: *c_idx,
                c_dist: *c_dist,
                c_off: *c_off,
                positions,
                tree,
                params,
                ratio_floor,
                tags,
            }) {
                return Some(out);
            }
        }
    }
    None
}

fn collect_neighbours<F>(
    a_idx: usize,
    a_pos: Point2<F>,
    tree: &KdTree<F, 2>,
    positions: &[Point2<F>],
    k: usize,
) -> Vec<NeighbourRow<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let eps = lit::<F>(1e-3_f32);
    let mut neighbours: Vec<NeighbourRow<F>> = tree
        .nearest_n::<SquaredEuclidean>(&[a_pos.x, a_pos.y], k)
        .into_iter()
        .filter_map(|nn| {
            let idx = nn.item as usize;
            if idx == a_idx {
                return None;
            }
            let p = positions[idx];
            let off = Vector2::new(p.x - a_pos.x, p.y - a_pos.y);
            let d = nn.distance.sqrt();
            if !d.is_finite() || d <= eps {
                return None;
            }
            Some((idx, d, off))
        })
        .collect();
    neighbours.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    neighbours
}

fn split_neighbours_by_axis<F: Float>(
    neighbours: &[NeighbourRow<F>],
    axis0: F,
    axis1: F,
    axis_tol: F,
) -> (Vec<NeighbourRow<F>>, Vec<NeighbourRow<F>>) {
    let mut b = Vec::new();
    let mut c = Vec::new();
    for (idx, dist, off) in neighbours {
        let ang = wrap_pi(off.y.atan2(off.x));
        let d0 = angular_dist_pi(ang, axis0);
        let d1 = angular_dist_pi(ang, axis1);
        if d0 <= axis_tol && d0 < d1 {
            b.push((*idx, *dist, *off));
        } else if d1 <= axis_tol && d1 < d0 {
            c.push((*idx, *dist, *off));
        }
    }
    (b, c)
}

struct QuadCompleteArgs<'a, F>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    a_idx: usize,
    a_pos: Point2<F>,
    b_idx: usize,
    b_dist: F,
    b_off: Vector2<F>,
    c_idx: usize,
    c_dist: F,
    c_off: Vector2<F>,
    positions: &'a [Point2<F>],
    tree: &'a KdTree<F, 2>,
    params: &'a SeedParams<F>,
    ratio_floor: F,
    /// Optional per-feature parity tags. When `Some`, the D candidate
    /// must additionally satisfy `tags[d] == 0` (matching the canonical
    /// chess pattern A/D in pool 0, B/C in pool 1).
    tags: Option<&'a [u8]>,
}

fn try_complete_quad<F>(args: QuadCompleteArgs<'_, F>) -> Option<SeedSearchOutput<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let half = lit::<F>(0.5_f32);
    let (bc_lo, bc_hi) = if args.b_dist <= args.c_dist {
        (args.b_dist, args.c_dist)
    } else {
        (args.c_dist, args.b_dist)
    };
    if bc_lo / bc_hi < args.ratio_floor {
        return None;
    }

    let pred = args.a_pos + args.b_off + args.c_off;
    let avg_edge = (args.b_dist + args.c_dist) * half;
    let close_px = args.params.close_tol_rel * avg_edge;
    let close_px_sq = close_px * close_px;

    let d_idx = find_nearest(args.tree, pred, close_px_sq, |idx| {
        idx == args.a_idx
            || idx == args.b_idx
            || idx == args.c_idx
            || !tag_matches(args.tags, idx, 0)
    })?;
    let d_pos = args.positions[d_idx];

    let bd = (d_pos - args.positions[args.b_idx]).norm();
    let cd = (d_pos - args.positions[args.c_idx]).norm();
    let all = [args.b_dist, args.c_dist, bd, cd];
    let (emin, emax) = min_max(&all)?;
    if emax <= F::zero() || emin / emax < args.ratio_floor {
        return None;
    }

    let quarter = lit::<F>(0.25_f32);
    let cell_size = (args.b_dist + args.c_dist + bd + cd) * quarter;
    let seed = Seed::new(args.a_idx, args.b_idx, args.c_idx, d_idx);
    Some(SeedSearchOutput::new(seed, cell_size))
}

fn find_nearest<F, X>(
    tree: &KdTree<F, 2>,
    target: Point2<F>,
    radius_sq: F,
    exclude: X,
) -> Option<usize>
where
    F: Float + kiddo::float::kdtree::Axis,
    X: Fn(usize) -> bool,
{
    let mut best: Option<(usize, F)> = None;
    for nn in tree.within_unsorted::<SquaredEuclidean>(&[target.x, target.y], radius_sq) {
        let idx = nn.item as usize;
        if exclude(idx) {
            continue;
        }
        let d2 = nn.distance;
        if best.map(|b| d2 < b.1).unwrap_or(true) {
            best = Some((idx, d2));
        }
    }
    best.map(|(idx, _)| idx)
}

fn min_max<F: Float>(values: &[F]) -> Option<(F, F)> {
    let mut iter = values.iter().copied();
    let first = iter.next()?;
    let mut lo = first;
    let mut hi = first;
    for v in iter {
        if v < lo {
            lo = v;
        }
        if v > hi {
            hi = v;
        }
    }
    Some((lo, hi))
}

// ---------- undirected angle helpers (local; the broader stats/circular ----
//            helpers stay quarantined for Phase C). -----------------------

#[inline]
fn wrap_pi<F: Float>(theta: F) -> F {
    let pi = F::pi();
    let mut t = theta % pi;
    if t < F::zero() {
        t += pi;
    }
    if t >= pi {
        t -= pi;
    }
    t
}

#[inline]
fn angular_dist_pi<F: Float>(a: F, b: F) -> F {
    let pi = F::pi();
    let mut diff = ((a - b) % pi + pi) % pi;
    let comp = pi - diff;
    if comp < diff {
        diff = comp;
    }
    diff
}

#[cfg(test)]
mod tests {
    use nalgebra::Point2;

    use super::*;
    use crate::feature::{LocalAxis, PointFeature};

    fn axis_aligned_features<F>(rows: i32, cols: i32, s: F) -> Vec<OrientedFeature<F, 2>>
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let origin = lit::<F>(50.0_f32);
        let mut out = Vec::with_capacity((rows * cols) as usize);
        let mut idx = 0_usize;
        for j in 0..rows {
            for i in 0..cols {
                let x = lit::<F>(i as f32) * s + origin;
                let y = lit::<F>(j as f32) * s + origin;
                let point = PointFeature::new(idx, Point2::new(x, y));
                let axes = [
                    LocalAxis::new(F::zero(), None),
                    LocalAxis::new(F::frac_pi_2(), None),
                ];
                out.push(OrientedFeature::new(point, axes));
                idx += 1;
            }
        }
        out
    }

    fn assert_finds_quad_on_clean_grid<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let s = lit::<F>(20.0_f32);
        let features = axis_aligned_features::<F>(5, 5, s);
        let out = find_quad::<F>(&features, &SeedParams::<F>::default())
            .expect("seed quad on clean grid");
        let rel_diff = (out.cell_size - s) / s;
        let abs = if rel_diff < F::zero() {
            -rel_diff
        } else {
            rel_diff
        };
        assert!(abs < lit::<F>(0.05_f32));
    }

    fn assert_returns_none_below_four<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let features: Vec<OrientedFeature<F, 2>> = Vec::new();
        let out = find_quad::<F>(&features, &SeedParams::<F>::default());
        assert!(out.is_none());
    }

    #[test]
    fn finds_quad_clean_grid_f32() {
        assert_finds_quad_on_clean_grid::<f32>();
    }

    #[test]
    fn finds_quad_clean_grid_f64() {
        assert_finds_quad_on_clean_grid::<f64>();
    }

    #[test]
    fn returns_none_below_four_f32() {
        assert_returns_none_below_four::<f32>();
    }

    #[test]
    fn returns_none_below_four_f64() {
        assert_returns_none_below_four::<f64>();
    }
}
