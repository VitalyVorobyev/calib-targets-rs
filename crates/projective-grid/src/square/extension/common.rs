//! Shared per-cell attachment ladder for Stage 6 homography extension.
//!
//! [`try_attach_at_cell`] runs the filter pipeline common to both the
//! global-H and local-H passes: parity gate, ambiguity check,
//! [`GrowValidator::accept_candidate`], and [`GrowValidator::edge_ok`].
//! Callers supply the sorted candidate list (already filtered by
//! `is_eligible` and `by_corner`); this function decides whether to
//! attach the best candidate and reports the outcome.

use crate::square::grow::{Admit, GrowResult, GrowValidator, LabelledNeighbour};
use nalgebra::Point2;
use std::collections::HashMap;

/// Outcome of one call to [`try_attach_at_cell`].
pub(super) enum TryCellResult {
    /// No eligible candidates in the search radius.
    NoCandidates,
    /// ≥ 2 near-equidistant candidates — cannot pick safely.
    Ambiguous,
    /// `accept_candidate` returned `Reject`.
    ValidatorRejected,
    /// No cardinal labelled neighbour passed `edge_ok`.
    EdgeRejected,
    /// Corner attached; callers should update `grow` and counters.
    Attached(usize),
}

/// Run the shared per-cell filter ladder for Stage 6.
///
/// `hits` must already be sorted by distance (ascending), with entries
/// already filtered for `is_eligible` and not in `by_corner`.
///
/// `pred` is the predicted pixel position for `cell` (from whichever
/// homography the caller fit). It is forwarded to `accept_candidate`
/// so the validator can measure the residual.
pub(super) fn try_attach_at_cell<V: GrowValidator>(
    cell: (i32, i32),
    pred: Point2<f32>,
    hits: &[(usize, f32)],
    ambiguity_factor: f32,
    grow: &GrowResult,
    positions: &[Point2<f32>],
    validator: &V,
) -> TryCellResult {
    if hits.is_empty() {
        return TryCellResult::NoCandidates;
    }
    if hits.len() >= 2 {
        let d0 = hits[0].1.max(f32::EPSILON);
        let d1 = hits[1].1;
        if d1 / d0 < ambiguity_factor {
            return TryCellResult::Ambiguous;
        }
    }

    let candidate_idx = hits[0].0;
    let neighbours = collect_labelled_neighbours(cell, &grow.labelled, positions);
    if matches!(
        validator.accept_candidate(candidate_idx, cell, pred, &neighbours),
        Admit::Reject
    ) {
        return TryCellResult::ValidatorRejected;
    }

    if !any_cardinal_edge_ok(candidate_idx, cell, &grow.labelled, validator) {
        return TryCellResult::EdgeRejected;
    }

    TryCellResult::Attached(candidate_idx)
}

/// Collect 8-connected labelled neighbours of `pos`.
pub(super) fn collect_labelled_neighbours(
    pos: (i32, i32),
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> Vec<LabelledNeighbour> {
    let mut out = Vec::new();
    for dj in -1..=1_i32 {
        for di in -1..=1_i32 {
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

/// Check that at least one cardinal labelled neighbour of `pos` has an
/// edge accepted by the validator.
///
/// Returns `true` when there are no cardinal labelled neighbours
/// (safety net — `enumerate_extension_cells` only emits boundary-adjacent
/// cells, so this branch is exercised mainly in tests).
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
    !found_any
}
