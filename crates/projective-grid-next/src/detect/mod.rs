//! Detection task facade.
//!
//! Phase C ships the first working implementation for
//! [`(LatticeKind::Square, Evidence::Oriented2)`](Evidence::Oriented2): a
//! seed-and-grow pipeline (seed → BFS grow → validate → fit). Phase D
//! adds the axis-driven topological grid finder as a second algorithm
//! choice for the same evidence slot — both produce the same
//! [`GridSolution`] shape, the caller picks via [`SquareAlgorithm`].
//! All other `(lattice, evidence)` combinations remain typed
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
pub use square::TopologicalParams;

/// Algorithm selector for `(LatticeKind::Square, Evidence::Oriented2)`.
///
/// Both variants accept the same [`Evidence::Oriented2`] input and
/// produce the same [`GridSolution`] output; they differ in how they
/// build the grid graph.
///
/// * [`SeedAndGrow`](Self::SeedAndGrow) is the default — battle-tested
///   on all four calibration target families.
/// * [`Topological`](Self::Topological) is the Shu/Brunton/Fiala 2009
///   axis-driven grid finder. Image-free; tends to recover denser grids
///   on clean inputs but is more sensitive to per-corner axis quality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum SquareAlgorithm {
    /// Phase C seed-quad finder + BFS grow + shared validate + fit.
    #[default]
    SeedAndGrow,
    /// Phase D axis-driven topological grid finder + shared validate + fit.
    Topological,
}

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
/// The single `max_residual_px` knob from the Phase-A shell sits alongside
/// per-algorithm sub-configs. The
/// [`algorithm`](Self::algorithm) selector decides which of the
/// `(Square, Oriented2)` paths consumes which sub-config; combinations
/// not run by the chosen algorithm leave their sub-configs untouched.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct DetectionParams<F: Float> {
    /// Residual threshold in image pixels for algorithms that fit a lattice.
    pub max_residual_px: F,
    /// Algorithm picker for `(Square, Oriented2)`. Defaults to
    /// [`SquareAlgorithm::SeedAndGrow`] so Phase C consumers compile
    /// and pass without code changes.
    pub algorithm: SquareAlgorithm,
    /// Seed-quad finder tuning — consumed iff `algorithm ==
    /// SquareAlgorithm::SeedAndGrow`.
    pub seed: SeedParams<F>,
    /// BFS grow engine tuning — consumed iff `algorithm ==
    /// SquareAlgorithm::SeedAndGrow`.
    pub grow: GrowParams<F>,
    /// Topological grid-finder tuning — consumed iff `algorithm ==
    /// SquareAlgorithm::Topological`.
    pub topological: TopologicalParams<F>,
    /// Post-detection validation tuning — shared between both
    /// `(Square, Oriented2)` algorithm paths.
    pub validate: ValidateParams<F>,
}

impl<F: Float> Default for DetectionParams<F> {
    fn default() -> Self {
        Self {
            max_residual_px: lit::<F>(2.0),
            algorithm: SquareAlgorithm::default(),
            seed: SeedParams::default(),
            grow: GrowParams::default(),
            topological: TopologicalParams::default(),
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

    /// Builder-style override: pick a different `(Square, Oriented2)`
    /// algorithm. Default is [`SquareAlgorithm::SeedAndGrow`].
    pub fn with_algorithm(mut self, algorithm: SquareAlgorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Builder-style override: replace the topological sub-config.
    pub fn with_topological(mut self, topological: TopologicalParams<F>) -> Self {
        self.topological = topological;
        self
    }

    /// Builder-style override: replace the validate sub-config.
    pub fn with_validate(mut self, validate: ValidateParams<F>) -> Self {
        self.validate = validate;
        self
    }

    /// Builder-style override: replace the seed-quad sub-config.
    pub fn with_seed(mut self, seed: SeedParams<F>) -> Self {
        self.seed = seed;
        self
    }

    /// Builder-style override: replace the BFS-grow sub-config.
    pub fn with_grow(mut self, grow: GrowParams<F>) -> Self {
        self.grow = grow;
        self
    }

    /// Builder-style override: replace the max residual threshold.
    pub fn with_max_residual_px(mut self, max_residual_px: F) -> Self {
        self.max_residual_px = max_residual_px;
        self
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
/// Support matrix after Phase D:
///
/// * `(Square, Oriented2)` — two algorithm choices, picked via
///   [`DetectionParams::algorithm`]:
///     - [`SquareAlgorithm::SeedAndGrow`] (default): Phase C
///       seed-quad finder + BFS grow + validate + fit.
///     - [`SquareAlgorithm::Topological`]: Phase D axis-driven SBF09
///       grid finder + validate + fit.
///
///   Both return a labelled [`GridSolution`] with a fitted projective
///   transform; downstream consumers stay agnostic.
/// * Every other combination — typed [`GridError::UnsupportedCombination`].
///
/// `(Square, Positions)`, `(Square, Oriented1)`, `(Square, Oriented3)`,
/// `(Square, CoordinateHypotheses)`, and every `(Hex, *)` variant stay
/// `UnsupportedCombination` — no working algorithm exists in the
/// legacy crate or on-disk salvage to migrate for those slots.
pub fn detect_grid<F>(request: DetectionRequest<'_, F>) -> Result<GridSolution<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    match (request.lattice, request.evidence) {
        (LatticeKind::Square, Evidence::Oriented2(features)) => match request.params.algorithm {
            SquareAlgorithm::SeedAndGrow => square::detect_square_oriented2_seed_grow(
                features,
                request.dimensions,
                &request.params,
            ),
            SquareAlgorithm::Topological => square::detect_square_oriented2_topological(
                features,
                request.dimensions,
                &request.params,
            ),
        },
        _ => Err(GridError::UnsupportedCombination {
            task: GridTask::Detection,
            lattice: request.lattice,
            evidence: request.evidence.kind(),
        }),
    }
}
