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
    /// Board `[Δcol, Δrow]` translation carrying detected marker cells onto
    /// board cells (a cell translation, not the corner alignment's).
    pub translation: [i32; 2],
    /// Aggregate evidence for this hypothesis: the weighted sum, over the
    /// cells it expects a marker in, of the log-likelihood ratio of the
    /// expected marker against a cell of random bits. Positive when the cells
    /// support the placement.
    pub score: f32,
    /// Number of cells that contributed evidence to this hypothesis.
    ///
    /// Diagnostics-only: the production matcher selects on `score` alone, so
    /// this count is compiled in only with the `diagnostics` feature.
    #[cfg(feature = "diagnostics")]
    pub contributing_cells: usize,
}

/// The chosen placement, its runner-up, and the acceptance margin between them.
pub(super) struct Selection {
    pub(super) best: DiagHypothesis,
    pub(super) runner_up: Option<DiagHypothesis>,
    /// Number of hypotheses scored.
    pub(super) total: usize,
    /// See [`margin`].
    pub(super) margin: f32,
}

/// How decisively `best` beats the runner-up: the evidence gap between them
/// as a share of `best_capacity`, the evidence `best` would collect if every
/// cell it expects a marker in read that marker perfectly
/// (`Σ w · bits² · ln 2`).
///
/// Normalizing by the attainable evidence rather than by the best score keeps
/// the margin meaningful when the evidence itself is weak: placements that
/// differ only in nearly uninformative cells score close to `0`, and their
/// gap relative to the best score could be arbitrarily large, but it is a
/// small share of what their cells could have shown. A placement whose cells
/// do not favour it over chance (`best ≤ 0`) has no margin at all; with no
/// competing placement a supported one has margin `1`.
fn margin(best: f32, best_capacity: f32, runner_up: Option<f32>) -> f32 {
    if best <= 0.0 {
        return 0.0;
    }
    let Some(second) = runner_up.filter(|s| s.is_finite()) else {
        return 1.0;
    };
    (best - second).max(0.0) / best_capacity.max(f32::EPSILON)
}

/// The hypothesis as a *cell* alignment: it maps a marker cell's grid
/// coordinate (its top-left corner) to the board cell it lands on. Convert
/// with [`calib_targets_core::cell_to_corner_alignment`] before relabelling
/// corners.
pub(super) fn hypothesis_to_alignment(h: &DiagHypothesis) -> GridAlignment {
    GRID_TRANSFORMS_D4[h.rotation as usize].with_translation(h.translation)
}

#[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
pub(super) fn enumerate_hypotheses(
    board: &CharucoBoard,
    cells: &[MarkerCell],
    matrix: &ScoreMatrix,
) -> Option<Selection> {
    let spec = board.spec();
    let cols = spec.cols as i32;
    let rows = spec.rows as i32;

    let mut best: Option<(DiagHypothesis, f32)> = None;
    let mut runner_up: Option<DiagHypothesis> = None;
    let mut total = 0usize;

    for rot_idx in 0..4u8 {
        let transform = GRID_TRANSFORMS_D4[rot_idx as usize];
        let mapped: Vec<Coord> = cells.iter().map(|c| transform.apply(c.gc)).collect();
        let Some((tx_lo, tx_hi, ty_lo, ty_hi)) = translation_window(&mapped, cols, rows) else {
            continue;
        };

        for tx in tx_lo..=tx_hi {
            for ty in ty_lo..=ty_hi {
                let scored = score_hypothesis(board, matrix, &mapped, [tx, ty], rot_idx);
                total += 1;
                let h = DiagHypothesis {
                    rotation: rot_idx,
                    translation: [tx, ty],
                    score: scored.evidence,
                    #[cfg(feature = "diagnostics")]
                    contributing_cells: scored.contributing,
                };
                match best {
                    None => best = Some((h, scored.capacity)),
                    Some((b, _)) => {
                        if h.score > b.score {
                            runner_up = Some(b);
                            best = Some((h, scored.capacity));
                        } else if runner_up.map(|r| h.score > r.score).unwrap_or(true) {
                            runner_up = Some(h);
                        }
                    }
                }
            }
        }
    }

    let (best, best_capacity) = best?;
    Some(Selection {
        best,
        runner_up,
        total,
        margin: margin(best.score, best_capacity, runner_up.map(|h| h.score)),
    })
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

/// A hypothesis's evidence and how much evidence its cells could carry.
struct HypothesisScore {
    /// `Σ w · evidence` over the cells the hypothesis expects a marker in.
    evidence: f32,
    /// `Σ w · bits² · ln 2` over the same cells: the evidence they would
    /// carry if each read its expected marker perfectly.
    capacity: f32,
    /// Number of those cells (with non-zero weight).
    contributing: usize,
}

fn score_hypothesis(
    board: &CharucoBoard,
    matrix: &ScoreMatrix,
    mapped: &[Coord],
    translation: [i32; 2],
    rot: u8,
) -> HypothesisScore {
    let mut evidence = 0.0f32;
    let mut capacity = 0.0f32;
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
        // Log-likelihood ratio against the null model, so hypotheses that
        // expect markers in different cells are compared on equal terms
        // (see `ScoreMatrix::evidence`).
        evidence += w * matrix.evidence(ci, expected_id, rot);
        capacity += w * matrix.max_evidence();
        contributing += 1;
    }
    // A hypothesis that expects no marker in any informative cell is not
    // supported by the image at all; treat it as invalid so the comparison
    // loop discards it rather than letting its neutral `0` beat a placement
    // the cells contradict. Diagnostics still observe `contributing == 0`.
    if contributing == 0 {
        evidence = f32::NEG_INFINITY;
    }
    HypothesisScore {
        evidence,
        capacity,
        contributing,
    }
}

/// Index in [`GRID_TRANSFORMS_D4`] of the given transform (its 90°-rotation
/// step), or `0` if not found. Shared by marker emission and diagnostics to
/// recover the rotation from a chosen alignment.
pub(super) fn rotation_index_for(transform: &GridTransform) -> u8 {
    for (i, t) in GRID_TRANSFORMS_D4.iter().enumerate().take(4) {
        if t.matrix() == transform.matrix() {
            return i as u8;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::super::score_matrix::build_score_matrix;
    use super::super::BoardMatchConfig;
    use super::*;
    use crate::board::{CharucoBoardSpec, MarkerLayout};
    use calib_targets_aruco::{builtins, CellSamples};
    use nalgebra::Point2;

    const OTSU: u8 = 100;

    /// Samples of a cell showing `code` (row-major, black = 1) with a black
    /// border: each bit reads `contrast` gray levels to its side of the
    /// threshold.
    fn marker_samples(code: u64, contrast: u8) -> CellSamples {
        let (bits, border, side) = (4, 1, 6);
        let mut mean_grid = vec![OTSU - contrast; side * side];
        for by in 0..bits {
            for bx in 0..bits {
                let black = (code >> (by * bits + bx)) & 1 == 1;
                mean_grid[(border + by) * side + border + bx] = if black {
                    OTSU - contrast
                } else {
                    OTSU + contrast
                };
            }
        }
        CellSamples::new(side, bits, border, mean_grid, OTSU, 1.0)
    }

    /// Samples of a plain, saturated square: every bit sits on the threshold
    /// (it carries no pattern), and one border sample out of twenty read
    /// darker, which gives the cell a small but non-zero weight.
    fn plain_samples() -> CellSamples {
        CellSamples::new(6, 4, 1, vec![OTSU; 36], OTSU, 0.05)
    }

    /// A 4 × 4-cell view of a 6 × 6-square board, placed at board cell
    /// `(1, 1)` without rotation: marker squares show their marker with a
    /// modest contrast (each cell's bits favour it, but not with certainty),
    /// plain squares show nothing.
    fn partial_view(board: &CharucoBoard) -> (Vec<MarkerCell>, Vec<Option<CellSamples>>) {
        let dict = board.spec().dictionary;
        let mut cells = Vec::new();
        let mut samples = Vec::new();
        for v in 0..4 {
            for u in 0..4 {
                let gc = Coord::new(u, v);
                cells.push(MarkerCell::new(gc, [Point2::new(0.0, 0.0); 4]));
                samples.push(Some(match board.marker_id_at(Coord::new(u + 1, v + 1)) {
                    Some(id) => marker_samples(dict.codes()[id as usize], 6),
                    None => plain_samples(),
                }));
            }
        }
        (cells, samples)
    }

    /// The placement the markers support wins, even though every placement
    /// that lays the board's plain squares over the observed markers expects
    /// markers only in the nearly uninformative plain cells. Summing raw
    /// log-likelihoods (all `≤ 0`) would prefer such a placement: it pays a
    /// small weighted penalty in a few plain cells instead of the imperfect
    /// fit of every marker cell, and a ChArUco board labelled through it is
    /// shifted by an odd number of squares.
    #[test]
    fn markers_outweigh_a_placement_that_expects_them_in_plain_squares() {
        let board = CharucoBoard::new(
            CharucoBoardSpec::new(6, 6, 1.0, 0.75, builtins::DICT_4X4_50)
                .with_marker_layout(MarkerLayout::OpenCvCharuco),
        )
        .expect("valid board");
        let (cells, samples) = partial_view(&board);
        let matrix =
            build_score_matrix(&board, &samples, &BoardMatchConfig::default()).expect("matrix");

        let selection = enumerate_hypotheses(&board, &cells, &matrix).expect("hypotheses");
        assert_eq!(
            (selection.best.rotation, selection.best.translation),
            (0, [1, 1]),
            "chose {:?} over the true placement",
            selection.best
        );
        assert!(selection.best.score > 0.0);
        assert!(
            selection.margin >= BoardMatchConfig::default().alignment_min_margin,
            "the true placement must clear the margin gate, got {}",
            selection.margin
        );
    }

    /// Placements that differ only in nearly uninformative cells are not told
    /// apart: their gap is a tiny share of what their cells could show, and a
    /// placement the cells do not favour over chance has no margin at all.
    #[test]
    fn weak_or_unsupported_evidence_has_no_margin() {
        // Two placements scoring 0.05 and 0.01 nats over cells that could
        // have shown 11 nats.
        assert!(margin(0.05, 11.0, Some(0.01)) < 0.01);
        assert_eq!(margin(-3.0, 11.0, Some(-5.0)), 0.0);
        assert_eq!(margin(0.0, 11.0, None), 0.0);
        assert_eq!(margin(5.0, 11.0, None), 1.0);
        // A decisive gap between well-evidenced placements.
        assert!(margin(100.0, 110.0, Some(-20.0)) > 1.0);
    }

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
