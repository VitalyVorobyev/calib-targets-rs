use serde::{Deserialize, Serialize};

use calib_targets_chessboard::{ChessboardParams, GridGraphParams};
use calib_targets_core::{GridAlignment, TargetDetection};

use crate::circle_score::{CircleCandidate, CirclePolarity, CircleScoreParams};
use crate::coords::{CellCoords, CellOffset};

/// One expected marker circle on the board.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkerCircleSpec {
    /// Expected cell coordinate (top-left corner indices).
    pub cell: CellCoords,
    pub polarity: CirclePolarity,
}

/// Fixed marker board layout: chessboard size plus 3 circle markers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerBoardLayout {
    /// Full checkerboard dimensions (inner corners).
    pub rows: u32,
    pub cols: u32,
    /// Expected circle markers.
    pub circles: [MarkerCircleSpec; 3],
}

/// Circle matching settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CircleMatchParams {
    /// Keep only the top-N candidates per polarity before matching.
    pub max_candidates_per_polarity: usize,
    /// Optional max distance in cell units to accept a match.
    pub max_distance_cells: Option<f32>,
    /// Minimum number of consistent matches needed to return a grid offset.
    pub min_offset_inliers: usize,
}

impl Default for CircleMatchParams {
    fn default() -> Self {
        Self {
            max_candidates_per_polarity: 6,
            max_distance_cells: None,
            min_offset_inliers: 1,
        }
    }
}

/// Parameters for marker-board detection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerBoardParams {
    pub layout: MarkerBoardLayout,
    #[serde(default = "default_marker_chessboard_params")]
    pub chessboard: ChessboardParams,
    #[serde(default)]
    pub grid_graph: GridGraphParams,
    #[serde(default)]
    pub circle_score: CircleScoreParams,
    #[serde(default)]
    pub match_params: CircleMatchParams,
    /// Optional ROI in cell coords to restrict circle search: [i0, j0, i1, j1].
    #[serde(default)]
    pub roi_cells: Option<[i32; 4]>,
}

impl MarkerBoardParams {
    pub fn new(layout: MarkerBoardLayout) -> Self {
        let mut chessboard = default_marker_chessboard_params();
        chessboard.expected_rows = Some(layout.rows);
        chessboard.expected_cols = Some(layout.cols);
        Self {
            layout,
            chessboard,
            grid_graph: GridGraphParams::default(),
            circle_score: CircleScoreParams::default(),
            match_params: CircleMatchParams::default(),
            roi_cells: None,
        }
    }
}

fn default_marker_chessboard_params() -> ChessboardParams {
    ChessboardParams {
        completeness_threshold: 0.05,
        ..ChessboardParams::default()
    }
}

/// Result of matching expected circles to detected candidates.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CircleMatch {
    pub expected: MarkerCircleSpec,
    pub matched_index: Option<usize>,
    pub distance_cells: Option<f32>,
    pub offset_cells: Option<CellOffset>,
}

/// Marker-board detection result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerBoardDetectionResult {
    pub detection: TargetDetection,
    pub inliers: Vec<usize>,
    pub circle_candidates: Vec<CircleCandidate>,
    pub circle_matches: Vec<CircleMatch>,
    pub alignment: Option<GridAlignment>,
    pub alignment_inliers: usize,
}

impl MarkerBoardDetectionResult {
    /// Return the board-aligned corner detection if alignment is available.
    pub fn aligned_detection(&self) -> Option<TargetDetection> {
        self.alignment.map(|_| self.detection.clone())
    }
}
