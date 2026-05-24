//! Consistency-check task facade.

use std::collections::{HashMap, HashSet};

use nalgebra::Point2;

use crate::error::{GridError, Result};
use crate::feature::{CoordinateHypothesis, PointFeature};
use crate::float::{lit, Float};
use crate::geometry::{apply_projective, estimate_projective};
use crate::lattice::{GridDimensions, LatticeKind};
use crate::result::{
    ConsistencyReport, GridEntry, GridSolution, LabelledGrid, LatticeFit, RejectedFeature,
    RejectionReason, ResidualSummary,
};

/// Parameters for coordinate-hypothesis consistency checks.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ConsistencyParams<F: Float> {
    /// Maximum accepted reprojection residual in image pixels.
    pub max_residual_px: F,
}

impl<F: Float> Default for ConsistencyParams<F> {
    fn default() -> Self {
        Self {
            max_residual_px: lit::<F>(2.0),
        }
    }
}

impl<F: Float> ConsistencyParams<F> {
    /// Construct consistency parameters from a residual threshold in pixels.
    pub fn new(max_residual_px: F) -> Self {
        Self { max_residual_px }
    }
}

/// Coordinate-hypothesis consistency request.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ConsistencyRequest<'a, F: Float> {
    /// Lattice family to check.
    pub lattice: LatticeKind,
    /// Position-only features referenced by hypotheses.
    pub features: &'a [PointFeature<F>],
    /// Caller-supplied coordinate hypotheses.
    pub hypotheses: &'a [CoordinateHypothesis<F>],
    /// Optional known grid dimensions.
    pub dimensions: Option<GridDimensions>,
    /// Consistency parameters.
    pub params: ConsistencyParams<F>,
}

impl<'a, F: Float> ConsistencyRequest<'a, F> {
    /// Construct a consistency request.
    pub fn new(
        lattice: LatticeKind,
        features: &'a [PointFeature<F>],
        hypotheses: &'a [CoordinateHypothesis<F>],
        dimensions: Option<GridDimensions>,
        params: ConsistencyParams<F>,
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
pub fn check_consistency<F: Float>(
    request: ConsistencyRequest<'_, F>,
) -> Result<ConsistencyReport<F>> {
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
    let mut residual_sum = F::zero();
    let mut residual_max = F::zero();

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
    let residuals =
        ResidualSummary::new(count, residual_sum / lit::<F>(count as f32), residual_max);
    let fit = LatticeFit::new(model_to_image, residuals);
    let grid = LabelledGrid::new(request.lattice, entries, request.dimensions);
    let passed = rejected.is_empty();
    let solution = GridSolution::new(grid, Some(fit), rejected);
    Ok(ConsistencyReport::new(passed, solution))
}

fn validate_features<F: Float>(features: &[PointFeature<F>]) -> Result<HashMap<usize, Point2<F>>> {
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

fn validate_hypotheses<F: Float>(
    hypotheses: &[CoordinateHypothesis<F>],
    positions_by_source: &HashMap<usize, Point2<F>>,
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

fn distance<F: Float>(a: Point2<F>, b: Point2<F>) -> F {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}
