//! Square BFS seed-and-grow engine.
//!
//! Phase-C scope: position prediction + ambiguity-gated attachment +
//! per-edge axis-alignment and length gates. No parity, no
//! consumer-supplied policy, no diagnostics sink. Returns a labelled
//! `Coord -> feature_index` map rebased so the bounding-box minimum is
//! `(0, 0)`.

use std::collections::{HashMap, HashSet, VecDeque};

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};

use crate::feature::OrientedFeature;
use crate::float::{lit, Float};
use crate::lattice::{Coord, SQUARE_CARDINAL_OFFSETS};
use crate::seed::SeedSearchOutput;

/// Tunable knobs for the BFS seed-and-grow engine that backs the
/// `(LatticeKind::Square, Evidence::Oriented2)` seed-and-grow algorithm.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct GrowParams<F: Float> {
    /// Candidate search radius around each prediction, expressed as a
    /// fraction of the seed-derived cell size. Default `0.45`.
    pub attach_search_rel: F,
    /// Acceptance gate: `second_nearest / nearest >= attach_ambiguity_factor`.
    /// Default `1.3`.
    pub attach_ambiguity_factor: F,
    /// Per-edge length tolerance (absolute fraction of cell size). An edge is
    /// rejected when `len / cell_size` falls outside `[1 - tol, 1 + tol]`.
    /// Default `0.35`.
    pub edge_length_tol: F,
    /// Per-edge axis-alignment tolerance in radians. The candidate's two
    /// axes must each lie within `axis_align_tol_rad` of one of the seed
    /// axes (and pick distinct axes). Default `25° = 0.4363 rad`.
    pub axis_align_tol_rad: F,
    /// Whether to consume per-neighbour local-step estimates when available.
    /// Default `true`.
    pub local_step_fallback: bool,
    /// Minimum number of labelled cardinal neighbours whose edge length
    /// to the candidate must fall within `[1 - edge_length_tol, 1 +
    /// edge_length_tol] * cell_size` for the candidate to be admitted.
    /// Edges outside the band no longer auto-reject the candidate; the
    /// gate now passes when at least this many edges land in-band.
    /// Default `u8::MAX` (i.e. every labelled cardinal neighbour must
    /// agree, preserving the historical "any out-of-band edge rejects"
    /// behaviour). Set to `1` to match the legacy
    /// `projective_grid::square::grow::any_cardinal_edge_ok` "at least
    /// one must pass" semantics that the chessboard adapter relies on.
    pub cardinal_edge_quorum: u8,
    /// Multiplier on `attach_search_rel * cell_size` for cells *outside*
    /// the current labelled bounding box (predicted from labelled
    /// neighbours). When the BFS extrapolates past the labelled bbox edge
    /// the prediction is less reliable, so widening the search radius
    /// trades a bit of ambiguity-gate exposure for recall on warped
    /// boards. Default `1.0` (preserves current behaviour). The chessboard
    /// adapter sets this to `2.0` to match legacy
    /// `projective_grid::square::grow::GrowParams::boundary_search_factor`.
    pub boundary_search_factor: F,
    /// Optional global grow-axis override (radians, modulo π, matching
    /// [`crate::feature::LocalAxis::angle_rad`]). When `Some([u, v])`,
    /// `bfs_grow` uses these angles for the global u/v directions instead
    /// of deriving them from the seed quad's `B-A` / `C-A` chords. Useful
    /// when a higher-level cluster step has already estimated reliable
    /// grid axes (e.g. the chessboard cluster centres) and the seed-
    /// derived axes would drift on perspective-warped boards. Default
    /// `None` (preserves current seed-derived behaviour).
    pub global_axis_u_v: Option<[F; 2]>,
}

impl<F: Float> Default for GrowParams<F> {
    fn default() -> Self {
        Self {
            attach_search_rel: lit::<F>(0.45_f32),
            attach_ambiguity_factor: lit::<F>(1.3_f32),
            edge_length_tol: lit::<F>(0.35_f32),
            axis_align_tol_rad: lit::<F>(25.0_f32) * F::pi() / lit::<F>(180.0_f32),
            local_step_fallback: true,
            cardinal_edge_quorum: u8::MAX,
            boundary_search_factor: F::one(),
            global_axis_u_v: None,
        }
    }
}

impl<F: Float> GrowParams<F> {
    /// Construct grow params from the two primary tolerances; the rest take
    /// their defaults.
    pub fn new(attach_search_rel: F, attach_ambiguity_factor: F) -> Self {
        Self {
            attach_search_rel,
            attach_ambiguity_factor,
            ..Self::default()
        }
    }

    /// Builder-style override: supply a global grow-axis pair `[u_angle,
    /// v_angle]` (radians, modulo π) instead of deriving from the seed.
    /// See [`Self::global_axis_u_v`].
    pub fn with_global_axis_u_v(mut self, axes: [F; 2]) -> Self {
        self.global_axis_u_v = Some(axes);
        self
    }

    /// Builder-style override: minimum labelled cardinal neighbours whose
    /// edge length to the candidate must be within band. See
    /// [`Self::cardinal_edge_quorum`].
    pub fn with_cardinal_edge_quorum(mut self, quorum: u8) -> Self {
        self.cardinal_edge_quorum = quorum;
        self
    }

    /// Builder-style override: search-radius multiplier when extrapolating
    /// past the labelled bounding box. See [`Self::boundary_search_factor`].
    pub fn with_boundary_search_factor(mut self, factor: F) -> Self {
        self.boundary_search_factor = factor;
        self
    }
}

/// Outcome of [`bfs_grow`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GrowResult<F: Float> {
    /// `Coord -> feature_index` map, rebased so the bounding-box minimum is
    /// `(0, 0)`.
    pub labelled: HashMap<Coord, usize>,
    /// Mean cell size in pixels used by the engine; copied from the seed.
    pub cell_size: F,
}

/// Grow a labelled `Coord -> feature_index` map from a 2×2 seed via
/// breadth-first axis-aligned predict-and-attach.
///
/// The returned map is rebased so `min.u == 0` and `min.v == 0`.
pub fn bfs_grow<F>(
    features: &[OrientedFeature<F, 2>],
    seed: &SeedSearchOutput<F>,
    params: &GrowParams<F>,
) -> GrowResult<F>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let positions: Vec<Point2<F>> = features.iter().map(|f| f.point.position).collect();
    let cell_size = seed.cell_size;
    let (axis_u, axis_v) = match params.global_axis_u_v {
        Some([u_ang, v_ang]) => (angle_to_unit_vector(u_ang), angle_to_unit_vector(v_ang)),
        None => derive_seed_axes(seed, &positions),
    };

    let tree = build_tree(&positions);

    let mut state = GrowState::with_seed(seed);
    let search_radius = params.attach_search_rel * cell_size;

    while let Some(coord) = state.boundary.pop_front() {
        if state.labelled.contains_key(&coord) {
            continue;
        }
        let attempt = AttemptCtx {
            features,
            positions: &positions,
            params,
            tree: &tree,
            cell_size,
            axis_u,
            axis_v,
            search_radius,
        };
        if let Some(idx) = try_attach(coord, &state, &attempt) {
            state.attach(coord, idx);
        }
    }

    let (rebased, _bbox) = rebase(state.labelled);
    GrowResult {
        labelled: rebased,
        cell_size,
    }
}

// ----------------------------- internals -----------------------------------

struct GrowState {
    labelled: HashMap<Coord, usize>,
    by_feature: HashSet<usize>,
    boundary: VecDeque<Coord>,
    enqueued: HashSet<Coord>,
}

impl GrowState {
    fn with_seed<F: Float>(seed: &SeedSearchOutput<F>) -> Self {
        let mut state = Self {
            labelled: HashMap::new(),
            by_feature: HashSet::new(),
            boundary: VecDeque::new(),
            enqueued: HashSet::new(),
        };
        for (coord, idx) in [
            (Coord::new(0, 0), seed.seed.a),
            (Coord::new(1, 0), seed.seed.b),
            (Coord::new(0, 1), seed.seed.c),
            (Coord::new(1, 1), seed.seed.d),
        ] {
            state.labelled.insert(coord, idx);
            state.by_feature.insert(idx);
        }
        for coord in [
            Coord::new(0, 0),
            Coord::new(1, 0),
            Coord::new(0, 1),
            Coord::new(1, 1),
        ] {
            state.enqueue_cardinal(coord);
        }
        state
    }

    fn enqueue_cardinal(&mut self, coord: Coord) {
        for offset in &SQUARE_CARDINAL_OFFSETS {
            let next = Coord::new(coord.u + offset.u, coord.v + offset.v);
            if self.labelled.contains_key(&next) {
                continue;
            }
            if self.enqueued.insert(next) {
                self.boundary.push_back(next);
            }
        }
    }

    fn attach(&mut self, coord: Coord, idx: usize) {
        self.labelled.insert(coord, idx);
        self.by_feature.insert(idx);
        self.enqueue_cardinal(coord);
    }
}

struct AttemptCtx<'a, F>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    features: &'a [OrientedFeature<F, 2>],
    positions: &'a [Point2<F>],
    params: &'a GrowParams<F>,
    tree: &'a KdTree<F, 2>,
    cell_size: F,
    axis_u: Vector2<F>,
    axis_v: Vector2<F>,
    search_radius: F,
}

#[derive(Clone, Copy, Debug)]
struct LabelledNeighbour<F: Float> {
    coord: Coord,
    position: Point2<F>,
    local_step_u: Option<Vector2<F>>,
    local_step_v: Option<Vector2<F>>,
}

fn try_attach<F>(coord: Coord, state: &GrowState, attempt: &AttemptCtx<'_, F>) -> Option<usize>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let neighbours = collect_labelled_neighbours(coord, state, attempt.positions);
    if neighbours.is_empty() {
        return None;
    }

    let prediction = predict_from_neighbours(coord, &neighbours, attempt)?;

    // Extrapolation widens the candidate search radius when the target
    // sits outside the labelled bbox along at least one axis. Mirrors the
    // legacy `is_extrapolating` + `boundary_search_factor` logic.
    let radius = if is_extrapolating(coord, &neighbours) {
        attempt.search_radius * attempt.params.boundary_search_factor
    } else {
        attempt.search_radius
    };

    let candidates = collect_candidates(
        prediction,
        radius,
        attempt.tree,
        &state.by_feature,
        attempt.positions,
    );
    if candidates.is_empty() {
        return None;
    }

    let candidate = choose_unambiguous(&candidates, attempt.params.attach_ambiguity_factor)?;
    if !candidate_axes_align(candidate.idx, attempt) {
        return None;
    }
    if !cardinal_edges_ok(coord, candidate.idx, state, attempt) {
        return None;
    }

    Some(candidate.idx)
}

/// True when every labelled neighbour sits on the same side of `target`
/// along at least one of the two grid axes — i.e. the target is being
/// extrapolated outward from the labelled set rather than interpolated
/// between two opposing sides.
///
/// Mirrors the legacy `projective_grid::square::grow::is_extrapolating`.
/// Extrapolation accumulates foreshortening error linearly, so the
/// caller widens its search radius via
/// [`GrowParams::boundary_search_factor`] when this fires.
fn is_extrapolating<F: Float>(target: Coord, neighbours: &[LabelledNeighbour<F>]) -> bool {
    let mut has_neg_du = false;
    let mut has_pos_du = false;
    let mut has_neg_dv = false;
    let mut has_pos_dv = false;
    for n in neighbours {
        let du = target.u - n.coord.u;
        let dv = target.v - n.coord.v;
        if du > 0 {
            has_neg_du = true;
        } else if du < 0 {
            has_pos_du = true;
        }
        if dv > 0 {
            has_neg_dv = true;
        } else if dv < 0 {
            has_pos_dv = true;
        }
    }
    !(has_neg_du && has_pos_du && has_neg_dv && has_pos_dv)
}

fn predict_from_neighbours<F>(
    target: Coord,
    neighbours: &[LabelledNeighbour<F>],
    attempt: &AttemptCtx<'_, F>,
) -> Option<Point2<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let global_u_step = attempt.axis_u * attempt.cell_size;
    let global_v_step = attempt.axis_v * attempt.cell_size;

    let mut sum_x = F::zero();
    let mut sum_y = F::zero();
    let mut weight_sum = F::zero();

    for n in neighbours {
        let di = lit::<F>((target.u - n.coord.u) as f32);
        let dj = lit::<F>((target.v - n.coord.v) as f32);
        let d2 = di * di + dj * dj;
        let w = if d2 > F::zero() {
            F::one() / d2
        } else {
            F::one()
        };

        let i_step = match (attempt.params.local_step_fallback, n.local_step_u) {
            (true, Some(step)) => step,
            _ => global_u_step,
        };
        let j_step = match (attempt.params.local_step_fallback, n.local_step_v) {
            (true, Some(step)) => step,
            _ => global_v_step,
        };

        let off = i_step * di + j_step * dj;
        sum_x += w * (n.position.x + off.x);
        sum_y += w * (n.position.y + off.y);
        weight_sum += w;
    }

    if weight_sum == F::zero() {
        return None;
    }

    Some(Point2::new(sum_x / weight_sum, sum_y / weight_sum))
}

fn collect_labelled_neighbours<F>(
    coord: Coord,
    state: &GrowState,
    positions: &[Point2<F>],
) -> Vec<LabelledNeighbour<F>>
where
    F: Float,
{
    let mut out = Vec::new();
    for dj in -1..=1 {
        for di in -1..=1 {
            if di == 0 && dj == 0 {
                continue;
            }
            let at = Coord::new(coord.u + di, coord.v + dj);
            if let Some(&idx) = state.labelled.get(&at) {
                let mut neighbour = LabelledNeighbour {
                    coord: at,
                    position: positions[idx],
                    local_step_u: None,
                    local_step_v: None,
                };
                neighbour.local_step_u = local_step_at(at, Coord::new(1, 0), state, positions);
                neighbour.local_step_v = local_step_at(at, Coord::new(0, 1), state, positions);
                out.push(neighbour);
            }
        }
    }
    out
}

fn local_step_at<F>(
    at: Coord,
    step: Coord,
    state: &GrowState,
    positions: &[Point2<F>],
) -> Option<Vector2<F>>
where
    F: Float,
{
    let here_idx = *state.labelled.get(&at)?;
    let here = positions[here_idx];
    let fwd = Coord::new(at.u + step.u, at.v + step.v);
    let bwd = Coord::new(at.u - step.u, at.v - step.v);
    let fwd_pos = state.labelled.get(&fwd).map(|&i| positions[i]);
    let bwd_pos = state.labelled.get(&bwd).map(|&i| positions[i]);
    match (fwd_pos, bwd_pos) {
        (Some(f), Some(b)) => Some((f - b) * lit::<F>(0.5_f32)),
        (Some(f), None) => Some(f - here),
        (None, Some(b)) => Some(here - b),
        (None, None) => None,
    }
}

#[derive(Clone, Copy, Debug)]
struct Candidate<F: Float> {
    idx: usize,
    distance: F,
}

fn collect_candidates<F>(
    target: Point2<F>,
    radius: F,
    tree: &KdTree<F, 2>,
    excluded: &HashSet<usize>,
    positions: &[Point2<F>],
) -> Vec<Candidate<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let r2 = radius * radius;
    let mut out: Vec<Candidate<F>> = tree
        .within_unsorted::<SquaredEuclidean>(&[target.x, target.y], r2)
        .into_iter()
        .filter_map(|nn| {
            let idx = nn.item as usize;
            if excluded.contains(&idx) {
                return None;
            }
            let _ = positions[idx];
            Some(Candidate {
                idx,
                distance: nn.distance.sqrt(),
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.distance
            .partial_cmp(&b.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

fn choose_unambiguous<F: Float>(
    candidates: &[Candidate<F>],
    ambiguity_factor: F,
) -> Option<Candidate<F>> {
    if candidates.is_empty() {
        return None;
    }
    let first = candidates[0];
    if candidates.len() == 1 {
        return Some(first);
    }
    let second = candidates[1];
    // Multiplicative form so a nearest distance of exactly zero (clean
    // synthetic grid where prediction lands on the feature) doesn't divide.
    if second.distance < ambiguity_factor * first.distance {
        return None;
    }
    Some(first)
}

fn candidate_axes_align<F>(idx: usize, attempt: &AttemptCtx<'_, F>) -> bool
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let axes = &attempt.features[idx].axes;
    let alpha = wrap_pi(axes[0].angle_rad);
    let beta = wrap_pi(axes[1].angle_rad);
    let theta_u = wrap_pi(attempt.axis_u.y.atan2(attempt.axis_u.x));
    let theta_v = wrap_pi(attempt.axis_v.y.atan2(attempt.axis_v.x));
    let tol = attempt.params.axis_align_tol_rad;

    let (alpha_u, alpha_v) = (
        angular_dist_pi(alpha, theta_u),
        angular_dist_pi(alpha, theta_v),
    );
    let (beta_u, beta_v) = (
        angular_dist_pi(beta, theta_u),
        angular_dist_pi(beta, theta_v),
    );
    // Each candidate axis must align with one of the seed axes, and the
    // two candidate axes must pick distinct seed axes (no double-up).
    let alpha_pick = if alpha_u <= tol && alpha_u <= alpha_v {
        Some(0)
    } else if alpha_v <= tol {
        Some(1)
    } else {
        None
    };
    let beta_pick = if beta_u <= tol && beta_u <= beta_v {
        Some(0)
    } else if beta_v <= tol {
        Some(1)
    } else {
        None
    };
    match (alpha_pick, beta_pick) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    }
}

fn cardinal_edges_ok<F>(
    coord: Coord,
    candidate_idx: usize,
    state: &GrowState,
    attempt: &AttemptCtx<'_, F>,
) -> bool
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let to_pos = attempt.positions[candidate_idx];
    let one = F::one();
    let low = one - attempt.params.edge_length_tol;
    let high = one + attempt.params.edge_length_tol;
    let mut found = 0_u32;
    let mut in_band = 0_u32;
    for offset in &SQUARE_CARDINAL_OFFSETS {
        let neighbour = Coord::new(coord.u + offset.u, coord.v + offset.v);
        if let Some(&n_idx) = state.labelled.get(&neighbour) {
            found += 1;
            let from_pos = attempt.positions[n_idx];
            let dx = to_pos.x - from_pos.x;
            let dy = to_pos.y - from_pos.y;
            let length = (dx * dx + dy * dy).sqrt();
            let ratio = length / attempt.cell_size;
            if ratio >= low && ratio <= high {
                in_band += 1;
            }
        }
    }
    if found == 0 {
        // No labelled cardinal neighbour means the candidate was reached
        // through diagonals only; defer (the safety net mirrors the legacy
        // engine).
        return true;
    }
    // `cardinal_edge_quorum` is the minimum number of labelled cardinal
    // neighbours whose edge length must agree with `cell_size`. A quorum
    // of `u8::MAX` (the default) collapses to "every labelled cardinal
    // neighbour must agree" because `found <= 4 < u8::MAX`. A quorum of
    // `1` matches the legacy "any cardinal edge ok" semantics.
    let required = u32::from(attempt.params.cardinal_edge_quorum).min(found);
    in_band >= required
}

fn derive_seed_axes<F: Float>(
    seed: &SeedSearchOutput<F>,
    positions: &[Point2<F>],
) -> (Vector2<F>, Vector2<F>) {
    let eps = lit::<F>(1e-6_f32);
    let a = positions[seed.seed.a];
    let b = positions[seed.seed.b];
    let c = positions[seed.seed.c];
    let raw_u = b - a;
    let raw_v = c - a;
    let nu = raw_u.norm().max(eps);
    let nv = raw_v.norm().max(eps);
    (raw_u / nu, raw_v / nv)
}

/// Convert an undirected axis angle (radians, modulo π) into a unit
/// direction vector. Mirrors how `derive_seed_axes` normalises a chord —
/// the global-axis override path uses caller-supplied angles instead of
/// the seed-derived chords but otherwise feeds the same downstream code.
fn angle_to_unit_vector<F: Float>(angle_rad: F) -> Vector2<F> {
    Vector2::new(angle_rad.cos(), angle_rad.sin())
}

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

fn rebase(labelled: HashMap<Coord, usize>) -> (HashMap<Coord, usize>, (Coord, Coord)) {
    if labelled.is_empty() {
        return (HashMap::new(), (Coord::new(0, 0), Coord::new(0, 0)));
    }
    let (min_u, min_v) = labelled
        .keys()
        .fold((i32::MAX, i32::MAX), |(a, b), c| (a.min(c.u), b.min(c.v)));
    let (max_u, max_v) = labelled
        .keys()
        .fold((i32::MIN, i32::MIN), |(a, b), c| (a.max(c.u), b.max(c.v)));
    let rebased: HashMap<Coord, usize> = labelled
        .into_iter()
        .map(|(c, idx)| (Coord::new(c.u - min_u, c.v - min_v), idx))
        .collect();
    let bbox = (Coord::new(0, 0), Coord::new(max_u - min_u, max_v - min_v));
    (rebased, bbox)
}

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
    use super::*;
    use crate::feature::{LocalAxis, PointFeature};
    use crate::seed::Seed;

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

    fn seed_first_2x2<F>(features: &[OrientedFeature<F, 2>], cols: i32) -> SeedSearchOutput<F>
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let c = cols as usize;
        let a = features[0].point.position;
        let b = features[1].point.position;
        let cell = (b - a).norm();
        SeedSearchOutput::new(Seed::new(0, 1, c, c + 1), cell)
    }

    fn assert_grows_clean_grid<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let s = lit::<F>(20.0_f32);
        let rows = 5_i32;
        let cols = 5_i32;
        let features = axis_aligned_features::<F>(rows, cols, s);
        let seed = seed_first_2x2(&features, cols);
        let result = bfs_grow(&features, &seed, &GrowParams::<F>::default());
        assert_eq!(result.labelled.len(), (rows * cols) as usize);
        let bbox_min = result
            .labelled
            .keys()
            .fold((i32::MAX, i32::MAX), |(u, v), c| (u.min(c.u), v.min(c.v)));
        let bbox_max = result
            .labelled
            .keys()
            .fold((i32::MIN, i32::MIN), |(u, v), c| (u.max(c.u), v.max(c.v)));
        assert_eq!(bbox_min, (0, 0));
        assert_eq!(bbox_max, (cols - 1, rows - 1));
        let (mi, mj) = result
            .labelled
            .keys()
            .fold((i32::MAX, i32::MAX), |(a, b), c| (a.min(c.u), b.min(c.v)));
        assert_eq!((mi, mj), (0, 0));
    }

    fn assert_rebases_origin_when_seed_off_zero<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let s = lit::<F>(20.0_f32);
        let rows = 4_i32;
        let cols = 4_i32;
        let features = axis_aligned_features::<F>(rows, cols, s);
        let cols_u = cols as usize;
        let a = 2 + 2 * cols_u;
        let b = 3 + 2 * cols_u;
        let c = 2 + 3 * cols_u;
        let d = 3 + 3 * cols_u;
        let cell = (features[b].point.position - features[a].point.position).norm();
        let seed = SeedSearchOutput::new(Seed::new(a, b, c, d), cell);
        let result = bfs_grow(&features, &seed, &GrowParams::<F>::default());
        assert_eq!(result.labelled.len(), (rows * cols) as usize);
        let (mi, mj) = result
            .labelled
            .keys()
            .fold((i32::MAX, i32::MAX), |(a, b), c| (a.min(c.u), b.min(c.v)));
        assert_eq!((mi, mj), (0, 0));
    }

    #[test]
    fn grows_clean_grid_f32() {
        assert_grows_clean_grid::<f32>();
    }

    #[test]
    fn grows_clean_grid_f64() {
        assert_grows_clean_grid::<f64>();
    }

    #[test]
    fn rebases_origin_when_seed_off_zero_f32() {
        assert_rebases_origin_when_seed_off_zero::<f32>();
    }

    #[test]
    fn rebases_origin_when_seed_off_zero_f64() {
        assert_rebases_origin_when_seed_off_zero::<f64>();
    }
}
