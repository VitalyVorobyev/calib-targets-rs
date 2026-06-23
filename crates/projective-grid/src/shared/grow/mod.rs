//! Shared square-lattice growth primitives: candidate search, ambiguity
//! resolution, the per-cell boundary decision, and the [`SquareAttachPolicy`]
//! contract.
//!
//! The growth helpers — KD-tree candidate search, per-neighbour prediction
//! averaging, ambiguity filtering, per-edge gating — are pure geometry and
//! work for any square-grid pattern. Pattern-specific invariants (alternating
//! labels, local evidence checks, caller-specific constraints) plug in via the
//! [`SquareAttachPolicy`] trait.
//!
//! These primitives back the boundary-extension and interior-fill engines
//! ([`crate::shared::grow_extend`], [`crate::shared::extension`],
//! [`crate::shared::fill`]) and the geometry-only recovery schedule
//! ([`crate::shared::recovery_schedule`]). The chessboard crate composes the same
//! primitives directly for its topological recovery path.
//!
//! The policy is asked four questions:
//! - **`is_eligible(idx)`** — can this corner index be considered as
//!   a candidate at all? (typically: accepted by an upstream feature
//!   classifier and not blacklisted by the caller)
//! - **`required_label_at(i, j)`** — what optional caller-defined label is
//!   required at this grid cell? Opaque `u8`; the policy picks the scheme.
//!   `None` means "no label constraint".
//! - **`accept_candidate(idx, at, prediction, neighbours)`** — once
//!   the generic search has found a candidate passing geometric
//!   checks, is it caller-legal?
//! - **`edge_ok(candidate_idx, neighbour_idx, at_cand, at_neigh)`** —
//!   soft per-edge check at attachment time.
//!
//! # Non-goals
//!
//! These primitives do **not** do post-growth validation (line
//! collinearity / local-H residuals). See
//! [`crate::shared::validate`](mod@crate::shared::validate) for
//! that.
//!
//! # Module layout
//!
//! - `params` — the [`SquareAttachPolicy`] contract, the per-candidate
//!   data carriers ([`Admit`], [`LabelledNeighbour`], [`FillEdgeCtx`]),
//!   plus [`GrowParams`] / [`GrowResult`].
//! - `predict` — the per-neighbour prediction geometry.
//! - this module — candidate search / ambiguity resolution and the per-edge
//!   cardinal gate, all re-exporting the above so existing
//!   `crate::shared::grow::*` paths stay flat.

mod params;
mod predict;

pub use params::{
    Admit, FillEdgeCtx, GrowParams, GrowResult, LabelledNeighbour, SquareAttachPolicy,
};
pub(crate) use predict::{collect_labelled_neighbours, is_extrapolating, predict_from_neighbours};

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::Point2;
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) fn enqueue_cardinal_neighbours(
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

pub(crate) fn collect_candidates<V: SquareAttachPolicy>(
    tree: &KdTree<f32, 2>,
    slot_to_corner: &[usize],
    prediction: Point2<f32>,
    search_r: f32,
    policy: &V,
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
            let Some(got) = policy.label_of(idx) else {
                continue;
            };
            if got != req {
                continue;
            }
        }
        let d = nn.distance.sqrt();
        out.push((idx, d));
    }
    // Break exact distance ties by corner index. `within_unsorted` returns
    // candidates in nondeterministic order; the ambiguity check and the
    // first-Accept pick in `choose_unambiguous` are order-sensitive, so without
    // the index tiebreak the attach decision can vary run-to-run.
    out.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    out
}

pub(crate) enum CandidateChoice {
    None,
    Ambiguous,
    Unique(usize),
}

pub(crate) fn choose_unambiguous<V: SquareAttachPolicy>(
    candidates: &[(usize, f32)],
    ambiguity_factor: f32,
    prediction: Point2<f32>,
    positions: &[Point2<f32>],
    policy: &V,
    at: (i32, i32),
    neighbours: &[LabelledNeighbour],
) -> CandidateChoice {
    // Filter by policy in distance order; pick the first Accept.
    // Ambiguity check uses raw geometric ranks (two geometrically-close
    // candidates, regardless of policy opinion).
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
        match policy.accept_candidate(idx, at, prediction, neighbours) {
            Admit::Accept => return CandidateChoice::Unique(idx),
            Admit::Reject => continue,
        }
    }
    CandidateChoice::None
}

pub(crate) fn any_cardinal_edge_ok<V: SquareAttachPolicy>(
    c_idx: usize,
    pos: (i32, i32),
    labelled: &HashMap<(i32, i32), usize>,
    policy: &V,
) -> bool {
    let mut found_any = false;
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (pos.0 + di, pos.1 + dj);
        if let Some(&n_idx) = labelled.get(&neigh) {
            found_any = true;
            if policy.edge_ok(c_idx, n_idx, pos, neigh) {
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
    use nalgebra::Vector2;

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
}
