//! Generic BFS-style growth from a 2×2 seed over a square lattice.
//!
//! The growth algorithm — BFS queue, KD-tree candidate search, per-
//! neighbour prediction averaging, ambiguity filtering — is pure
//! geometry and works for any square-grid pattern. Pattern-specific
//! invariants (parity rules, axis clustering, marker constraints)
//! plug in via the [`GrowValidator`] trait.
//!
//! # Design
//!
//! The generic function manages:
//! - The labelled `(i, j) → corner_index` map.
//! - The BFS boundary queue and "seen" set.
//! - A KD-tree over eligible candidate positions.
//! - Per-neighbour prediction averaging (grid vectors `u`, `v`).
//! - Ambiguity resolution (nearest vs second-nearest ratio).
//! - Final rebase so the bounding-box minimum is `(0, 0)`.
//!
//! The validator is asked four questions:
//! - **`is_eligible(idx)`** — can this corner index be considered as
//!   a candidate at all? (typically: pre-filtered / in a cluster / not
//!   blacklisted)
//! - **`required_label_at(i, j)`** — what pattern label is required at
//!   this grid cell? Opaque `u8`; the validator picks the scheme.
//!   `None` means "no label constraint".
//! - **`accept_candidate(idx, at, prediction, neighbours)`** — once
//!   the generic search has found a candidate passing geometric
//!   checks, is it pattern-legal?
//! - **`edge_ok(candidate_idx, neighbour_idx, at_cand, at_neigh)`** —
//!   soft per-edge check at attachment time.
//!
//! # Non-goals
//!
//! This function does **not** do post-growth validation (line
//! collinearity / local-H residuals). See
//! [`crate::square::validate`](mod@crate::square::validate) for
//! that.

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};
use std::collections::{HashMap, HashSet, VecDeque};

pub use crate::square::seed::Seed;

/// Per-candidate decision from a [`GrowValidator`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admit {
    /// Accept this candidate at the given grid cell.
    Accept,
    /// Reject this candidate; the generic code may move on to the
    /// next nearest (if any).
    Reject,
}

/// Information about an existing labelled neighbour, passed to the
/// validator during candidate evaluation.
#[derive(Clone, Copy, Debug)]
pub struct LabelledNeighbour {
    pub idx: usize,
    pub at: (i32, i32),
    pub position: Point2<f32>,
}

/// Pattern-specific validation hooks for [`bfs_grow`].
///
/// Implementations typically hold references to the caller's corner
/// data (axes, labels, strengths) plus the pattern's tuning
/// parameters, and use `idx` to look up the relevant per-corner
/// record inside each callback.
pub trait GrowValidator {
    /// Is this corner index a possible candidate at all? Called
    /// once per corner when the KD-tree is built.
    fn is_eligible(&self, idx: usize) -> bool;

    /// Optional pattern-required label at grid cell `(i, j)`.
    /// Return `None` for no constraint.
    fn required_label_at(&self, i: i32, j: i32) -> Option<u8>;

    /// Return the label of the corner at `idx`. Must agree with
    /// `required_label_at` at attachment time. Called during
    /// candidate filtering.
    fn label_of(&self, idx: usize) -> Option<u8>;

    /// Accept or reject a candidate for attachment at grid cell
    /// `at` given its geometric prediction and existing labelled
    /// neighbours. Called per candidate in order of increasing
    /// distance to `prediction`.
    fn accept_candidate(
        &self,
        idx: usize,
        at: (i32, i32),
        prediction: Point2<f32>,
        neighbours: &[LabelledNeighbour],
    ) -> Admit;

    /// Soft per-edge check: is the induced edge between the just-
    /// attached candidate and one of its cardinal-labelled neighbours
    /// admissible? At least one cardinal edge must pass for the
    /// attachment to stick; otherwise the position is marked a hole
    /// and the candidate is rolled back.
    ///
    /// Default: accept all edges (no soft check).
    fn edge_ok(
        &self,
        _candidate_idx: usize,
        _neighbour_idx: usize,
        _at_candidate: (i32, i32),
        _at_neighbour: (i32, i32),
    ) -> bool {
        true
    }
}

/// Tolerances for [`bfs_grow`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct GrowParams {
    /// Candidate-search radius (fraction of `cell_size`) around each
    /// prediction. Applies when the target is being **interpolated**
    /// between labelled neighbours on opposite sides.
    pub attach_search_rel: f32,
    /// Ambiguity factor: if the second-nearest candidate is within
    /// `factor × nearest_distance`, the attachment is skipped.
    pub attach_ambiguity_factor: f32,
    /// Multiplier on `attach_search_rel` when the target is being
    /// **extrapolated** outward from the labelled set (every labelled
    /// neighbour sits on the same side of the target along at least one
    /// axis). Defaults to 2.0 — opens the search up enough to absorb
    /// the perspective-foreshortening overshoot at the image edge while
    /// still rejecting marker-internal corners which sit several cell-
    /// widths away.
    pub boundary_search_factor: f32,
}

impl Default for GrowParams {
    fn default() -> Self {
        Self {
            attach_search_rel: 0.35,
            attach_ambiguity_factor: 1.5,
            boundary_search_factor: 2.0,
        }
    }
}

impl GrowParams {
    pub fn new(attach_search_rel: f32, attach_ambiguity_factor: f32) -> Self {
        Self {
            attach_search_rel,
            attach_ambiguity_factor,
            ..Self::default()
        }
    }
}

/// Outcome of a grow pass.
#[derive(Debug, Default)]
pub struct GrowResult {
    /// `(i, j) → corner_index` map of accepted labels. Rebased so the
    /// bounding-box minimum is `(0, 0)`.
    pub labelled: HashMap<(i32, i32), usize>,
    /// Inverse map.
    pub by_corner: HashMap<usize, (i32, i32)>,
    /// Positions with ≥ 2 candidates inside the ambiguity window.
    pub ambiguous: HashSet<(i32, i32)>,
    /// Positions with no accepted candidate.
    pub holes: HashSet<(i32, i32)>,
    /// Grid vectors carried forward — overlays / boosters use them.
    pub grid_u: Vector2<f32>,
    pub grid_v: Vector2<f32>,
}

/// Grow a labelled `(i, j)` grid from a 2×2 seed using BFS over the
/// lattice boundary.
///
/// `positions` must be indexed 1:1 with the caller's corner array;
/// the validator uses the same indices.
///
/// Returns the labelled map rebased so the bounding-box minimum is
/// `(0, 0)`. The caller is responsible for any per-corner state
/// updates after the call (e.g., marking corners as "labelled" in a
/// local stage enum).
pub fn bfs_grow<V: GrowValidator>(
    positions: &[Point2<f32>],
    seed: Seed,
    cell_size: f32,
    params: &GrowParams,
    validator: &V,
) -> GrowResult {
    // Grid unit vectors inferred from the seed corners (pixel space).
    let grid_u = {
        let raw = positions[seed.b] - positions[seed.a];
        let n = raw.norm().max(1e-6);
        raw / n
    };
    let grid_v = {
        let raw = positions[seed.c] - positions[seed.a];
        let n = raw.norm().max(1e-6);
        raw / n
    };

    // KD-tree over eligible corners.
    let mut tree: KdTree<f32, 2> = KdTree::new();
    let mut tree_slot_to_corner: Vec<usize> = Vec::new();
    for (idx, pos) in positions.iter().enumerate() {
        if validator.is_eligible(idx) {
            tree.add(&[pos.x, pos.y], tree_slot_to_corner.len() as u64);
            tree_slot_to_corner.push(idx);
        }
    }

    let mut labelled: HashMap<(i32, i32), usize> = HashMap::new();
    let mut by_corner: HashMap<usize, (i32, i32)> = HashMap::new();
    let mut ambiguous: HashSet<(i32, i32)> = HashSet::new();
    let mut holes: HashSet<(i32, i32)> = HashSet::new();

    for (ij, idx) in [
        ((0, 0), seed.a),
        ((1, 0), seed.b),
        ((0, 1), seed.c),
        ((1, 1), seed.d),
    ] {
        labelled.insert(ij, idx);
        by_corner.insert(idx, ij);
    }

    let mut boundary: VecDeque<(i32, i32)> = VecDeque::new();
    let mut seen_boundary: HashSet<(i32, i32)> = HashSet::new();
    for ij in labelled.keys().copied().collect::<Vec<_>>() {
        enqueue_cardinal_neighbours(ij, &labelled, &mut boundary, &mut seen_boundary);
    }

    while let Some(pos) = boundary.pop_front() {
        if labelled.contains_key(&pos) {
            continue;
        }
        let (decision, _neighbours) = process_boundary_cell(
            pos,
            positions,
            &labelled,
            &by_corner,
            &tree,
            &tree_slot_to_corner,
            grid_u,
            grid_v,
            cell_size,
            params,
            validator,
        );
        match decision {
            BoundaryDecision::Hole | BoundaryDecision::EdgeRejected => {
                holes.insert(pos);
            }
            BoundaryDecision::Ambiguous => {
                ambiguous.insert(pos);
            }
            BoundaryDecision::Attach(c_idx) => {
                labelled.insert(pos, c_idx);
                by_corner.insert(c_idx, pos);
                enqueue_cardinal_neighbours(pos, &labelled, &mut boundary, &mut seen_boundary);
            }
        }
    }

    // Rebase so (min_i, min_j) = (0, 0).
    let (min_i, min_j) = labelled
        .keys()
        .fold((i32::MAX, i32::MAX), |(a, b), &(i, j)| (a.min(i), b.min(j)));
    if min_i != 0 || min_j != 0 {
        let rebased: HashMap<(i32, i32), usize> = labelled
            .into_iter()
            .map(|((i, j), idx)| ((i - min_i, j - min_j), idx))
            .collect();
        let rebased_by_corner: HashMap<usize, (i32, i32)> =
            rebased.iter().map(|(&ij, &idx)| (idx, ij)).collect();
        labelled = rebased;
        by_corner = rebased_by_corner;
    }
    let rebase_pos = |(i, j)| (i - min_i, j - min_j);
    let ambiguous: HashSet<(i32, i32)> = ambiguous.into_iter().map(rebase_pos).collect();
    let holes: HashSet<(i32, i32)> = holes.into_iter().map(rebase_pos).collect();

    GrowResult {
        labelled,
        by_corner,
        ambiguous,
        holes,
        grid_u,
        grid_v,
    }
}

pub(super) fn enqueue_cardinal_neighbours(
    pos: (i32, i32),
    labelled: &HashMap<(i32, i32), usize>,
    boundary: &mut VecDeque<(i32, i32)>,
    seen: &mut HashSet<(i32, i32)>,
) {
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (pos.0 + di, pos.1 + dj);
        if !labelled.contains_key(&neigh) && seen.insert(neigh) {
            boundary.push_back(neigh);
        }
    }
}

pub(super) fn collect_labelled_neighbours(
    pos: (i32, i32),
    window_half: i32,
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> Vec<LabelledNeighbour> {
    let mut out = Vec::new();
    for dj in -window_half..=window_half {
        for di in -window_half..=window_half {
            if di == 0 && dj == 0 {
                continue;
            }
            let at = (pos.0 + di, pos.1 + dj);
            if let Some(&idx) = labelled.get(&at) {
                out.push(LabelledNeighbour {
                    idx,
                    at,
                    position: positions[idx],
                });
            }
        }
    }
    out
}

/// Distance-weighted average of per-neighbour axis-vector predictions.
///
/// Use this function for in-the-loop BFS attachment where arbitrary
/// labelled neighbours are available. For post-grow outlier detection
/// using cardinal midpoint averaging, see
/// [`crate::square::smoothness::square_predict_grid_position`].
///
/// For each labelled neighbour `N_k` at `(i_k, j_k)`, the prediction is
/// `pred_k = pos(N_k) + (Δi · i_step_k) + (Δj · j_step_k)` where
/// `Δi = target.i − i_k`, `Δj = target.j − j_k`, and `i_step_k` /
/// `j_step_k` are the **local** grid-step vectors observed at `N_k`:
///
/// - If `(i_k+1, j_k)` and `(i_k−1, j_k)` are both labelled, the i-step is
///   the central difference `(pos(i_k+1, j_k) − pos(i_k−1, j_k)) / 2`.
/// - Otherwise, a one-sided difference from whichever neighbour is
///   labelled.
/// - Otherwise, fall back to the global `cell_size · u`. Same for j.
///
/// This linearises the grid **at every neighbour individually** instead of
/// trusting the seed's global `(u, v, cell_size)` — critical under strong
/// perspective foreshortening, where the cell pitch on the far edge of
/// the labelled set is materially different from the seed's mean. With
/// the global-only model, BFS predictions on the foreshortened side
/// overshoot the next true corner by more than the search radius and
/// growth terminates prematurely.
///
/// Predictions are averaged with weights `1 / (Δi² + Δj²)` so cardinal
/// neighbours (grid distance 1) carry weight 1.0 while diagonal
/// neighbours (grid distance √2) carry weight 0.5 — variance addition
/// per grid step.
///
/// A neighbour at the target cell itself (`Δi = Δj = 0`) would yield an
/// infinite weight; in practice [`bfs_grow`] never enqueues such a
/// neighbour (they're already labelled), but for robustness we treat
/// `Δi = Δj = 0` as weight 1.0 to avoid `NaN`.
pub fn predict_from_neighbours(
    target: (i32, i32),
    neighbours: &[LabelledNeighbour],
    u: Vector2<f32>,
    v: Vector2<f32>,
    cell_size: f32,
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> Point2<f32> {
    debug_assert!(!neighbours.is_empty());
    let global_i_step = u * cell_size;
    let global_j_step = v * cell_size;

    let mut sum_x = 0.0_f32;
    let mut sum_y = 0.0_f32;
    let mut sum_w = 0.0_f32;
    for n in neighbours {
        let di = (target.0 - n.at.0) as f32;
        let dj = (target.1 - n.at.1) as f32;
        let d2 = di * di + dj * dj;
        let w = if d2 > 0.0 { 1.0 / d2 } else { 1.0 };

        let i_step = local_step_at(n.at, (1, 0), labelled, positions).unwrap_or(global_i_step);
        let j_step = local_step_at(n.at, (0, 1), labelled, positions).unwrap_or(global_j_step);

        let off = i_step * di + j_step * dj;
        sum_x += w * (n.position.x + off.x);
        sum_y += w * (n.position.y + off.y);
        sum_w += w;
    }
    Point2::new(sum_x / sum_w, sum_y / sum_w)
}

/// True when every labelled neighbour sits on the same side of `target`
/// along at least one of the two grid axes — i.e., the target is being
/// extrapolated outward from the labelled set rather than interpolated
/// between two opposing sides.
///
/// This is the geometric signal that the search prediction is less
/// reliable: extrapolation accumulates foreshortening error linearly,
/// while interpolation has neighbours on both sides bracketing the
/// truth.
pub(super) fn is_extrapolating(target: (i32, i32), neighbours: &[LabelledNeighbour]) -> bool {
    let mut has_neg_di = false;
    let mut has_pos_di = false;
    let mut has_neg_dj = false;
    let mut has_pos_dj = false;
    for n in neighbours {
        let di = target.0 - n.at.0;
        let dj = target.1 - n.at.1;
        if di > 0 {
            has_neg_di = true; // neighbour is on the −i side of target
        } else if di < 0 {
            has_pos_di = true;
        }
        if dj > 0 {
            has_neg_dj = true;
        } else if dj < 0 {
            has_pos_dj = true;
        }
    }
    !(has_neg_di && has_pos_di && has_neg_dj && has_pos_dj)
}

/// Estimate the local grid-step vector at labelled cell `at` along
/// direction `step = (di, dj)` using a finite-difference of labelled
/// neighbours. Returns `None` when neither the forward nor backward
/// neighbour is labelled.
pub(super) fn local_step_at(
    at: (i32, i32),
    step: (i32, i32),
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> Option<Vector2<f32>> {
    let here = labelled.get(&at).map(|&i| positions[i])?;
    let fwd = (at.0 + step.0, at.1 + step.1);
    let bwd = (at.0 - step.0, at.1 - step.1);
    let fwd_pos = labelled.get(&fwd).map(|&i| positions[i]);
    let bwd_pos = labelled.get(&bwd).map(|&i| positions[i]);
    match (fwd_pos, bwd_pos) {
        (Some(f), Some(b)) => {
            let v = (f - b) * 0.5;
            Some(v)
        }
        (Some(f), None) => Some(f - here),
        (None, Some(b)) => Some(here - b),
        (None, None) => None,
    }
}

pub(super) fn collect_candidates<V: GrowValidator>(
    tree: &KdTree<f32, 2>,
    slot_to_corner: &[usize],
    prediction: Point2<f32>,
    search_r: f32,
    validator: &V,
    required_label: Option<u8>,
    by_corner: &HashMap<usize, (i32, i32)>,
) -> Vec<(usize, f32)> {
    let r2 = search_r * search_r;
    let mut out: Vec<(usize, f32)> = Vec::new();
    for nn in tree
        .within_unsorted::<SquaredEuclidean>(&[prediction.x, prediction.y], r2)
        .into_iter()
    {
        let idx = slot_to_corner[nn.item as usize];
        if by_corner.contains_key(&idx) {
            continue;
        }
        if let Some(req) = required_label {
            let Some(got) = validator.label_of(idx) else {
                continue;
            };
            if got != req {
                continue;
            }
        }
        let d = nn.distance.sqrt();
        out.push((idx, d));
    }
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}

pub(super) enum CandidateChoice {
    None,
    Ambiguous,
    Unique(usize),
}

pub(super) fn choose_unambiguous<V: GrowValidator>(
    candidates: &[(usize, f32)],
    ambiguity_factor: f32,
    prediction: Point2<f32>,
    positions: &[Point2<f32>],
    validator: &V,
    at: (i32, i32),
    neighbours: &[LabelledNeighbour],
) -> CandidateChoice {
    // Filter by validator in distance order; pick the first Accept.
    // Ambiguity check uses raw geometric ranks (two geometrically-close
    // candidates, regardless of validator opinion).
    if candidates.is_empty() {
        return CandidateChoice::None;
    }
    if candidates.len() >= 2 {
        let (_, d0) = candidates[0];
        let (_, d1) = candidates[1];
        if d0 <= f32::EPSILON {
            return CandidateChoice::Ambiguous;
        }
        if d1 / d0 < ambiguity_factor {
            return CandidateChoice::Ambiguous;
        }
    }
    for &(idx, _dist) in candidates {
        let pos = positions[idx];
        let _ = pos; // reserved for future per-candidate metric
        match validator.accept_candidate(idx, at, prediction, neighbours) {
            Admit::Accept => return CandidateChoice::Unique(idx),
            Admit::Reject => continue,
        }
    }
    CandidateChoice::None
}

pub(super) fn any_cardinal_edge_ok<V: GrowValidator>(
    c_idx: usize,
    pos: (i32, i32),
    labelled: &HashMap<(i32, i32), usize>,
    validator: &V,
) -> bool {
    let mut found_any = false;
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (pos.0 + di, pos.1 + dj);
        if let Some(&n_idx) = labelled.get(&neigh) {
            found_any = true;
            if validator.edge_ok(c_idx, n_idx, pos, neigh) {
                return true;
            }
        }
    }
    // No cardinal neighbours → defer (position reached via BFS from a
    // labelled neighbour, so this is a safety net).
    !found_any
}

/// Outcome of processing one boundary cell.
pub(super) enum BoundaryDecision {
    /// No eligible candidates in the search radius.
    Hole,
    /// Multiple near-equidistant candidates — cannot pick unambiguously.
    Ambiguous,
    /// The edge check blocked the unique candidate.
    EdgeRejected,
    /// Unique candidate accepted; caller should attach this corner index.
    Attach(usize),
}

/// Process one cell from the BFS boundary queue.
///
/// Collects labelled neighbours, predicts the target pixel position,
/// searches candidates, resolves ambiguity, and checks `edge_ok`.
/// Returns a [`BoundaryDecision`] that the caller applies to the mutable
/// state. Keeping the decision logic in one place makes `bfs_grow` and
/// `extend_from_labelled` share the same filter pipeline without
/// duplicating code.
///
/// # Parameters
/// - `pos`: the cell being processed.
/// - `positions`: full corner position array.
/// - `labelled` / `by_corner`: current label state.
/// - `tree` / `tree_slot_to_corner`: KD-tree over eligible candidates.
/// - `grid_u`, `grid_v`, `cell_size`, `params`: growth geometry.
/// - `validator`: pattern-specific filter.
#[allow(clippy::too_many_arguments)]
pub(super) fn process_boundary_cell<V: GrowValidator>(
    pos: (i32, i32),
    positions: &[Point2<f32>],
    labelled: &HashMap<(i32, i32), usize>,
    by_corner: &HashMap<usize, (i32, i32)>,
    tree: &KdTree<f32, 2>,
    tree_slot_to_corner: &[usize],
    grid_u: Vector2<f32>,
    grid_v: Vector2<f32>,
    cell_size: f32,
    params: &GrowParams,
    validator: &V,
) -> (BoundaryDecision, Vec<LabelledNeighbour>) {
    let neighbours = collect_labelled_neighbours(pos, 1, labelled, positions);
    if neighbours.is_empty() {
        return (BoundaryDecision::Hole, neighbours);
    }

    let prediction = predict_from_neighbours(
        pos,
        &neighbours,
        grid_u,
        grid_v,
        cell_size,
        labelled,
        positions,
    );

    let search_r = params.attach_search_rel * cell_size;
    let extrapolating = is_extrapolating(pos, &neighbours);
    let local_search_r = if extrapolating {
        search_r * params.boundary_search_factor
    } else {
        search_r
    };

    let required_label = validator.required_label_at(pos.0, pos.1);
    let candidates = collect_candidates(
        tree,
        tree_slot_to_corner,
        prediction,
        local_search_r,
        validator,
        required_label,
        by_corner,
    );

    let choice = choose_unambiguous(
        &candidates,
        params.attach_ambiguity_factor,
        prediction,
        positions,
        validator,
        pos,
        &neighbours,
    );

    let decision = match choice {
        CandidateChoice::None => BoundaryDecision::Hole,
        CandidateChoice::Ambiguous => BoundaryDecision::Ambiguous,
        CandidateChoice::Unique(c_idx) => {
            if !any_cardinal_edge_ok(c_idx, pos, labelled, validator) {
                BoundaryDecision::EdgeRejected
            } else {
                BoundaryDecision::Attach(c_idx)
            }
        }
    };
    (decision, neighbours)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Trivial validator: every corner eligible, no label constraint,
    /// accept everything.
    struct OpenValidator;

    impl GrowValidator for OpenValidator {
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
    }

    #[test]
    fn predict_weights_diagonal_less_than_cardinal() {
        // Demonstrate the 1/(Δi² + Δj²) weighting on **isolated** labelled
        // neighbours — placed far enough apart in (i, j) that the local-step
        // lookup returns `None` for both, exercising the global (u, v,
        // cell_size) fallback path.
        //
        // target = (5, 5)
        //   - cardinal at (5, 4), pos = (50, 40)
        //   - diagonal at (3, 3), pos = (30, 30 + 4)  (4 px y-bias)
        //
        // Both neighbours' adjacent (i, j) cells are unlabelled, so each
        // falls back to the global step `cell_size · u`, `cell_size · v`.
        // Cardinal prediction at target: (50, 40) + (0, 10) = (50, 50).
        // Diagonal prediction at target: (30, 34) + (20, 20) = (50, 54).
        //
        // Weights: cardinal Δd²=1 → w=1.0; diagonal Δd²=8 → w=0.125.
        // Weighted y: (50 + 0.125·54) / 1.125 ≈ 50.444 px.
        // Equal-weight average would be (50 + 54)/2 = 52, so the
        // diagonal's bias has been suppressed by the d² down-weighting.
        let s = 10.0_f32;
        let u = Vector2::new(1.0, 0.0);
        let v = Vector2::new(0.0, 1.0);
        let target = (5, 5);
        let cardinal = LabelledNeighbour {
            idx: 0,
            at: (5, 4),
            position: Point2::new(50.0, 40.0),
        };
        let diagonal = LabelledNeighbour {
            idx: 1,
            at: (3, 3),
            position: Point2::new(30.0, 34.0),
        };
        let positions = vec![cardinal.position, diagonal.position];
        let mut labelled = HashMap::new();
        labelled.insert(cardinal.at, 0usize);
        labelled.insert(diagonal.at, 1usize);
        let pred = predict_from_neighbours(
            target,
            &[cardinal, diagonal],
            u,
            v,
            s,
            &labelled,
            &positions,
        );
        let expected_y = (50.0 + 0.125 * 54.0) / 1.125;
        assert!(
            (pred.x - 50.0).abs() < 1e-4,
            "predicted x {} should equal 50",
            pred.x
        );
        assert!(
            (pred.y - expected_y).abs() < 1e-4,
            "predicted y {} should equal {} (1/d² weighted)",
            pred.y,
            expected_y
        );
        let equal_weight_y = (50.0 + 54.0) * 0.5;
        assert!(
            (pred.y - 50.0) < (equal_weight_y - 50.0),
            "weighted bias {} should be smaller than equal-weight bias {}",
            pred.y - 50.0,
            equal_weight_y - 50.0,
        );
    }

    #[test]
    fn predict_with_only_cardinal_recovers_exact_offset() {
        let s = 12.0_f32;
        let u = Vector2::new(1.0, 0.0);
        let v = Vector2::new(0.0, 1.0);
        let target = (2, 2);
        let neighbour = LabelledNeighbour {
            idx: 0,
            at: (1, 2),
            position: Point2::new(s, 2.0 * s),
        };
        let positions = vec![neighbour.position];
        let mut labelled = HashMap::new();
        labelled.insert(neighbour.at, 0usize);
        let pred = predict_from_neighbours(target, &[neighbour], u, v, s, &labelled, &positions);
        assert!((pred.x - 2.0 * s).abs() < 1e-4);
        assert!((pred.y - 2.0 * s).abs() < 1e-4);
    }

    #[test]
    fn predict_uses_local_step_when_neighbour_has_own_neighbours() {
        // Foreshortened-grid scenario:
        //   labelled (i, j) | image position
        //   ---------------- | --------------
        //   (3, 0)            | (300, 0)   ← neighbour we extrapolate from
        //   (4, 0)            | (310, 0)   ← +1 step at (3,0) is only +10 px
        //   (5, 0)            | (320, 0)
        //
        // The seed's global cell_size is 50 px (a far-region estimate). The
        // global model would predict target (2, 0) at (300 - 50, 0) = (250, 0),
        // missing the actual location at (290, 0) by 40 px.
        //
        // The local-step model uses the central-difference at (3, 0):
        //   i_step = (pos(4, 0) − pos(2, 0)) / 2  but (2, 0) is unlabelled
        //   so it falls back to one-sided: pos(3, 0) − pos(4, 0) = (−10, 0)
        //   wait — that's BACKWARD. Let me redo: forward (4, 0) is labelled,
        //   so i_step ← pos(4, 0) − pos(3, 0) = (+10, 0). For target (2, 0),
        //   prediction = pos(3, 0) + (2 − 3) · (+10, 0) = (290, 0). ✓
        let u = Vector2::new(1.0, 0.0);
        let v = Vector2::new(0.0, 1.0);
        let global_cell_size = 50.0_f32;
        let neighbour = LabelledNeighbour {
            idx: 0,
            at: (3, 0),
            position: Point2::new(300.0, 0.0),
        };
        let mut positions = vec![neighbour.position];
        let mut labelled: HashMap<(i32, i32), usize> = HashMap::new();
        labelled.insert((3, 0), 0);
        positions.push(Point2::new(310.0, 0.0));
        labelled.insert((4, 0), 1);
        positions.push(Point2::new(320.0, 0.0));
        labelled.insert((5, 0), 2);

        let pred = predict_from_neighbours(
            (2, 0),
            &[neighbour],
            u,
            v,
            global_cell_size,
            &labelled,
            &positions,
        );
        // Adaptive prediction lands on the foreshortened position, not the
        // 50-px global step.
        assert!(
            (pred.x - 290.0).abs() < 1e-3,
            "expected adaptive prediction at x=290, got {}",
            pred.x
        );
        assert!((pred.y - 0.0).abs() < 1e-3);
    }

    #[test]
    fn predict_falls_back_to_global_when_no_local_steps() {
        // Single isolated neighbour with no labelled +i / +j peers — the
        // local-step lookup returns None for both directions and the global
        // (u, v, cell_size) fallback produces the same answer as the
        // pre-refactor implementation.
        let u = Vector2::new(1.0, 0.0);
        let v = Vector2::new(0.0, 1.0);
        let s = 25.0_f32;
        let neighbour = LabelledNeighbour {
            idx: 0,
            at: (4, 4),
            position: Point2::new(100.0, 100.0),
        };
        let positions = vec![neighbour.position];
        let mut labelled: HashMap<(i32, i32), usize> = HashMap::new();
        labelled.insert((4, 4), 0);
        let pred = predict_from_neighbours((5, 4), &[neighbour], u, v, s, &labelled, &positions);
        assert!((pred.x - (100.0 + s)).abs() < 1e-3);
        assert!((pred.y - 100.0).abs() < 1e-3);
    }

    #[test]
    fn open_validator_grows_clean_grid() {
        let s = 20.0_f32;
        let rows = 6_i32;
        let cols = 6_i32;
        let mut positions = Vec::new();
        let mut seed_idx = [0usize; 4];
        for j in 0..rows {
            for i in 0..cols {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let k = positions.len();
                positions.push(Point2::new(x, y));
                if (i, j) == (0, 0) {
                    seed_idx[0] = k;
                }
                if (i, j) == (1, 0) {
                    seed_idx[1] = k;
                }
                if (i, j) == (0, 1) {
                    seed_idx[2] = k;
                }
                if (i, j) == (1, 1) {
                    seed_idx[3] = k;
                }
            }
        }

        let seed = Seed {
            a: seed_idx[0],
            b: seed_idx[1],
            c: seed_idx[2],
            d: seed_idx[3],
        };
        let res = bfs_grow(&positions, seed, s, &GrowParams::default(), &OpenValidator);
        assert_eq!(res.labelled.len(), (rows * cols) as usize);
        // Origin rebased to (0, 0).
        let (mi, mj) = res
            .labelled
            .keys()
            .fold((i32::MAX, i32::MAX), |(a, b), &(i, j)| (a.min(i), b.min(j)));
        assert_eq!((mi, mj), (0, 0));
    }
}
