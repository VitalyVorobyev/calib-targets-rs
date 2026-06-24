//! Board-placement hypothesis enumeration and selection.
//!
//! Enumerates the D4 rotation × integer-translation board placements that keep
//! the observed cells on the board, scores each against the [`ScoreMatrix`],
//! and tracks the best and runner-up. [`DiagHypothesis`] is the selection
//! result the production matcher consumes (rotation + translation → alignment,
//! plus the score driving the margin gate).

use super::score_matrix::ScoreMatrix;
use crate::board::CharucoBoard;
use calib_targets_aruco::MarkerCell;
use calib_targets_core::{Coord, GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
#[cfg(feature = "diagnostics")]
use serde::Serialize;
#[cfg(feature = "tracing")]
use tracing::instrument;

/// One scored board-placement hypothesis.
///
/// Always compiled: this is not a diagnostics-only record but the core
/// hypothesis-selection result the production matcher consumes (it is turned
/// into the chosen `GridAlignment` and its `score` drives the margin gate).
/// Only its *exposure* on the public diagnostics surface is feature-gated
/// (see `detector/mod.rs`).
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "diagnostics", derive(Serialize))]
pub struct DiagHypothesis {
    /// Board rotation, in 90° steps.
    pub rotation: u8,
    /// Board `[Δcol, Δrow]` translation on the grid.
    pub translation: [i32; 2],
    /// Aggregate score of this hypothesis across contributing cells.
    pub score: f32,
    /// Number of cells that contributed evidence to this hypothesis.
    ///
    /// Diagnostics-only: the production matcher selects on `score` alone, so
    /// this count is compiled in only with the `diagnostics` feature.
    #[cfg(feature = "diagnostics")]
    pub contributing_cells: usize,
}

pub(super) fn margin_from_scores(best: f32, runner_up: Option<f32>) -> f32 {
    let Some(second) = runner_up else {
        return 1.0;
    };
    if !second.is_finite() {
        return 1.0;
    }
    let delta = (best - second).max(0.0);
    let denom = best.abs().max(second.abs()).max(1e-3);
    delta / denom
}

pub(super) fn hypothesis_to_alignment(h: &DiagHypothesis) -> GridAlignment {
    GridAlignment {
        transform: GRID_TRANSFORMS_D4[h.rotation as usize],
        translation: h.translation,
    }
}

#[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
pub(super) fn enumerate_hypotheses(
    board: &CharucoBoard,
    cells: &[MarkerCell],
    matrix: &ScoreMatrix,
) -> Option<(DiagHypothesis, Option<DiagHypothesis>, usize)> {
    let spec = board.spec();
    let cols = spec.cols as i32;
    let rows = spec.rows as i32;

    let mut best: Option<DiagHypothesis> = None;
    let mut runner_up: Option<DiagHypothesis> = None;
    let mut total = 0usize;
    let mut any_window = false;

    for rot_idx in 0..4u8 {
        let transform = GRID_TRANSFORMS_D4[rot_idx as usize];
        let mapped: Vec<Coord> = cells
            .iter()
            .map(|c| transform.apply(c.gc.u, c.gc.v))
            .collect();
        let Some((tx_lo, tx_hi, ty_lo, ty_hi)) = translation_window(&mapped, cols, rows) else {
            continue;
        };
        any_window = true;

        for tx in tx_lo..=tx_hi {
            for ty in ty_lo..=ty_hi {
                // `_contributing` feeds the diagnostics-only field; the
                // production matcher selects on `score` alone. Underscore-
                // prefixed so it is not flagged unused when the field is gated
                // out (feature off).
                let (score, _contributing) =
                    score_hypothesis(board, matrix, &mapped, [tx, ty], rot_idx);
                total += 1;
                let h = DiagHypothesis {
                    rotation: rot_idx,
                    translation: [tx, ty],
                    score,
                    #[cfg(feature = "diagnostics")]
                    contributing_cells: _contributing,
                };
                match best {
                    None => best = Some(h),
                    Some(b) => {
                        if h.score > b.score {
                            runner_up = Some(b);
                            best = Some(h);
                        } else if runner_up.map(|r| h.score > r.score).unwrap_or(true) {
                            runner_up = Some(h);
                        }
                    }
                }
            }
        }
    }

    if !any_window || best.is_none() {
        return None;
    }
    Some((best.unwrap(), runner_up, total))
}

fn translation_window(mapped: &[Coord], cols: i32, rows: i32) -> Option<(i32, i32, i32, i32)> {
    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    for g in mapped {
        min_x = min_x.min(g.u);
        max_x = max_x.max(g.u);
        min_y = min_y.min(g.v);
        max_y = max_y.max(g.v);
    }
    if min_x == i32::MAX {
        return None;
    }
    let tx_lo = -min_x;
    let tx_hi = cols - 1 - max_x;
    let ty_lo = -min_y;
    let ty_hi = rows - 1 - max_y;
    if tx_lo > tx_hi || ty_lo > ty_hi {
        return None;
    }
    Some((tx_lo, tx_hi, ty_lo, ty_hi))
}

fn score_hypothesis(
    board: &CharucoBoard,
    matrix: &ScoreMatrix,
    mapped: &[Coord],
    translation: [i32; 2],
    rot: u8,
) -> (f32, usize) {
    let mut total = 0.0f32;
    let mut contributing = 0usize;
    for (ci, g) in mapped.iter().enumerate() {
        let bc = Coord::new(g.u + translation[0], g.v + translation[1]);
        let Some(expected_id) = board.marker_id_at(bc) else {
            continue;
        };
        let w = matrix.weights[ci];
        if w <= 0.0 {
            continue;
        }
        let s = matrix.score(ci, expected_id, rot);
        total += w * s;
        contributing += 1;
    }
    // Per-cell scores are log-likelihoods (≤ 0). A hypothesis with no
    // contributing cells would score 0, beating every real hypothesis with
    // negative evidence. Treat zero-evidence as invalid so the comparison
    // loop discards it; diagnostics still observe `contributing == 0`.
    if contributing == 0 {
        return (f32::NEG_INFINITY, 0);
    }
    (total, contributing)
}

/// Index in [`GRID_TRANSFORMS_D4`] of the given transform (its 90°-rotation
/// step), or `0` if not found. Shared by marker emission and diagnostics to
/// recover the rotation from a chosen alignment.
pub(super) fn rotation_index_for(transform: &GridTransform) -> u8 {
    for (i, t) in GRID_TRANSFORMS_D4.iter().enumerate().take(4) {
        if t == transform {
            return i as u8;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translation_window_clamps_to_board() {
        let mapped = [Coord::new(0, 0), Coord::new(1, 1), Coord::new(2, 2)];
        let win = translation_window(&mapped, 5, 5).unwrap();
        assert_eq!(win, (0, 2, 0, 2));
    }

    #[test]
    fn translation_window_rejects_oversize() {
        let mapped = [Coord::new(0, 0), Coord::new(10, 10)];
        assert!(translation_window(&mapped, 5, 5).is_none());
    }
}
