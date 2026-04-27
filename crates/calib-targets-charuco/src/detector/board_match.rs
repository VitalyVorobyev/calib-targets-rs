//! Board-level ChArUco matcher: compute a per-cell × per-marker-id soft-bit
//! log-likelihood score matrix, enumerate D4 rotation × integer-translation
//! board hypotheses, pick the one that maximises the aggregate weighted
//! score, and re-emit markers under that constraint.
//!
//! Invoked by [`crate::CharucoDetector`] when
//! [`crate::CharucoParams::use_board_level_matcher`] is `true`; replaces the
//! rotation-vote + translation-vote alignment from [`crate::alignment`].

use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use calib_targets_aruco::{rotate_code_u64, sample_cell, CellSamples, MarkerCell, MarkerDetection};
use calib_targets_core::{
    GrayImageView, GridAlignment, GridCoords, GridTransform, GRID_TRANSFORMS_D4,
};
use nalgebra::Point2;
use serde::Serialize;

/// Configuration of the board-level matcher.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BoardMatchConfig {
    pub px_per_square: f32,
    pub bit_likelihood_slope: f32,
    pub per_bit_floor: f32,
    pub alignment_min_margin: f32,
    pub cell_weight_border_threshold: f32,
}

impl Default for BoardMatchConfig {
    fn default() -> Self {
        Self {
            px_per_square: 60.0,
            bit_likelihood_slope: 36.0,
            per_bit_floor: -6.0,
            alignment_min_margin: 0.05,
            cell_weight_border_threshold: 0.5,
        }
    }
}

/// Structured diagnostics produced by the board-level matcher. Serialised
/// per-frame by the sweep runner for Python overlays.
#[derive(Clone, Debug, Serialize, Default)]
pub struct BoardMatchDiagnostics {
    pub cells: Vec<CellDiag>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chosen: Option<DiagHypothesis>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_up: Option<DiagHypothesis>,
    pub margin: f32,
    pub total_hypotheses: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection: Option<RejectReason>,
    pub board_cols: u32,
    pub board_rows: u32,
    pub bits_per_side: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct CellDiag {
    pub gc: GridCoords,
    pub corners_img: [[f32; 2]; 4],
    pub sampled: bool,
    pub otsu: u8,
    pub border_black: f32,
    pub weight: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapped_bc: Option<[i32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_id: Option<u32>,
    pub expected_score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best: Option<CellBestMatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub expected_bit_ll: Vec<f32>,
    pub interior_means: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct CellBestMatch {
    pub marker_id: u32,
    pub rotation: u8,
    pub score: f32,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct DiagHypothesis {
    pub rotation: u8,
    pub translation: [i32; 2],
    pub score: f32,
    pub contributing_cells: usize,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RejectReason {
    NoCells,
    EmptyBoard,
    TranslationWindowEmpty,
    MarginBelowGate { margin: f32, required: f32 },
    NoEmittedMarkers,
}

/// Dense per-cell × per-marker-id × per-rotation score matrix.
struct ScoreMatrix {
    num_markers: usize,
    scores: Vec<f32>,
    weights: Vec<f32>,
}

impl ScoreMatrix {
    #[inline]
    fn idx(&self, cell: usize, marker: u32, rot: u8) -> usize {
        cell * self.num_markers * 4 + (marker as usize) * 4 + rot as usize
    }

    #[inline]
    fn score(&self, cell: usize, marker: u32, rot: u8) -> f32 {
        self.scores[self.idx(cell, marker, rot)]
    }
}

/// Full-diagnostic entry point. Returns `(match_result, diagnostics)`.
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
    let spec = board.spec();
    let bits = spec.dictionary.marker_size;

    let mut diag = BoardMatchDiagnostics {
        board_cols: spec.cols,
        board_rows: spec.rows,
        bits_per_side: bits,
        ..BoardMatchDiagnostics::default()
    };

    if cells.is_empty() {
        diag.rejection = Some(RejectReason::NoCells);
        return (None, diag);
    }

    let samples: Vec<Option<CellSamples>> = cells
        .iter()
        .map(|c| sample_cell(image, c, cfg.px_per_square, scan_cfg, bits))
        .collect();

    let matrix = match build_score_matrix(board, &samples, cfg) {
        Some(m) => m,
        None => {
            diag.rejection = Some(RejectReason::EmptyBoard);
            fill_per_cell(&mut diag, cells, &samples, None, bits);
            return (None, diag);
        }
    };

    let (best, runner_up, total) = match enumerate_hypotheses(board, cells, &matrix) {
        Some(x) => x,
        None => {
            diag.rejection = Some(RejectReason::TranslationWindowEmpty);
            fill_per_cell(&mut diag, cells, &samples, Some(&matrix), bits);
            return (None, diag);
        }
    };
    diag.total_hypotheses = total;
    diag.chosen = Some(best);
    diag.runner_up = runner_up;
    diag.margin = margin_from_scores(best.score, runner_up.map(|h| h.score));

    let chosen_align = hypothesis_to_alignment(&best);
    fill_per_cell(&mut diag, cells, &samples, Some(&matrix), bits);
    fill_expected_from_board(&mut diag, board, &chosen_align, &matrix, bits);

    if diag.margin < cfg.alignment_min_margin {
        diag.rejection = Some(RejectReason::MarginBelowGate {
            margin: diag.margin,
            required: cfg.alignment_min_margin,
        });
        return (None, diag);
    }

    let markers = emit_markers(
        board,
        cells,
        &samples,
        &matrix,
        &chosen_align,
        cfg.px_per_square,
    );
    if markers.is_empty() {
        diag.rejection = Some(RejectReason::NoEmittedMarkers);
        return (None, diag);
    }

    let n_markers = markers.len();
    log::debug!(
        "board-level matcher: {} markers emitted, alignment margin = {:.3}",
        n_markers,
        diag.margin,
    );

    (
        Some((
            markers,
            CharucoAlignment {
                alignment: chosen_align,
                marker_inliers: (0..n_markers).collect(),
            },
        )),
        diag,
    )
}

fn margin_from_scores(best: f32, runner_up: Option<f32>) -> f32 {
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

fn hypothesis_to_alignment(h: &DiagHypothesis) -> GridAlignment {
    GridAlignment {
        transform: GRID_TRANSFORMS_D4[h.rotation as usize],
        translation: h.translation,
    }
}

fn enumerate_hypotheses(
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
        let mapped: Vec<GridCoords> = cells
            .iter()
            .map(|c| transform.apply(c.gc.i, c.gc.j))
            .collect();
        let Some((tx_lo, tx_hi, ty_lo, ty_hi)) = translation_window(&mapped, cols, rows) else {
            continue;
        };
        any_window = true;

        for tx in tx_lo..=tx_hi {
            for ty in ty_lo..=ty_hi {
                let (score, contributing) =
                    score_hypothesis(board, matrix, &mapped, [tx, ty], rot_idx);
                total += 1;
                let h = DiagHypothesis {
                    rotation: rot_idx,
                    translation: [tx, ty],
                    score,
                    contributing_cells: contributing,
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
        let bc = alignment.map(cell.gc.i, cell.gc.j);
        cell.mapped_bc = Some([bc.i, bc.j]);
        if bc.i < 0 || bc.j < 0 || bc.i >= cols || bc.j >= rows {
            continue;
        }
        let Some(id) = board.marker_id_at(bc) else {
            continue;
        };
        cell.expected_id = Some(id);
        cell.expected_score = matrix.score(ci, id, rot);
        if cell.sampled && !cell.interior_means.is_empty() {
            let base = dict.codes[id as usize];
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
    for m in 0..matrix.num_markers {
        for rot in 0..4u8 {
            let s = matrix.score(ci, m as u32, rot);
            if !s.is_finite() {
                continue;
            }
            let cand = CellBestMatch {
                marker_id: m as u32,
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

fn build_score_matrix(
    board: &CharucoBoard,
    samples: &[Option<CellSamples>],
    cfg: &BoardMatchConfig,
) -> Option<ScoreMatrix> {
    let dict = board.spec().dictionary;
    let bits = dict.marker_size;
    let num_markers = board.marker_count();
    if num_markers == 0 {
        return None;
    }
    let num_cells = samples.len();

    let mut rotated_codes: Vec<[u64; 4]> = Vec::with_capacity(num_markers);
    for (id, _) in board.iter_marker_positions() {
        let base = dict.codes[id as usize];
        rotated_codes.push([
            base,
            rotate_code_u64(base, bits, 1),
            rotate_code_u64(base, bits, 2),
            rotate_code_u64(base, bits, 3),
        ]);
    }

    let n_interior = bits * bits;
    let mut scores = vec![f32::NEG_INFINITY; num_cells * num_markers * 4];
    let mut weights = vec![0.0f32; num_cells];

    for (ci, maybe) in samples.iter().enumerate() {
        let Some(s) = maybe else {
            continue;
        };
        weights[ci] = cell_weight(s, cfg);
        let border = s.border_bits;
        let cells_per_side = s.cells_per_side;

        let mut interior_means = vec![0u8; n_interior];
        for by in 0..bits {
            for bx in 0..bits {
                let cx = border + bx;
                let cy = border + by;
                interior_means[by * bits + bx] = s.mean_grid[cy * cells_per_side + cx];
            }
        }
        let thresh = s.otsu_threshold as f32;
        let slope_over_255 = cfg.bit_likelihood_slope / 255.0;

        for (m_idx, codes) in rotated_codes.iter().enumerate() {
            for rot in 0..4u8 {
                let code = codes[rot as usize];
                let mut total = 0.0f32;
                for (k, &mean) in interior_means.iter().enumerate().take(n_interior) {
                    let expected = ((code >> k) & 1) as u8;
                    let expected_sign = if expected == 1 { 1.0 } else { -1.0 };
                    let logit = expected_sign * slope_over_255 * (thresh - mean as f32);
                    total += log_sigmoid(logit).max(cfg.per_bit_floor);
                }
                let idx = ci * num_markers * 4 + m_idx * 4 + rot as usize;
                scores[idx] = total;
            }
        }
    }

    Some(ScoreMatrix {
        num_markers,
        scores,
        weights,
    })
}

fn cell_weight(s: &CellSamples, cfg: &BoardMatchConfig) -> f32 {
    if cfg.cell_weight_border_threshold <= 0.0 {
        return 1.0;
    }
    let ratio = s.border_black_fraction / cfg.cell_weight_border_threshold;
    ratio.clamp(0.0, 1.0)
}

fn log_sigmoid(x: f32) -> f32 {
    if x >= 0.0 {
        -(1.0 + (-x).exp()).ln()
    } else {
        x - (1.0 + x.exp()).ln()
    }
}

fn translation_window(mapped: &[GridCoords], cols: i32, rows: i32) -> Option<(i32, i32, i32, i32)> {
    let mut min_x = i32::MAX;
    let mut max_x = i32::MIN;
    let mut min_y = i32::MAX;
    let mut max_y = i32::MIN;
    for g in mapped {
        min_x = min_x.min(g.i);
        max_x = max_x.max(g.i);
        min_y = min_y.min(g.j);
        max_y = max_y.max(g.j);
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
    mapped: &[GridCoords],
    translation: [i32; 2],
    rot: u8,
) -> (f32, usize) {
    let mut total = 0.0f32;
    let mut contributing = 0usize;
    for (ci, g) in mapped.iter().enumerate() {
        let bc = GridCoords {
            i: g.i + translation[0],
            j: g.j + translation[1],
        };
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

fn emit_markers(
    board: &CharucoBoard,
    cells: &[MarkerCell],
    samples: &[Option<CellSamples>],
    matrix: &ScoreMatrix,
    alignment: &GridAlignment,
    px_per_square: f32,
) -> Vec<MarkerDetection> {
    let mut out = Vec::new();
    for (ci, cell) in cells.iter().enumerate() {
        let bc = alignment.map(cell.gc.i, cell.gc.j);
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
        let bits = dict.marker_size;
        let base = dict.codes[expected_id as usize];
        let observed_code = rotate_code_u64(base, bits, rot);
        let gc = rotate_gc_top_left(cell.gc, rot);

        // Rectified-pixel cell corners: a `px_per_square × px_per_square`
        // square anchored at the cell's pre-rotation top-left in the
        // rectified canvas. Mirrors the legacy decode path
        // (calib-targets-aruco scan.rs ~line 487) so downstream consumers
        // (FFI / JSON / overlays) get real geometry instead of a unit
        // square.
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

fn cell_rect_corners_at(gc: GridCoords, px_per_square: f32) -> [Point2<f32>; 4] {
    let x0 = gc.i as f32 * px_per_square;
    let y0 = gc.j as f32 * px_per_square;
    let s = px_per_square;
    [
        Point2::new(x0, y0),
        Point2::new(x0 + s, y0),
        Point2::new(x0 + s, y0 + s),
        Point2::new(x0, y0 + s),
    ]
}

fn samples_bit_count(s: &CellSamples) -> usize {
    (s.bits_per_side * s.bits_per_side).max(1)
}

fn sigmoid01(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn rotation_index_for(transform: &GridTransform) -> u8 {
    for (i, t) in GRID_TRANSFORMS_D4.iter().enumerate().take(4) {
        if t == transform {
            return i as u8;
        }
    }
    0
}

fn rotate_gc_top_left(gc0: GridCoords, rot: u8) -> GridCoords {
    match rot & 3 {
        0 => gc0,
        1 => GridCoords {
            i: gc0.i + 1,
            j: gc0.j,
        },
        2 => GridCoords {
            i: gc0.i + 1,
            j: gc0.j + 1,
        },
        3 => GridCoords {
            i: gc0.i,
            j: gc0.j + 1,
        },
        _ => gc0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_sigmoid_matches_reference() {
        for &x in &[-5.0f32, -1.0, 0.0, 1.0, 5.0] {
            let a = log_sigmoid(x);
            let b = (1.0 / (1.0 + (-x).exp())).ln();
            assert!((a - b).abs() < 1e-5, "log_sigmoid({x}) = {a}, expected {b}");
        }
    }

    #[test]
    fn translation_window_clamps_to_board() {
        let mapped = [
            GridCoords { i: 0, j: 0 },
            GridCoords { i: 1, j: 1 },
            GridCoords { i: 2, j: 2 },
        ];
        let win = translation_window(&mapped, 5, 5).unwrap();
        assert_eq!(win, (0, 2, 0, 2));
    }

    #[test]
    fn translation_window_rejects_oversize() {
        let mapped = [GridCoords { i: 0, j: 0 }, GridCoords { i: 10, j: 10 }];
        assert!(translation_window(&mapped, 5, 5).is_none());
    }
}
