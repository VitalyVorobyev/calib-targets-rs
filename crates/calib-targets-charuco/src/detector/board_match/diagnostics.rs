//! Opt-in board-matcher diagnostics (`diagnostics` feature).
//!
//! Carries the full evidence the board matcher produced: per-cell scores and
//! samples, the chosen / runner-up hypotheses, the acceptance margin, and the
//! rejection reason. Implemented as a [`MatchSink`] over the shared
//! [`match_board_core`] orchestration, so it reuses the exact scoring /
//! hypothesis / emit logic of the production path and adds only the per-cell
//! fills — no duplicated matcher logic, and zero cost on the production build
//! (this whole module is compiled out when the feature is off).

use super::hypothesis::{rotation_index_for, DiagHypothesis};
use super::score_matrix::ScoreMatrix;
use super::{match_board_core, BoardMatchConfig, MatchSink};
use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use calib_targets_aruco::{rotate_code_u64, CellSamples, MarkerCell, MarkerDetection};
use calib_targets_core::{log_sigmoid, Coord, GrayImageView, GridAlignment};
use serde::Serialize;
#[cfg(feature = "tracing")]
use tracing::instrument;

/// Structured diagnostics produced by the board-level matcher: per-cell and
/// per-hypothesis matcher evidence for debugging and overlay rendering.
#[derive(Clone, Debug, Serialize, Default)]
#[non_exhaustive]
pub struct BoardMatchDiagnostics {
    /// Per-cell diagnostic record for every cell the matcher considered.
    pub cells: Vec<CellDiag>,
    /// The board-placement hypothesis the matcher chose; `None` when the
    /// match was rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen: Option<DiagHypothesis>,
    /// The second-best hypothesis, for margin inspection; `None` when
    /// fewer than two hypotheses were scored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_up: Option<DiagHypothesis>,
    /// Score margin between the chosen and runner-up hypotheses.
    pub margin: f32,
    /// Total number of board-placement hypotheses scored.
    pub total_hypotheses: usize,
    /// Reason the match was rejected; `None` when the match succeeded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection: Option<RejectReason>,
    /// Board width in squares.
    pub board_cols: u32,
    /// Board height in squares.
    pub board_rows: u32,
    /// Number of marker bits per side (the marker's grid resolution).
    pub bits_per_side: usize,
}

/// Per-cell diagnostic record produced by the board matcher.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct CellDiag {
    /// The cell's `(i, j)` grid coordinate.
    pub gc: Coord,
    /// The cell's four corners `[TL, TR, BR, BL]` in image pixels.
    pub corners_img: [[f32; 2]; 4],
    /// `true` when the cell was sampled (it had all four corners).
    pub sampled: bool,
    /// Otsu binarization threshold computed for this cell.
    pub otsu: u8,
    /// Fraction of the cell border that read as black.
    pub border_black: f32,
    /// Weight this cell contributed to hypothesis scoring.
    pub weight: f32,
    /// The board-cell `[col, row]` this cell mapped to under the chosen
    /// hypothesis; `None` when unmapped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapped_bc: Option<[i32; 2]>,
    /// The marker ID expected at the mapped board cell; `None` when the
    /// cell carries no marker.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_id: Option<u32>,
    /// Score of the observed bits against the expected marker.
    pub expected_score: f32,
    /// The best-matching marker for this cell over all IDs and rotations;
    /// `None` when no marker scored.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best: Option<CellBestMatch>,
    /// Per-bit log-likelihoods of the observed bits under the expected
    /// marker; empty when not computed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub expected_bit_ll: Vec<f32>,
    /// Mean interior-bit intensities sampled from the cell.
    pub interior_means: Vec<u8>,
}

/// The best marker match found for a single cell.
#[derive(Clone, Copy, Debug, Serialize)]
#[non_exhaustive]
pub struct CellBestMatch {
    /// Dictionary ID of the best-matching marker.
    pub marker_id: u32,
    /// Rotation, in 90° steps, that produced the best match.
    pub rotation: u8,
    /// Match score (higher is better).
    pub score: f32,
}

/// Reason the board matcher rejected a frame.
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RejectReason {
    /// No grid cells were available to sample.
    NoCells,
    /// The board specification has zero squares.
    EmptyBoard,
    /// No valid board translation could be enumerated.
    TranslationWindowEmpty,
    /// The chosen-vs-runner-up margin fell below the acceptance gate.
    MarginBelowGate {
        /// The observed score margin.
        margin: f32,
        /// The minimum margin required to accept.
        required: f32,
    },
    /// The match produced no markers to emit.
    NoEmittedMarkers,
}

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
    let rot = rotation_index_for(&alignment.transform);
    let dict = board.spec().dictionary;

    for (ci, cell) in diag.cells.iter_mut().enumerate() {
        let bc = alignment.map(cell.gc.u, cell.gc.v);
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
