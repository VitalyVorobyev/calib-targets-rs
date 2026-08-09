//! Output and geometry-check types for the detector pipeline.
//!
//! These are pure data carriers: the [`ChessboardDetection`] result and
//! its [`ChessboardCorner`] entries, plus the [`GeometryCheckTrace`]
//! returned by the mandatory final geometry check. No pipeline logic lives
//! here — see the sibling stage modules for the stage bodies.

use calib_targets_core::Coord;

use nalgebra::Point2;
use serde::{Deserialize, Serialize};

/// A single labelled chessboard corner.
///
/// Not to be confused with [`ChessCorner`](crate::ChessCorner): this is the
/// labelled grid corner the detector *returns*, not the raw ChESS feature fed
/// into it.
///
/// `#[non_exhaustive]`: construct with [`ChessboardCorner::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChessboardCorner {
    /// Sub-pixel image position.
    pub position: Point2<f32>,
    /// Grid label `(u, v)`. A chessboard corner is always labelled — non-optional.
    ///
    /// `u` runs along grid `i` (rightward), `v` along grid `j` (downward).
    pub grid: Coord,
    /// Index into the detector's input `&[ChessCorner]` slice that produced this corner.
    pub input_index: usize,
    /// Corner score.
    pub score: f32,
}

impl ChessboardCorner {
    /// Create a corner from its position, grid label, input provenance, and score.
    pub fn new(position: Point2<f32>, grid: Coord, input_index: usize, score: f32) -> Self {
        Self {
            position,
            grid,
            input_index,
            score,
        }
    }
}

/// Result of chessboard detection: the labelled corner set.
///
/// `#[non_exhaustive]`: construct with [`ChessboardDetection::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChessboardDetection {
    /// The labelled corners.
    pub corners: Vec<ChessboardCorner>,
    /// Grid cell size in pixels, estimated from the labelled component's
    /// median cardinal-edge length. `None` when no component was recovered
    /// (no detection is emitted in that case, so this is `Some` for every
    /// returned detection). Exposed on the stable result so consumers can
    /// scale geometry checks and overlays.
    pub cell_size: Option<f32>,
}

impl ChessboardDetection {
    /// Create a detection from its labelled corner set.
    ///
    /// `cell_size` defaults to `None`; populate it with
    /// [`ChessboardDetection::with_cell_size`].
    pub fn new(corners: Vec<ChessboardCorner>) -> Self {
        Self {
            corners,
            cell_size: None,
        }
    }

    /// Set the grid [`cell_size`](Self::cell_size) (builder style).
    #[must_use]
    pub fn with_cell_size(mut self, cell_size: f32) -> Self {
        self.cell_size = Some(cell_size);
        self
    }
}

/// Outcome of the mandatory final geometry check.
///
/// Returned by [`run_geometry_check`](super::geometry_check::run_geometry_check).
/// The geometry check can only drop labelled corners or refuse the
/// detection; these counters report which predicate did the dropping.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct GeometryCheckTrace {
    /// Number of labelled corners that failed the geometry check
    /// and were dropped from the final detection.
    pub dropped: u32,
    /// Drops attributed to the line-collinearity predicate.
    pub dropped_line_collinearity: u32,
    /// Drops attributed to the local-homography residual predicate.
    pub dropped_local_h_residual: u32,
    /// Drops attributed to the direct local wrong-label check
    /// (interior skipped-corner edges and duplicate-pixel labels).
    pub dropped_edge_invariant: u32,
    /// Number of labelled corners dropped because they were not in
    /// the largest cardinally-connected component. Catches isolated
    /// false-positive labels.
    pub dropped_disconnected: u32,
    /// Number of cardinally-connected components found before the
    /// drop pass. `1` is the chessboard contract; `> 1` always
    /// triggers `dropped_disconnected > 0`.
    pub components_seen: u32,
    /// Whether the detection was refused entirely because the
    /// surviving labelled count fell below `min_labeled_corners`.
    pub detection_refused: bool,
}

/// One exact label at a chessboard-specific diagnostics checkpoint.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ChessboardLabelTrace {
    /// First grid coordinate.
    pub u: i32,
    /// Second grid coordinate.
    pub v: i32,
    /// Index into the ChESS corner slice.
    pub corner_index: usize,
}

/// Axis-cluster outcome for one ChESS corner at the input checkpoint.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChessboardClusterTrace {
    /// The corner failed strength or sigma admission and did not vote.
    NotAdmitted,
    /// The first axis slot matched the first global cluster center.
    Canonical,
    /// The second axis slot matched the first global cluster center.
    Swapped,
    /// The corner was admitted but neither assignment passed cluster tolerance.
    Rejected,
}

/// Exact chessboard admission and clustering state for one input corner.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Copy, Debug, Serialize)]
pub struct ChessboardFeatureTrace {
    /// Index into the ChESS corner slice.
    pub corner_index: usize,
    /// Whether the ChESS response passed `min_corner_strength`.
    pub strength_admitted: bool,
    /// Whether both axis uncertainties passed the derived sigma gate.
    pub sigma_admitted: bool,
    /// Whether both admission predicates passed.
    pub prefilter_admitted: bool,
    /// Exact clustering outcome after admission.
    pub cluster: ChessboardClusterTrace,
}

/// Net chessboard-specific changes after generic projective-grid detection.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Debug, Serialize)]
pub struct ChessboardStageTrace {
    /// Per-corner strength/sigma admission and cluster assignment.
    pub features: Vec<ChessboardFeatureTrace>,
    /// Components after chessboard recovery and its component merge.
    pub recovered_components: Vec<Vec<ChessboardLabelTrace>>,
    /// Corner indices absent from the generic result and present after recovery.
    pub recovery_additions: Vec<usize>,
    /// Recovered corner indices removed or refused by the final geometry gate.
    pub final_drops: Vec<usize>,
}

/// Exact generic trace and the chessboard-specific continuation from one run.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Debug, Serialize)]
pub struct ChessboardTopologicalTrace {
    /// Exact stages produced by `projective-grid`.
    pub projective_grid: projective_grid::diagnostics::trace::TopologicalTrace,
    /// Net recovery and final-gate changes.
    pub chessboard: ChessboardStageTrace,
    /// Public detections emitted by this same execution.
    pub detections: Vec<ChessboardDetection>,
}
