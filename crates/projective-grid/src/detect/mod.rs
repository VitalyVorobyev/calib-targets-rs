//! Detection task facade.
//!
//! The first working implementation targets
//! [`(LatticeKind::Square, Evidence::Oriented2)`](Evidence::Oriented2): a
//! seed-and-grow pipeline (seed → BFS grow → validate → fit). The same
//! evidence slot also supports an axis-driven topological grid finder.
//! Both produce the same [`GridSolution`] shape; callers select the
//! algorithm via [`SquareAlgorithm`].
//! All other `(lattice, evidence)` combinations remain typed
//! [`GridError::UnsupportedCombination`] placeholders.
//!
//! The detection surface is pinned to `f32`. The generic-`F` surface that
//! remains in the crate is the pure-geometry [`crate::geometry`] module.

pub mod advanced;
mod square;

use crate::error::{EvidenceKind, GridError, GridTask, Result};
use crate::feature::{CoordinateHypothesis, OrientedFeature, PointFeature};
use crate::lattice::{GridDimensions, LatticeKind};
use crate::result::{GridSolution, RejectedFeature};

pub use crate::detect::advanced::square::grow::GrowParams;
pub use crate::detect::advanced::square::seed::finder::SeedQuadParams as SeedParams;
pub use crate::detect::advanced::square::validate::ValidationParams as ValidateParams;
pub use square::TopologicalParams;

/// Algorithm selector for `(LatticeKind::Square, Evidence::Oriented2)`.
///
/// Both variants accept the same [`Evidence::Oriented2`] input and
/// produce the same [`GridSolution`] output; they differ in how they
/// build the grid graph.
///
/// * [`SeedAndGrow`](Self::SeedAndGrow) is the default mature square
///   assembly path.
/// * [`Topological`](Self::Topological) is the Shu/Brunton/Fiala 2009
///   axis-driven grid finder. Image-free; tends to recover denser grids
///   on clean inputs but is more sensitive to per-corner axis quality.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum SquareAlgorithm {
    /// Seed-quad finder + BFS grow + shared validate + fit.
    #[default]
    SeedAndGrow,
    /// Axis-driven topological grid finder + shared validate + fit.
    Topological,
}

/// Evidence supplied to a detection task.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Evidence<'a> {
    /// Position-only point features.
    Positions(&'a [PointFeature]),
    /// Point features with one local lattice direction.
    Oriented1(&'a [OrientedFeature<1>]),
    /// Point features with two local lattice directions.
    Oriented2(&'a [OrientedFeature<2>]),
    /// Point features with three local lattice directions.
    Oriented3(&'a [OrientedFeature<3>]),
    /// Point features plus caller-supplied coordinate hypotheses.
    CoordinateHypotheses {
        /// Position-only features.
        features: &'a [PointFeature],
        /// Proposed coordinate labels.
        hypotheses: &'a [CoordinateHypothesis],
    },
}

impl Evidence<'_> {
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
///
/// The sub-config types (`SeedParams`, `GrowParams`, `ValidateParams`)
/// are the advanced-engine configs and do not implement `PartialEq`, so
/// neither does `DetectionParams`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DetectionParams {
    /// Residual threshold in image pixels for algorithms that fit a lattice.
    pub max_residual_px: f32,
    /// Algorithm picker for `(Square, Oriented2)`. Defaults to
    /// [`SquareAlgorithm::SeedAndGrow`] so Phase C consumers compile
    /// without requiring an explicit algorithm choice.
    pub algorithm: SquareAlgorithm,
    /// Seed-quad finder tuning — consumed iff `algorithm ==
    /// SquareAlgorithm::SeedAndGrow`.
    pub seed: SeedParams,
    /// BFS grow engine tuning — consumed iff `algorithm ==
    /// SquareAlgorithm::SeedAndGrow`.
    pub grow: GrowParams,
    /// Topological grid-finder tuning — consumed iff `algorithm ==
    /// SquareAlgorithm::Topological`.
    pub topological: TopologicalParams,
    /// Post-detection validation tuning — shared between both
    /// `(Square, Oriented2)` algorithm paths.
    pub validate: ValidateParams,
}

impl Default for DetectionParams {
    fn default() -> Self {
        Self {
            max_residual_px: 2.0,
            algorithm: SquareAlgorithm::default(),
            seed: SeedParams::default(),
            grow: GrowParams::default(),
            topological: TopologicalParams::default(),
            validate: ValidateParams::default(),
        }
    }
}

impl DetectionParams {
    /// Construct detection parameters from just the residual threshold; the
    /// sub-configs take their defaults.
    pub fn new(max_residual_px: f32) -> Self {
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
    pub fn with_topological(mut self, topological: TopologicalParams) -> Self {
        self.topological = topological;
        self
    }

    /// Builder-style override: replace the validate sub-config.
    pub fn with_validate(mut self, validate: ValidateParams) -> Self {
        self.validate = validate;
        self
    }

    /// Builder-style override: replace the seed-quad sub-config.
    pub fn with_seed(mut self, seed: SeedParams) -> Self {
        self.seed = seed;
        self
    }

    /// Builder-style override: replace the BFS-grow sub-config.
    pub fn with_grow(mut self, grow: GrowParams) -> Self {
        self.grow = grow;
        self
    }

    /// Builder-style override: replace the max residual threshold.
    pub fn with_max_residual_px(mut self, max_residual_px: f32) -> Self {
        self.max_residual_px = max_residual_px;
        self
    }
}

/// Detection request.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DetectionRequest<'a> {
    /// Lattice family to recover.
    pub lattice: LatticeKind,
    /// Evidence available to the detector.
    pub evidence: Evidence<'a>,
    /// Optional known grid dimensions.
    pub dimensions: Option<GridDimensions>,
    /// Detection parameters.
    pub params: DetectionParams,
}

impl<'a> DetectionRequest<'a> {
    /// Construct a detection request.
    pub fn new(
        lattice: LatticeKind,
        evidence: Evidence<'a>,
        dimensions: Option<GridDimensions>,
        params: DetectionParams,
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
///     - [`SquareAlgorithm::SeedAndGrow`] (default): seed-quad finder +
///       BFS grow + validate + fit.
///     - [`SquareAlgorithm::Topological`]: axis-driven SBF09 grid finder +
///       validate + fit.
///
///   Both return a labelled [`GridSolution`] with a fitted projective
///   transform; downstream consumers stay agnostic.
/// * Every other combination — typed [`GridError::UnsupportedCombination`].
///
/// `(Square, Positions)`, `(Square, Oriented1)`, `(Square, Oriented3)`,
/// `(Square, CoordinateHypotheses)`, and every `(Hex, *)` variant stay
/// `UnsupportedCombination` — no working algorithm exists in the
/// current implementation for those slots.
///
/// **Multi-component results.** Both algorithms can produce more than one
/// connected component (seed-and-grow assembles each disconnected patch
/// independently, then runs local component merge; the topological path
/// labels each connected quad-mesh component). This entry point returns
/// the largest component only. Use [`detect_grid_all`] when secondary
/// components must be preserved with their own `(u, v)` labels.
pub fn detect_grid(request: DetectionRequest<'_>) -> Result<GridSolution> {
    let mut report = detect_grid_all(request)?;
    if report.solutions.is_empty() {
        Err(GridError::InsufficientEvidence)
    } else {
        Ok(report.solutions.remove(0))
    }
}

/// Multi-component variant of [`detect_grid`].
///
/// Returns a [`DetectionReport`] with one [`GridSolution`] per
/// qualifying connected component, ordered by labelled-count
/// descending (ties broken by smallest labelled `source_index`). Both
/// the seed-and-grow and topological algorithms may return several
/// solutions.
///
/// Features that no component admitted are surfaced in the *first*
/// solution's `rejected` vector (the same shape callers of
/// [`detect_grid`] saw historically). Per-component validation drops
/// and over-residual entries stay attached to their owning component's
/// `rejected` vector.
///
/// The same `UnsupportedCombination` matrix applies as for
/// [`detect_grid`].
pub fn detect_grid_all(request: DetectionRequest<'_>) -> Result<DetectionReport> {
    let solutions = match (request.lattice, request.evidence) {
        (LatticeKind::Square, Evidence::Oriented2(features)) => match request.params.algorithm {
            SquareAlgorithm::SeedAndGrow => square::detect_square_oriented2_seed_grow(
                features,
                request.dimensions,
                &request.params,
            )?,
            SquareAlgorithm::Topological => square::detect_square_oriented2_topological_all(
                features,
                request.dimensions,
                &request.params,
            )?,
        },
        _ => {
            return Err(GridError::UnsupportedCombination {
                task: GridTask::Detection,
                lattice: request.lattice,
                evidence: request.evidence.kind(),
            })
        }
    };
    Ok(DetectionReport::new(solutions, Vec::new()))
}

/// Multi-component detection result returned by [`detect_grid_all`].
///
/// `solutions` is ordered by labelled-count descending; the first
/// entry is the same `GridSolution` a single-component
/// [`detect_grid`] caller historically received. `rejected` is a
/// crate-level slot reserved for features that no component admitted
/// when the orchestrator (rather than a particular component) is the
/// authoritative source of that information. It is left empty for the
/// two algorithms currently implemented — the per-component rejected
/// vectors already cover the wire shape.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DetectionReport {
    /// Per-component labelled solutions, ordered by component size
    /// descending.
    pub solutions: Vec<GridSolution>,
    /// Features that no component admitted, scoped to the orchestrator
    /// (not a particular component). Currently empty for both
    /// `SquareAlgorithm` variants — see the struct-level docs.
    pub rejected: Vec<RejectedFeature>,
}

impl DetectionReport {
    /// Construct a detection report.
    pub fn new(solutions: Vec<GridSolution>, rejected: Vec<RejectedFeature>) -> Self {
        Self {
            solutions,
            rejected,
        }
    }
}
