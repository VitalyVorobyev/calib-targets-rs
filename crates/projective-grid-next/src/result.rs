//! Shared output types for detection and consistency tasks.

use nalgebra::{Point2, Projective2};

use crate::float::Float;
use crate::lattice::{Coord, GridDimensions, LatticeKind};

/// One labelled grid feature in a solved grid.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct GridEntry<F: Float> {
    /// Lattice coordinate assigned to this feature.
    pub coord: Coord,
    /// Caller-owned feature source index.
    pub source_index: usize,
    /// Image-frame pixel-center position.
    pub image_position: Point2<F>,
    /// Reprojection residual in image pixels, when a fit was computed.
    pub residual_px: Option<F>,
}

impl<F: Float> GridEntry<F> {
    /// Construct a labelled grid entry.
    pub fn new(
        coord: Coord,
        source_index: usize,
        image_position: Point2<F>,
        residual_px: Option<F>,
    ) -> Self {
        Self {
            coord,
            source_index,
            image_position,
            residual_px,
        }
    }
}

/// A labelled grid component.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct LabelledGrid<F: Float> {
    /// Lattice family of this grid.
    pub lattice: LatticeKind,
    /// Labelled feature entries.
    pub entries: Vec<GridEntry<F>>,
    /// Inclusive coordinate bounding box, if the grid is non-empty.
    pub bbox: Option<(Coord, Coord)>,
    /// Optional known dimensions supplied by the caller.
    pub dimensions: Option<GridDimensions>,
}

impl<F: Float> LabelledGrid<F> {
    /// Construct a labelled grid.
    pub fn new(
        lattice: LatticeKind,
        entries: Vec<GridEntry<F>>,
        dimensions: Option<GridDimensions>,
    ) -> Self {
        let bbox = bbox_for_entries(&entries);
        Self {
            lattice,
            entries,
            bbox,
            dimensions,
        }
    }
}

/// Residual summary in image pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ResidualSummary<F: Float> {
    /// Number of residuals included in the summary.
    pub count: usize,
    /// Mean residual in pixels.
    pub mean_px: F,
    /// Maximum residual in pixels.
    pub max_px: F,
}

impl<F: Float> ResidualSummary<F> {
    /// Construct a residual summary.
    pub fn new(count: usize, mean_px: F, max_px: F) -> Self {
        Self {
            count,
            mean_px,
            max_px,
        }
    }
}

/// Fitted lattice-to-image transform plus residual summary.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct LatticeFit<F: Float> {
    /// Projective mapping from model-plane lattice coordinates to image pixels.
    pub model_to_image: Projective2<F>,
    /// Residual summary in image pixels.
    pub residuals: ResidualSummary<F>,
}

impl<F: Float> LatticeFit<F> {
    /// Construct a lattice fit.
    pub fn new(model_to_image: Projective2<F>, residuals: ResidualSummary<F>) -> Self {
        Self {
            model_to_image,
            residuals,
        }
    }
}

/// Reason why an observed feature did not pass a task gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RejectionReason {
    /// Reprojection residual exceeded the configured threshold.
    ResidualTooHigh,
}

/// Rejected feature record.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct RejectedFeature<F: Float> {
    /// Caller-owned source index.
    pub source_index: usize,
    /// Coordinate associated with the rejection, if one was proposed.
    pub coord: Option<Coord>,
    /// Residual in image pixels, if available.
    pub residual_px: Option<F>,
    /// Rejection reason.
    pub reason: RejectionReason,
}

impl<F: Float> RejectedFeature<F> {
    /// Construct a rejected-feature record.
    pub fn new(
        source_index: usize,
        coord: Option<Coord>,
        residual_px: Option<F>,
        reason: RejectionReason,
    ) -> Self {
        Self {
            source_index,
            coord,
            residual_px,
            reason,
        }
    }
}

/// Shared successful solution shape for grid tasks.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct GridSolution<F: Float> {
    /// Labelled grid entries.
    pub grid: LabelledGrid<F>,
    /// Lattice fit, when the task computed one.
    pub fit: Option<LatticeFit<F>>,
    /// Features rejected by task gates.
    pub rejected: Vec<RejectedFeature<F>>,
}

impl<F: Float> GridSolution<F> {
    /// Construct a grid solution.
    pub fn new(
        grid: LabelledGrid<F>,
        fit: Option<LatticeFit<F>>,
        rejected: Vec<RejectedFeature<F>>,
    ) -> Self {
        Self {
            grid,
            fit,
            rejected,
        }
    }
}

/// Report returned by coordinate-hypothesis consistency checks.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ConsistencyReport<F: Float> {
    /// `true` when all residuals satisfy the configured threshold.
    pub passed: bool,
    /// Labelled solution and residual diagnostics.
    pub solution: GridSolution<F>,
}

impl<F: Float> ConsistencyReport<F> {
    /// Construct a consistency report.
    pub fn new(passed: bool, solution: GridSolution<F>) -> Self {
        Self { passed, solution }
    }
}

fn bbox_for_entries<F: Float>(entries: &[GridEntry<F>]) -> Option<(Coord, Coord)> {
    let first = entries.first()?;
    let mut min = first.coord;
    let mut max = first.coord;
    for entry in &entries[1..] {
        min.u = min.u.min(entry.coord.u);
        min.v = min.v.min(entry.coord.v);
        max.u = max.u.max(entry.coord.u);
        max.v = max.v.max(entry.coord.v);
    }
    Some((min, max))
}
