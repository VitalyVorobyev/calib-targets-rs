//! Consistency-check task facade.

use std::collections::{HashMap, HashSet};

use nalgebra::Point2;

use crate::error::{GridError, Result};
use crate::feature::{CoordinateHypothesis, PointFeature};
use crate::geometry::{apply_projective, estimate_projective};
use crate::lattice::{GridDimensions, LatticeKind};
use crate::result::{
    ConsistencyReport, GridEntry, GridSolution, LabelledGrid, LatticeFit, RejectedFeature,
    RejectionReason, ResidualSummary,
};

/// Parameters for coordinate-hypothesis consistency checks.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ConsistencyParams {
    /// Maximum accepted reprojection residual in image pixels.
    pub max_residual_px: f32,
}

impl Default for ConsistencyParams {
    fn default() -> Self {
        Self {
            max_residual_px: 2.0,
        }
    }
}

impl ConsistencyParams {
    /// Construct consistency parameters from a residual threshold in pixels.
    pub fn new(max_residual_px: f32) -> Self {
        Self { max_residual_px }
    }
}

/// Coordinate-hypothesis consistency request.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ConsistencyRequest<'a> {
    /// Lattice family to check.
    pub lattice: LatticeKind,
    /// Position-only features referenced by hypotheses.
    pub features: &'a [PointFeature],
    /// Caller-supplied coordinate hypotheses.
    pub hypotheses: &'a [CoordinateHypothesis],
    /// Optional known grid dimensions.
    pub dimensions: Option<GridDimensions>,
    /// Consistency parameters.
    pub params: ConsistencyParams,
}

impl<'a> ConsistencyRequest<'a> {
    /// Construct a consistency request.
    pub fn new(
        lattice: LatticeKind,
        features: &'a [PointFeature],
        hypotheses: &'a [CoordinateHypothesis],
        dimensions: Option<GridDimensions>,
        params: ConsistencyParams,
    ) -> Self {
        Self {
            lattice,
            features,
            hypotheses,
            dimensions,
            params,
        }
    }
}

/// Check whether caller-supplied coordinate hypotheses are geometrically
/// consistent under the requested lattice family.
pub fn check_consistency(request: ConsistencyRequest<'_>) -> Result<ConsistencyReport> {
    let positions_by_source = validate_features(request.features)?;
    validate_hypotheses(request.hypotheses, &positions_by_source)?;

    if request.hypotheses.len() < 4 {
        return Err(GridError::InsufficientEvidence);
    }

    let mut model_points = Vec::with_capacity(request.hypotheses.len());
    let mut image_points = Vec::with_capacity(request.hypotheses.len());
    for hypothesis in request.hypotheses {
        model_points.push(request.lattice.model_point(hypothesis.coord));
        image_points.push(positions_by_source[&hypothesis.source_index]);
    }

    let model_to_image = estimate_projective(&model_points, &image_points)?;

    let mut entries = Vec::with_capacity(request.hypotheses.len());
    let mut rejected = Vec::new();
    let mut residual_sum = 0.0_f32;
    let mut residual_max = 0.0_f32;

    for hypothesis in request.hypotheses {
        let model = request.lattice.model_point(hypothesis.coord);
        let actual = positions_by_source[&hypothesis.source_index];
        let predicted =
            apply_projective(&model_to_image, model).ok_or(GridError::DegenerateGeometry)?;
        let residual = distance(actual, predicted);
        residual_sum += residual;
        if residual > residual_max {
            residual_max = residual;
        }
        if residual > request.params.max_residual_px {
            rejected.push(RejectedFeature::new(
                hypothesis.source_index,
                Some(hypothesis.coord),
                Some(residual),
                RejectionReason::ResidualTooHigh,
            ));
        }
        entries.push(GridEntry::new(
            hypothesis.coord,
            hypothesis.source_index,
            actual,
            Some(residual),
        ));
    }

    entries.sort_by_key(|entry| (entry.coord, entry.source_index));
    rejected.sort_by_key(|entry| (entry.coord, entry.source_index));

    let count = entries.len();
    let residuals = ResidualSummary::new(count, residual_sum / count as f32, residual_max);
    let fit = LatticeFit::new(model_to_image, residuals);
    let grid = LabelledGrid::new(request.lattice, entries, request.dimensions);
    let passed = rejected.is_empty();
    let solution = GridSolution::new(grid, Some(fit), rejected);
    Ok(ConsistencyReport::new(passed, solution))
}

fn validate_features(features: &[PointFeature]) -> Result<HashMap<usize, Point2<f32>>> {
    let mut out = HashMap::with_capacity(features.len());
    for feature in features {
        if !feature.position.x.is_finite() || !feature.position.y.is_finite() {
            return Err(GridError::InconsistentInput(format!(
                "feature {} has non-finite position",
                feature.source_index
            )));
        }
        if out.insert(feature.source_index, feature.position).is_some() {
            return Err(GridError::InconsistentInput(format!(
                "duplicate feature source_index {}",
                feature.source_index
            )));
        }
    }
    Ok(out)
}

fn validate_hypotheses(
    hypotheses: &[CoordinateHypothesis],
    positions_by_source: &HashMap<usize, Point2<f32>>,
) -> Result<()> {
    let mut seen_sources = HashSet::with_capacity(hypotheses.len());
    let mut seen_coords = HashSet::with_capacity(hypotheses.len());
    for hypothesis in hypotheses {
        if !positions_by_source.contains_key(&hypothesis.source_index) {
            return Err(GridError::InconsistentInput(format!(
                "hypothesis references missing feature source_index {}",
                hypothesis.source_index
            )));
        }
        if !seen_sources.insert(hypothesis.source_index) {
            return Err(GridError::InconsistentInput(format!(
                "duplicate hypothesis for feature source_index {}",
                hypothesis.source_index
            )));
        }
        if !seen_coords.insert(hypothesis.coord) {
            return Err(GridError::InconsistentInput(format!(
                "duplicate hypothesis for coordinate ({}, {})",
                hypothesis.coord.u, hypothesis.coord.v
            )));
        }
    }
    Ok(())
}

fn distance(a: Point2<f32>, b: Point2<f32>) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}
