use serde::{Deserialize, Serialize};

use calib_targets_chessboard::ChessboardParams;
use calib_targets_core::{
    default_chess_config, Coord, DetectorConfig, GridAlignment, LabeledCorner, TargetDetection,
    TargetKind,
};
use nalgebra::Point2;

use crate::circle_score::{CirclePolarity, CircleScoreParams};
use crate::coords::{CellCoords, CellOffset};

/// One expected marker circle on the board.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkerCircleSpec {
    /// Expected cell coordinate (top-left corner indices).
    pub cell: CellCoords,
    /// Expected polarity of the disk in that cell.
    pub polarity: CirclePolarity,
}

impl MarkerCircleSpec {
    /// Build a marker-circle spec for a cell with the given polarity.
    pub fn new(cell: CellCoords, polarity: CirclePolarity) -> Self {
        Self { cell, polarity }
    }
}

/// Fixed marker board layout: chessboard size plus 3 circle markers.
#[non_exhaustive]
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
    /// Printed circle diameter as a fraction of the square side.
    ///
    /// Every radius the circle scorer probes is relative to this, so it is the
    /// one place the disk size is stated: it is a property of the printed
    /// board, not of the imaging conditions, and a second copy in
    /// [`CircleScoreParams`] could only ever disagree with what was printed.
    /// Matches `circle_diameter_rel` on the printable spec.
    #[serde(default = "default_circle_diameter_rel")]
    pub circle_diameter_rel: f32,
}

/// The printed circle diameter assumed when a board layout does not say.
///
/// Matches `calib-targets-print`'s own default for the same field.
pub fn default_circle_diameter_rel() -> f32 {
    0.5
}

impl MarkerBoardSpec {
    /// Build a marker-board layout from its checkerboard size and the three
    /// expected circle markers. The square size defaults to unset; attach it
    /// with [`MarkerBoardSpec::with_cell_size`].
    pub fn new(rows: u32, cols: u32, circles: [MarkerCircleSpec; 3]) -> Self {
        Self {
            rows,
            cols,
            cell_size: None,
            circles,
            circle_diameter_rel: default_circle_diameter_rel(),
        }
    }

    /// Attach a square size (world units, e.g. millimeters), enabling
    /// `target_position` on detections.
    #[must_use]
    pub fn with_cell_size(mut self, cell_size: f32) -> Self {
        self.cell_size = Some(cell_size);
        self
    }

    /// Override the printed circle diameter, as a fraction of the square side.
    #[must_use]
    pub fn with_circle_diameter_rel(mut self, circle_diameter_rel: f32) -> Self {
        self.circle_diameter_rel = circle_diameter_rel;
        self
    }
}

impl Default for MarkerBoardSpec {
    fn default() -> Self {
        // An L whose polarities alternate with `(i + j)` parity, so each disk
        // contrasts with the square it is printed on. The previous default put
        // a white disk on a white square — a marker that renders as nothing.
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
                    cell: CellCoords { i: 3, j: 3 },
                    polarity: CirclePolarity::White,
                },
            ],
            circle_diameter_rel: default_circle_diameter_rel(),
        }
    }
}

/// Circle matching settings.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CircleMatchParams {
    /// Keep only the top-N candidates per polarity, ranked by contrast,
    /// before the frame sweep.
    pub max_candidates_per_polarity: usize,
    /// Expected circles that must agree on one board frame before it is
    /// returned.
    ///
    /// Defaults to `3` — the whole layout. The three circles exist for exactly
    /// one reason, to break the board's 4-fold rotational symmetry, and fewer
    /// than all three cannot do it: any single circle is consistent with all
    /// four rotations, and two leave no redundancy to check a wrong pairing
    /// against. Lowering this trades the "never a false label" half of the
    /// detection contract for recall on a board with an occluded marker.
    pub min_offset_inliers: usize,
}

impl Default for CircleMatchParams {
    fn default() -> Self {
        Self {
            max_candidates_per_polarity: 6,
            min_offset_inliers: 3,
        }
    }
}

/// Parameters for marker-board detection.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MarkerBoardParams {
    /// The fixed marker-board layout to detect.
    pub board: MarkerBoardSpec,
    /// ChESS corner front-end configuration for the main detection pass.
    ///
    /// Defaults to [`default_chess_config`]. Override it to run the corner
    /// pass coarse-to-fine (`MultiscaleConfig::Pyramid`) on large frames, or
    /// to pre-upscale low-resolution boards (`UpscaleConfig::Fixed`) whose
    /// corners would otherwise fall inside the ChESS ring margin. Corner
    /// positions are always reported in input-image pixels regardless.
    ///
    /// Every whole-image entry point runs this front-end over the input image
    /// to produce the corner cloud:
    /// [`MarkerBoardDetector::detect`](crate::MarkerBoardDetector::detect),
    /// [`MarkerBoardDetector::detect_corners`](crate::MarkerBoardDetector::detect_corners),
    /// and the facade's `detect_marker_board` / `detect_marker_board_best`. The
    /// `*_with_corners` entry points consume an already-detected
    /// `&[ChessCorner]` and never read this field.
    #[serde(default = "default_chess_config")]
    pub chess: DetectorConfig,
    /// Chessboard-detector parameters for the underlying corner-grid step.
    #[serde(default = "default_marker_chessboard_params")]
    pub chessboard: ChessboardParams,
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
    /// Construct parameters for the given board layout with all tuning at
    /// defaults.
    pub fn for_board(board: MarkerBoardSpec) -> Self {
        // chessboard detector is scale-invariant — `expected_rows/cols`
        // and `completeness_threshold` from v1 no longer apply. The marker
        // circles supply the geometry constraint.
        Self {
            board,
            chess: default_chess_config(),
            chessboard: default_marker_chessboard_params(),
            circle_score: CircleScoreParams::default(),
            match_params: CircleMatchParams::default(),
            roi_cells: None,
        }
    }

    /// Multi-config sweep preset built on top of
    /// [`ChessboardParams::sweep_default`].
    ///
    /// The sweep varies exactly one axis: the shared grid-build front-end.
    /// Circle scoring and circle-to-layout matching stay at the
    /// [`Self::for_board`] values in every config — those knobs describe the
    /// printed board, not the imaging conditions, so sweeping them would be
    /// fitting per-dataset constants rather than covering a real degree of
    /// freedom.
    pub fn sweep_for_board(board: &MarkerBoardSpec) -> Vec<Self> {
        let base = Self::for_board(board.clone());
        ChessboardParams::sweep_default()
            .into_iter()
            .map(|mut chessboard| {
                chessboard.min_corner_strength = base.chessboard.min_corner_strength;
                Self {
                    chessboard,
                    ..base.clone()
                }
            })
            .collect()
    }
}

impl Default for MarkerBoardParams {
    fn default() -> Self {
        Self::for_board(MarkerBoardSpec::default())
    }
}

fn default_marker_chessboard_params() -> ChessboardParams {
    ChessboardParams::default()
}

/// Result of matching expected circles to detected candidates.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CircleMatch {
    /// The expected circle this entry describes.
    pub expected: MarkerCircleSpec,
    /// Index into the detected-candidate list of the matched circle;
    /// `None` when no candidate matched.
    pub matched_index: Option<usize>,
    /// Detected-to-board cell offset implied by this match; `None` when
    /// unmatched.
    pub offset_cells: Option<CellOffset>,
}

impl CircleMatch {
    /// Build an unmatched entry for an expected circle.
    pub fn unmatched(expected: MarkerCircleSpec) -> Self {
        Self {
            expected,
            matched_index: None,
            offset_cells: None,
        }
    }

    /// Record the matched candidate and the detected-to-board cell offset of
    /// the frame that explained it.
    #[must_use]
    pub fn with_match(mut self, matched_index: usize, offset_cells: CellOffset) -> Self {
        self.matched_index = Some(matched_index);
        self.offset_cells = Some(offset_cells);
        self
    }
}

/// Marker-board detection result.
///
/// Carries only the facts a consumer needs to *use* a marker-board
/// detection: the labelled corners and the optional grid alignment. The
/// evidence about *how* the board was found — scored circle hypotheses,
/// expected-to-detected circle pairings, per-corner provenance, and the
/// alignment-inlier count — lives in the opt-in diagnostics channel.
#[cfg_attr(
    feature = "diagnostics",
    doc = "See [`crate::diagnostics::MarkerBoardDiagnostics`], obtained via",
    doc = "[`crate::MarkerBoardDetector::diagnose_with_corners`]."
)]
///
/// `#[non_exhaustive]`: construct with [`MarkerBoardDetection::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarkerBoardDetection {
    /// Labelled checkerboard corners.
    pub corners: Vec<MarkerBoardCorner>,
    /// Grid alignment to the known board layout; `None` when alignment
    /// could not be resolved.
    pub alignment: Option<GridAlignment>,
}

impl MarkerBoardDetection {
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
    ///
    /// `u` runs along grid `i` (rightward), `v` along grid `j` (downward).
    pub grid: Coord,
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
    pub fn new(position: Point2<f32>, grid: Coord, score: f32) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The sweep preset mirrors `ChessboardParams::sweep_default` one-for-one
    /// and varies only that axis: every config keeps the board layout, the
    /// circle-scoring knobs and the matching knobs `for_board` chose.
    #[test]
    fn sweep_for_board_varies_only_the_grid_axis() {
        let board = MarkerBoardSpec::default();
        let base = MarkerBoardParams::for_board(board.clone());
        let sweep = MarkerBoardParams::sweep_for_board(&board);

        assert_eq!(sweep.len(), ChessboardParams::sweep_default().len());
        let tunings: Vec<String> = sweep
            .iter()
            .map(|p| format!("{:?}", p.chessboard.effective_tuning()))
            .collect();
        let mut distinct = tunings.clone();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            tunings.len(),
            "every sweep config must carry a distinct chessboard tuning"
        );
        for params in &sweep {
            assert_eq!(
                format!("{:?}", params.circle_score),
                format!("{:?}", base.circle_score),
                "circle scoring must not be swept"
            );
            assert_eq!(
                format!("{:?}", params.match_params),
                format!("{:?}", base.match_params),
                "circle matching must not be swept"
            );
            assert_eq!(
                params.chessboard.min_corner_strength, base.chessboard.min_corner_strength,
                "the corner-strength floor is pinned to the `for_board` value"
            );
        }
    }
}
