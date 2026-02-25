use crate::board::CharucoBoardSpec;
use calib_targets_aruco::ScanDecodeConfig;
use calib_targets_chessboard::{ChessboardParams, GridGraphParams};
use chess_corners_core::{ChessParams, RefinerKind, SaddlePointConfig};
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
    pub corner_redetect_params: ChessParams,
}

/// Build the `ChessParams` used for local re-detection inside a small ROI.
///
/// Lower threshold and looser cluster requirement compared to the global scan,
/// because we already know approximately where the true corner should be.
pub(crate) fn default_redetect_params() -> ChessParams {
    ChessParams {
        threshold_rel: 0.05,
        nms_radius: 2,
        min_cluster_size: 1,
        refiner: RefinerKind::SaddlePoint(SaddlePointConfig::default()),
        ..ChessParams::default()
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
            corner_validation_threshold_rel: 0.08,
            corner_redetect_params: default_redetect_params(),
        }
    }
}
