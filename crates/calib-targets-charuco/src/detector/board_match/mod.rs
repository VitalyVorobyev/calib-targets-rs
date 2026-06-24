//! Board-level ChArUco matcher: compute a per-cell × per-marker-id soft-bit
//! log-likelihood score matrix, enumerate D4 rotation × integer-translation
//! board hypotheses, pick the one that maximises the aggregate weighted
//! score, and re-emit markers under that constraint.
//!
//! The sole marker-to-board matcher, invoked by [`crate::CharucoDetector`];
//! produces the [`CharucoAlignment`] the corner-mapping stage consumes.
//!
//! ## Module layout
//!
//! * [`score_matrix`] — the evidence table (`ScoreMatrix`) and its builder.
//! * [`hypothesis`] — board-placement enumeration / selection and the
//!   [`DiagHypothesis`] selection result.
//! * [`emit`] — marker emission under the chosen alignment.
//! * [`diagnostics`] — opt-in introspection (`diagnostics` feature only).
//!
//! ## Production vs. diagnostics
//!
//! Two entry points share one orchestration core ([`match_board_core`]):
//!
//! * [`match_board`] — the production matcher. Always compiled. Runs the
//!   stages and emits the match with **zero** diagnostic allocation.
//! * [`diagnostics::match_board_diag`] — the introspection matcher, behind the
//!   `diagnostics` feature. Runs the *same* stages and additionally fills a
//!   [`diagnostics::BoardMatchDiagnostics`] record for overlays.
//!
//! The split is a sink: [`match_board_core`] is generic over a [`MatchSink`]
//! and routes every diagnostic touchpoint through it. The production path
//! passes the no-op [`NoDiag`] sink (all hooks inline away); the diagnostics
//! path passes `&mut BoardMatchDiagnostics`.

mod emit;
mod hypothesis;
mod score_matrix;

#[cfg(feature = "diagnostics")]
mod diagnostics;

use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use calib_targets_aruco::{sample_cell, CellSamples, MarkerCell, MarkerDetection};
use calib_targets_core::{GrayImageView, GridAlignment};
#[cfg(feature = "tracing")]
use tracing::instrument;

use emit::emit_markers;
use hypothesis::{enumerate_hypotheses, hypothesis_to_alignment, margin_from_scores};
use score_matrix::{build_score_matrix, ScoreMatrix};

// `DiagHypothesis` is always compiled (the production matcher selects on it).
// Its *public* exposure is gated by `detector/mod.rs`; re-exported `pub` here
// so that gated `pub use` can reach it.
pub use hypothesis::DiagHypothesis;

// The diagnostics types are `pub` in the child module; re-export them `pub`
// (and `match_board_diag` `pub(crate)`) so `detector/mod.rs`'s feature-gated
// `pub use board_match::{...}` reaches the public surface.
#[cfg(feature = "diagnostics")]
pub(crate) use diagnostics::match_board_diag;
#[cfg(feature = "diagnostics")]
pub use diagnostics::{BoardMatchDiagnostics, CellBestMatch, CellDiag, RejectReason};

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

/// Diagnostic sink threaded through [`match_board_core`].
///
/// Every diagnostic touchpoint in the orchestration core calls a hook on this
/// trait. The production path uses [`NoDiag`] (all hooks are empty and inline
/// away, so the hot path allocates nothing); the `diagnostics` path uses
/// `&mut BoardMatchDiagnostics`, whose impl performs the per-cell fills.
///
/// Hook parameters are restricted to always-compiled types so the trait
/// definition (and the [`NoDiag`] impl) compile with the feature off.
///
/// Module-private: implemented by [`NoDiag`] here and by
/// `BoardMatchDiagnostics` in the child [`diagnostics`] module; never escapes
/// `board_match`.
trait MatchSink {
    /// Record the board geometry header (cols, rows, bits-per-side).
    fn set_header(&mut self, _cols: u32, _rows: u32, _bits: usize) {}
    /// The frame was rejected because there were no cells to sample.
    fn reject_no_cells(&mut self) {}
    /// The score matrix could not be built (empty board); fill per-cell with
    /// no matrix context.
    fn reject_empty_board(
        &mut self,
        _cells: &[MarkerCell],
        _samples: &[Option<CellSamples>],
        _bits: usize,
    ) {
    }
    /// No translation window admitted any hypothesis; fill per-cell with the
    /// score matrix context.
    fn reject_translation_window(
        &mut self,
        _cells: &[MarkerCell],
        _samples: &[Option<CellSamples>],
        _matrix: &ScoreMatrix,
        _bits: usize,
    ) {
    }
    /// Record the chosen / runner-up hypotheses, the total scored, and the
    /// margin between the top two.
    fn record_hypotheses(
        &mut self,
        _best: &DiagHypothesis,
        _runner_up: Option<&DiagHypothesis>,
        _total: usize,
        _margin: f32,
    ) {
    }
    /// Fill the full per-cell record plus the expected-from-board annotations
    /// under the chosen alignment. Runs whether or not the match is later
    /// rejected by the margin gate.
    fn fill_cells_and_expected(
        &mut self,
        _board: &CharucoBoard,
        _cells: &[MarkerCell],
        _samples: &[Option<CellSamples>],
        _matrix: &ScoreMatrix,
        _alignment: &GridAlignment,
        _bits: usize,
    ) {
    }
    /// The chosen-vs-runner-up margin fell below the acceptance gate.
    fn reject_margin(&mut self, _margin: f32, _required: f32) {}
    /// The accepted hypothesis emitted no markers.
    fn reject_no_markers(&mut self) {}
}

/// No-op diagnostic sink used by the production [`match_board`] path.
struct NoDiag;
impl MatchSink for NoDiag {}

/// Production board matcher. Returns the matched markers and alignment, or
/// `None` when no hypothesis cleared the margin gate.
///
/// Computes and allocates **no** diagnostics — this is the hot `detect()`
/// path. For introspection use [`diagnostics::match_board_diag`]
/// (`diagnostics` feature).
#[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
pub(crate) fn match_board(
    image: &GrayImageView<'_>,
    cells: &[MarkerCell],
    board: &CharucoBoard,
    scan_cfg: &calib_targets_aruco::ScanDecodeConfig,
    cfg: &BoardMatchConfig,
) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
    match_board_core(image, cells, board, scan_cfg, cfg, &mut NoDiag)
}

/// Shared orchestration core for both [`match_board`] and
/// [`diagnostics::match_board_diag`].
///
/// Runs `sample_cells` → `build_score_matrix` → `enumerate_hypotheses` →
/// margin gate → `emit_markers` and returns the match result. Every
/// diagnostic touchpoint is routed through `sink`; for the production path
/// `sink` is [`NoDiag`] and the hooks compile away.
fn match_board_core<S: MatchSink>(
    image: &GrayImageView<'_>,
    cells: &[MarkerCell],
    board: &CharucoBoard,
    scan_cfg: &calib_targets_aruco::ScanDecodeConfig,
    cfg: &BoardMatchConfig,
    sink: &mut S,
) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
    let spec = board.spec();
    let bits = spec.dictionary.marker_size();
    sink.set_header(spec.cols, spec.rows, bits);

    if cells.is_empty() {
        sink.reject_no_cells();
        return None;
    }

    let samples = sample_cells(image, cells, cfg.px_per_square, scan_cfg, bits);

    let matrix = match build_score_matrix(board, &samples, cfg) {
        Some(m) => m,
        None => {
            sink.reject_empty_board(cells, &samples, bits);
            return None;
        }
    };

    let (best, runner_up, total) = match enumerate_hypotheses(board, cells, &matrix) {
        Some(x) => x,
        None => {
            sink.reject_translation_window(cells, &samples, &matrix, bits);
            return None;
        }
    };
    let margin = margin_from_scores(best.score, runner_up.map(|h| h.score));
    sink.record_hypotheses(&best, runner_up.as_ref(), total, margin);

    let chosen_align = hypothesis_to_alignment(&best);
    sink.fill_cells_and_expected(board, cells, &samples, &matrix, &chosen_align, bits);

    if margin < cfg.alignment_min_margin {
        sink.reject_margin(margin, cfg.alignment_min_margin);
        return None;
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
        sink.reject_no_markers();
        return None;
    }

    let n_markers = markers.len();
    log::debug!(
        "board-level matcher: {} markers emitted, alignment margin = {:.3}",
        n_markers,
        margin,
    );

    Some((
        markers,
        CharucoAlignment {
            alignment: chosen_align,
            marker_inliers: (0..n_markers).collect(),
        },
    ))
}

/// Sample every candidate cell's interior bit grid.
///
/// One [`sample_cell`] per cell, each of which warps the cell into the
/// rectified canvas (4-point homography), samples the per-bit mean grid,
/// and computes the cell's Otsu threshold. Extracted into its own function
/// so the `tracing` span captures the aggregate per-cell aruco sampling cost
/// (homography + sampling + Otsu) in one place.
#[cfg_attr(feature = "tracing", instrument(level = "info", skip_all))]
fn sample_cells(
    image: &GrayImageView<'_>,
    cells: &[MarkerCell],
    px_per_square: f32,
    scan_cfg: &calib_targets_aruco::ScanDecodeConfig,
    bits: usize,
) -> Vec<Option<CellSamples>> {
    cells
        .iter()
        .map(|c| sample_cell(image, c, px_per_square, scan_cfg, bits))
        .collect()
}
