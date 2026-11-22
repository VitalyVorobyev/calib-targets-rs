//! Opt-in board-matcher diagnostics (`diagnostics` feature).
//!
//! Fills the [`BoardMatchDiagnostics`] record (defined in
//! [`crate::diagnostics`]) with the full evidence the board matcher produced:
//! per-cell scores and samples, the chosen / runner-up hypotheses, the
//! acceptance margin, and the rejection reason. Implemented as a [`MatchSink`] over the shared
//! [`match_board_core`] orchestration, so it reuses the exact scoring /
//! hypothesis / emit logic of the production path and adds only the per-cell
//! fills — no duplicated matcher logic, and zero cost on the production build
//! (this whole module is compiled out when the feature is off).

use super::hypothesis::{rotation_index_for, DiagHypothesis};
use super::score_matrix::ScoreMatrix;
use super::{match_board_core, BoardMatchConfig, MatchSink};
use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use crate::diagnostics::{BoardMatchDiagnostics, CellBestMatch, CellDiag, RejectReason};
use calib_targets_aruco::{rotate_code_u64, CellSamples, MarkerCell, MarkerDetection};
use calib_targets_core::{log_sigmoid, GrayImageView, GridAlignment};
#[cfg(feature = "tracing")]
use tracing::instrument;

/// Full-diagnostic entry point. Returns `(match_result, diagnostics)`.
///
/// Shares [`match_board_core`] with the production
/// [`super::match_board`] (no duplicated scoring / hypothesis logic) and
/// additionally fills a [`BoardMatchDiagnostics`] record.
#[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
pub(crate) fn match_board_diag(
    image: &GrayImageView<'_>,
    cells: &[MarkerCell],
    board: &CharucoBoard,
    scan_cfg: &calib_targets_aruco::ScanDecodeConfig,
    cfg: &BoardMatchConfig,
) -> (
    Option<(Vec<MarkerDetection>, CharucoAlignment)>,
    BoardMatchDiagnostics,
) {
    let mut diag = BoardMatchDiagnostics::default();
    let result = match_board_core(image, cells, board, scan_cfg, cfg, &mut diag);
    (result, diag)
}

/// The `diagnostics`-feature sink: fills the [`BoardMatchDiagnostics`] record
/// at each touchpoint of [`match_board_core`].
impl MatchSink for BoardMatchDiagnostics {
    fn set_header(&mut self, cols: u32, rows: u32, bits: usize) {
        self.board_cols = cols;
        self.board_rows = rows;
        self.bits_per_side = bits;
    }

    fn reject_no_cells(&mut self) {
        self.rejection = Some(RejectReason::NoCells);
    }

    fn reject_empty_board(
        &mut self,
        cells: &[MarkerCell],
        samples: &[Option<CellSamples>],
        bits: usize,
    ) {
        self.rejection = Some(RejectReason::EmptyBoard);
        fill_per_cell(self, cells, samples, None, bits);
    }

    fn reject_translation_window(
        &mut self,
        cells: &[MarkerCell],
        samples: &[Option<CellSamples>],
        matrix: &ScoreMatrix,
        bits: usize,
    ) {
        self.rejection = Some(RejectReason::TranslationWindowEmpty);
        fill_per_cell(self, cells, samples, Some(matrix), bits);
    }

    fn record_hypotheses(
        &mut self,
        best: &DiagHypothesis,
        runner_up: Option<&DiagHypothesis>,
        total: usize,
        margin: f32,
    ) {
        self.total_hypotheses = total;
        self.chosen = Some(*best);
        self.runner_up = runner_up.copied();
        self.margin = margin;
    }

    fn fill_cells_and_expected(
        &mut self,
        board: &CharucoBoard,
        cells: &[MarkerCell],
        samples: &[Option<CellSamples>],
        matrix: &ScoreMatrix,
        alignment: &GridAlignment,
        bits: usize,
    ) {
        fill_per_cell(self, cells, samples, Some(matrix), bits);
        fill_expected_from_board(self, board, alignment, matrix, bits);
    }

    fn reject_chance(&mut self, evidence: f32, chance_level: f32) {
        self.rejection = Some(RejectReason::EvidenceAtChance {
            evidence,
            chance_level,
        });
    }

    fn reject_margin(&mut self, margin: f32, required: f32) {
        self.rejection = Some(RejectReason::MarginBelowGate { margin, required });
    }

    fn reject_no_markers(&mut self) {
        self.rejection = Some(RejectReason::NoEmittedMarkers);
    }
}

#[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
fn fill_per_cell(
    diag: &mut BoardMatchDiagnostics,
    cells: &[MarkerCell],
    samples: &[Option<CellSamples>],
    matrix: Option<&ScoreMatrix>,
    bits: usize,
) {
    diag.cells.reserve(cells.len());
    for (ci, cell) in cells.iter().enumerate() {
        let s_opt = samples[ci].as_ref();
        let sampled = s_opt.is_some();
        let (otsu, border_black) = match s_opt {
            Some(s) => (s.otsu_threshold, s.border_black_fraction),
            None => (0u8, 0.0),
        };
        let weight = matrix.map(|m| m.weights[ci]).unwrap_or(0.0);
        let interior_means = extract_interior_means(s_opt, bits);
        let best = matrix
            .and_then(|m| best_match_for_cell(m, ci))
            .filter(|b| b.score.is_finite());

        diag.cells.push(CellDiag {
            gc: cell.gc,
            corners_img: [
                [cell.corners_img[0].x, cell.corners_img[0].y],
                [cell.corners_img[1].x, cell.corners_img[1].y],
                [cell.corners_img[2].x, cell.corners_img[2].y],
                [cell.corners_img[3].x, cell.corners_img[3].y],
            ],
            sampled,
            otsu,
            border_black,
            weight,
            mapped_bc: None,
            expected_id: None,
            expected_score: f32::NAN,
            best,
            expected_bit_ll: Vec::new(),
            interior_means,
        });
    }
}

#[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
fn fill_expected_from_board(
    diag: &mut BoardMatchDiagnostics,
    board: &CharucoBoard,
    alignment: &GridAlignment,
    matrix: &ScoreMatrix,
    bits: usize,
) {
    let cols = diag.board_cols as i32;
    let rows = diag.board_rows as i32;
    let rot = rotation_index_for(alignment);
    let dict = board.spec().dictionary;

    for (ci, cell) in diag.cells.iter_mut().enumerate() {
        let bc = alignment.apply(cell.gc);
        cell.mapped_bc = Some([bc.u, bc.v]);
        if bc.u < 0 || bc.v < 0 || bc.u >= cols || bc.v >= rows {
            continue;
        }
        let Some(id) = board.marker_id_at(bc) else {
            continue;
        };
        cell.expected_id = Some(id);
        cell.expected_score = matrix.score(ci, id, rot);
        if cell.sampled && !cell.interior_means.is_empty() {
            let base = dict.codes()[id as usize];
            let code = rotate_code_u64(base, bits, rot);
            cell.expected_bit_ll = per_bit_log_likelihood(cell, code, bits);
        }
    }
}

fn per_bit_log_likelihood(cell: &CellDiag, expected_code: u64, bits: usize) -> Vec<f32> {
    let n = bits * bits;
    let mut out = Vec::with_capacity(n);
    let thresh = cell.otsu as f32;
    const KAPPA_OVER_255: f32 = 12.0 / 255.0;
    for k in 0..n {
        if k >= cell.interior_means.len() {
            out.push(0.0);
            continue;
        }
        let expected = ((expected_code >> k) & 1) as u8;
        let sign = if expected == 1 { 1.0 } else { -1.0 };
        let logit = sign * (thresh - cell.interior_means[k] as f32) * KAPPA_OVER_255;
        out.push(log_sigmoid(logit));
    }
    out
}

fn extract_interior_means(s: Option<&CellSamples>, bits: usize) -> Vec<u8> {
    let Some(s) = s else {
        return Vec::new();
    };
    let border = s.border_bits;
    let cells_per_side = s.cells_per_side;
    let mut out = Vec::with_capacity(bits * bits);
    for by in 0..bits {
        for bx in 0..bits {
            let cx = border + bx;
            let cy = border + by;
            out.push(s.mean_grid[cy * cells_per_side + cx]);
        }
    }
    out
}

fn best_match_for_cell(matrix: &ScoreMatrix, ci: usize) -> Option<CellBestMatch> {
    let mut best: Option<CellBestMatch> = None;
    for slot in 0..matrix.num_markers {
        for rot in 0..4u8 {
            let s = matrix.score_slot(ci, slot, rot);
            if !s.is_finite() {
                continue;
            }
            let cand = CellBestMatch {
                marker_id: matrix.marker_ids[slot],
                rotation: rot,
                score: s,
            };
            if best.map(|b| s > b.score).unwrap_or(true) {
                best = Some(cand);
            }
        }
    }
    best
}
