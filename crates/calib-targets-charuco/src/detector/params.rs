use crate::board::CharucoBoardSpec;
use calib_targets_aruco::ScanDecodeConfig;
use calib_targets_chessboard::{ChessboardParams, GridGraphParams};
use calib_targets_core::{ChessCornerParams, RefinerConfig, SaddlePointConfig};
use serde::{Deserialize, Serialize};

/// Configuration for the ChArUco detector.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoDetectorParams {
    /// Pixels per board square in the canonical sampling space.
    pub px_per_square: f32,
    /// Chessboard detection parameters.
    pub chessboard: ChessboardParams,
    /// ChArUco board parameters
    pub charuco: CharucoBoardSpec,
    /// Grid graph parameters.
    pub graph: GridGraphParams,
    /// Marker scan parameters.
    ///
    /// `CharucoDetectorParams::for_board` uses a slightly smaller inset
    /// (`inset_frac = 0.06`) to improve real-image robustness. If
    /// `scan.marker_size_rel <= 0.0`, it is filled from the board spec.
    pub scan: ScanDecodeConfig,
    /// Maximum Hamming distance for marker matching.
    pub max_hamming: u8,
    /// Minimal number of marker inliers needed to accept the alignment.
    pub min_marker_inliers: usize,
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
}

fn default_grid_smoothness_threshold_rel() -> f32 {
    0.05
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
        refiner: RefinerConfig::SaddlePoint(SaddlePointConfig::default()),
        ..ChessCornerParams::default()
    }
}

pub(crate) fn to_chess_params(params: &ChessCornerParams) -> chess_corners_core::ChessParams {
    let mut out = chess_corners_core::ChessParams::default();
    out.use_radius10 = params.use_radius10;
    out.descriptor_use_radius10 = params.descriptor_use_radius10;
    out.threshold_rel = params.threshold_rel;
    out.threshold_abs = params.threshold_abs;
    out.nms_radius = params.nms_radius;
    out.min_cluster_size = params.min_cluster_size;
    out.refiner = to_refiner_kind(&params.refiner);
    out
}

fn to_refiner_kind(refiner: &RefinerConfig) -> chess_corners_core::RefinerKind {
    match refiner {
        RefinerConfig::CenterOfMass(cfg) => {
            chess_corners_core::RefinerKind::CenterOfMass(chess_corners_core::CenterOfMassConfig {
                radius: cfg.radius,
            })
        }
        RefinerConfig::Forstner(cfg) => {
            chess_corners_core::RefinerKind::Forstner(chess_corners_core::ForstnerConfig {
                radius: cfg.radius,
                min_trace: cfg.min_trace,
                min_det: cfg.min_det,
                max_condition_number: cfg.max_condition_number,
                max_offset: cfg.max_offset,
            })
        }
        RefinerConfig::SaddlePoint(cfg) => {
            chess_corners_core::RefinerKind::SaddlePoint(chess_corners_core::SaddlePointConfig {
                radius: cfg.radius,
                det_margin: cfg.det_margin,
                max_offset: cfg.max_offset,
                min_abs_det: cfg.min_abs_det,
            })
        }
    }
}

impl CharucoDetectorParams {
    /// Build a reasonable default configuration for the given board.
    pub fn for_board(charuco: &CharucoBoardSpec) -> Self {
        let chessboard = ChessboardParams {
            min_corner_strength: 0.5,
            min_corners: 32,
            expected_rows: Some(charuco.rows - 1),
            expected_cols: Some(charuco.cols - 1),
            completeness_threshold: 0.05,
            ..ChessboardParams::default()
        };

        let graph = GridGraphParams::default();

        let scan = ScanDecodeConfig {
            marker_size_rel: charuco.marker_size_rel,
            inset_frac: 0.06,
            // Lower than the default (0.85) — downstream alignment validation
            // rejects false positives, so a looser bar here improves recall on
            // blurry or unevenly-lit images.
            min_border_score: 0.75,
            ..ScanDecodeConfig::default()
        };

        let max_hamming = charuco.dictionary.max_correction_bits.min(2);

        Self {
            px_per_square: 60.0,
            chessboard,
            charuco: *charuco,
            graph,
            scan,
            max_hamming,
            min_marker_inliers: 8,
            grid_smoothness_threshold_rel: 0.05,
            corner_validation_threshold_rel: 0.08,
            corner_redetect_params: default_redetect_params(),
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
            _ => panic!("refiner kind mismatch"),
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
            refiner: RefinerConfig::Forstner(ForstnerConfig {
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
            RefinerConfig::CenterOfMass(CenterOfMassConfig { radius: 6 }),
            RefinerConfig::Forstner(ForstnerConfig {
                radius: 3,
                min_trace: 10.0,
                min_det: 0.25,
                max_condition_number: 128.0,
                max_offset: 1.0,
            }),
            RefinerConfig::SaddlePoint(SaddlePointConfig {
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
