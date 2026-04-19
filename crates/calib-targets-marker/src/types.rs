use serde::{Deserialize, Serialize};

use calib_targets_chessboard::DetectorParams;
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
pub struct MarkerBoardSpec {
    /// Full checkerboard dimensions (inner corners).
    pub rows: u32,
    pub cols: u32,
    /// Optional square size (in your chosen world units, e.g. millimeters).
    ///
    /// When provided, detections will populate `LabeledCorner.target_position`.
    #[serde(default)]
    pub cell_size: Option<f32>,
    /// Expected circle markers.
    pub circles: [MarkerCircleSpec; 3],
}

impl Default for MarkerBoardSpec {
    fn default() -> Self {
        Self {
            rows: 6,
            cols: 8,
            cell_size: None,
            circles: [
                MarkerCircleSpec {
                    cell: CellCoords { i: 2, j: 2 },
                    polarity: CirclePolarity::White,
                },
                MarkerCircleSpec {
                    cell: CellCoords { i: 3, j: 2 },
                    polarity: CirclePolarity::Black,
                },
                MarkerCircleSpec {
                    cell: CellCoords { i: 2, j: 3 },
                    polarity: CirclePolarity::White,
                },
            ],
        }
    }
}

/// Circle matching settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CircleMatchParams {
    /// Keep only the top-N candidates per polarity before matching.
    pub max_candidates_per_polarity: usize,
    /// Optional max distance in cell units to accept a match.
    pub max_distance_cells: Option<f32>,
    /// Minimum number of consistent matches needed to return a grid alignment.
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
    pub layout: MarkerBoardSpec,
    #[serde(default = "default_marker_chessboard_params")]
    pub chessboard: DetectorParams,
    #[serde(default)]
    pub circle_score: CircleScoreParams,
    #[serde(default)]
    pub match_params: CircleMatchParams,
    /// Optional ROI in cell coords to restrict circle search: [i0, j0, i1, j1].
    #[serde(default)]
    pub roi_cells: Option<[i32; 4]>,
}

impl MarkerBoardParams {
    pub fn new(layout: MarkerBoardSpec) -> Self {
        // chessboard detector is scale-invariant — `expected_rows/cols`
        // and `completeness_threshold` from v1 no longer apply. The marker
        // circles supply the geometry constraint.
        Self {
            layout,
            chessboard: default_marker_chessboard_params(),
            circle_score: CircleScoreParams::default(),
            match_params: CircleMatchParams::default(),
            roi_cells: None,
        }
    }
}

impl Default for MarkerBoardParams {
    fn default() -> Self {
        Self::new(MarkerBoardSpec::default())
    }
}

fn default_marker_chessboard_params() -> DetectorParams {
    DetectorParams::default()
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
