//! Consistency-check task facade.

use std::collections::{HashMap, HashSet};

use nalgebra::Point2;

use crate::error::{GridError, Result};
use crate::feature::{CoordinateHypothesis, PointFeature};
use crate::geometry::{apply_projective, estimate_projective};
use crate::lattice::{GridDimensions, LatticeKind};
use crate::result::{
    ConsistencyReport, GridEntry, LabelledGrid, LatticeFit, RejectedFeature, RejectionReason,
    ResidualSummary,
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
    if request.params.max_residual_px.is_nan() || request.params.max_residual_px < 0.0 {
        return Err(GridError::InconsistentInput(
            "max_residual_px must be non-negative or +infinity".to_owned(),
        ));
    }
    if let Some(dimensions) = request.dimensions {
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err(GridError::InconsistentInput(
                "grid dimensions count feature positions and must be non-zero".to_owned(),
            ));
        }
    }

    let positions_by_source = validate_features(request.features)?;
    validate_hypotheses(request.hypotheses, &positions_by_source)?;

    if request.hypotheses.len() < 4 {
        return Err(GridError::InsufficientEvidence);
    }

    if let Some(dimensions) = request.dimensions {
        let min_u = request
            .hypotheses
            .iter()
            .map(|h| i64::from(h.coord.u))
            .min()
            .unwrap_or(0);
        let max_u = request
            .hypotheses
            .iter()
            .map(|h| i64::from(h.coord.u))
            .max()
            .unwrap_or(0);
        let min_v = request
            .hypotheses
            .iter()
            .map(|h| i64::from(h.coord.v))
            .min()
            .unwrap_or(0);
        let max_v = request
            .hypotheses
            .iter()
            .map(|h| i64::from(h.coord.v))
            .max()
            .unwrap_or(0);
        let span_u = max_u - min_u + 1;
        let span_v = max_v - min_v + 1;
        if span_u > dimensions.width as i64 || span_v > dimensions.height as i64 {
            return Err(GridError::InconsistentInput(format!(
                "hypothesis coordinate span ({span_u}x{span_v}) exceeds declared dimensions ({}x{})",
                dimensions.width, dimensions.height
            )));
        }
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
    Ok(ConsistencyReport::new(passed, grid, fit, rejected))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Coord;

    #[test]
    fn check_consistency_rejects_nan_residual_and_invalid_dimensions() {
        let features = vec![
            PointFeature::new(0, Point2::new(0.0, 0.0)),
            PointFeature::new(1, Point2::new(10.0, 0.0)),
            PointFeature::new(2, Point2::new(0.0, 10.0)),
            PointFeature::new(3, Point2::new(10.0, 10.0)),
        ];
        let hypotheses = vec![
            CoordinateHypothesis::new(0, Coord::new(0, 0), None),
            CoordinateHypothesis::new(1, Coord::new(1, 0), None),
            CoordinateHypothesis::new(2, Coord::new(0, 1), None),
            CoordinateHypothesis::new(3, Coord::new(1, 1), None),
        ];
        let err = check_consistency(ConsistencyRequest::new(
            LatticeKind::Square,
            &features,
            &hypotheses,
            None,
            ConsistencyParams::new(f32::NAN),
        ))
        .unwrap_err();
        assert!(matches!(err, GridError::InconsistentInput(_)));

        let err_zero = check_consistency(ConsistencyRequest::new(
            LatticeKind::Square,
            &features,
            &hypotheses,
            Some(GridDimensions::new(0, 2)),
            ConsistencyParams::default(),
        ))
        .unwrap_err();
        assert!(matches!(err_zero, GridError::InconsistentInput(_)));

        let err_span = check_consistency(ConsistencyRequest::new(
            LatticeKind::Square,
            &features,
            &hypotheses,
            Some(GridDimensions::new(1, 2)),
            ConsistencyParams::default(),
        ))
        .unwrap_err();
        assert!(matches!(err_span, GridError::InconsistentInput(_)));
    }
}
