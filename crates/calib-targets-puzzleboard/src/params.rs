//! Detector parameters for PuzzleBoard.

use crate::board::PuzzleBoardSpec;
use crate::detector::DecodeConfig;
use calib_targets_chessboard::ChessboardParams;
use calib_targets_core::{ChessCornerParams, RefinerKindConfig, SaddlePointConfig};
use serde::{Deserialize, Serialize};

/// Configuration for the PuzzleBoard detector.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PuzzleBoardParams {
    /// Pixels per board square in the rectified sampling space.
    #[serde(default = "default_px_per_square")]
    pub px_per_square: f32,
    /// Chessboard detection parameters (grid graph, corner filters, …).
    #[serde(default)]
    pub chessboard: ChessboardParams,
    /// Board geometry.
    pub board: PuzzleBoardSpec,
    /// Decoding knobs.
    #[serde(default)]
    pub decode: DecodeConfig,
    /// ChESS detector parameters used for local re-detection of suspicious corners.
    ///
    /// Not serialised — reconstructed from defaults on deserialisation.
    #[serde(skip, default = "default_redetect_params")]
    pub corner_redetect_params: ChessCornerParams,
}

fn default_px_per_square() -> f32 {
    60.0
}

pub(crate) fn default_redetect_params() -> ChessCornerParams {
    ChessCornerParams {
        threshold_rel: 0.05,
        nms_radius: 2,
        min_cluster_size: 1,
        refiner: RefinerKindConfig::SaddlePoint(SaddlePointConfig::default()),
        ..ChessCornerParams::default()
    }
}

impl PuzzleBoardParams {
    /// Reasonable defaults for the given board geometry.
    pub fn for_board(board: &PuzzleBoardSpec) -> Self {
        let chessboard = ChessboardParams {
            min_corner_strength: 0.1,
            min_corners: 20,
            expected_rows: Some(board.inner_rows()),
            expected_cols: Some(board.inner_cols()),
            completeness_threshold: 0.02,
            graph: calib_targets_chessboard::GridGraphParams {
                // PuzzleBoard targets are usually printed at high DPI; default
                // chessboard spacing is tuned for thumbnails and cuts off large
                // grids. Widen the range to cover typical 100–400 px/cell.
                min_spacing_pix: 8.0,
                max_spacing_pix: 600.0,
                ..calib_targets_chessboard::GridGraphParams::default()
            },
            ..ChessboardParams::default()
        };
        Self {
            px_per_square: 60.0,
            chessboard,
            board: *board,
            decode: DecodeConfig::default(),
            corner_redetect_params: default_redetect_params(),
        }
    }

    /// Three-config sweep preset (canonical + high-threshold + low-threshold).
    ///
    /// Mirrors [`calib_targets_charuco::CharucoParams::sweep_for_board`].
    pub fn sweep_for_board(board: &PuzzleBoardSpec) -> Vec<Self> {
        let base = Self::for_board(board);
        let mut high = base.clone();
        high.chessboard.chess.threshold_value = 0.15;
        let mut low = base.clone();
        low.chessboard.chess.threshold_value = 0.08;
        vec![base, high, low]
    }
}
