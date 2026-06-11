//! Shared output types for detection and consistency tasks.
//!
//! The detection surface is pinned to `f32`; see [`crate::feature`] for
//! the rationale.

use nalgebra::{Point2, Projective2};

use crate::lattice::{Coord, GridDimensions, LatticeKind};

/// One labelled grid feature in a solved grid.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct GridEntry {
    /// Lattice coordinate assigned to this feature.
    pub coord: Coord,
    /// Caller-owned feature source index.
    pub source_index: usize,
    /// Image-frame pixel-center position.
    pub image_position: Point2<f32>,
    /// Reprojection residual in image pixels, when a fit was computed.
    pub residual_px: Option<f32>,
}

impl GridEntry {
    /// Construct a labelled grid entry.
    pub fn new(
        coord: Coord,
        source_index: usize,
        image_position: Point2<f32>,
        residual_px: Option<f32>,
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
pub struct LabelledGrid {
    /// Lattice family of this grid.
    pub lattice: LatticeKind,
    /// Labelled feature entries.
    pub entries: Vec<GridEntry>,
    /// Inclusive coordinate bounding box, if the grid is non-empty.
    pub bbox: Option<(Coord, Coord)>,
    /// Optional known dimensions supplied by the caller.
    pub dimensions: Option<GridDimensions>,
}

impl LabelledGrid {
    /// Construct a labelled grid.
    pub fn new(
        lattice: LatticeKind,
        entries: Vec<GridEntry>,
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

    /// Linear-scan lookup of the labelled entry with the given source index.
    pub fn find(&self, source_index: usize) -> Option<&GridEntry> {
        self.entries.iter().find(|e| e.source_index == source_index)
    }
}

/// Residual summary in image pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ResidualSummary {
    /// Number of residuals included in the summary.
    pub count: usize,
    /// Mean residual in pixels.
    pub mean_px: f32,
    /// Maximum residual in pixels.
    pub max_px: f32,
}

impl ResidualSummary {
    /// Construct a residual summary.
    pub fn new(count: usize, mean_px: f32, max_px: f32) -> Self {
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
pub struct LatticeFit {
    /// Projective mapping from model-plane lattice coordinates to image pixels.
    pub model_to_image: Projective2<f32>,
    /// Residual summary in image pixels.
    pub residuals: ResidualSummary,
}

impl LatticeFit {
    /// Construct a lattice fit.
    pub fn new(model_to_image: Projective2<f32>, residuals: ResidualSummary) -> Self {
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
    /// Feature was never labelled by the detection pipeline (e.g. noise
    /// outside the recovered lattice support).
    Unlabelled,
    /// Feature was labelled by the seed-and-grow pass but dropped by the
    /// post-grow validation stage (line collinearity, local-H residual,
    /// or edge-length band).
    ValidationDropped,
}

/// Rejected feature record.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct RejectedFeature {
    /// Caller-owned source index.
    pub source_index: usize,
    /// Coordinate associated with the rejection, if one was proposed.
    pub coord: Option<Coord>,
    /// Residual in image pixels, if available.
    pub residual_px: Option<f32>,
    /// Rejection reason.
    pub reason: RejectionReason,
}

impl RejectedFeature {
    /// Construct a rejected-feature record.
    pub fn new(
        source_index: usize,
        coord: Option<Coord>,
        residual_px: Option<f32>,
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
pub struct GridSolution {
    /// Labelled grid entries.
    pub grid: LabelledGrid,
    /// Lattice fit, when the task computed one.
    pub fit: Option<LatticeFit>,
    /// Features rejected by task gates.
    pub rejected: Vec<RejectedFeature>,
}

impl GridSolution {
    /// Construct a grid solution.
    pub fn new(
        grid: LabelledGrid,
        fit: Option<LatticeFit>,
        rejected: Vec<RejectedFeature>,
    ) -> Self {
        Self {
            grid,
            fit,
            rejected,
        }
    }

    /// Linear-scan lookup of the rejection record for the given source index, if any.
    pub fn rejected_for(&self, source_index: usize) -> Option<&RejectedFeature> {
        self.rejected
            .iter()
            .find(|r| r.source_index == source_index)
    }
}

/// Report returned by coordinate-hypothesis consistency checks.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ConsistencyReport {
    /// `true` when all residuals satisfy the configured threshold.
    pub passed: bool,
    /// Labelled solution and residual diagnostics.
    pub solution: GridSolution,
}

impl ConsistencyReport {
    /// Construct a consistency report.
    pub fn new(passed: bool, solution: GridSolution) -> Self {
        Self { passed, solution }
    }

    /// Convenience accessor for the maximum residual in pixels from the fitted lattice,
    /// when one was computed.
    pub fn max_residual_px(&self) -> Option<f32> {
        Some(self.solution.fit.as_ref()?.residuals.max_px)
    }
}

fn bbox_for_entries(entries: &[GridEntry]) -> Option<(Coord, Coord)> {
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

#[cfg(test)]
mod tests {
    use nalgebra::{Point2, Projective2};

    use super::*;

    fn make_identity_fit() -> LatticeFit {
        LatticeFit::new(
            Projective2::identity(),
            ResidualSummary::new(1, 0.5_f32, 1.0_f32),
        )
    }

    #[test]
    fn max_residual_px_none_when_fit_absent() {
        let grid = LabelledGrid::new(LatticeKind::Square, vec![], None);
        let solution = GridSolution::new(grid, None, vec![]);
        let report = ConsistencyReport::new(true, solution);
        assert_eq!(report.max_residual_px(), None);
    }

    #[test]
    fn max_residual_px_some_when_fit_present() {
        let grid = LabelledGrid::new(LatticeKind::Square, vec![], None);
        let fit = make_identity_fit();
        let solution = GridSolution::new(grid, Some(fit), vec![]);
        let report = ConsistencyReport::new(true, solution);
        assert_eq!(report.max_residual_px(), Some(1.0_f32));
    }

    #[test]
    fn labelled_grid_find_present_and_absent() {
        let entry = GridEntry::new(Coord::new(0, 0), 42, Point2::new(1.0_f32, 2.0), None);
        let grid = LabelledGrid::new(LatticeKind::Square, vec![entry], None);
        assert!(grid.find(42).is_some());
        assert!(grid.find(99).is_none());
    }

    #[test]
    fn grid_solution_rejected_for_present_and_absent() {
        let rejected =
            RejectedFeature::new(5, None, Some(3.0_f32), RejectionReason::ResidualTooHigh);
        let grid = LabelledGrid::new(LatticeKind::Square, vec![], None);
        let solution = GridSolution::new(grid, None, vec![rejected]);
        assert!(solution.rejected_for(5).is_some());
        assert!(solution.rejected_for(0).is_none());
    }
}
