use crate::board::CharucoBoardSpec;
use calib_targets_aruco::ScanDecodeConfig;
use calib_targets_chessboard::{ChessboardParams, GridGraphParams};
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
        }
    }
}
