//! Marker emission under a chosen board alignment.
//!
//! Given the winning [`GridAlignment`], walks the candidate cells, looks up the
//! marker each maps to on the board, and emits a [`MarkerDetection`] with
//! rectified + image-space geometry and a calibrated confidence score.

use super::hypothesis::rotation_index_for;
use super::score_matrix::ScoreMatrix;
use crate::board::CharucoBoard;
use calib_targets_aruco::{rotate_code_u64, CellSamples, MarkerCell, MarkerDetection};
use calib_targets_core::{cell_rect_corners_at, Coord, GridAlignment};
#[cfg(feature = "tracing")]
use tracing::instrument;

#[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
pub(super) fn emit_markers(
    board: &CharucoBoard,
    cells: &[MarkerCell],
    samples: &[Option<CellSamples>],
    matrix: &ScoreMatrix,
    alignment: &GridAlignment,
    px_per_square: f32,
) -> Vec<MarkerDetection> {
    let mut out = Vec::new();
    for (ci, cell) in cells.iter().enumerate() {
        let bc = alignment.map(cell.gc.u, cell.gc.v);
        let Some(expected_id) = board.marker_id_at(bc) else {
            continue;
        };
        let rot = rotation_index_for(&alignment.transform);
        let s = matrix.score(ci, expected_id, rot);
        if !s.is_finite() {
            continue;
        }
        let Some(samp) = samples[ci].as_ref() else {
            continue;
        };
        let dict = board.spec().dictionary;
        let bits = dict.marker_size();
        let base = dict.codes()[expected_id as usize];
        let observed_code = rotate_code_u64(base, bits, rot);
        let gc = rotate_gc_top_left(cell.gc, rot);

        // Rectified-pixel cell corners: a `px_per_square × px_per_square`
        // square anchored at the cell's pre-rotation top-left in the
        // rectified canvas. Giving downstream consumers (FFI / JSON /
        // overlays) real cell geometry rather than a unit square lets them
        // place and draw the cell in the rectified frame.
        let corners_rect = cell_rect_corners_at(cell.gc, px_per_square);

        let m = MarkerDetection {
            id: expected_id,
            gc,
            rotation: rot,
            hamming: 0,
            score: sigmoid01(s / samples_bit_count(samp) as f32),
            border_score: samp.border_black_fraction,
            code: observed_code,
            inverted: false,
            corners_rect,
            corners_img: Some(cell.corners_img),
        };
        out.push(m);
    }
    out
}

fn samples_bit_count(s: &CellSamples) -> usize {
    (s.bits_per_side * s.bits_per_side).max(1)
}

fn sigmoid01(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn rotate_gc_top_left(gc0: Coord, rot: u8) -> Coord {
    match rot & 3 {
        0 => gc0,
        1 => Coord::new(gc0.u + 1, gc0.v),
        2 => Coord::new(gc0.u + 1, gc0.v + 1),
        3 => Coord::new(gc0.u, gc0.v + 1),
        _ => gc0,
    }
}
