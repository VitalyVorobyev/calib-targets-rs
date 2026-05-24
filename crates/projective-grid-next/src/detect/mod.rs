//! Detection task facade.
//!
//! Phase C ships a working implementation for
//! [`(LatticeKind::Square, Evidence::Oriented2)`](Evidence::Oriented2): a
//! seed-and-grow pipeline (seed → BFS grow → validate → fit). All other
//! `(lattice, evidence)` combinations remain typed
//! [`GridError::UnsupportedCombination`] placeholders.

mod square;

use crate::error::{EvidenceKind, GridError, GridTask, Result};
use crate::feature::{CoordinateHypothesis, OrientedFeature, PointFeature};
use crate::float::{lit, Float};
use crate::lattice::{GridDimensions, LatticeKind};
use crate::result::GridSolution;
use crate::seed::SeedParams;
use crate::validate::ValidateParams;

pub use crate::grow::GrowParams;

/// Evidence supplied to a detection task.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Evidence<'a, F: Float> {
    /// Position-only point features.
    Positions(&'a [PointFeature<F>]),
    /// Point features with one local lattice direction.
    Oriented1(&'a [OrientedFeature<F, 1>]),
    /// Point features with two local lattice directions.
    Oriented2(&'a [OrientedFeature<F, 2>]),
    /// Point features with three local lattice directions.
    Oriented3(&'a [OrientedFeature<F, 3>]),
    /// Point features plus caller-supplied coordinate hypotheses.
    CoordinateHypotheses {
        /// Position-only features.
        features: &'a [PointFeature<F>],
        /// Proposed coordinate labels.
        hypotheses: &'a [CoordinateHypothesis<F>],
    },
}

impl<F: Float> Evidence<'_, F> {
    /// Return this evidence's kind for dispatch and typed errors.
    pub fn kind(&self) -> EvidenceKind {
        match self {
            Self::Positions(_) => EvidenceKind::Positions,
            Self::Oriented1(_) => EvidenceKind::Oriented1,
            Self::Oriented2(_) => EvidenceKind::Oriented2,
            Self::Oriented3(_) => EvidenceKind::Oriented3,
            Self::CoordinateHypotheses { .. } => EvidenceKind::CoordinateHypotheses,
        }
    }
}

/// Detection parameters.
///
/// The single `max_residual_px` knob from the Phase-A shell now sits alongside
/// the three structured sub-configs consumed by the seed-and-grow pipeline.
/// Combinations that don't run that pipeline ignore the sub-configs.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct DetectionParams<F: Float> {
    /// Residual threshold in image pixels for algorithms that fit a lattice.
    pub max_residual_px: F,
    /// Seed-quad finder tuning.
    pub seed: SeedParams<F>,
    /// BFS grow engine tuning.
    pub grow: GrowParams<F>,
    /// Post-grow validation tuning.
    pub validate: ValidateParams<F>,
}

impl<F: Float> Default for DetectionParams<F> {
    fn default() -> Self {
        Self {
            max_residual_px: lit::<F>(2.0),
            seed: SeedParams::default(),
            grow: GrowParams::default(),
            validate: ValidateParams::default(),
        }
    }
}

impl<F: Float> DetectionParams<F> {
    /// Construct detection parameters from just the residual threshold; the
    /// sub-configs take their defaults.
    pub fn new(max_residual_px: F) -> Self {
        Self {
            max_residual_px,
            ..Self::default()
        }
    }
}

/// Detection request.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct DetectionRequest<'a, F: Float> {
    /// Lattice family to recover.
    pub lattice: LatticeKind,
    /// Evidence available to the detector.
    pub evidence: Evidence<'a, F>,
    /// Optional known grid dimensions.
    pub dimensions: Option<GridDimensions>,
    /// Detection parameters.
    pub params: DetectionParams<F>,
}

impl<'a, F: Float> DetectionRequest<'a, F> {
    /// Construct a detection request.
    pub fn new(
        lattice: LatticeKind,
        evidence: Evidence<'a, F>,
        dimensions: Option<GridDimensions>,
        params: DetectionParams<F>,
    ) -> Self {
        Self {
            lattice,
            evidence,
            dimensions,
            params,
        }
    }
}

/// Detect a grid from feature evidence.
///
/// Phase-C support matrix:
///
/// * `(Square, Oriented2)` — seed-and-grow port; returns a labelled
///   [`GridSolution`] with a fitted projective transform.
/// * Every other combination — typed [`GridError::UnsupportedCombination`].
pub fn detect_grid<F>(request: DetectionRequest<'_, F>) -> Result<GridSolution<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    match (request.lattice, request.evidence) {
        (LatticeKind::Square, Evidence::Oriented2(features)) => {
            square::detect_square_oriented2(features, request.dimensions, &request.params)
        }
        _ => Err(GridError::UnsupportedCombination {
            task: GridTask::Detection,
            lattice: request.lattice,
            evidence: request.evidence.kind(),
        }),
    }
}
