//! Output and geometry-check types for the detector pipeline.
//!
//! These are pure data carriers: the [`ChessboardDetection`] result and
//! its [`ChessboardCorner`] entries, plus the [`GeometryCheckTrace`]
//! returned by the mandatory final geometry check. No pipeline logic lives
//! here — see the sibling stage modules for the stage bodies.

use calib_targets_core::GridCoords;

use nalgebra::Point2;
use serde::Serialize;

/// A single labelled chessboard corner.
///
/// `#[non_exhaustive]`: construct with [`ChessboardCorner::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct ChessboardCorner {
    /// Sub-pixel image position.
    pub position: Point2<f32>,
    /// Grid label (i, j). A chessboard corner is always labelled — non-optional.
    pub grid: GridCoords,
    /// Index into the detector's input `&[ChessCorner]` slice that produced this corner.
    pub input_index: usize,
    /// Corner score.
    pub score: f32,
}

impl ChessboardCorner {
    /// Create a corner from its position, grid label, input provenance, and score.
    pub fn new(position: Point2<f32>, grid: GridCoords, input_index: usize, score: f32) -> Self {
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
#[derive(Clone, Debug, Serialize)]
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
