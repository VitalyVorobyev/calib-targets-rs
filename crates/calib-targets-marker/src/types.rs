use serde::{Deserialize, Serialize};

use calib_targets_chessboard::DetectorParams;
use calib_targets_core::{GridAlignment, TargetDetection};

use crate::circle_score::{CirclePolarity, CircleScoreParams};
use crate::coords::{CellCoords, CellOffset};

/// One expected marker circle on the board.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkerCircleSpec {
    /// Expected cell coordinate (top-left corner indices).
    pub cell: CellCoords,
    /// Expected polarity of the disk in that cell.
    pub polarity: CirclePolarity,
}

/// Fixed marker board layout: chessboard size plus 3 circle markers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerBoardSpec {
    /// Number of inner-corner rows of the checkerboard.
    pub rows: u32,
    /// Number of inner-corner columns of the checkerboard.
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
    /// The fixed marker-board layout to detect.
    pub layout: MarkerBoardSpec,
    /// Chessboard-detector parameters for the underlying corner-grid step.
    #[serde(default = "default_marker_chessboard_params")]
    pub chessboard: DetectorParams,
    /// Per-cell circular-marker scoring parameters.
    #[serde(default)]
    pub circle_score: CircleScoreParams,
    /// Circle-to-layout matching parameters.
    #[serde(default)]
    pub match_params: CircleMatchParams,
    /// Optional ROI in cell coords to restrict circle search: [i0, j0, i1, j1].
    #[serde(default)]
    pub roi_cells: Option<[i32; 4]>,
}

impl MarkerBoardParams {
    /// Construct parameters for the given layout with all tuning at defaults.
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
    /// The expected circle this entry describes.
    pub expected: MarkerCircleSpec,
    /// Index into the detected-candidate list of the matched circle;
    /// `None` when no candidate matched.
    pub matched_index: Option<usize>,
    /// Distance, in cell units, between expected and matched cell;
    /// `None` when unmatched.
    pub distance_cells: Option<f32>,
    /// Detected-to-board cell offset implied by this match; `None` when
    /// unmatched.
    pub offset_cells: Option<CellOffset>,
}

/// Marker-board detection result.
///
/// Carries only the facts a consumer needs to *use* a marker-board
/// detection: the labelled corners and the optional grid alignment. The
/// evidence about *how* the board was found — scored circle hypotheses,
/// expected-to-detected circle pairings, per-corner provenance, and the
/// alignment-inlier count — lives in
/// [`crate::diagnostics::MarkerBoardDiagnostics`], obtained via the
/// detector's `*_with_diagnostics` entry points.
///
/// `#[non_exhaustive]`: construct with [`MarkerBoardDetectionResult::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerBoardDetectionResult {
    /// The labelled corner detection.
    pub detection: TargetDetection,
    /// Grid alignment to the known board layout; `None` when alignment
    /// could not be resolved.
    pub alignment: Option<GridAlignment>,
}

impl MarkerBoardDetectionResult {
    /// Create a result from its detection and optional grid alignment.
    pub fn new(detection: TargetDetection, alignment: Option<GridAlignment>) -> Self {
        Self {
            detection,
            alignment,
        }
    }

    /// Return the board-aligned corner detection if alignment is available.
    pub fn aligned_detection(&self) -> Option<TargetDetection> {
        self.alignment.map(|_| self.detection.clone())
    }
}
