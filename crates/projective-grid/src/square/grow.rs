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

use crate::circular_stats as cs;
use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};
use std::collections::{HashMap, HashSet, VecDeque};

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
    /// prediction.
    pub attach_search_rel: f32,
    /// Ambiguity factor: if the second-nearest candidate is within
    /// `factor × nearest_distance`, the attachment is skipped.
    pub attach_ambiguity_factor: f32,
}

impl Default for GrowParams {
    fn default() -> Self {
        Self {
            attach_search_rel: 0.35,
            attach_ambiguity_factor: 1.5,
        }
    }
}

impl GrowParams {
    pub fn new(attach_search_rel: f32, attach_ambiguity_factor: f32) -> Self {
        Self {
            attach_search_rel,
            attach_ambiguity_factor,
        }
    }
}

/// Seed quad: corner indices at grid cells `(0, 0), (1, 0), (0, 1),
/// (1, 1)` respectively.
#[derive(Clone, Copy, Debug)]
pub struct Seed {
    pub a: usize,
    pub b: usize,
    pub c: usize,
    pub d: usize,
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
    let _ = cs::wrap_pi; // keeps `cs` in scope for future use

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

    let search_r = params.attach_search_rel * cell_size;

    while let Some(pos) = boundary.pop_front() {
        if labelled.contains_key(&pos) {
            continue;
        }

        let neighbours = collect_labelled_neighbours(pos, 1, &labelled, positions);
        if neighbours.is_empty() {
            holes.insert(pos);
            continue;
        }

        let prediction = predict_from_neighbours(pos, &neighbours, grid_u, grid_v, cell_size);

        let required_label = validator.required_label_at(pos.0, pos.1);
        let candidates = collect_candidates(
            &tree,
            &tree_slot_to_corner,
            prediction,
            search_r,
            validator,
            required_label,
            &by_corner,
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
        match choice {
            CandidateChoice::None => {
                holes.insert(pos);
            }
            CandidateChoice::Ambiguous => {
                ambiguous.insert(pos);
            }
            CandidateChoice::Unique(c_idx) => {
                if !any_cardinal_edge_ok(c_idx, pos, &labelled, validator) {
                    holes.insert(pos);
                    continue;
                }
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

fn enqueue_cardinal_neighbours(
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

fn collect_labelled_neighbours(
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

/// Average of per-neighbour axis-vector predictions:
/// `pred_k = pos(N_k) + di·s·u + dj·s·v`, averaged equally.
pub fn predict_from_neighbours(
    target: (i32, i32),
    neighbours: &[LabelledNeighbour],
    u: Vector2<f32>,
    v: Vector2<f32>,
    cell_size: f32,
) -> Point2<f32> {
    debug_assert!(!neighbours.is_empty());
    let mut sum_x = 0.0_f32;
    let mut sum_y = 0.0_f32;
    for n in neighbours {
        let di = (target.0 - n.at.0) as f32;
        let dj = (target.1 - n.at.1) as f32;
        let off = u * (di * cell_size) + v * (dj * cell_size);
        sum_x += n.position.x + off.x;
        sum_y += n.position.y + off.y;
    }
    let denom = neighbours.len() as f32;
    Point2::new(sum_x / denom, sum_y / denom)
}

fn collect_candidates<V: GrowValidator>(
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

enum CandidateChoice {
    None,
    Ambiguous,
    Unique(usize),
}

fn choose_unambiguous<V: GrowValidator>(
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

fn any_cardinal_edge_ok<V: GrowValidator>(
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
