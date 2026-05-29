//! Post-grow fill pass: interior holes + line extrapolation.
//!
//! [`fill_grid_holes`] is the pattern-agnostic "booster" pass that
//! runs **after** the precision core (seed + grow + validate) has
//! converged. It enumerates `(i, j)` cells inside the labelled
//! bounding box that are still empty, plus `±1` cells immediately
//! outside each row / column boundary, and tries to attach a
//! candidate at each one using the same per-cell ladder as
//! [`crate::detect::advanced::square::grow::bfs_grow`].
//!
//! # Precision contract
//!
//! Fill attachments must obey the same invariants as BFS-grow
//! attachments. The policy's existing gates (`is_eligible`,
//! `required_label_at`, `label_of`, `accept_candidate`) are reused
//! verbatim; the edge check delegates to [`SquareAttachPolicy::fill_edge_ok`]
//! which, by default, forwards to [`SquareAttachPolicy::edge_ok`].
//!
//! # Pattern extension points
//!
//! Two optional methods on [`SquareAttachPolicy`] let pattern crates
//! customise the fill pass without touching this module:
//!
//! - [`SquareAttachPolicy::eligible_for_fill`] — widen the admissible
//!   candidate set during the booster pass (e.g. admit near-cluster
//!   corners that the precision core dropped by a hair).
//! - [`SquareAttachPolicy::fill_edge_ok`] — replace the scalar-`cell_size`
//!   edge-length check with a labelled-set-aware (e.g. directional
//!   median) variant. Useful when a component is strongly anisotropic
//!   before final recovery has filled the boundary.
//!
//! Patterns whose precision core is strict enough that the booster
//! pass should not relax anything need not override either method —
//! the defaults forward to `is_eligible` and `edge_ok`.
//!
//! # Iteration
//!
//! The pass iterates until a full scan attaches zero new corners,
//! capped at `max_iters`. Each iteration rebuilds the KD-tree over
//! `eligible_for_fill` candidates because the previous iteration's
//! attachments shrink the eligible set.

use crate::detect::advanced::square::grow::{
    collect_labelled_neighbours, predict_from_neighbours, Admit, FillEdgeCtx, GrowResult,
    SquareAttachPolicy,
};
use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::Point2;

/// Tunables for [`fill_grid_holes`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct FillParams {
    /// Candidate-search radius (fraction of `cell_size`) around each
    /// per-cell prediction.
    pub attach_search_rel: f32,
    /// Ambiguity factor: if the second-nearest candidate is within
    /// `factor × nearest_distance`, the cell is skipped.
    pub attach_ambiguity_factor: f32,
    /// Maximum number of full scans. Defaults to 1 — most boards
    /// converge in one pass, and the precision contract is identical
    /// per iteration, so additional iterations only help when an
    /// attached corner enables further attachments in the same
    /// cell's 3×3 window.
    pub max_iters: usize,
}

impl Default for FillParams {
    fn default() -> Self {
        Self {
            attach_search_rel: 0.35,
            attach_ambiguity_factor: 1.5,
            max_iters: 1,
        }
    }
}

impl FillParams {
    /// Construct fill parameters from the search radius (relative to
    /// `cell_size`), the ambiguity factor, and the maximum scan count.
    pub fn new(attach_search_rel: f32, attach_ambiguity_factor: f32, max_iters: usize) -> Self {
        Self {
            attach_search_rel,
            attach_ambiguity_factor,
            max_iters,
        }
    }
}

/// Counters returned by [`fill_grid_holes`].
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct FillStats {
    /// Total number of corners attached across all iterations.
    pub added: usize,
    /// Number of full scans performed.
    pub iterations: usize,
    /// Corner indices of all attached corners (parallel to
    /// [`attached_cells`][FillStats::attached_cells]). Callers use
    /// this to update per-corner state without re-scanning
    /// `grow.labelled`.
    pub attached_indices: Vec<usize>,
    /// Grid cells at which each attached corner was placed.
    pub attached_cells: Vec<(i32, i32)>,
}

/// Run the unified fill pass: interior gap fill + line extrapolation.
///
/// Mutates `grow.labelled` and `grow.by_corner` in place. The caller
/// is responsible for downstream per-corner state updates (e.g.
/// marking newly-attached corners as "Labeled" in a local stage enum)
/// — `fill_grid_holes` records the attached indices in
/// [`FillStats::attached_indices`] so the caller can sweep them
/// without re-scanning the labelled map.
///
/// Returns a [`FillStats`] summary.
pub fn fill_grid_holes<V: SquareAttachPolicy>(
    positions: &[Point2<f32>],
    grow: &mut GrowResult,
    cell_size: f32,
    params: &FillParams,
    policy: &V,
) -> FillStats {
    let mut stats = FillStats::default();
    if grow.labelled.is_empty() {
        return stats;
    }

    for _iter in 0..params.max_iters.max(1) {
        stats.iterations += 1;
        let (tree, slot_to_corner) = build_fill_tree(positions, grow, policy);
        let ctx = FillCtx {
            positions,
            cell_size,
            params,
            tree: &tree,
            slot_to_corner: &slot_to_corner,
            policy,
        };

        let cells = enumerate_fill_cells(grow);
        let mut added_this_iter = 0usize;
        for cell in cells {
            if grow.labelled.contains_key(&cell) {
                continue;
            }
            if let Some(attached_idx) = try_fill_cell(cell, grow, &ctx) {
                stats.attached_indices.push(attached_idx);
                stats.attached_cells.push(cell);
                added_this_iter += 1;
            }
        }
        stats.added += added_this_iter;
        if added_this_iter == 0 {
            break;
        }
    }

    stats
}

fn build_fill_tree<V: SquareAttachPolicy>(
    positions: &[Point2<f32>],
    grow: &GrowResult,
    policy: &V,
) -> (KdTree<f32, 2>, Vec<usize>) {
    let mut tree: KdTree<f32, 2> = KdTree::new();
    let mut slot_to_corner: Vec<usize> = Vec::new();
    for (idx, pos) in positions.iter().enumerate() {
        if policy.eligible_for_fill(idx) && !grow.by_corner.contains_key(&idx) {
            tree.add(&[pos.x, pos.y], slot_to_corner.len() as u64);
            slot_to_corner.push(idx);
        }
    }
    (tree, slot_to_corner)
}

/// Cells to try: every unlabelled cell inside the labelled bbox plus
/// `±1` rows / columns just outside.
fn enumerate_fill_cells(grow: &GrowResult) -> Vec<(i32, i32)> {
    use std::collections::HashSet;
    let mut out: HashSet<(i32, i32)> = HashSet::new();
    let (mut min_i, mut max_i, mut min_j, mut max_j) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    for &(i, j) in grow.labelled.keys() {
        min_i = min_i.min(i);
        max_i = max_i.max(i);
        min_j = min_j.min(j);
        max_j = max_j.max(j);
    }

    // Interior gap fill: every unlabelled cell inside the bbox.
    for j in min_j..=max_j {
        for i in min_i..=max_i {
            if !grow.labelled.contains_key(&(i, j)) {
                out.insert((i, j));
            }
        }
    }

    // Line extrapolation: ±1 beyond the bbox ends, at every row
    // and column that has any labelled member.
    for j in min_j..=max_j {
        out.insert((min_i - 1, j));
        out.insert((max_i + 1, j));
    }
    for i in min_i..=max_i {
        out.insert((i, min_j - 1));
        out.insert((i, max_j + 1));
    }

    // Determinism: the fill pass attaches corners greedily in scan
    // order, and two adjacent candidate cells can compete for the same
    // boundary corner (whichever cell is visited first claims it). A
    // `HashSet` iteration order is randomized per process, so returning
    // the raw set order makes the labelled boundary extent vary run to
    // run for identical input. Sort by `(i, j)` to pin the scan order;
    // this is byte-stable across runs and does not change the outcome
    // for inputs whose attachments are uncontested (the common case for
    // the seed-and-grow caller, which is already order-invariant).
    let mut cells: Vec<(i32, i32)> = out.into_iter().collect();
    cells.sort_unstable();
    cells
}

/// Per-iteration context for the fill pass.
///
/// Bundles the references that every per-cell call would otherwise
/// have to re-thread. Each iteration of [`fill_grid_holes`] builds one
/// of these once and reuses it for all candidate cells.
struct FillCtx<'a, V: SquareAttachPolicy> {
    positions: &'a [Point2<f32>],
    cell_size: f32,
    params: &'a FillParams,
    tree: &'a KdTree<f32, 2>,
    slot_to_corner: &'a [usize],
    policy: &'a V,
}

fn try_fill_cell<V: SquareAttachPolicy>(
    cell: (i32, i32),
    grow: &mut GrowResult,
    ctx: &FillCtx<'_, V>,
) -> Option<usize> {
    let positions = ctx.positions;
    let cell_size = ctx.cell_size;
    let params = ctx.params;
    let tree = ctx.tree;
    let slot_to_corner = ctx.slot_to_corner;
    let policy = ctx.policy;
    // Collect 8-connected labelled neighbours of `cell`. For interior
    // gaps this will usually be 4+; for line extensions it will be 1
    // (the last corner of the line) up to 3 (with diagonals from
    // adjacent labelled rows).
    let neighbours = collect_labelled_neighbours(cell, 1, &grow.labelled, positions);
    if neighbours.is_empty() {
        return None;
    }

    // Adaptive prediction: each labelled neighbour contributes a
    // finite-difference local-step from its own labelled peers when
    // available, falling back to the global `(u, v) × cell_size` step
    // otherwise.
    let pred = predict_from_neighbours(
        cell,
        &neighbours,
        grow.axis_i,
        grow.axis_j,
        cell_size,
        &grow.labelled,
        positions,
    );

    // Optional caller-defined label required at this cell.
    let required_label = policy.required_label_at(cell.0, cell.1);

    // Candidate search.
    let search_r = params.attach_search_rel * cell_size;
    let r2 = search_r * search_r;
    let mut hits: Vec<(usize, f32)> = Vec::new();
    for nn in tree
        .within_unsorted::<SquaredEuclidean>(&[pred.x, pred.y], r2)
        .into_iter()
    {
        let slot = nn.item as usize;
        let idx = slot_to_corner[slot];
        if grow.by_corner.contains_key(&idx) {
            continue;
        }
        if let Some(req) = required_label {
            if policy.label_of(idx) != Some(req) {
                continue;
            }
        }
        if matches!(
            policy.accept_candidate(idx, cell, pred, &neighbours),
            Admit::Reject
        ) {
            continue;
        }
        hits.push((idx, nn.distance.sqrt()));
    }
    hits.sort_by(|a, b| a.1.total_cmp(&b.1));

    let candidate_idx = match hits.len() {
        0 => return None,
        1 => hits[0].0,
        _ => {
            let d0 = hits[0].1.max(f32::EPSILON);
            let d1 = hits[1].1;
            if d1 / d0 < params.attach_ambiguity_factor {
                return None;
            }
            hits[0].0
        }
    };

    // Edge-invariant check via the policy's fill-aware variant.
    // At least one cardinal labelled neighbour must accept the
    // candidate; a policy can swap the scalar
    // `cell_size` for a directional median here when its component
    // is strongly anisotropic.
    if !any_cardinal_fill_edge_ok(
        candidate_idx,
        cell,
        positions,
        &grow.labelled,
        policy,
        cell_size,
    ) {
        return None;
    }

    // Attach.
    grow.labelled.insert(cell, candidate_idx);
    grow.by_corner.insert(candidate_idx, cell);
    Some(candidate_idx)
}

fn any_cardinal_fill_edge_ok<V: SquareAttachPolicy>(
    c_idx: usize,
    pos: (i32, i32),
    positions: &[Point2<f32>],
    labelled: &std::collections::HashMap<(i32, i32), usize>,
    policy: &V,
    cell_size: f32,
) -> bool {
    let mut found_any = false;
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (pos.0 + di, pos.1 + dj);
        if let Some(&n_idx) = labelled.get(&neigh) {
            found_any = true;
            let ctx = FillEdgeCtx {
                candidate_idx: c_idx,
                neighbour_idx: n_idx,
                at_candidate: pos,
                at_neighbour: neigh,
                labelled,
                positions,
                cell_size,
            };
            if policy.fill_edge_ok(ctx) {
                return true;
            }
        }
    }
    !found_any
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detect::advanced::square::grow::{Admit, LabelledNeighbour};
    use std::collections::HashMap;

    /// Trivial policy: every corner eligible, no label constraint,
    /// accept everything, accept all edges.
    struct OpenValidator;

    impl SquareAttachPolicy for OpenValidator {
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
    fn fill_pass_attaches_interior_hole() {
        // 3×3 grid; (1, 1) un-labelled, surrounded by 8 neighbours.
        let s = 20.0_f32;
        let mut positions: Vec<Point2<f32>> = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                positions.push(Point2::new(50.0 + i as f32 * s, 50.0 + j as f32 * s));
            }
        }
        let mut labelled: HashMap<(i32, i32), usize> = HashMap::new();
        let mut by_corner: HashMap<usize, (i32, i32)> = HashMap::new();
        for j in 0..3 {
            for i in 0..3 {
                if (i, j) == (1, 1) {
                    continue;
                }
                let idx = (j * 3 + i) as usize;
                labelled.insert((i, j), idx);
                by_corner.insert(idx, (i, j));
            }
        }
        let mut grow = GrowResult {
            labelled,
            by_corner,
            ambiguous: Default::default(),
            holes: Default::default(),
            axis_i: nalgebra::Vector2::new(1.0, 0.0),
            axis_j: nalgebra::Vector2::new(0.0, 1.0),
            rebase_i_mod2: 0,
            rebase_j_mod2: 0,
        };
        let stats = fill_grid_holes(
            &positions,
            &mut grow,
            s,
            &FillParams::default(),
            &OpenValidator,
        );
        assert_eq!(stats.added, 1);
        assert_eq!(grow.labelled.get(&(1, 1)), Some(&4));
        assert_eq!(stats.attached_indices, vec![4]);
        assert_eq!(stats.attached_cells, vec![(1, 1)]);
    }
}
