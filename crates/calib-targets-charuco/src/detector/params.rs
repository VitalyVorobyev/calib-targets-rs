use crate::board::CharucoBoardSpec;
use calib_targets_aruco::ScanDecodeConfig;
use calib_targets_chessboard::DetectorParams;
use calib_targets_core::{ChessCornerParams, RefinerKindConfig, SaddlePointConfig};
use serde::{Deserialize, Serialize};

/// Configuration for the ChArUco detector.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoParams {
    /// Pixels per board square in the canonical sampling space.
    #[serde(default = "default_px_per_square")]
    pub px_per_square: f32,
    /// Chessboard detection parameters.
    ///
    /// Note: the [`graph_build_algorithm`] field is overridden to
    /// [`GraphBuildAlgorithm::ChessboardV2`] inside the ChArUco detector
    /// regardless of the value passed in here. The topological pipeline's
    /// axis-driven cell test cannot reason about marker-bearing cells,
    /// whose embedded bit features perturb the per-corner ChESS axes;
    /// only the seed-and-grow pipeline reliably labels ChArUco grids.
    ///
    /// [`graph_build_algorithm`]: calib_targets_chessboard::DetectorParams::graph_build_algorithm
    /// [`GraphBuildAlgorithm::ChessboardV2`]: calib_targets_chessboard::GraphBuildAlgorithm::ChessboardV2
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
    /// (see `docs/charuco_concept.md`).
    ///
    /// When `true`, the detector computes a per-cell × per-marker-id score
    /// matrix, enumerates (D4 rotation × integer translation) board
    /// hypotheses, and picks the one that maximises Σᵢ wᵢ · sᵢ(m_{p_i(H)}).
    ///
    /// Default: `false` (legacy rotation + translation vote alignment).
    /// Opt in by setting this to `true` when decoding difficult targets
    /// such as small-cell AprilTag boards; the ChArUco regression sweep on
    /// `privatedata/target_0.png` goes from 0/6 to 3/6 detected frames
    /// with the board-level matcher, and the flagship dataset improves
    /// wrong-id count from 3 to 0.
    #[serde(default = "default_use_board_level_matcher")]
    pub use_board_level_matcher: bool,
    /// Logistic slope κ used in the soft-bit log-likelihood when
    /// [`Self::use_board_level_matcher`] is `true`. Larger = more confident
    /// per bit; 8–16 is a reasonable range.
    #[serde(default = "default_bit_likelihood_slope")]
    pub bit_likelihood_slope: f32,
    /// Clip floor applied to each per-bit log-likelihood term before
    /// summing across bits, so a single wildly-wrong bit cannot dominate
    /// a cell score.
    #[serde(default = "default_per_bit_floor")]
    pub per_bit_floor: f32,
    /// Minimum `(best − runner-up) / |best|` margin required for the
    /// board-level matcher to accept a hypothesis. Below this, detection
    /// is rejected rather than mislabelled.
    #[serde(default = "default_alignment_min_margin")]
    pub alignment_min_margin: f32,
    /// Border-black fraction threshold below which a cell's weight is
    /// attenuated linearly toward 0 in the board-level score.
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
    8
}

fn default_min_secondary_marker_inliers() -> usize {
    2
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
    false
}

/// Build the ChESS parameters used for local re-detection inside a small ROI.
///
/// Lower threshold and looser cluster requirement compared to the global scan,
/// because we already know approximately where the true corner should be.
pub(crate) fn default_redetect_params() -> ChessCornerParams {
    ChessCornerParams {
        threshold_rel: 0.05,
        nms_radius: 2,
        min_cluster_size: 1,
        refiner: RefinerKindConfig::SaddlePoint(SaddlePointConfig::default()),
        ..ChessCornerParams::default()
    }
}

pub(crate) fn to_chess_params(params: &ChessCornerParams) -> chess_corners::ChessParams {
    let mut out = chess_corners::ChessParams::default();
    out.use_radius10 = params.use_radius10;
    out.descriptor_use_radius10 = params.descriptor_use_radius10;
    out.threshold_rel = params.threshold_rel;
    out.threshold_abs = params.threshold_abs;
    out.nms_radius = params.nms_radius;
    out.min_cluster_size = params.min_cluster_size;
    out.refiner = to_refiner_kind(&params.refiner);
    out
}

fn to_refiner_kind(refiner: &RefinerKindConfig) -> chess_corners::RefinerKind {
    match refiner {
        RefinerKindConfig::CenterOfMass(cfg) => {
            chess_corners::RefinerKind::CenterOfMass(chess_corners::CenterOfMassConfig {
                radius: cfg.radius,
            })
        }
        RefinerKindConfig::Forstner(cfg) => {
            chess_corners::RefinerKind::Forstner(chess_corners::ForstnerConfig {
                radius: cfg.radius,
                min_trace: cfg.min_trace,
                min_det: cfg.min_det,
                max_condition_number: cfg.max_condition_number,
                max_offset: cfg.max_offset,
            })
        }
        RefinerKindConfig::SaddlePoint(cfg) => {
            chess_corners::RefinerKind::SaddlePoint(chess_corners::SaddlePointConfig {
                radius: cfg.radius,
                det_margin: cfg.det_margin,
                max_offset: cfg.max_offset,
                min_abs_det: cfg.min_abs_det,
            })
        }
        // NOTE: update this adapter when new RefinerKindConfig variants are added upstream.
        _ => unreachable!("unhandled RefinerKindConfig variant — update to_refiner_kind"),
    }
}

impl CharucoParams {
    /// Three-config sweep preset built on top of
    /// [`DetectorParams::sweep_default`] (canonical + tighter + looser
    /// chessboard tolerances).
    pub fn sweep_for_board(board: &CharucoBoardSpec) -> Vec<Self> {
        let base = Self::for_board(board);
        DetectorParams::sweep_default()
            .into_iter()
            .map(|mut chessboard| {
                chessboard.min_corner_strength = base.chessboard.min_corner_strength;
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
        chessboard.min_corner_strength = 0.5;

        let scan = ScanDecodeConfig {
            marker_size_rel: board.marker_size_rel,
            inset_frac: 0.06,
            // Lower than the default (0.85) — downstream alignment validation
            // rejects false positives, so a looser bar here improves recall on
            // blurry or unevenly-lit images.
            min_border_score: 0.75,
            ..ScanDecodeConfig::default()
        };

        let max_hamming = board.dictionary.max_correction_bits.min(2);

        Self {
            px_per_square: 60.0,
            chessboard,
            board: *board,
            scan,
            max_hamming,
            min_marker_inliers: 8,
            min_secondary_marker_inliers: 2,
            grid_smoothness_threshold_rel: 0.05,
            corner_validation_threshold_rel: 0.08,
            corner_redetect_params: default_redetect_params(),
            use_board_level_matcher: false,
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
    use calib_targets_core::{CenterOfMassConfig, ForstnerConfig};

    fn assert_refiner_eq(
        actual: &chess_corners_core::RefinerKind,
        expected: &chess_corners_core::RefinerKind,
    ) {
        match (actual, expected) {
            (
                chess_corners_core::RefinerKind::CenterOfMass(actual),
                chess_corners_core::RefinerKind::CenterOfMass(expected),
            ) => assert_eq!(actual.radius, expected.radius),
            (
                chess_corners_core::RefinerKind::Forstner(actual),
                chess_corners_core::RefinerKind::Forstner(expected),
            ) => {
                assert_eq!(actual.radius, expected.radius);
                assert_eq!(actual.min_trace, expected.min_trace);
                assert_eq!(actual.min_det, expected.min_det);
                assert_eq!(actual.max_condition_number, expected.max_condition_number);
                assert_eq!(actual.max_offset, expected.max_offset);
            }
            (
                chess_corners_core::RefinerKind::SaddlePoint(actual),
                chess_corners_core::RefinerKind::SaddlePoint(expected),
            ) => {
                assert_eq!(actual.radius, expected.radius);
                assert_eq!(actual.det_margin, expected.det_margin);
                assert_eq!(actual.max_offset, expected.max_offset);
                assert_eq!(actual.min_abs_det, expected.min_abs_det);
            }
            _ => unreachable!("refiner kind mismatch"),
        }
    }

    fn assert_chess_params_eq(
        actual: &chess_corners_core::ChessParams,
        expected: &chess_corners_core::ChessParams,
    ) {
        assert_eq!(actual.use_radius10, expected.use_radius10);
        assert_eq!(
            actual.descriptor_use_radius10,
            expected.descriptor_use_radius10
        );
        assert_eq!(actual.threshold_rel, expected.threshold_rel);
        assert_eq!(actual.threshold_abs, expected.threshold_abs);
        assert_eq!(actual.nms_radius, expected.nms_radius);
        assert_eq!(actual.min_cluster_size, expected.min_cluster_size);
        assert_refiner_eq(&actual.refiner, &expected.refiner);
    }

    #[test]
    fn default_redetect_params_match_previous_external_values() {
        let actual = to_chess_params(&default_redetect_params());

        let mut expected = chess_corners_core::ChessParams::default();
        expected.threshold_rel = 0.05;
        // chess-corners 0.6 ships `threshold_abs = Some(0.0)` by default.
        // The ChArUco re-detect path deliberately opts into relative mode
        // (to apply a sensitive 0.05 fraction-of-max threshold), so the
        // converted params clear `threshold_abs` to let `threshold_rel`
        // take effect.
        expected.threshold_abs = None;
        expected.nms_radius = 2;
        expected.min_cluster_size = 1;
        expected.refiner = chess_corners_core::RefinerKind::SaddlePoint(
            chess_corners_core::SaddlePointConfig::default(),
        );

        assert_chess_params_eq(&actual, &expected);
    }

    #[test]
    fn conversion_preserves_non_default_fields() {
        let params = ChessCornerParams {
            use_radius10: true,
            descriptor_use_radius10: Some(false),
            threshold_rel: 0.3,
            threshold_abs: Some(7.5),
            nms_radius: 4,
            min_cluster_size: 3,
            refiner: RefinerKindConfig::Forstner(ForstnerConfig {
                radius: 5,
                min_trace: 12.0,
                min_det: 0.5,
                max_condition_number: 64.0,
                max_offset: 2.0,
            }),
        };

        let actual = to_chess_params(&params);
        let mut expected = chess_corners_core::ChessParams::default();
        expected.use_radius10 = true;
        expected.descriptor_use_radius10 = Some(false);
        expected.threshold_rel = 0.3;
        expected.threshold_abs = Some(7.5);
        expected.nms_radius = 4;
        expected.min_cluster_size = 3;
        expected.refiner =
            chess_corners_core::RefinerKind::Forstner(chess_corners_core::ForstnerConfig {
                radius: 5,
                min_trace: 12.0,
                min_det: 0.5,
                max_condition_number: 64.0,
                max_offset: 2.0,
            });

        assert_chess_params_eq(&actual, &expected);
    }

    #[test]
    fn all_refiner_variants_convert() {
        let variants = [
            RefinerKindConfig::CenterOfMass(CenterOfMassConfig { radius: 6 }),
            RefinerKindConfig::Forstner(ForstnerConfig {
                radius: 3,
                min_trace: 10.0,
                min_det: 0.25,
                max_condition_number: 128.0,
                max_offset: 1.0,
            }),
            RefinerKindConfig::SaddlePoint(SaddlePointConfig {
                radius: 4,
                det_margin: 0.05,
                max_offset: 0.75,
                min_abs_det: 0.025,
            }),
        ];

        for refiner in variants {
            let params = ChessCornerParams {
                refiner,
                ..ChessCornerParams::default()
            };
            let converted = to_chess_params(&params);
            assert_refiner_eq(&converted.refiner, &to_refiner_kind(&params.refiner));
        }
    }
}
