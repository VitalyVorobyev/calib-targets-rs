//! Detection task facade.
//!
//! The working implementations target the square lattice with any of the three
//! input-feature kinds. [`Evidence::Oriented2`] is the native shape, assembled
//! by the axis-driven topological grid finder (Delaunay → quad-mesh →
//! flood-fill → validate → fit). [`Evidence::Positions`] (orientation-free) and
//! [`Evidence::Oriented1`] (single-axis) are synthesized up to the Oriented2
//! shape ([`crate::orient`]) and then run the same assembler, with the
//! geometry-only [`RecoverySchedule`] enabled to recover the recall the
//! synthesized-axis frontier would otherwise leave on the table — so all three
//! square input kinds share one back-half. All produce the same
//! [`GridSolution`] shape.
//!
//! The remaining combinations — [`Evidence::Oriented3`],
//! [`Evidence::CoordinateHypotheses`], and every `(Hex, *)` variant — are
//! typed [`GridError::UnsupportedCombination`] placeholders (see the support
//! matrix on [`detect_grid`]).
//!
//! The detection surface is pinned to `f32`. The generic-`F` surface that
//! remains in the crate is the pure-geometry [`crate::geometry`] module.

use crate::error::{EvidenceKind, GridError, GridTask, Result};
use crate::feature::{CoordinateHypothesis, OrientedFeature, PointFeature};
use crate::lattice::{GridDimensions, LatticeKind};
use crate::result::{GridSolution, RejectedFeature};

pub use crate::shared::recovery_schedule::{RecoveryParams, RecoverySchedule};
pub use crate::shared::validate::ValidationParams as ValidateParams;
pub use crate::topological::TopologicalParams;

/// Algorithm selector for `(LatticeKind::Square, Evidence::Oriented2)`.
///
/// The square grid is assembled by the [`Topological`](Self::Topological)
/// axis-driven grid finder (the Shu/Brunton/Fiala 2009 image-free variant:
/// Delaunay triangulation + axis-driven cell test + flood-fill + validate +
/// fit). It is the only square assembler.
///
/// The enum is retained as a single-variant, `#[non_exhaustive]` type so the
/// public request shape (`detect_grid` / `detect_grid_all` /
/// [`DetectionParams::algorithm`]) stays stable across the seed-and-grow
/// retirement and a future alternative builder can be added without a breaking
/// change. The historical `SeedAndGrow` variant — a self-consistent 4-corner
/// seed plus BFS grow — was removed once the topological builder matched or
/// beat it on every shipping path. Its geometry-only recovery schedule
/// ([`RecoverySchedule`]) survives the retirement and now powers the
/// topological synthesized-axis path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum SquareAlgorithm {
    /// Axis-driven topological grid finder + shared validate + fit; the only
    /// square assembler.
    #[default]
    Topological,
}

/// Evidence supplied to a detection task.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Evidence<'a> {
    /// Position-only point features.
    Positions(&'a [PointFeature]),
    /// Point features with one local lattice direction. Supported for
    /// `Square`: the orthogonal direction is synthesized from neighbour
    /// geometry ([`crate::orient::synthesize_oriented2_from_oriented1`]).
    Oriented1(&'a [OrientedFeature<1>]),
    /// Point features with two local lattice directions — the native square
    /// input shape consumed by both algorithms.
    Oriented2(&'a [OrientedFeature<2>]),
    /// Point features with three local lattice directions. **Hex-native
    /// evidence**: a hexagonal lattice has three axis families, and a feature
    /// detector that recovers all three feeds them here. The hex detection
    /// path is the intended consumer; until a detector consumes it this stays
    /// [`GridError::UnsupportedCombination`].
    Oriented3(&'a [OrientedFeature<3>]),
    /// Point features plus caller-supplied coordinate hypotheses. **Roadmap
    /// slot** for decode-feedback labelling: a caller that has partially
    /// decoded marker / ring IDs supplies them as `(source_index, coord)`
    /// hypotheses to bias or seed the labelling. No detection algorithm
    /// consumes hypotheses yet, so this stays
    /// [`GridError::UnsupportedCombination`]; the
    /// [`check_consistency`](crate::check::check_consistency) task is the only
    /// current consumer of [`crate::feature::CoordinateHypothesis`].
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
/// The single `max_residual_px` knob from the Phase-A shell sits alongside the
/// topological assembler's sub-config, the shared post-detection validation
/// tuning, and the post-recovery schedule. The
/// [`algorithm`](Self::algorithm) selector is retained for request-shape
/// stability; it currently has a single value
/// ([`SquareAlgorithm::Topological`]).
///
/// The sub-config types (`TopologicalParams`, `ValidateParams`,
/// `RecoveryParams`) include advanced-engine configs that do not implement
/// `PartialEq`, so neither does `DetectionParams`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DetectionParams {
    /// Residual threshold in image pixels for algorithms that fit a lattice.
    pub max_residual_px: f32,
    /// Algorithm picker for `(Square, Oriented2)`. Defaults to
    /// [`SquareAlgorithm::Topological`] (the only square assembler); retained
    /// for request-shape stability.
    pub algorithm: SquareAlgorithm,
    /// Topological grid-finder tuning.
    pub topological: TopologicalParams,
    /// Post-detection validation tuning.
    pub validate: ValidateParams,
    /// Post-convergence recovery schedule. Defaults to
    /// [`RecoverySchedule::Auto`]: the facade runs the geometry-only recovery
    /// schedule for the synthesized-axis paths (`Evidence::Positions` /
    /// `Evidence::Oriented1`) and skips it for native `Evidence::Oriented2`.
    /// Callers that own a downstream recovery stage set this to
    /// [`RecoverySchedule::Off`].
    pub recovery: RecoverySchedule,
}

impl Default for DetectionParams {
    fn default() -> Self {
        Self {
            max_residual_px: 2.0,
            algorithm: SquareAlgorithm::default(),
            topological: TopologicalParams::default(),
            validate: ValidateParams::default(),
            recovery: RecoverySchedule::default(),
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

    /// Builder-style override: pick the `(Square, Oriented2)` algorithm.
    /// Default is [`SquareAlgorithm::Topological`] (the only square assembler).
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

    /// Builder-style override: replace the max residual threshold.
    pub fn with_max_residual_px(mut self, max_residual_px: f32) -> Self {
        self.max_residual_px = max_residual_px;
        self
    }

    /// Builder-style override: set the post-convergence recovery schedule.
    pub fn with_recovery(mut self, recovery: RecoverySchedule) -> Self {
        self.recovery = recovery;
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
/// # Support matrix
///
/// | `(lattice, evidence)` | Status |
/// |---|---|
/// | `(Square, Oriented2)` | supported — topological assembler |
/// | `(Square, Oriented1)` | supported — synthesize 2nd axis, then Oriented2 |
/// | `(Square, Positions)` | supported — synthesize both axes, then Oriented2 |
/// | `(Square, Oriented3)` | `UnsupportedCombination` |
/// | `(Square, CoordinateHypotheses)` | `UnsupportedCombination` (roadmap) |
/// | `(Hex, Oriented3)` | supported — topological only |
/// | `(Hex, Positions)` | supported — synthesize 3 axes, then hex topological |
/// | `(Hex, Oriented1 / Oriented2)` | `UnsupportedCombination` |
///
/// * `(Square, Oriented2)` — the [`SquareAlgorithm::Topological`] axis-driven
///   SBF09 grid finder (Delaunay → quad-mesh → flood-fill → validate → fit)
///   returns a labelled [`GridSolution`] with a fitted projective transform;
///   downstream consumers stay agnostic.
/// * `(Square, Positions)` — orientation-free input. Each corner's two
///   local grid directions are synthesized from neighbour geometry
///   ([`crate::orient::synthesize_oriented2`]) and then fed to the topological
///   assembler, exactly as for `(Square, Oriented2)` — with the geometry-only
///   [`RecoverySchedule`] enabled to recover the synthesized-axis recall.
///   Use this for dot / circle grids and for chessboards whose corners carry
///   no axis estimate.
/// * `(Square, Oriented1)` — single-axis input. The supplied axis is kept
///   and the orthogonal grid direction is recovered from neighbour geometry
///   ([`crate::orient::synthesize_oriented2_from_oriented1`]); the resulting
///   [`OrientedFeature<2>`] then runs the topological assembler, exactly as for
///   `(Square, Positions)`. Use this for detectors that recover one dominant
///   edge orientation per feature but not the orthogonal one.
/// * Every other combination — typed [`GridError::UnsupportedCombination`].
///
/// * `(Hex, Oriented3)` — hex-native triple-axis evidence. Runs the hex
///   topological grid finder (Delaunay triangles *are* the unit cells; no
///   diagonal class, no triangle-pair merge; axial `(q, r)` flood-fill walk).
///   Hex is **topological-only** with **no recovery schedule**.
/// * `(Hex, Positions)` — orientation-free hex input. The three local grid
///   directions are synthesized from neighbour geometry
///   ([`crate::orient::synthesize_oriented3`]) and then fed to the hex
///   topological path, mirroring the `(Square, Positions)` seam.
///
/// `(Square, Oriented3)` (square does not consume triple-axis evidence),
/// `(Square, CoordinateHypotheses)` (a decode-feedback roadmap slot), and
/// `(Hex, Oriented1)` / `(Hex, Oriented2)` (hex needs three axis families)
/// stay `UnsupportedCombination` — no working algorithm exists for those slots.
///
/// **Multi-component results.** The topological assembler can produce more
/// than one connected component (it labels each connected quad-mesh component,
/// then runs local component merge). This entry point returns the largest
/// component only. Use [`detect_grid_all`] when secondary components must be
/// preserved with their own `(u, v)` labels.
pub fn detect_grid(request: DetectionRequest<'_>) -> Result<GridSolution> {
    let mut report = detect_grid_all(request)?;
    if report.solutions.is_empty() {
        Err(GridError::InsufficientEvidence)
    } else {
        Ok(report.solutions.remove(0))
    }
}

/// Dispatch oriented-2 features (caller-supplied or synthesized) to the
/// topological square assembler. The single dispatch point shared by the
/// `Oriented2`, `Positions`, and `Oriented1` arms so the three input kinds
/// reach identical strategy code.
fn run_square_oriented2(
    features: &[OrientedFeature<2>],
    request: &DetectionRequest<'_>,
    synthesized_axes: bool,
) -> Result<Vec<GridSolution>> {
    match request.params.algorithm {
        SquareAlgorithm::Topological => {
            crate::topological::detect_square_oriented2_topological_all(
                features,
                request.dimensions,
                &request.params,
                synthesized_axes,
            )
        }
    }
}

/// Dispatch hex triple-axis features (caller-supplied or synthesized) to the
/// hex topological path.
///
/// Hex detection is **topological-only**. The algorithm selector enum is named
/// [`SquareAlgorithm`]; its sole [`Topological`](SquareAlgorithm::Topological)
/// variant is the default-equivalent path here (see the support matrix on
/// [`detect_grid`]).
fn run_hex_oriented3(
    features: &[OrientedFeature<3>],
    request: &DetectionRequest<'_>,
) -> Result<Vec<GridSolution>> {
    match request.params.algorithm {
        SquareAlgorithm::Topological => crate::topological::detect_hex_oriented3_topological_all(
            features,
            request.dimensions,
            &request.params,
        ),
    }
}

/// Multi-component variant of [`detect_grid`].
///
/// Returns a [`DetectionReport`] with one [`GridSolution`] per
/// qualifying connected component, ordered by labelled-count
/// descending (ties broken by smallest labelled `source_index`). The
/// topological assembler may return several solutions.
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
        (LatticeKind::Square, Evidence::Oriented2(features)) => {
            // Native two-axis evidence: no synthesis, so the recovery schedule
            // stays off under `RecoverySchedule::Auto` (byte-compat).
            run_square_oriented2(features, &request, false)?
        }
        (LatticeKind::Square, Evidence::Positions(features)) => {
            // Orientation-free input: recover each corner's two local grid
            // directions from neighbour geometry, then run the chosen square
            // strategy. Both strategies consume `OrientedFeature<2>`, so the
            // synthesized axes feed either path unchanged. The axes are
            // synthesized, so `Auto` enables the recovery schedule.
            let oriented = crate::orient::synthesize_oriented2(features);
            run_square_oriented2(&oriented, &request, true)?
        }
        (LatticeKind::Square, Evidence::Oriented1(features)) => {
            // Single-axis input: keep the supplied axis and recover the second
            // local grid direction from neighbour geometry, then run the chosen
            // square strategy. Same Oriented2 back-half as the Positions path;
            // the second axis is synthesized, so `Auto` enables recovery.
            let oriented = crate::orient::synthesize_oriented2_from_oriented1(features);
            run_square_oriented2(&oriented, &request, true)?
        }
        (LatticeKind::Hex, Evidence::Oriented3(features)) => {
            // Hex-native triple-axis evidence. Hex detection is
            // topological-only.
            run_hex_oriented3(features, &request)?
        }
        (LatticeKind::Hex, Evidence::Positions(features)) => {
            // Orientation-free hex input: synthesize the three local grid
            // directions from neighbour geometry, then run the hex topological
            // path. Mirrors the `(Square, Positions)` synthesis seam.
            let oriented = crate::orient::synthesize_oriented3(features);
            run_hex_oriented3(&oriented, &request)?
        }
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
/// topological assembler — the per-component rejected vectors already
/// cover the wire shape.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DetectionReport {
    /// Per-component labelled solutions, ordered by component size
    /// descending.
    pub solutions: Vec<GridSolution>,
    /// Features that no component admitted, scoped to the orchestrator
    /// (not a particular component). Currently empty for the topological
    /// assembler — see the struct-level docs.
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
