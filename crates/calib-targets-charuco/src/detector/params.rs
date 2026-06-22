use crate::board::CharucoBoardSpec;
use calib_targets_aruco::ScanDecodeConfig;
use calib_targets_chessboard::{AdvancedTuning, DetectorParams};
use chess_corners::low_level::{ChessParams as ChessCornerParams, RefinerKind};
use chess_corners::SaddlePointConfig;
use serde::{Deserialize, Serialize};

/// Configuration for the ChArUco detector.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoParams {
    /// Pixels per board square in the canonical sampling space.
    #[serde(default = "default_px_per_square")]
    pub px_per_square: f32,
    /// Chessboard detection parameters.
    ///
    /// ChArUco runs on the **topological** grid builder (the workspace
    /// default). The `min_corner_strength` floor set by
    /// [`CharucoParams::for_board`] keeps marker-bit saddles out of the grid,
    /// so the topological cell test labels ChArUco grids cleanly, and the
    /// marker-decode stages downstream of grid construction are
    /// builder-agnostic. The decode is precision-clean on the topological grid
    /// (zero self-consistency wrong-ids on the private flagship sweep).
    #[serde(default)]
    pub chessboard: DetectorParams,
    /// ChArUco board parameters
    #[serde(alias = "charuco")]
    pub board: CharucoBoardSpec,
    /// Marker scan parameters.
    ///
    /// `CharucoParams::for_board` uses a slightly smaller inset
    /// (`inset_frac = 0.06`) to improve real-image robustness. If
    /// `scan.marker_size_rel <= 0.0`, it is filled from the board spec.
    #[serde(default)]
    pub scan: ScanDecodeConfig,
    /// Maximum Hamming distance for marker matching.
    #[serde(default)]
    pub max_hamming: u8,
    /// Minimal number of marker inliers needed to accept the alignment.
    #[serde(default = "default_min_marker_inliers")]
    pub min_marker_inliers: usize,
    /// Minimum marker inliers for secondary (non-largest) components.
    ///
    /// Lower than [`Self::min_marker_inliers`] because even a few markers suffice
    /// to confirm alignment for a small grid fragment.
    #[serde(default = "default_min_secondary_marker_inliers")]
    pub min_secondary_marker_inliers: usize,
    /// Relative threshold for local grid smoothness pre-filter.
    ///
    /// Each grid corner's position is predicted from its immediate neighbors
    /// via midpoint averaging.  If the actual position deviates by more than
    /// `grid_smoothness_threshold_rel * px_per_square` pixels, the corner is
    /// re-detected locally or removed.
    ///
    /// Set to `f32::INFINITY` to disable.
    /// Default: `0.05` (3 px at 60 px/sq).
    #[serde(default = "default_grid_smoothness_threshold_rel")]
    pub grid_smoothness_threshold_rel: f32,
    /// Relative threshold for marker-constrained corner validation.
    ///
    /// A detected ChArUco corner is considered a false corner if its pixel
    /// position deviates from the marker-predicted seed by more than
    /// `corner_validation_threshold_rel * px_per_square` pixels.
    ///
    /// Set to `f32::INFINITY` to disable validation entirely.
    /// Typical value: `0.08` (8 % of a board square side, ~5 px at 60 px/sq).
    #[serde(default = "default_corner_validation_threshold_rel")]
    pub corner_validation_threshold_rel: f32,
    /// ChESS detector parameters used for local corner re-detection.
    ///
    /// When validation identifies a false corner, these parameters control
    /// the ChESS response computation and subpixel refinement in a small
    /// patch centred on the marker-predicted seed position.
    ///
    /// Not serialised — reconstructed from defaults on deserialisation.
    #[serde(skip)]
    pub corner_redetect_params: ChessCornerParams,
    /// Replace the per-marker hard-threshold decode + rotation/translation
    /// vote alignment with a board-level soft-bit log-likelihood matcher
    /// (see `docs/algorithms/charuco_concept.md`).
    ///
    /// When `true`, the detector computes a per-cell × per-marker-id score
    /// matrix, enumerates (D4 rotation × integer translation) board
    /// hypotheses, and picks the one that maximises Σᵢ wᵢ · sᵢ(m_{p_i(H)}).
    ///
    /// Default: `false` (legacy rotation + translation vote alignment).
    /// Opt in by setting this to `true` when decoding difficult targets
    /// such as small-cell AprilTag boards; on our internal regression set
    /// the board-level matcher recovers frames the legacy vote drops and
    /// removes the residual wrong-id labels.
    #[serde(default = "default_use_board_level_matcher")]
    pub use_board_level_matcher: bool,
    /// Logistic slope κ used in the soft-bit log-likelihood when
    /// [`Self::use_board_level_matcher`] is `true`. Larger = more confident
    /// per bit; 8–16 is a reasonable range.
    ///
    /// **Unstable:** this board-level-matcher tuning knob is **NOT covered by
    /// semver** and may be retuned, retyped, or removed between minor versions
    /// as the matcher evolves. Leave it at [`Default`] unless tuning against a
    /// specific dataset with evidence.
    #[serde(default = "default_bit_likelihood_slope")]
    pub bit_likelihood_slope: f32,
    /// Clip floor applied to each per-bit log-likelihood term before
    /// summing across bits, so a single wildly-wrong bit cannot dominate
    /// a cell score.
    ///
    /// **Unstable:** this board-level-matcher tuning knob is **NOT covered by
    /// semver** and may be retuned, retyped, or removed between minor versions
    /// as the matcher evolves. Leave it at [`Default`] unless tuning against a
    /// specific dataset with evidence.
    #[serde(default = "default_per_bit_floor")]
    pub per_bit_floor: f32,
    /// Minimum `(best − runner-up) / |best|` margin required for the
    /// board-level matcher to accept a hypothesis. Below this, detection
    /// is rejected rather than mislabelled.
    ///
    /// **Unstable:** this board-level-matcher tuning knob is **NOT covered by
    /// semver** and may be retuned, retyped, or removed between minor versions
    /// as the matcher evolves. Leave it at [`Default`] unless tuning against a
    /// specific dataset with evidence.
    #[serde(default = "default_alignment_min_margin")]
    pub alignment_min_margin: f32,
    /// Border-black fraction threshold below which a cell's weight is
    /// attenuated linearly toward 0 in the board-level score.
    ///
    /// **Unstable:** this board-level-matcher tuning knob is **NOT covered by
    /// semver** and may be retuned, retyped, or removed between minor versions
    /// as the matcher evolves. Leave it at [`Default`] unless tuning against a
    /// specific dataset with evidence.
    #[serde(default = "default_cell_weight_border_threshold")]
    pub cell_weight_border_threshold: f32,
}

fn default_grid_smoothness_threshold_rel() -> f32 {
    0.05
}

fn default_corner_validation_threshold_rel() -> f32 {
    0.08
}

fn default_px_per_square() -> f32 {
    60.0
}

fn default_min_marker_inliers() -> usize {
    // Board-appropriate floor (the board-level matcher is the default; see
    // `default_use_board_level_matcher`). The legacy vote matcher wants 8.
    1
}

fn default_min_secondary_marker_inliers() -> usize {
    1
}

fn default_bit_likelihood_slope() -> f32 {
    // Empirically tuned on a 22×22 DICT_4X4_1000 private dataset and a
    // 68×68 DICT_APRILTAG_36h10 (3× upscaled) private dataset:
    // κ=36 is the minimum value that clears every frame on both datasets
    // with zero self-consistency wrong-ids.
    // Smaller κ compresses the per-bit logit and lets runner-up
    // hypotheses nearly tie the top; larger κ does not change outcomes.
    36.0
}

fn default_per_bit_floor() -> f32 {
    -6.0
}

fn default_alignment_min_margin() -> f32 {
    0.05
}

fn default_cell_weight_border_threshold() -> f32 {
    0.5
}

fn default_use_board_level_matcher() -> bool {
    true
}

/// Build the ChESS parameters used for local re-detection inside a small ROI.
///
/// Lower threshold and looser cluster requirement compared to the global scan,
/// because we already know approximately where the true corner should be.
pub(crate) fn default_redetect_params() -> ChessCornerParams {
    let mut params = ChessCornerParams::default();
    params.threshold_rel = 0.05;
    params.nms_radius = 2;
    params.min_cluster_size = 1;
    params.refiner = RefinerKind::SaddlePoint(SaddlePointConfig::default());
    params
}

/// Convert a `ChessCornerParams` into the upstream
/// `chess_corners::low_level::ChessParams`.
///
/// Since `ChessCornerParams` is now a re-export of
/// `chess_corners::low_level::ChessParams`, this is an identity-like operation.
pub(crate) fn to_chess_params(params: &ChessCornerParams) -> chess_corners::low_level::ChessParams {
    params.clone()
}

impl CharucoParams {
    /// Three-config sweep preset built on top of
    /// [`DetectorParams::sweep_default`] (canonical + tighter + looser
    /// chessboard tolerances).
    pub fn sweep_for_board(board: &CharucoBoardSpec) -> Vec<Self> {
        let base = Self::for_board(board);
        // The ChArUco base sets a strength floor (stable field) and disables
        // the standalone final edge-shape gate (advanced knob). Re-apply both
        // to every recall-bracketed sweep config, preserving each config's own
        // advanced overrides (the tighter / looser tolerances).
        let base_strength = base.chessboard.min_corner_strength;
        let base_edge_shape_check = base
            .chessboard
            .effective_tuning()
            .enable_final_edge_shape_check;
        DetectorParams::sweep_default()
            .into_iter()
            .map(|mut chessboard| {
                // `sweep_default()` already builds topological configs (the
                // workspace default), which is the builder ChArUco runs on.
                chessboard.min_corner_strength = base_strength;
                let mut advanced = chessboard.effective_tuning().into_owned();
                advanced.enable_final_edge_shape_check = base_edge_shape_check;
                chessboard = chessboard.with_advanced(advanced);
                Self {
                    chessboard,
                    ..base.clone()
                }
            })
            .collect()
    }

    /// Build a reasonable default configuration for the given board.
    ///
    /// The chessboard detector is scale-invariant and discovers cell
    /// size from the seed itself, so v1's `expected_rows` / `expected_cols`
    /// / `completeness_threshold` / explicit `min_corners` gates are no
    /// longer needed — ChArUco's marker-driven alignment is the geometry
    /// gate.
    pub fn for_board(board: &CharucoBoardSpec) -> Self {
        let mut chessboard = DetectorParams::default();
        // `DetectorParams::default()` already selects the topological builder,
        // which is the builder ChArUco runs on (see `CharucoParams::chessboard`).
        // Absolute ChESS-strength floor. In defocused regions the corner
        // detector fires weakly on ArUco-marker bit saddles that align with
        // a grid extrapolation; those false corners are grid-consistent
        // (they pass the homography validation) and so survive into the
        // ChArUco product as biased corners — geometry alone cannot reject
        // them (the weak-frontier ceiling). Cutting weak corners *before*
        // the grid grows keeps the grid out of the blur entirely, which on
        // the private regression set clears every reviewed marker-bit false
        // corner (zero product-false), and — because the marker cells are
        // sampled from that grid — also *improves* marker decode (fewer
        // spurious cells), recovering frames the looser floor lost. The cost
        // is the weakest blurred-margin corners (least useful for
        // calibration). The board alignment is a *location* tool, never a
        // corner-drop gate, so this floor — not marker presence — is the
        // precision lever.
        chessboard.min_corner_strength = 33.0;
        // ChArUco has marker-ID and board-alignment validation after
        // chessboard grid recovery. Keep the chessboard component
        // recall-oriented here; the standalone chessboard detector
        // still enables the stricter final edge-shape gate by default.
        // `enable_final_edge_shape_check` is an advanced knob, so route it
        // through an `AdvancedTuning` override.
        let mut advanced = AdvancedTuning::default();
        advanced.enable_final_edge_shape_check = false;
        chessboard = chessboard.with_advanced(advanced);

        let scan = ScanDecodeConfig::default()
            .with_marker_size_rel(board.marker_size_rel)
            .with_inset_frac(0.06)
            // Lower than the default (0.85) — downstream alignment validation
            // rejects false positives, so a looser bar here improves recall on
            // blurry or unevenly-lit images.
            .with_min_border_score(0.75);

        let max_hamming = board.dictionary.max_correction_bits().min(2);

        Self {
            px_per_square: 60.0,
            chessboard,
            board: *board,
            scan,
            max_hamming,
            // Board-level soft-LL matcher is the default (see
            // `use_board_level_matcher` below): it is robust on partial /
            // blurry views where the legacy rotation+translation vote needs
            // many markers, so it takes board-appropriate low inlier floors
            // (1 primary / 1 secondary, gated by `alignment_min_margin`).
            // The legacy fallback wants the higher 8 / 2 floors; callers
            // opting into it should raise these.
            min_marker_inliers: 1,
            min_secondary_marker_inliers: 1,
            grid_smoothness_threshold_rel: 0.05,
            corner_validation_threshold_rel: 0.08,
            corner_redetect_params: default_redetect_params(),
            // Default to the board-level soft-LL matcher: on the internal
            // 22×22 regression set it is 120/120 with zero self-consistency
            // wrong-ids, vs the legacy vote matcher's lower recall and
            // higher wrong-id noise. The legacy matcher stays a documented
            // opt-in (`use_board_level_matcher = false`).
            use_board_level_matcher: true,
            bit_likelihood_slope: default_bit_likelihood_slope(),
            per_bit_floor: default_per_bit_floor(),
            alignment_min_margin: default_alignment_min_margin(),
            cell_weight_border_threshold: default_cell_weight_border_threshold(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_redetect_params_uses_saddle_point_refiner() {
        let params = default_redetect_params();
        assert!((params.threshold_rel - 0.05).abs() < 1e-6);
        assert_eq!(params.nms_radius, 2);
        assert_eq!(params.min_cluster_size, 1);
        assert!(
            matches!(params.refiner, RefinerKind::SaddlePoint(_)),
            "expected SaddlePoint refiner, got {:?}",
            params.refiner,
        );
    }

    #[test]
    fn to_chess_params_is_identity() {
        // Since ChessCornerParams IS chess_corners::low_level::ChessParams, to_chess_params
        // should round-trip perfectly.
        let mut params = ChessCornerParams::default();
        params.threshold_rel = 0.3;
        params.nms_radius = 4;
        let converted = to_chess_params(&params);
        assert!((converted.threshold_rel - 0.3).abs() < 1e-6);
        assert_eq!(converted.nms_radius, 4);
    }
}
