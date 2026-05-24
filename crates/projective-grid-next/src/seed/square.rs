//! Square seed-quad finder: KD-tree + edge-ratio gate + midpoint gate.
//!
//! The detector's first non-local commitment is a *seed quad*: four observations
//! at grid cells `(0, 0), (1, 0), (0, 1), (1, 1)` whose edge lengths match
//! within tolerance, whose chord angles align with corner `axes`, and whose
//! geometry is internally consistent (no intermediate observation sits between
//! two seed corners — that would mean the quad has skipped a row / column of
//! the true grid).
//!
//! Port of `square::seed::finder::find_quad` from the legacy crate, with two
//! structural changes:
//!
//! * **Float-generic** throughout. The legacy finder hardcoded `f32`.
//! * **Pattern policy through [`SeedQuadContext`]**. The legacy
//!   `SeedQuadValidator` trait mixed eligibility (which corners can play A vs
//!   B/C) with per-edge invariants. The new context routes eligibility through
//!   [`LabelPolicy`] (see `ctx.label_policy()`),
//!   keeps `axes_at` for chord classification, and provides a single
//!   `validate_seed` hook so pattern-aware consumers (chessboard parity,
//!   midpoint-violation rules) can reject candidates while keeping the
//!   default behaviour permissive.
//!
//! ## Algorithm
//!
//! 1. Partition eligible observations into A-class (typically the densest /
//!    most-confident set; this is the role chessboard's "Canonical" cluster
//!    plays) and B/C-class. By default — when no `LabelPolicy` parity rule is
//!    set — every eligible observation is in both classes; chessboard
//!    consumers wire the split via [`LabelPolicy::label_of`] (parity 0 → A,
//!    parity 1 → B/C).
//! 2. For each `A`: KD-tree-search the BC set for the `k_bc` nearest
//!    neighbours, classify each by chord-direction against `axes[0]` /
//!    `axes[1]`, and enumerate `(B, C)` pairs among the shortest few in each
//!    list.
//! 3. Predict `D = A + (B − A) + (C − A)` (parallelogram completion), KD-search
//!    the A-class for the nearest hit within a fraction of the seed's mean
//!    edge length, and verify all four edges agree pairwise within the ratio
//!    tolerance.
//! 4. Run `ctx.validate_seed(seed)` to apply pattern-specific midpoint /
//!    parallelogram-center checks.
//! 5. Return the first quad passing every gate.

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};

use crate::diagnostics::{DiagnosticSink, Event};
use crate::feature::{AxisEstimate, Observation};
use crate::float::Float;
use crate::lattice::Coord;
use crate::policy::LabelPolicy;
use crate::stats::{angular_dist_pi, wrap_pi};

/// Grid positions of the four seed corners in the "canonical" seed quad
/// layout. The grow engine assumes this layout.
pub const SEED_QUAD_GRID: [Coord; 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];

/// A 2×2 seed quad: observation indices at grid cells `(0, 0), (1, 0),
/// `(0, 1), (1, 1)` respectively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Seed {
    /// Index at grid cell `(0, 0)`.
    pub a: usize,
    /// Index at grid cell `(1, 0)`.
    pub b: usize,
    /// Index at grid cell `(0, 1)`.
    pub c: usize,
    /// Index at grid cell `(1, 1)`.
    pub d: usize,
}

impl Seed {
    /// Construct a seed quad from four observation indices in the canonical
    /// `(0, 0), (1, 0), (0, 1), (1, 1)` cell layout.
    pub const fn new(a: usize, b: usize, c: usize, d: usize) -> Self {
        Self { a, b, c, d }
    }

    /// Return the four indices in the canonical layout order.
    pub const fn as_array(&self) -> [usize; 4] {
        [self.a, self.b, self.c, self.d]
    }
}

/// Bundles a seed quad with the cell-size estimate derived from its own edges.
///
/// `cell_size` is the **self-consistent** mean of the four seed-edge lengths
/// — the only cell-size estimate that is by construction compatible with the
/// rest of the seed. See CLAUDE.md "Cell-size estimation gotcha".
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct SeedOutput<F: Float> {
    /// The 2×2 quad.
    pub seed: Seed,
    /// Cell size in pixels, mean of the seed's four edge lengths.
    pub cell_size: F,
}

impl<F: Float> SeedOutput<F> {
    /// Construct a seed-output pair from a quad and its derived cell size.
    pub const fn new(seed: Seed, cell_size: F) -> Self {
        Self { seed, cell_size }
    }
}

/// `SeedQuadContext`: pattern-aware hooks consulted by [`find_quad`].
///
/// Default impls keep the context permissive — `axes_at` returns `None` (then
/// chord classification falls back to the next neighbour), `validate_seed`
/// returns `true`. Pattern-aware consumers (chessboard parity, midpoint
/// violation rules) override the relevant defaults; eligibility and parity
/// flow through `ctx.label_policy()`.
pub trait SeedQuadContext<F: Float> {
    /// The active [`LabelPolicy`]. Drives eligibility and (optionally) parity
    /// pre-classification into A vs B/C candidates.
    fn label_policy(&self) -> &LabelPolicy<F>;

    /// The two undirected grid-axis estimates at observation `idx`. Returning
    /// `None` means "axes unknown" — the finder still considers the
    /// observation but cannot use it as an A candidate (axes drive chord
    /// classification).
    #[allow(unused_variables)]
    fn axes_at(&self, idx: usize) -> Option<[AxisEstimate<F>; 2]> {
        None
    }

    /// Pattern-specific veto. Called after every geometric gate has passed.
    /// Default accepts every seed; chessboard impls plug in the midpoint /
    /// parallelogram-center violation check.
    #[allow(unused_variables)]
    fn validate_seed(&self, seed: &SeedQuad<F>) -> bool {
        true
    }
}

/// Bundles the per-seed evidence passed to [`SeedQuadContext::validate_seed`].
///
/// Validators inspect the four corners' positions and the derived cell size to
/// implement pattern-specific rules (e.g. chessboard's midpoint-violation
/// check that flags a 2× spacing seed).
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct SeedQuad<F: Float> {
    /// Indices of the four corners in canonical `(0,0)/(1,0)/(0,1)/(1,1)` order.
    pub corners: [usize; 4],
    /// Positions of the four corners.
    pub positions: [Point2<F>; 4],
    /// Mean of the four seed-edge lengths in pixels.
    pub cell_size: F,
}

/// Tuning knobs for [`find_quad`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct SeedQuadParams<F: Float> {
    /// Angular tolerance (radians) for classifying a chord against the
    /// corner's `axes[0]` vs `axes[1]`. Chord directions outside the
    /// tolerance from both axes are dropped.
    pub axis_tol_rad: F,
    /// Edge-length ratio tolerance: `min_edge / max_edge >= 1 - edge_ratio_tol`
    /// for all four seed edges.
    pub edge_ratio_tol: F,
    /// Search radius for the `D` corner around the parallelogram prediction,
    /// expressed as a fraction of the seed's mean `(|AB| + |AC|) / 2`.
    pub close_tol_rel: F,
    /// `K` in the KD-tree query for B/C neighbours of each A candidate.
    pub k_bc: usize,
    /// Per-axis cap on enumerated B / C candidates when running the inner
    /// pair search.
    pub top_per_axis: usize,
}

impl<F: Float> Default for SeedQuadParams<F> {
    fn default() -> Self {
        Self {
            // 15° expressed via radian conversion of an f32 literal.
            axis_tol_rad: crate::float::lit::<F>(15.0_f32) * F::pi()
                / crate::float::lit::<F>(180.0_f32),
            edge_ratio_tol: crate::float::lit::<F>(0.30_f32),
            close_tol_rel: crate::float::lit::<F>(0.30_f32),
            k_bc: 32,
            top_per_axis: 6,
        }
    }
}

impl<F: Float> SeedQuadParams<F> {
    /// Construct fully-specified params from the three caller-tunable values.
    /// Other fields take their defaults.
    pub fn new(axis_tol_rad: F, edge_ratio_tol: F, close_tol_rel: F) -> Self {
        Self {
            axis_tol_rad,
            edge_ratio_tol,
            close_tol_rel,
            ..Self::default()
        }
    }

    /// Set the `K` in the KD-tree query for B/C neighbours.
    #[must_use]
    pub fn with_k_bc(mut self, k_bc: usize) -> Self {
        self.k_bc = k_bc;
        self
    }

    /// Set the per-axis cap on enumerated B / C candidates.
    #[must_use]
    pub fn with_top_per_axis(mut self, top_per_axis: usize) -> Self {
        self.top_per_axis = top_per_axis;
        self
    }
}

/// Run the square seed-quad finder.
///
/// Returns the first quad — `(A, B, C, D)` plus mean edge length as
/// `cell_size` — that passes every pattern-agnostic geometric check AND
/// `ctx.validate_seed`. Returns `None` when no quad satisfies all constraints.
///
/// Emits `Event::SeedFound` on success and `Event::SeedRejected` on common
/// failure modes (`no_a_candidates`, `no_bc_candidates`, `no_quad`).
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_observations = observations.len()),
    )
)]
pub fn find_quad<F, C>(
    observations: &[Observation<F>],
    params: &SeedQuadParams<F>,
    ctx: &C,
    sink: &mut impl DiagnosticSink<F>,
) -> Option<SeedOutput<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SeedQuadContext<F> + ?Sized,
{
    let (a_indices, bc_indices) = classify_candidates(observations, ctx);
    if a_indices.is_empty() {
        sink.emit(Event::SeedRejected {
            reason: "no_a_candidates",
        });
        return None;
    }
    if bc_indices.is_empty() {
        sink.emit(Event::SeedRejected {
            reason: "no_bc_candidates",
        });
        return None;
    }

    let ratio_floor = ratio_floor(params.edge_ratio_tol);
    let a_tree = build_tree(observations, &a_indices);
    let bc_tree = build_tree(observations, &bc_indices);

    let search = SeedSearchCtx {
        observations,
        params,
        a_indices: &a_indices,
        bc_indices: &bc_indices,
        a_tree: &a_tree,
        bc_tree: &bc_tree,
        ratio_floor,
        ctx,
    };

    for &a_idx in &a_indices {
        if let Some(out) = try_seed_from_a(a_idx, &search) {
            sink.emit(Event::SeedFound {
                corners: out.seed.as_array(),
                cell_size: out.cell_size,
            });
            return Some(out);
        }
    }

    sink.emit(Event::SeedRejected { reason: "no_quad" });
    None
}

// ---- Internal helpers ----

fn ratio_floor<F: Float>(edge_ratio_tol: F) -> F {
    let one = F::one();
    let min_ratio = one - edge_ratio_tol;
    let max_ratio = one + edge_ratio_tol;
    min_ratio / max_ratio
}

fn classify_candidates<F, C>(observations: &[Observation<F>], ctx: &C) -> (Vec<usize>, Vec<usize>)
where
    F: Float,
    C: SeedQuadContext<F> + ?Sized,
{
    use crate::policy::ParityRule;
    let policy = ctx.label_policy();
    let use_parity = !matches!(policy.parity_rule(), ParityRule::None);
    let mut a = Vec::new();
    let mut bc = Vec::new();
    for idx in 0..observations.len() {
        if !policy.is_eligible(idx) {
            continue;
        }
        if use_parity {
            match policy.label_of(idx) {
                Some(tag) if tag.parity_bit() == 0 => a.push(idx),
                Some(_) => bc.push(idx),
                None => {
                    // Untagged observations under a parity rule are
                    // tag-agnostic per `LabelPolicy::agrees` semantics; treat
                    // them as eligible in both roles to stay permissive.
                    a.push(idx);
                    bc.push(idx);
                }
            }
        } else {
            a.push(idx);
            bc.push(idx);
        }
    }
    (a, bc)
}

fn build_tree<F>(observations: &[Observation<F>], indices: &[usize]) -> KdTree<F, 2>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let mut tree: KdTree<F, 2> = KdTree::new();
    for (slot, &idx) in indices.iter().enumerate() {
        let p = observations[idx].position;
        tree.add(&[p.x, p.y], slot as u64);
    }
    tree
}

/// Shared references threaded through the per-A loop. Bundled to keep
/// function signatures under the workspace's `too_many_arguments = "deny"`
/// clippy lint.
struct SeedSearchCtx<'a, F, C>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SeedQuadContext<F> + ?Sized,
{
    observations: &'a [Observation<F>],
    params: &'a SeedQuadParams<F>,
    a_indices: &'a [usize],
    bc_indices: &'a [usize],
    a_tree: &'a KdTree<F, 2>,
    bc_tree: &'a KdTree<F, 2>,
    ratio_floor: F,
    ctx: &'a C,
}

type NeighbourRow<F> = (usize, F, Vector2<F>);

fn try_seed_from_a<F, C>(a_idx: usize, search: &SeedSearchCtx<'_, F, C>) -> Option<SeedOutput<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SeedQuadContext<F> + ?Sized,
{
    let a_axes = search.ctx.axes_at(a_idx)?;
    let a_pos = search.observations[a_idx].position;
    let a_axis0 = wrap_pi(a_axes[0].angle);
    let a_axis1 = wrap_pi(a_axes[1].angle);

    let neighbours = collect_bc_neighbours(a_idx, a_pos, search);
    if neighbours.len() < 2 {
        return None;
    }

    let (b_cands, c_cands) =
        split_neighbours_by_axis(&neighbours, a_axis0, a_axis1, search.params.axis_tol_rad);
    if b_cands.is_empty() || c_cands.is_empty() {
        return None;
    }

    for (b_idx, b_dist, b_off) in b_cands.iter().take(search.params.top_per_axis) {
        for (c_idx, c_dist, c_off) in c_cands.iter().take(search.params.top_per_axis) {
            if b_idx == c_idx {
                continue;
            }
            if let Some(out) = try_complete_quad(
                a_idx, a_pos, *b_idx, *b_dist, *b_off, *c_idx, *c_dist, *c_off, search,
            ) {
                return Some(out);
            }
        }
    }
    None
}

fn collect_bc_neighbours<F, C>(
    a_idx: usize,
    a_pos: Point2<F>,
    search: &SeedSearchCtx<'_, F, C>,
) -> Vec<NeighbourRow<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SeedQuadContext<F> + ?Sized,
{
    let eps = crate::float::lit::<F>(1e-3_f32);
    let mut neighbours: Vec<NeighbourRow<F>> = search
        .bc_tree
        .nearest_n::<SquaredEuclidean>(&[a_pos.x, a_pos.y], search.params.k_bc)
        .into_iter()
        .filter_map(|nn| {
            let slot = nn.item as usize;
            let idx = search.bc_indices[slot];
            if idx == a_idx {
                return None;
            }
            let p = search.observations[idx].position;
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

#[allow(clippy::too_many_arguments)]
fn try_complete_quad<F, C>(
    a_idx: usize,
    a_pos: Point2<F>,
    b_idx: usize,
    b_dist: F,
    b_off: Vector2<F>,
    c_idx: usize,
    c_dist: F,
    c_off: Vector2<F>,
    search: &SeedSearchCtx<'_, F, C>,
) -> Option<SeedOutput<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SeedQuadContext<F> + ?Sized,
{
    let half = crate::float::lit::<F>(0.5_f32);
    let (bc_lo, bc_hi) = if b_dist <= c_dist {
        (b_dist, c_dist)
    } else {
        (c_dist, b_dist)
    };
    if bc_lo / bc_hi < search.ratio_floor {
        return None;
    }

    let pred = a_pos + b_off + c_off;
    let avg_edge = (b_dist + c_dist) * half;
    let close_px = search.params.close_tol_rel * avg_edge;
    let close_px_sq = close_px * close_px;

    let d_idx = find_d_candidate(a_idx, pred, close_px_sq, search)?;
    let d_pos = search.observations[d_idx].position;

    let bd = (d_pos - search.observations[b_idx].position).norm();
    let cd = (d_pos - search.observations[c_idx].position).norm();
    let all = [b_dist, c_dist, bd, cd];
    let (emin, emax) = min_max(&all)?;
    if emax <= F::zero() || emin / emax < search.ratio_floor {
        return None;
    }

    let quarter = crate::float::lit::<F>(0.25_f32);
    let cell_size = (b_dist + c_dist + bd + cd) * quarter;
    let seed = Seed::new(a_idx, b_idx, c_idx, d_idx);

    let positions = [
        a_pos,
        search.observations[b_idx].position,
        search.observations[c_idx].position,
        d_pos,
    ];
    let evidence = SeedQuad {
        corners: seed.as_array(),
        positions,
        cell_size,
    };
    if !search.ctx.validate_seed(&evidence) {
        return None;
    }

    Some(SeedOutput::new(seed, cell_size))
}

fn find_d_candidate<F, C>(
    a_idx: usize,
    pred: Point2<F>,
    close_px_sq: F,
    search: &SeedSearchCtx<'_, F, C>,
) -> Option<usize>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SeedQuadContext<F> + ?Sized,
{
    let mut best: Option<(usize, F)> = None;
    for nn in search
        .a_tree
        .within_unsorted::<SquaredEuclidean>(&[pred.x, pred.y], close_px_sq)
    {
        let slot = nn.item as usize;
        let d_idx = search.a_indices[slot];
        if d_idx == a_idx {
            continue;
        }
        let d2 = nn.distance;
        if best.map(|b| d2 < b.1).unwrap_or(true) {
            best = Some((d_idx, d2));
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

// ---- Midpoint-violation helper (pattern-aware utility) ----

/// Bundles every parameter consumed by [`seed_has_midpoint_violation`].
///
/// Keeping the parameters in a context struct stays under the workspace's
/// `too_many_arguments = "deny"` lint and reads better at chessboard call
/// sites that pass several index lists.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct MidpointCtx<'a, F: Float> {
    /// Every observation's pixel position.
    pub positions: &'a [Point2<F>],
    /// The four corner indices of the candidate seed.
    pub seed_quad: [usize; 4],
    /// Cell-size estimate (mean of the seed's four edge lengths in pixels).
    pub cell_size: F,
    /// Tolerance as a fraction of `cell_size`. Pattern-specific lists are
    /// matched against `tol = midpoint_tol_rel * cell_size`; the
    /// `all_positions` fallback uses half that tolerance to stay precise.
    pub midpoint_tol_rel: F,
    /// Indices the consumer expects to fall near edge midpoints when the
    /// seed has skipped a row of the true grid. For chessboard: the
    /// "Swapped" cluster.
    pub on_edge_midpoint: &'a [usize],
    /// Indices the consumer expects to fall near the parallelogram centre
    /// when the seed skips two rows. For chessboard: the "Canonical" cluster.
    pub on_parallelogram_center: &'a [usize],
    /// Full-population fallback indices tested against both midpoints and
    /// centre with a tighter tolerance. Pass an empty slice to disable.
    pub all_positions: &'a [usize],
}

/// Detect the 2× spacing seed mislabel where the candidate quad spans two
/// cells of the true grid.
///
/// Returns `true` when any of the seed's four edge midpoints or the quad
/// centre coincides with a real corner *other than the seed itself*. The
/// pattern-aware lists (`on_edge_midpoint`, `on_parallelogram_center`) use the
/// nominal tolerance; the `all_positions` fallback uses half that tolerance
/// because it admits marker-internal corners that may legitimately sit near a
/// cell midpoint.
pub fn seed_has_midpoint_violation<F: Float>(ctx: MidpointCtx<'_, F>) -> bool {
    let half = crate::float::lit::<F>(0.5_f32);
    let tol = ctx.midpoint_tol_rel * ctx.cell_size;
    let tol_sq = tol * tol;
    let fallback_tol = tol * half;
    let fallback_tol_sq = fallback_tol * fallback_tol;

    let [a, b, c, d] = ctx.seed_quad;
    let pa = ctx.positions[a];
    let pb = ctx.positions[b];
    let pc = ctx.positions[c];
    let pd = ctx.positions[d];

    let midpoints = [
        Point2::from((pa.coords + pb.coords) * half),
        Point2::from((pa.coords + pc.coords) * half),
        Point2::from((pb.coords + pd.coords) * half),
        Point2::from((pc.coords + pd.coords) * half),
    ];

    for mp in midpoints {
        if any_within(
            ctx.positions,
            ctx.on_edge_midpoint,
            mp,
            tol_sq,
            &ctx.seed_quad,
        ) {
            return true;
        }
        if any_within(
            ctx.positions,
            ctx.all_positions,
            mp,
            fallback_tol_sq,
            &ctx.seed_quad,
        ) {
            return true;
        }
    }

    let centre = Point2::from((pa.coords + pd.coords) * half);
    if any_within(
        ctx.positions,
        ctx.on_parallelogram_center,
        centre,
        tol_sq,
        &ctx.seed_quad,
    ) {
        return true;
    }
    if any_within(
        ctx.positions,
        ctx.all_positions,
        centre,
        fallback_tol_sq,
        &ctx.seed_quad,
    ) {
        return true;
    }
    false
}

fn any_within<F: Float>(
    positions: &[Point2<F>],
    candidates: &[usize],
    target: Point2<F>,
    tol_sq: F,
    exclude: &[usize],
) -> bool {
    for &idx in candidates {
        if exclude.contains(&idx) {
            continue;
        }
        let p = positions[idx];
        let dx = p.x - target.x;
        let dy = p.y - target.y;
        if dx * dx + dy * dy <= tol_sq {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{NoOpSink, RecordingSink};
    use crate::float::lit;

    /// `SeedQuadContext` impl over an axis-aligned grid: every observation
    /// eligible, axes always `(0, π/2)`, no veto.
    struct AxisAlignedCtx<F: Float> {
        policy: LabelPolicy<F>,
    }

    impl<F: Float> AxisAlignedCtx<F> {
        fn new(n: usize) -> Self {
            Self {
                policy: LabelPolicy::builder(n).build(),
            }
        }
    }

    impl<F: Float> SeedQuadContext<F> for AxisAlignedCtx<F> {
        fn label_policy(&self) -> &LabelPolicy<F> {
            &self.policy
        }
        fn axes_at(&self, _idx: usize) -> Option<[AxisEstimate<F>; 2]> {
            Some([
                AxisEstimate::from_angle(F::zero()),
                AxisEstimate::from_angle(F::frac_pi_2()),
            ])
        }
    }

    fn axis_aligned_grid<F: Float>(rows: i32, cols: i32, s: F) -> Vec<Observation<F>> {
        let mut out = Vec::with_capacity((rows * cols) as usize);
        let origin = lit::<F>(50.0_f32);
        for j in 0..rows {
            for i in 0..cols {
                let x = lit::<F>(i as f32) * s + origin;
                let y = lit::<F>(j as f32) * s + origin;
                out.push(Observation::new(Point2::new(x, y)));
            }
        }
        out
    }

    fn assert_finds_quad_on_clean_grid<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let s = lit::<F>(20.0_f32);
        let obs = axis_aligned_grid::<F>(5, 5, s);
        let ctx = AxisAlignedCtx::<F>::new(obs.len());
        let mut sink = NoOpSink;
        let out = find_quad(&obs, &SeedQuadParams::<F>::default(), &ctx, &mut sink)
            .expect("seed quad on clean grid");
        let rel_diff = (out.cell_size - s) / s;
        assert!(crate::float::abs(rel_diff) < lit::<F>(0.05_f32));
    }

    fn assert_emits_seed_found_event<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let s = lit::<F>(20.0_f32);
        let obs = axis_aligned_grid::<F>(5, 5, s);
        let ctx = AxisAlignedCtx::<F>::new(obs.len());
        let mut sink = RecordingSink::<F>::new();
        find_quad(&obs, &SeedQuadParams::<F>::default(), &ctx, &mut sink).unwrap();
        assert!(sink
            .events()
            .iter()
            .any(|e| matches!(e, Event::SeedFound { .. })));
    }

    fn assert_emits_seed_rejected_when_empty<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        // Empty observation set yields a SeedRejected event from the
        // a_indices.is_empty() early return.
        let obs: Vec<Observation<F>> = Vec::new();
        let ctx = AxisAlignedCtx::<F>::new(0);
        let mut sink = RecordingSink::<F>::new();
        let result = find_quad(&obs, &SeedQuadParams::<F>::default(), &ctx, &mut sink);
        assert!(result.is_none());
        assert!(sink.events().iter().any(|e| matches!(
            e,
            Event::SeedRejected {
                reason: "no_a_candidates"
            }
        )));
    }

    fn assert_validator_can_reject<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        struct AlwaysRejectCtx<F: Float> {
            policy: LabelPolicy<F>,
        }
        impl<F: Float> SeedQuadContext<F> for AlwaysRejectCtx<F> {
            fn label_policy(&self) -> &LabelPolicy<F> {
                &self.policy
            }
            fn axes_at(&self, _idx: usize) -> Option<[AxisEstimate<F>; 2]> {
                Some([
                    AxisEstimate::from_angle(F::zero()),
                    AxisEstimate::from_angle(F::frac_pi_2()),
                ])
            }
            fn validate_seed(&self, _: &SeedQuad<F>) -> bool {
                false
            }
        }
        let s = lit::<F>(20.0_f32);
        let obs = axis_aligned_grid::<F>(5, 5, s);
        let ctx = AlwaysRejectCtx {
            policy: LabelPolicy::builder(obs.len()).build(),
        };
        let mut sink = NoOpSink;
        let out = find_quad(&obs, &SeedQuadParams::<F>::default(), &ctx, &mut sink);
        assert!(out.is_none());
    }

    fn assert_midpoint_violation_2x_mislabel<F: Float>() {
        // Seed at (0,0)/(20,0)/(0,20)/(20,20) with cell_size=20; an
        // intermediate corner at (10, 0) should trigger the midpoint check.
        let positions = vec![
            Point2::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(20.0_f32), F::zero()),
            Point2::new(F::zero(), lit::<F>(20.0_f32)),
            Point2::new(lit::<F>(20.0_f32), lit::<F>(20.0_f32)),
            Point2::new(lit::<F>(10.0_f32), F::zero()),
        ];
        let on_edge: Vec<usize> = vec![4];
        let mp_ctx = MidpointCtx::<F> {
            positions: &positions,
            seed_quad: [0, 1, 2, 3],
            cell_size: lit::<F>(20.0_f32),
            midpoint_tol_rel: lit::<F>(0.3_f32),
            on_edge_midpoint: &on_edge,
            on_parallelogram_center: &[],
            all_positions: &[],
        };
        assert!(seed_has_midpoint_violation(mp_ctx));
    }

    fn assert_midpoint_violation_absent_on_clean<F: Float>() {
        let positions = vec![
            Point2::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(10.0_f32), F::zero()),
            Point2::new(F::zero(), lit::<F>(10.0_f32)),
            Point2::new(lit::<F>(10.0_f32), lit::<F>(10.0_f32)),
        ];
        let mp_ctx = MidpointCtx::<F> {
            positions: &positions,
            seed_quad: [0, 1, 2, 3],
            cell_size: lit::<F>(10.0_f32),
            midpoint_tol_rel: lit::<F>(0.3_f32),
            on_edge_midpoint: &[],
            on_parallelogram_center: &[],
            all_positions: &[],
        };
        assert!(!seed_has_midpoint_violation(mp_ctx));
    }

    fn assert_midpoint_violation_via_fallback<F: Float>() {
        // Intermediate corner is in `all_positions` only.
        let positions = vec![
            Point2::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(20.0_f32), F::zero()),
            Point2::new(F::zero(), lit::<F>(20.0_f32)),
            Point2::new(lit::<F>(20.0_f32), lit::<F>(20.0_f32)),
            Point2::new(lit::<F>(10.0_f32), F::zero()),
        ];
        let all: Vec<usize> = vec![0, 1, 2, 3, 4];
        let mp_ctx = MidpointCtx::<F> {
            positions: &positions,
            seed_quad: [0, 1, 2, 3],
            cell_size: lit::<F>(20.0_f32),
            midpoint_tol_rel: lit::<F>(0.3_f32),
            on_edge_midpoint: &[],
            on_parallelogram_center: &[],
            all_positions: &all,
        };
        assert!(seed_has_midpoint_violation(mp_ctx));
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
    fn emits_seed_found_event_f32() {
        assert_emits_seed_found_event::<f32>();
    }
    #[test]
    fn emits_seed_found_event_f64() {
        assert_emits_seed_found_event::<f64>();
    }
    #[test]
    fn emits_seed_rejected_when_empty_f32() {
        assert_emits_seed_rejected_when_empty::<f32>();
    }
    #[test]
    fn emits_seed_rejected_when_empty_f64() {
        assert_emits_seed_rejected_when_empty::<f64>();
    }
    #[test]
    fn validator_can_reject_f32() {
        assert_validator_can_reject::<f32>();
    }
    #[test]
    fn validator_can_reject_f64() {
        assert_validator_can_reject::<f64>();
    }
    #[test]
    fn midpoint_violation_2x_mislabel_f32() {
        assert_midpoint_violation_2x_mislabel::<f32>();
    }
    #[test]
    fn midpoint_violation_2x_mislabel_f64() {
        assert_midpoint_violation_2x_mislabel::<f64>();
    }
    #[test]
    fn midpoint_violation_absent_on_clean_f32() {
        assert_midpoint_violation_absent_on_clean::<f32>();
    }
    #[test]
    fn midpoint_violation_absent_on_clean_f64() {
        assert_midpoint_violation_absent_on_clean::<f64>();
    }
    #[test]
    fn midpoint_violation_via_fallback_f32() {
        assert_midpoint_violation_via_fallback::<f32>();
    }
    #[test]
    fn midpoint_violation_via_fallback_f64() {
        assert_midpoint_violation_via_fallback::<f64>();
    }
}
