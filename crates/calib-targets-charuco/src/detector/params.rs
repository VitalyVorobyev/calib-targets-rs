use crate::board::CharucoBoard;
use calib_targets_aruco::ScanDecodeConfig;
use calib_targets_chessboard::{ChessboardParams, GridGraphParams};

/// Configuration for the ChArUco detector.
#[derive(Clone, Debug)]
pub struct CharucoDetectorParams {
    /// Pixels per board square in the canonical sampling space.
    pub px_per_square: f32,
    /// Chessboard detection parameters.
    pub chessboard: ChessboardParams,
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
    /// If true, build a full rectified mesh image for output/debugging.
    /// This is more expensive than per-cell decoding.
    pub build_rectified_image: bool,
    /// If true, fall back to full rectified decoding when per-cell alignment is weak.
    pub fallback_to_rectified: bool,
}

impl CharucoDetectorParams {
    /// Build a reasonable default configuration for the given board.
    pub fn for_board(board: &CharucoBoard) -> Self {
        let chessboard = ChessboardParams {
            min_corner_strength: 0.5,
            min_corners: 32,
            expected_rows: Some(board.expected_inner_rows()),
            expected_cols: Some(board.expected_inner_cols()),
            completeness_threshold: 0.05,
            ..ChessboardParams::default()
        };

        let graph = GridGraphParams::default();

        let scan = ScanDecodeConfig {
            marker_size_rel: board.spec().marker_size_rel,
            inset_frac: 0.06,
            ..ScanDecodeConfig::default()
        };

        let max_hamming = board.spec().dictionary.max_correction_bits.min(2);

        Self {
            px_per_square: 60.0,
            chessboard,
            graph,
            scan,
            max_hamming,
            min_marker_inliers: 8,
            build_rectified_image: false,
            fallback_to_rectified: true,
        }
    }
}
