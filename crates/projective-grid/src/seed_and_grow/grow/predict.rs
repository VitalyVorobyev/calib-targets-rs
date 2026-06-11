//! Per-neighbour grid-step prediction geometry for [`bfs_grow`](super::bfs_grow).
//!
//! This module owns the pure-geometry half of the grow: collecting the
//! labelled neighbours around a boundary cell, predicting that cell's image
//! position by locally linearising the grid at each neighbour (rather than
//! trusting the seed's global `(u, v, cell_size)`), the extrapolation-vs-
//! interpolation test that widens the search at the labelled-set frontier,
//! and the local finite-difference step estimator. No policy, no BFS queue,
//! no KD-tree — those live in [`super`](crate::seed_and_grow::grow). Tier:
//! advanced engine (semver-exempt pre-1.0).

use nalgebra::{Point2, Vector2};
use std::collections::HashMap;

use super::LabelledNeighbour;

pub(crate) fn collect_labelled_neighbours(
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
/// labelled neighbours are available.
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
/// infinite weight; in practice [`bfs_grow`](super::bfs_grow) never
/// enqueues such a neighbour (they're already labelled), but for robustness
/// we treat `Δi = Δj = 0` as weight 1.0 to avoid `NaN`.
pub(crate) fn predict_from_neighbours(
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
pub(crate) fn is_extrapolating(target: (i32, i32), neighbours: &[LabelledNeighbour]) -> bool {
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
fn local_step_at(
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
