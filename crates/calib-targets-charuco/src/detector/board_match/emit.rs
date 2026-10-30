//! Marker emission under a chosen board alignment.
//!
//! Given the winning [`GridAlignment`], walks the candidate cells, looks up the
//! marker each maps to on the board, and emits a [`MarkerDetection`] with
//! rectified + image-space geometry and a calibrated confidence score.
//!
//! A marker is emitted only where its own cell evidences it: the cell must
//! carry weight (a marker-like border) and its bits must favour the expected
//! marker over chance. The alignment alone never puts a marker in a cell — a
//! plain square or an unreadable cell the alignment happens to map onto a
//! marker position is not reported as a marker. The reported `code` and
//! `hamming` are the bits actually read from the cell, so a consumer can
//! check every marker against its pixels.

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
        let bc = alignment.apply(cell.gc);
        let Some(expected_id) = board.marker_id_at(bc) else {
            continue;
        };
        let rot = rotation_index_for(alignment);
        let s = matrix.score(ci, expected_id, rot);
        if !s.is_finite() {
            continue;
        }
        let Some(samp) = samples[ci].as_ref() else {
            continue;
        };
        if matrix.weights[ci] <= 0.0 || matrix.evidence(ci, expected_id, rot) <= 0.0 {
            continue;
        }
        let dict = board.spec().dictionary;
        let bits = dict.marker_size();
        let expected_code = rotate_code_u64(dict.codes()[expected_id as usize], bits, rot);
        let observed_code = read_code(samp);
        let hamming = (observed_code ^ expected_code).count_ones() as u8;
        let gc = rotate_gc_top_left(cell.gc, rot);

        // Rectified-pixel cell corners: a `px_per_square × px_per_square`
        // square anchored at the cell's pre-rotation top-left in the
        // rectified canvas. Giving downstream consumers (FFI / JSON /
        // overlays) real cell geometry rather than a unit square lets them
        // place and draw the cell in the rectified frame.
        let corners_rect = cell_rect_corners_at(cell.gc, px_per_square);

        let m = MarkerDetection::new(expected_id, gc, rot, hamming, observed_code, corners_rect)
            .with_scores(
                sigmoid01(s / samples_bit_count(samp) as f32),
                samp.border_black_fraction,
            )
            .with_corners_img(cell.corners_img);
        out.push(m);
    }
    out
}

/// The cell's interior bits read against its own Otsu threshold (row-major,
/// black = 1) — the hard-decision counterpart of the soft-bit score, in the
/// bit order of the dictionary codes.
fn read_code(s: &CellSamples) -> u64 {
    let bits = s.bits_per_side;
    let mut code = 0u64;
    for by in 0..bits {
        for bx in 0..bits {
            let mean = s.mean_grid[(s.border_bits + by) * s.cells_per_side + s.border_bits + bx];
            if mean < s.otsu_threshold {
                code |= 1 << (by * bits + bx);
            }
        }
    }
    code
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

#[cfg(test)]
mod tests {
    use super::super::score_matrix::build_score_matrix;
    use super::super::BoardMatchConfig;
    use super::*;
    use crate::board::{CharucoBoardSpec, MarkerLayout};
    use calib_targets_aruco::builtins;
    use calib_targets_core::GRID_TRANSFORMS_D4;
    use nalgebra::Point2;

    const OTSU: u8 = 100;

    /// A cell showing `code` (row-major, black = 1) inside a black border.
    fn showing(code: u64) -> CellSamples {
        let mut mean_grid = vec![OTSU - 40; 36];
        for k in 0..16 {
            let (by, bx) = (k / 4, k % 4);
            if (code >> k) & 1 == 0 {
                mean_grid[(1 + by) * 6 + 1 + bx] = OTSU + 40;
            }
        }
        CellSamples::new(6, 4, 1, mean_grid, OTSU, 1.0)
    }

    /// A cell is reported as the marker the alignment expects only when its
    /// own bits favour that marker, and the reported code is what it read.
    #[test]
    fn emits_only_markers_their_cells_show_with_the_bits_read() {
        let board = CharucoBoard::new(
            CharucoBoardSpec::new(6, 6, 1.0, 0.75, builtins::DICT_4X4_50)
                .with_marker_layout(MarkerLayout::OpenCvCharuco),
        )
        .expect("valid board");
        let codes = board.spec().dictionary.codes();
        let (a, b) = (Coord::new(1, 0), Coord::new(3, 0));
        let id_a = board.marker_id_at(a).expect("marker square");
        let id_b = board.marker_id_at(b).expect("marker square");
        let flipped = codes[id_a as usize] ^ 1;
        let cells = [
            MarkerCell::new(a, [Point2::new(0.0, 0.0); 4]),
            MarkerCell::new(b, [Point2::new(0.0, 0.0); 4]),
        ];
        // Cell `a` shows its marker with one bit misread; cell `b` shows
        // another marker's complement, which contradicts the one it is
        // aligned to.
        let samples = [
            Some(showing(flipped)),
            Some(showing(!codes[id_b as usize] & 0xFFFF)),
        ];
        let matrix =
            build_score_matrix(&board, &samples, &BoardMatchConfig::default()).expect("matrix");
        let identity = GRID_TRANSFORMS_D4[0].with_translation([0, 0]);

        let markers = emit_markers(&board, &cells, &samples, &matrix, &identity, 60.0);
        assert_eq!(
            markers.len(),
            1,
            "only the cell showing its marker is emitted"
        );
        assert_eq!(markers[0].id, id_a);
        assert_eq!(markers[0].code, flipped);
        assert_eq!(markers[0].hamming, 1);
    }
}
