//! Geometry-only post-convergence recovery schedule (advanced tier).
//!
//! After a component-assembly pass has produced one self-consistent labelled
//! component, recall on a foreshortened / partially-occluded grid is still
//! bounded by how far the growth frontier reached before the per-edge band or
//! the synthesized-axis voucher stalled it. This module composes the
//! boundary-extension and interior-fill engines, interleaved with revalidation
//! and the lattice-general drop filters, into a single fixed-point schedule
//! that pushes recall up to the dense-recovery level the topological walk
//! reaches — **without** any target-specific vocabulary.
//!
//! # Stage order
//!
//! The schedule mirrors the *geometry-only* subset of the chessboard
//! detector's `run_converged_iteration` sequence (extension → fill → final
//! geometry check), dropping the ChESS-coupled stages (slot-flip fix, cluster
//! refit, NoCluster rescue) that have no meaning for a generic
//! [`SquareAttachPolicy`]:
//!
//! 1. **boundary extension** — [`extend_via_local_homography`] fits a
//!    per-candidate local homography from the K nearest labelled corners and
//!    projects integer cells past the labelled boundary. Local-H tracks
//!    perspective foreshortening where one global H cannot, so it is the
//!    workhorse for the orientation-free perspective case. Followed by
//!    [`extend_from_labelled`] (cardinal-BFS extension) which mops up cells the
//!    local-H residual gate refused but a single-edge prediction can still
//!    reach.
//! 2. **interior fill** — [`fill_grid_holes`] enumerates still-empty cells
//!    inside the labelled bounding box (plus a one-cell skirt) and attaches a
//!    candidate at each using the same per-cell ladder as BFS grow.
//! 3. **revalidation** — the shared [`validate`](crate::shared::validate::validate)
//!    pass (line collinearity + local-H residual) drops any corner the
//!    extension / fill attached that does not cohere with its neighbourhood.
//! 4. **drop filters** — the lattice-general filters in
//!    [`crate::shared::validate::recovery`]: the topological wrong-label drops
//!    (overlong / off-axis / duplicate-pixel edges), then the
//!    largest-cardinally-connected-component filter (a square detection is one
//!    connected planar graph; any stranded sub-component is a false positive).
//!
//! The whole sequence repeats until a full pass attaches zero new corners (a
//! fixed point) or the iteration cap is reached. On a clean grid that the loop
//! already recovered fully, the first extension pass attaches nothing and the
//! schedule returns immediately.
//!
//! # Precision contract
//!
//! Every attachment runs through the same [`SquareAttachPolicy`] gates as BFS
//! grow (`is_eligible`, `required_label_at` / `label_of`, `accept_candidate`,
//! `edge_ok`) plus the extension residual gate, then through revalidation and
//! the drop filters. A corner whose geometry does not cohere is *dropped*, not
//! mislabelled. The schedule can therefore only ever *raise* recall toward the
//! true grid or *shrink* a component it cannot justify — it can never introduce
//! a wrong `(i, j)` label that the gates would not have caught on the BFS path.
//!
//! # Gating
//!
//! The schedule is opt-in via [`RecoverySchedule`] on the caller's params; the
//! facade enables it for the orientation-free / position paths, while the
//! chessboard topological adapter (which disables the facade validate/fit and
//! runs its own `CornerStage`-coupled recovery) leaves it off so its production
//! output stays byte-identical.

use std::collections::{HashMap, HashSet};

use nalgebra::{Point2, Vector2};

use crate::shared::extension::{extend_via_local_homography, LocalExtensionParams};
use crate::shared::fill::{fill_grid_holes, FillParams};
use crate::shared::grow::{GrowParams, GrowResult, SquareAttachPolicy};
use crate::shared::grow_extend::extend_from_labelled;
use crate::shared::validate::recovery::{largest_component_filter, topological_wrong_label_drops};
use crate::shared::validate::{self as pg_validate, ValidationParams};

/// Tuning for the geometry-only recovery schedule.
///
/// Defaults are conservative: a single fixed-point sweep of extension + fill
/// with the engines' own defaults, followed by revalidation and the
/// component / wrong-label drop filters. Raise [`max_sweeps`](Self::max_sweeps)
/// to let a strongly foreshortened grid propagate further outward.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct RecoveryParams {
    /// Local-homography boundary extension knobs.
    pub local_extension: LocalExtensionParams,
    /// Cardinal-BFS boundary extension knobs (the mop-up pass after local-H).
    pub bfs_extension: GrowParams,
    /// Interior-fill knobs.
    pub fill: FillParams,
    /// Maximum number of (extend → fill → validate → drop) sweeps. Each sweep
    /// is idempotent on a converged grid, so the schedule stops early on the
    /// first zero-attachment sweep.
    pub max_sweeps: u32,
    /// Whether to apply the topological wrong-label drop filter (overlong /
    /// off-axis / duplicate-pixel edges) after revalidation. The orientation-
    /// free path enables it; it is the strongest guard against a synthesized-
    /// axis mislabel slipping through the per-edge band.
    pub apply_wrong_label_drops: bool,
    /// Whether to keep only the largest cardinally-connected component after
    /// the drop filters. A square detection is one connected planar graph, so
    /// any stranded sub-component a drop orphaned is a false positive.
    pub apply_largest_component: bool,
}

impl Default for RecoveryParams {
    fn default() -> Self {
        Self {
            local_extension: LocalExtensionParams::default(),
            bfs_extension: GrowParams::default(),
            fill: FillParams::default(),
            max_sweeps: 4,
            apply_wrong_label_drops: true,
            apply_largest_component: true,
        }
    }
}

/// Whether a detection path runs the geometry-only recovery schedule.
///
/// The default is [`Auto`](Self::Auto): the detection facade enables the
/// schedule for the synthesized-axis paths (`Evidence::Positions` /
/// `Evidence::Oriented1`, whose recall is bounded by the BFS frontier) and
/// leaves it off for the native `Evidence::Oriented2` path (which stays
/// byte-compatible). A caller that runs its own `CornerStage`-coupled recovery
/// downstream — the chessboard topological adapter — sets it explicitly to
/// [`Off`](Self::Off) so the facade adds nothing, keeping production output
/// byte-identical.
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub enum RecoverySchedule {
    /// Facade decides per evidence kind (default): on for synthesized-axis
    /// paths, off for native `Oriented2`.
    #[default]
    Auto,
    /// Run no post-convergence recovery (the explicit byte-compat opt-out for
    /// callers that recover downstream themselves).
    Off,
    /// Always run the geometry-only recovery schedule with the given tuning.
    On(RecoveryParams),
}

impl RecoverySchedule {
    /// Resolve the schedule for a concrete dispatch: `Auto` becomes the default
    /// recovery params when `synthesized_axes` is set, otherwise off. `Off`
    /// stays off; `On(p)` always runs with `p`.
    pub(crate) fn resolve(&self, synthesized_axes: bool) -> Option<RecoveryParams> {
        match self {
            RecoverySchedule::Auto if synthesized_axes => Some(RecoveryParams::default()),
            RecoverySchedule::Auto => None,
            RecoverySchedule::Off => None,
            RecoverySchedule::On(p) => Some(p.clone()),
        }
    }
}

/// Summary of one [`run_schedule`] invocation. Data carrier.
#[derive(Clone, Debug, Default)]
pub struct RecoveryStats {
    /// Number of (extend → fill → validate → drop) sweeps actually run.
    pub sweeps: u32,
    /// Net corners added across the whole schedule (attachments minus drops).
    pub net_added: i64,
    /// Total corners attached by the extension + fill engines.
    pub attached: usize,
    /// Total corners dropped by revalidation + the drop filters.
    pub dropped: usize,
}

/// Run the geometry-only recovery schedule over a converged labelled component.
///
/// `grow` carries the converged labelled set plus the seed-derived axis
/// vectors (used by the cardinal-BFS extension for its prediction direction).
/// `cell_size` is the component's estimated cell pitch. `validate_params` is
/// the same [`ValidationParams`] the convergence loop used, so revalidation is
/// consistent with the inner gates. The schedule mutates `grow.labelled` /
/// `grow.by_corner` in place and returns a [`RecoveryStats`] summary.
///
/// `strength_of` maps a corner index to a detector-response strength; it is
/// reserved for callers that want a weak-leaf peel (the facade passes a
/// constant, so the weak-leaf pass is a no-op and only the connectivity /
/// wrong-label filters fire). Determinism: the engines and filters break ties
/// by index / sorted coordinate, so repeated runs are byte-identical.
pub fn run_schedule<V: SquareAttachPolicy>(
    positions: &[Point2<f32>],
    grow: &mut GrowResult,
    cell_size: f32,
    policy: &V,
    params: &RecoveryParams,
    validate_params: &ValidationParams,
) -> RecoveryStats {
    let mut stats = RecoveryStats::default();
    if grow.labelled.len() < 4 {
        return stats;
    }
    ensure_axes(grow, positions);

    let start = grow.labelled.len() as i64;
    for _sweep in 0..params.max_sweeps.max(1) {
        stats.sweeps += 1;
        let before = grow.labelled.len();

        // Stage 1: boundary extension (local-H then cardinal-BFS mop-up).
        let local = extend_via_local_homography(
            positions,
            grow,
            cell_size,
            &params.local_extension,
            policy,
        );
        stats.attached += local.attached;
        let bfs = extend_from_labelled(positions, grow, cell_size, &params.bfs_extension, policy);
        stats.attached += bfs.attached;

        // Stage 2: interior fill.
        let fill = fill_grid_holes(positions, grow, cell_size, &params.fill, policy);
        stats.attached += fill.added;

        // Stage 3 + 4: revalidate, then apply the lattice-general drop filters.
        let dropped = revalidate_and_filter(positions, grow, cell_size, params, validate_params);
        stats.dropped += dropped;

        // Fixed point: a sweep that neither grew nor shrank the labelled set
        // cannot make progress on the next one (the engines are idempotent on
        // a stable set), so stop.
        if grow.labelled.len() == before {
            break;
        }
    }

    stats.net_added = grow.labelled.len() as i64 - start;
    stats
}

/// Revalidate the labelled set and apply the wrong-label / largest-component
/// drop filters. Returns the number of corners dropped this sweep.
fn revalidate_and_filter(
    positions: &[Point2<f32>],
    grow: &mut GrowResult,
    cell_size: f32,
    params: &RecoveryParams,
    validate_params: &ValidationParams,
) -> usize {
    // Materialize the labelled set in deterministic (i, j)-sorted order so the
    // validate stage's input order is reproducible.
    let mut ordered: Vec<((i32, i32), usize)> =
        grow.labelled.iter().map(|(&k, &v)| (k, v)).collect();
    ordered.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

    let entries: Vec<pg_validate::LabelledEntry> = ordered
        .iter()
        .map(|&(grid, idx)| pg_validate::LabelledEntry {
            idx,
            pixel: positions[idx],
            grid,
        })
        .collect();
    let validation = pg_validate::validate(&entries, cell_size, validate_params);

    let mut drop: HashSet<usize> = validation.blacklist;

    if params.apply_wrong_label_drops {
        let wrong =
            topological_wrong_label_drops(&grow.labelled, |idx: usize| positions[idx], cell_size);
        drop.extend(wrong);
    }

    if params.apply_largest_component {
        let comp = largest_component_filter(&grow.labelled, &drop);
        drop.extend(comp.drop);
    }

    if drop.is_empty() {
        return 0;
    }
    let mut removed = 0usize;
    grow.labelled.retain(|_, &mut idx| {
        if drop.contains(&idx) {
            removed += 1;
            false
        } else {
            true
        }
    });
    grow.by_corner.retain(|idx, _| !drop.contains(idx));
    removed
}

/// Ensure `grow.axis_i` / `grow.axis_j` are usable unit vectors for the
/// cardinal-BFS extension. The facade reconstructs `GrowResult` from a labelled
/// map and may not carry seed axes, so estimate them from the labelled set's
/// mean cardinal edges when they are degenerate.
fn ensure_axes(grow: &mut GrowResult, positions: &[Point2<f32>]) {
    let needs = grow.axis_i.norm() < 1e-3 || grow.axis_j.norm() < 1e-3;
    if !needs {
        return;
    }
    let (mut sum_i, mut n_i) = (Vector2::<f32>::zeros(), 0u32);
    let (mut sum_j, mut n_j) = (Vector2::<f32>::zeros(), 0u32);
    for (&(i, j), &idx) in &grow.labelled {
        let here = positions[idx];
        if let Some(&n) = grow.labelled.get(&(i + 1, j)) {
            sum_i += positions[n] - here;
            n_i += 1;
        }
        if let Some(&n) = grow.labelled.get(&(i, j + 1)) {
            sum_j += positions[n] - here;
            n_j += 1;
        }
    }
    if n_i > 0 {
        let v = sum_i / n_i as f32;
        if v.norm() > 1e-3 {
            grow.axis_i = v.normalize();
        }
    }
    if n_j > 0 {
        let v = sum_j / n_j as f32;
        if v.norm() > 1e-3 {
            grow.axis_j = v.normalize();
        }
    }
    if grow.axis_i.norm() < 1e-3 {
        grow.axis_i = Vector2::new(1.0, 0.0);
    }
    if grow.axis_j.norm() < 1e-3 {
        grow.axis_j = Vector2::new(0.0, 1.0);
    }
}

/// Run the geometry-only recovery schedule over a set of merged components,
/// masking each component's recovery against the corners owned by the others
/// (single-claim across components), then rebasing each recovered component to
/// the non-negative `(i, j)` origin. Shared by both facades' synthesized-axis
/// path.
/// Shared borrows threaded through the recovery entry points. Bundling them
/// keeps the public `recover_components` / `recover_positions_component`
/// signatures within the workspace argument-count limit without an inline
/// clippy allow.
#[derive(Clone, Copy)]
pub(crate) struct RecoveryInputs<'a> {
    /// Features carrying positions + (synthesized) axes.
    pub features: &'a [crate::feature::OrientedFeature<2>],
    /// Corner positions, indexed 1:1 with `features`.
    pub positions: &'a [Point2<f32>],
    /// Per-corner robust local pitch (see [`local_pitch_of`]).
    pub local_pitch: &'a [f32],
    /// Recovery schedule tuning.
    pub params: &'a RecoveryParams,
    /// Validation tuning reused by the schedule's revalidation pass.
    pub validate_params: &'a ValidationParams,
}

pub(crate) fn recover_components(
    merged: Vec<HashMap<(i32, i32), usize>>,
    inputs: RecoveryInputs<'_>,
) -> Vec<HashMap<(i32, i32), usize>> {
    let RecoveryInputs { positions, .. } = inputs;
    // Recover largest-first. A perspective grid that the convergence loop
    // fragmented into one large component plus a few small ones (the BFS
    // frontier stalled on the foreshortened side, then re-seeded) is best
    // healed by letting the *largest* component's extension / fill absorb the
    // fragments' corners — the fragments carry their own (incompatible) local
    // origin, so the merge could not reunite them, but the big component's
    // local-H extension reaches them directly. So a corner is masked for a
    // component only if an *already-recovered* (i.e. larger) component claimed
    // it; a corner still sitting in a smaller, not-yet-recovered fragment is
    // left available for the larger component to absorb.
    let mut order: Vec<usize> = (0..merged.len()).collect();
    order.sort_by(|&a, &b| {
        merged[b]
            .len()
            .cmp(&merged[a].len())
            .then_with(|| min_index(&merged[a]).cmp(&min_index(&merged[b])))
    });

    let mut claimed: HashSet<usize> = HashSet::new();
    let mut recovered_by_slot: Vec<HashMap<(i32, i32), usize>> = vec![HashMap::new(); merged.len()];
    for &k in &order {
        // Drop any corner an already-recovered (larger) component absorbed, so
        // two solutions never reference the same corner index. A fragment whose
        // members were fully absorbed collapses to empty and is filtered out.
        let comp: HashMap<(i32, i32), usize> = merged[k]
            .iter()
            .filter(|(_, &idx)| !claimed.contains(&idx))
            .map(|(&k, &v)| (k, v))
            .collect();
        if comp.len() < 4 {
            for &idx in comp.values() {
                claimed.insert(idx);
            }
            recovered_by_slot[k] = if comp.is_empty() {
                HashMap::new()
            } else {
                rebase_to_origin(&comp)
            };
            continue;
        }
        let cell_size = cell_size_of(&comp, positions);
        let own: HashSet<usize> = comp.values().copied().collect();
        let masked: HashSet<usize> = claimed.difference(&own).copied().collect();
        let recovered = recover_positions_component(&comp, &masked, cell_size, inputs);
        claimed.extend(recovered.values().copied());
        recovered_by_slot[k] = rebase_to_origin(&recovered);
    }
    // Drop fragments that collapsed to empty after absorption.
    recovered_by_slot.retain(|m| !m.is_empty());
    recovered_by_slot
}

/// Smallest feature index in a labelled map (deterministic tie-break key).
fn min_index(labelled: &HashMap<(i32, i32), usize>) -> usize {
    labelled.values().copied().min().unwrap_or(usize::MAX)
}

/// Number of nearest neighbours pooled per corner for the robust local-pitch
/// estimate.
const LOCAL_PITCH_NEIGHBOURS: usize = 5;

/// Per-corner robust local pitch (upper-median of the nearest-neighbour
/// distances). Tracks perspective foreshortening while tolerating a minority of
/// off-lattice points sitting closer than the pitch. The topological facade's
/// synthesized-axis recovery entry uses this to gate per-edge growth against
/// the local cell scale.
pub(crate) fn local_pitch_of(positions: &[Point2<f32>]) -> Vec<f32> {
    use kiddo::{KdTree, SquaredEuclidean};
    let n = positions.len();
    if n < 2 {
        return vec![0.0; n];
    }
    let mut tree: KdTree<f32, 2> = KdTree::new();
    for (i, p) in positions.iter().enumerate() {
        tree.add(&[p.x, p.y], i as u64);
    }
    positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let hits = tree.nearest_n::<SquaredEuclidean>(&[p.x, p.y], LOCAL_PITCH_NEIGHBOURS + 1);
            let mut dists: Vec<f32> = hits
                .into_iter()
                .filter(|nn| nn.item as usize != i)
                .map(|nn| nn.distance.sqrt())
                .filter(|d| d.is_finite() && *d > 1e-3)
                .collect();
            if dists.is_empty() {
                return 0.0;
            }
            dists.sort_by(|a, b| a.total_cmp(b));
            dists[dists.len() / 2]
        })
        .collect()
}

/// Mean labelled-pair cardinal edge length for a component (the recovery
/// schedule's `cell_size`). Mirrors the facade's `estimate_cell_size`.
fn cell_size_of(labelled: &HashMap<(i32, i32), usize>, positions: &[Point2<f32>]) -> f32 {
    let mut sum = 0.0_f32;
    let mut count = 0usize;
    for (&(i, j), &idx) in labelled {
        let here = positions[idx];
        for (di, dj) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            if let Some(&n) = labelled.get(&(i + di, j + dj)) {
                sum += (positions[n] - here).norm();
                count += 1;
            }
        }
    }
    if count == 0 {
        1.0
    } else {
        sum / count as f32
    }
}

/// Rebase a labelled component so its bounding-box minimum sits at `(0, 0)`.
fn rebase_to_origin(labelled: &HashMap<(i32, i32), usize>) -> HashMap<(i32, i32), usize> {
    let min_i = labelled.keys().map(|&(i, _)| i).min().unwrap_or(0);
    let min_j = labelled.keys().map(|&(_, j)| j).min().unwrap_or(0);
    if min_i == 0 && min_j == 0 {
        return labelled.clone();
    }
    labelled
        .iter()
        .map(|(&(i, j), &idx)| ((i - min_i, j - min_j), idx))
        .collect()
}

/// Run the geometry-only recovery schedule over a labelled `(i, j) → index`
/// component using the geometry-first [`PositionsAttachPolicy`].
///
/// This is the entry the topological facade uses for the synthesized-axis
/// (`Evidence::Positions` / `Evidence::Oriented1`) path. `features` carries
/// positions + synthesized axes; `masked` lists corner
/// indices owned by *other* components (so the recovery can't steal them).
/// Returns the recovered (NOT yet rebased) labelled map; the caller rebases to
/// the non-negative `(i, j)` origin.
pub(crate) fn recover_positions_component(
    labelled: &HashMap<(i32, i32), usize>,
    masked: &HashSet<usize>,
    cell_size: f32,
    inputs: RecoveryInputs<'_>,
) -> HashMap<(i32, i32), usize> {
    use crate::shared::positions_policy::{PositionsAttachPolicy, PositionsTolerances};

    // 50° soft axis tolerance / 0.40 edge band — the position-policy defaults
    // documented in the facade.
    let tol = PositionsTolerances {
        soft_axis_tol_rad: 0.872_664_6,
        edge_length_tol: 0.40,
        cell_size,
    };
    let inner =
        PositionsAttachPolicy::new(inputs.features, inputs.positions, inputs.local_pitch, tol);
    let policy = MaskedPolicy {
        inner: &inner,
        masked,
    };
    let mut grow = grow_result_from_labelled(labelled, inputs.positions);
    run_schedule(
        inputs.positions,
        &mut grow,
        cell_size,
        &policy,
        inputs.params,
        inputs.validate_params,
    );
    grow.labelled
}

/// Wrap a [`SquareAttachPolicy`] to additionally mask out corner indices owned
/// by another component (single-claim across components during recovery).
struct MaskedPolicy<'a, V: SquareAttachPolicy> {
    inner: &'a V,
    masked: &'a HashSet<usize>,
}

impl<V: SquareAttachPolicy> SquareAttachPolicy for MaskedPolicy<'_, V> {
    fn is_eligible(&self, idx: usize) -> bool {
        !self.masked.contains(&idx) && self.inner.is_eligible(idx)
    }
    fn required_label_at(&self, i: i32, j: i32) -> Option<u8> {
        self.inner.required_label_at(i, j)
    }
    fn label_of(&self, idx: usize) -> Option<u8> {
        self.inner.label_of(idx)
    }
    fn accept_candidate(
        &self,
        idx: usize,
        at: (i32, i32),
        prediction: Point2<f32>,
        neighbours: &[crate::shared::grow::LabelledNeighbour],
    ) -> crate::shared::grow::Admit {
        self.inner.accept_candidate(idx, at, prediction, neighbours)
    }
    fn edge_ok(&self, c: usize, n: usize, ac: (i32, i32), an: (i32, i32)) -> bool {
        self.inner.edge_ok(c, n, ac, an)
    }
}

/// Reconstruct a [`GrowResult`] from a labelled `(i, j) → index` map for the
/// recovery schedule. Estimates the axis vectors from the labelled set; the
/// schedule's internal axis-repair step also defends against a degenerate
/// estimate.
pub(crate) fn grow_result_from_labelled(
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> GrowResult {
    let by_corner: HashMap<usize, (i32, i32)> = labelled.iter().map(|(&k, &v)| (v, k)).collect();
    let mut grow = GrowResult {
        labelled: labelled.clone(),
        by_corner,
        ..Default::default()
    };
    ensure_axes(&mut grow, positions);
    grow
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::grow::{Admit, LabelledNeighbour};

    /// Open policy: every corner eligible, no label constraint, accept all,
    /// edges within ±40% of the local cell size. Mirrors the geometry-only
    /// facade policy on a synthetic grid.
    struct OpenPolicy<'a> {
        positions: &'a [Point2<f32>],
        cell_size: f32,
    }

    impl SquareAttachPolicy for OpenPolicy<'_> {
        fn is_eligible(&self, _idx: usize) -> bool {
            true
        }
        fn required_label_at(&self, _i: i32, _j: i32) -> Option<u8> {
            None
        }
        fn label_of(&self, _idx: usize) -> Option<u8> {
            None
        }
        fn accept_candidate(
            &self,
            _idx: usize,
            _at: (i32, i32),
            _prediction: Point2<f32>,
            _neighbours: &[LabelledNeighbour],
        ) -> Admit {
            Admit::Accept
        }
        fn edge_ok(&self, c: usize, n: usize, _ac: (i32, i32), _an: (i32, i32)) -> bool {
            let len = (self.positions[c] - self.positions[n]).norm();
            let r = len / self.cell_size;
            (0.6..=1.4).contains(&r)
        }
    }

    /// Positions in row-major order plus a `(i, j) → index` map.
    type SyntheticGrid = (Vec<Point2<f32>>, HashMap<(i32, i32), usize>);

    /// Build an axis-aligned `rows × cols` grid.
    fn grid(rows: i32, cols: i32, s: f32) -> SyntheticGrid {
        let mut pos = Vec::new();
        let mut map = HashMap::new();
        let mut idx = 0usize;
        for j in 0..rows {
            for i in 0..cols {
                pos.push(Point2::new(i as f32 * s + 40.0, j as f32 * s + 40.0));
                map.insert((i, j), idx);
                idx += 1;
            }
        }
        (pos, map)
    }

    #[test]
    fn fills_interior_holes_and_extends_boundary() {
        let s = 30.0_f32;
        let (pos, full) = grid(7, 7, s);
        // Seed only the inner 3x3 block; the schedule must extend outward and
        // fill to recover the full 7x7.
        let mut seed: HashMap<(i32, i32), usize> = HashMap::new();
        for j in 2..5 {
            for i in 2..5 {
                seed.insert((i, j), full[&(i, j)]);
            }
        }
        let mut grow = grow_result_from_labelled(&seed, &pos);
        let policy = OpenPolicy {
            positions: &pos,
            cell_size: s,
        };
        let params = RecoveryParams::default();
        let vp = ValidationParams::default();
        let stats = run_schedule(&pos, &mut grow, s, &policy, &params, &vp);
        assert!(
            grow.labelled.len() >= 45,
            "recovered only {}/49 (sweeps {})",
            grow.labelled.len(),
            stats.sweeps
        );
        // Zero wrong labels: every recovered cell maps to the same index the
        // ground-truth grid assigned (up to the schedule's rebase, which is
        // identity here because the seed block sat at the interior).
        for (&cell, &idx) in &grow.labelled {
            assert_eq!(
                full.get(&cell),
                Some(&idx),
                "cell {cell:?} mislabelled to index {idx}"
            );
        }
    }

    #[test]
    fn decoys_off_lattice_are_never_labelled() {
        let s = 30.0_f32;
        let (mut pos, full) = grid(6, 6, s);
        let grid_n = pos.len();
        // Add off-lattice decoys: points sitting between cells and far away.
        // None of them sit on an integer lattice node, so a precision-correct
        // schedule must never attach them.
        let decoys = [
            Point2::new(40.0 + 0.5 * s, 40.0 + 0.5 * s), // cell centre
            Point2::new(40.0 + 2.5 * s, 40.0 + 1.5 * s),
            Point2::new(40.0 - 3.0 * s, 40.0 + 2.0 * s), // far outside
            Point2::new(40.0 + 9.0 * s, 40.0 + 9.0 * s),
        ];
        for d in decoys {
            pos.push(d);
        }
        // Seed an inner block, recover, and assert no decoy index is labelled.
        let mut seed: HashMap<(i32, i32), usize> = HashMap::new();
        for j in 1..4 {
            for i in 1..4 {
                seed.insert((i, j), full[&(i, j)]);
            }
        }
        let mut grow = grow_result_from_labelled(&seed, &pos);
        let policy = OpenPolicy {
            positions: &pos,
            cell_size: s,
        };
        let stats = run_schedule(
            &pos,
            &mut grow,
            s,
            &policy,
            &RecoveryParams::default(),
            &ValidationParams::default(),
        );
        for &(_, idx) in grow
            .by_corner
            .iter()
            .map(|(idx, c)| (c, idx))
            .collect::<Vec<_>>()
            .iter()
        {
            assert!(
                *idx < grid_n,
                "a decoy (index {idx} ≥ {grid_n}) was labelled (sweeps {})",
                stats.sweeps
            );
        }
        // And the true grid corners carry their true labels.
        for (&cell, &idx) in &grow.labelled {
            assert_eq!(full.get(&cell), Some(&idx), "cell {cell:?} mislabelled");
        }
    }

    #[test]
    fn idempotent_on_clean_full_grid() {
        let s = 30.0_f32;
        let (pos, full) = grid(5, 5, s);
        let mut grow = grow_result_from_labelled(&full, &pos);
        let policy = OpenPolicy {
            positions: &pos,
            cell_size: s,
        };
        let before = grow.labelled.len();
        let stats = run_schedule(
            &pos,
            &mut grow,
            s,
            &policy,
            &RecoveryParams::default(),
            &ValidationParams::default(),
        );
        assert_eq!(grow.labelled.len(), before, "schedule altered a full grid");
        assert_eq!(stats.net_added, 0);
    }
}
