//! Detection task facade.
//!
//! Detection is intentionally a typed placeholder in this corrective reset.
//! Algorithms will be ported only after the evidence and result contracts are
//! stable.

use crate::error::{EvidenceKind, GridError, GridTask, Result};
use crate::feature::{CoordinateHypothesis, OrientedFeature, PointFeature};
use crate::float::{lit, Float};
use crate::lattice::{GridDimensions, LatticeKind};
use crate::result::GridSolution;

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

/// Parameters for future detection algorithms.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct DetectionParams<F: Float> {
    /// Residual threshold in image pixels for algorithms that fit a lattice.
    pub max_residual_px: F,
}

impl<F: Float> Default for DetectionParams<F> {
    fn default() -> Self {
        Self {
            max_residual_px: lit::<F>(2.0),
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
/// All combinations currently return [`GridError::UnsupportedCombination`].
/// This keeps the public matrix explicit while the old square-specific
/// implementation remains quarantined from the facade.
pub fn detect_grid<F: Float>(request: DetectionRequest<'_, F>) -> Result<GridSolution<F>> {
    Err(GridError::UnsupportedCombination {
        task: GridTask::Detection,
        lattice: request.lattice,
        evidence: request.evidence.kind(),
    })
}
