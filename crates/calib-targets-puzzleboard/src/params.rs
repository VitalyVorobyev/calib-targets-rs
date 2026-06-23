//! Detector parameters for PuzzleBoard.

use crate::board::PuzzleBoardSpec;
use crate::detector::{PuzzleBoardDecodeConfig, PuzzleBoardScoringMode};
use calib_targets_chessboard::DetectorParams;
use chess_corners::low_level::{ChessParams as ChessCornerParams, RefinerKind};
use chess_corners::SaddlePointConfig;
use serde::{Deserialize, Serialize};

/// Configuration for the PuzzleBoard detector.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PuzzleBoardParams {
    /// Pixels per board square in the rectified sampling space.
    #[serde(default = "default_px_per_square")]
    pub px_per_square: f32,
    /// Chessboard detection parameters.
    #[serde(default)]
    pub chessboard: DetectorParams,
    /// Board geometry.
    pub board: PuzzleBoardSpec,
    /// Decoding knobs.
    #[serde(default)]
    pub decode: PuzzleBoardDecodeConfig,
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
    let mut params = ChessCornerParams::default();
    params.threshold_rel = 0.05;
    params.nms_radius = 2;
    params.min_cluster_size = 1;
    params.refiner = RefinerKind::SaddlePoint(SaddlePointConfig::default());
    params
}

impl PuzzleBoardParams {
    /// Reasonable defaults for the given board geometry.
    ///
    /// The chessboard detector is scale-invariant — it discovers cell
    /// size from the seed itself — so the previous `min_spacing_pix` /
    /// `max_spacing_pix` widening for high-DPI prints is no longer needed.
    /// `expected_rows` / `expected_cols` and the v1 `completeness_threshold`
    /// gate are likewise dropped: the PuzzleBoard decoder runs over each
    /// returned chessboard component and the master-pattern decode itself
    /// is the geometry gate.
    pub fn for_board(board: &PuzzleBoardSpec) -> Self {
        let mut chessboard = DetectorParams::default();
        // Align with the chessboard/ChArUco corner-strength floor (33): a
        // defocused board edge fires the ChESS detector weakly (strength
        // ≈ 15–30 vs a sharp board's ≈ 90+), and such corners — while
        // grid-consistent in position — pollute the blurred-region frontier
        // with false labels. The PuzzleBoard decoder is robust to the
        // missing weak corners but not to the wrong ones, so the floor is a
        // net win. (`DetectorParams::default()` already sets 33; kept
        // explicit here to document the PuzzleBoard intent.)
        chessboard.min_corner_strength = 33.0;
        Self {
            px_per_square: 60.0,
            chessboard,
            board: *board,
            decode: PuzzleBoardDecodeConfig::default(),
            corner_redetect_params: default_redetect_params(),
        }
    }

    /// Multi-config sweep preset built on top of
    /// [`DetectorParams::sweep_default`].
    ///
    /// The first pass keeps the default soft scorer and default BER gate.
    /// The second pass repeats the same chessboard sweep with the legacy
    /// hard-weighted scorer at the paper's 40% BER allowance, which recovers
    /// high-distortion author-reference fragments while leaving
    /// [`Self::for_board`] unchanged.
    pub fn sweep_for_board(board: &PuzzleBoardSpec) -> Vec<Self> {
        let base = Self::for_board(board);
        let chess_sweep = DetectorParams::sweep_default();
        let mut configs: Vec<Self> = chess_sweep
            .iter()
            .cloned()
            .map(|mut chessboard| {
                chessboard.min_corner_strength = base.chessboard.min_corner_strength;
                Self {
                    chessboard,
                    ..base.clone()
                }
            })
            .collect();
        configs.extend(chess_sweep.into_iter().map(|mut chessboard| {
            chessboard.min_corner_strength = base.chessboard.min_corner_strength;
            let mut params = Self {
                chessboard,
                ..base.clone()
            };
            params.decode.scoring_mode = PuzzleBoardScoringMode::HardWeighted;
            params.decode.max_bit_error_rate = 0.40;
            params
        }));
        configs
    }
}
