use serde::{Deserialize, Serialize};

use calib_targets_chessboard::DetectorParams;
use calib_targets_core::{GridAlignment, GridCoords, LabeledCorner, TargetDetection, TargetKind};
use nalgebra::Point2;

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
#[non_exhaustive]
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
#[non_exhaustive]
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
    /// Labelled checkerboard corners.
    pub corners: Vec<MarkerBoardCorner>,
    /// Grid alignment to the known board layout; `None` when alignment
    /// could not be resolved.
    pub alignment: Option<GridAlignment>,
}

impl MarkerBoardDetectionResult {
    /// Create a result from its typed corners and optional grid alignment.
    pub fn new(corners: Vec<MarkerBoardCorner>, alignment: Option<GridAlignment>) -> Self {
        Self { corners, alignment }
    }

    pub(crate) fn from_target_detection(
        detection: TargetDetection,
        alignment: Option<GridAlignment>,
    ) -> Self {
        debug_assert_eq!(detection.kind, TargetKind::CheckerboardMarker);
        let input_len = detection.corners.len();
        let corners: Vec<MarkerBoardCorner> = detection
            .corners
            .into_iter()
            .filter_map(MarkerBoardCorner::from_labeled)
            .collect();
        debug_assert_eq!(corners.len(), input_len);
        Self::new(corners, alignment)
    }

    /// Convert typed corners into the shared `TargetDetection` carrier.
    pub fn target_detection(&self) -> TargetDetection {
        TargetDetection::new(
            TargetKind::CheckerboardMarker,
            self.corners
                .iter()
                .map(MarkerBoardCorner::to_labeled)
                .collect(),
        )
    }

    /// Return the board-aligned corner detection if alignment is available.
    pub fn aligned_detection(&self) -> Option<TargetDetection> {
        self.alignment.map(|_| self.target_detection())
    }
}

/// A detected marker-board checkerboard corner.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerBoardCorner {
    /// Sub-pixel image position.
    pub position: Point2<f32>,
    /// Corner coordinate in the returned grid frame.
    pub grid: GridCoords,
    /// Board-canonical corner ID, available when circle alignment succeeded.
    pub id: Option<u32>,
    /// Physical board-space position, available when alignment and cell size are known.
    #[serde(default)]
    pub target_position: Option<Point2<f32>>,
    /// Detector-specific corner score; higher is better.
    pub score: f32,
}

impl MarkerBoardCorner {
    /// Create a marker-board corner from its required fields.
    pub fn new(position: Point2<f32>, grid: GridCoords, score: f32) -> Self {
        Self {
            position,
            grid,
            id: None,
            target_position: None,
            score,
        }
    }

    pub(crate) fn from_labeled(corner: LabeledCorner) -> Option<Self> {
        Some(Self {
            position: corner.position,
            grid: corner.grid?,
            id: corner.id,
            target_position: corner.target_position,
            score: corner.score,
        })
    }

    /// Attach a board-canonical corner ID.
    #[must_use]
    pub fn with_id(mut self, id: u32) -> Self {
        self.id = Some(id);
        self
    }

    /// Attach a physical board-space position.
    #[must_use]
    pub fn with_target_position(mut self, target_position: Point2<f32>) -> Self {
        self.target_position = Some(target_position);
        self
    }

    /// Convert this typed corner to the shared carrier used by diagnostics and bindings.
    pub fn to_labeled(&self) -> LabeledCorner {
        let mut corner = LabeledCorner::new(self.position, self.score).with_grid(self.grid);
        if let Some(id) = self.id {
            corner = corner.with_id(id);
        }
        if let Some(target_position) = self.target_position {
            corner = corner.with_target_position(target_position);
        }
        corner
    }
}
