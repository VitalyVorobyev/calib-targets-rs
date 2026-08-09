//! Detection task facade.
//!
//! Square supports its three applicable input-feature kinds; hex supports
//! position-only, single-family, and native three-family evidence.
//! [`Evidence::Oriented2`] is the native square shape, assembled
//! by the axis-driven topological grid finder (Delaunay → quad-mesh →
//! flood-fill → validate → fit). [`Evidence::Positions`] (orientation-free) and
//! [`Evidence::Oriented1`] (single-axis) are synthesized up to the Oriented2
//! shape through the expert orientation utilities and then run the same
//! assembler, with the geometry-only recovery schedule enabled to recover the recall the
//! synthesized-axis frontier would otherwise leave on the table — so all three
//! square input kinds share one back-half. All produce the same
//! [`GridDetection`] shape.
//!
//! Unsupported lattice/evidence pairs return a typed
//! [`GridError::UnsupportedCombination`] (see the support matrix on
//! [`detect_grid`]).
//!
//! The detection surface is pinned to `f32`. The generic-`F` surface that
//! remains in the crate is the pure-geometry [`crate::geometry`] module.

use crate::error::{EvidenceKind, GridError, GridTask, Result};
use crate::feature::{OrientedFeature, PointFeature};
use crate::lattice::{GridDimensions, LatticeKind};
use crate::result::{GridDetection, GridSolution};
use std::collections::HashSet;

use crate::shared::recovery_schedule::{RecoverySchedule, SquareAxisProvenance};
use crate::shared::validate::ValidationParams;
use crate::topological::TopologicalParams;

/// Evidence supplied to a detection task.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Evidence<'a> {
    /// Position-only point features.
    Positions(&'a [PointFeature]),
    /// Point features with one measured local lattice family. Square
    /// synthesizes one missing direction; hex internally synthesizes two while
    /// preserving the measured angle and uncertainty.
    Oriented1(&'a [OrientedFeature<1>]),
    /// Point features with two local lattice directions — the native square
    /// input shape consumed by both algorithms.
    Oriented2(&'a [OrientedFeature<2>]),
    /// Point features with three local lattice directions. **Hex-native
    /// evidence**: a hexagonal lattice has three axis families, and a feature
    /// detector that recovers all three feeds them here. The hex detection
    /// path consumes the axes as an unordered local set.
    Oriented3(&'a [OrientedFeature<3>]),
}

impl Evidence<'_> {
    /// Return this evidence's kind for dispatch and typed errors.
    pub fn kind(&self) -> EvidenceKind {
        match self {
            Self::Positions(_) => EvidenceKind::Positions,
            Self::Oriented1(_) => EvidenceKind::Oriented1,
            Self::Oriented2(_) => EvidenceKind::Oriented2,
            Self::Oriented3(_) => EvidenceKind::Oriented3,
        }
    }
}

/// Detection parameters for ordinary callers.
///
/// Defaults are the recommended production configuration. The only stable
/// user-facing knob is the maximum lattice-fit residual. Stage-specific
/// controls live in [`crate::expert::DetectionTuning`] and are opt-in.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DetectionParams {
    /// Residual threshold in image pixels for algorithms that fit a lattice.
    max_residual_px: f32,
    advanced: Option<Box<DetectionTuning>>,
}

impl Default for DetectionParams {
    fn default() -> Self {
        Self {
            max_residual_px: 2.0,
            advanced: None,
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

    /// Builder-style override: replace the max residual threshold.
    #[must_use]
    pub fn with_max_residual_px(mut self, max_residual_px: f32) -> Self {
        self.max_residual_px = max_residual_px;
        self
    }

    /// Attach opt-in expert tuning.
    ///
    /// Ordinary callers should leave this unset. The expert field set follows
    /// algorithm stages and is intentionally less stable than this facade.
    #[must_use]
    pub fn with_advanced(mut self, tuning: DetectionTuning) -> Self {
        self.advanced = Some(Box::new(tuning));
        self
    }

    /// Maximum accepted model-to-image residual, in image pixels.
    pub fn max_residual_px(&self) -> f32 {
        self.max_residual_px
    }

    pub(crate) fn tuning(&self) -> &DetectionTuning {
        self.advanced.as_deref().unwrap_or(&DEFAULT_TUNING)
    }
}

static DEFAULT_TUNING: std::sync::LazyLock<DetectionTuning> =
    std::sync::LazyLock::new(DetectionTuning::default);

/// Expert-only, stage-specific detection tuning.
///
/// This type is re-exported from [`crate::expert`]. Its fields are useful for
/// detector builders and diagnostic campaigns, but are intentionally absent
/// from the ordinary [`DetectionParams`] workflow.
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct DetectionTuning {
    /// Topological grid-finder tuning.
    pub topological: TopologicalParams,
    /// Post-detection structural validation tuning.
    pub validation: ValidationParams,
    /// Post-convergence recovery policy.
    pub recovery: RecoverySchedule,
}

impl DetectionTuning {
    /// Replace topological grid-finder tuning.
    #[must_use]
    pub fn with_topological(mut self, value: TopologicalParams) -> Self {
        self.topological = value;
        self
    }

    /// Replace structural validation tuning.
    #[must_use]
    pub fn with_validation(mut self, value: ValidationParams) -> Self {
        self.validation = value;
        self
    }

    /// Replace the post-convergence recovery policy.
    #[must_use]
    pub fn with_recovery(mut self, value: RecoverySchedule) -> Self {
        self.recovery = value;
        self
    }
}

/// Detection request.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct DetectionRequest<'a> {
    /// Lattice family to recover.
    lattice: LatticeKind,
    /// Evidence available to the detector.
    evidence: Evidence<'a>,
    /// Optional known grid dimensions.
    dimensions: Option<GridDimensions>,
    /// Detection parameters.
    params: DetectionParams,
}

impl<'a> DetectionRequest<'a> {
    /// Construct a request with production defaults and no positional optionals.
    pub fn new(lattice: LatticeKind, evidence: Evidence<'a>) -> Self {
        Self {
            lattice,
            evidence,
            dimensions: None,
            params: DetectionParams::default(),
        }
    }

    /// Constrain the maximum feature-coordinate span of a detected grid.
    #[must_use]
    pub fn with_dimensions(mut self, dimensions: GridDimensions) -> Self {
        self.dimensions = Some(dimensions);
        self
    }

    /// Replace the default detection parameters.
    #[must_use]
    pub fn with_params(mut self, params: DetectionParams) -> Self {
        self.params = params;
        self
    }

    pub(crate) fn lattice(&self) -> LatticeKind {
        self.lattice
    }

    pub(crate) fn evidence(&self) -> Evidence<'a> {
        self.evidence
    }

    pub(crate) fn dimensions(&self) -> Option<GridDimensions> {
        self.dimensions
    }

    pub(crate) fn params(&self) -> &DetectionParams {
        &self.params
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
/// | `(Hex, Oriented3)` | supported — topological only |
/// | `(Hex, Positions)` | supported — synthesize 3 axes, then hex topological |
/// | `(Hex, Oriented1)` | supported — keep trusted family, synthesize 2 axes |
/// | `(Hex, Oriented2)` | `UnsupportedCombination` |
///
/// * `(Square, Oriented2)` — the axis-driven SBF09 topological grid finder
///   (Delaunay → quad-mesh → flood-fill → validate → fit) returns a labelled
///   [`GridDetection`] with a fitted projective transform; downstream consumers
///   stay agnostic.
/// * `(Square, Positions)` — orientation-free input. Each corner's two
///   local grid directions are synthesized from neighbour geometry
///   ([`crate::expert::orientation::synthesize_oriented2`]) and then fed to the topological
///   assembler, exactly as for `(Square, Oriented2)` — with the geometry-only
///   [`RecoverySchedule`] enabled to recover the synthesized-axis recall.
///   Use this for dot / circle grids and for chessboards whose corners carry
///   no axis estimate.
/// * `(Square, Oriented1)` — single-axis input. The supplied axis is kept
///   and the orthogonal grid direction is recovered from neighbour geometry
///   ([`crate::expert::orientation::synthesize_oriented2_from_oriented1`]); the resulting
///   [`OrientedFeature<2>`] then runs the topological assembler, exactly as for
///   `(Square, Positions)`. Use this for detectors that recover one dominant
///   edge orientation per feature but not the orthogonal one.
/// * `(Hex, Oriented3)` — hex-native triple-axis evidence. Runs the hex
///   topological grid finder (Delaunay triangles *are* the unit cells; no
///   diagonal class, no triangle-pair merge; axial `(q, r)` flood-fill walk).
///   Hex is **topological-only** with **no recovery schedule**.
/// * `(Hex, Positions)` — orientation-free hex input. The three local grid
///   directions are synthesized from neighbour geometry
///   ([`crate::expert::orientation::synthesize_oriented3`]) and then fed to the hex
///   topological path, mirroring the `(Square, Positions)` seam.
/// * `(Hex, Oriented1)` — one measured axis representing the same physical
///   hex family at every feature. The measured observation is preserved and
///   the two missing local directions are inferred from neighbour geometry.
///
/// `(Square, Oriented3)` (square does not consume triple-axis evidence) and
/// `(Hex, Oriented2)` (no unambiguous physical-family contract)
/// stay `UnsupportedCombination` — no working algorithm exists for those slots.
///
/// **Multi-component results.** The topological assembler can produce more
/// than one connected component (it labels each connected quad-mesh component,
/// then runs local component merge). This entry point returns the largest
/// component only. Use [`detect_grid_all`] when secondary components must be
/// preserved with their own `(u, v)` labels.
pub fn detect_grid(request: DetectionRequest<'_>) -> Result<GridDetection> {
    let mut detections = detect_grid_all(request)?;
    if detections.is_empty() {
        Err(GridError::InsufficientEvidence)
    } else {
        Ok(detections.remove(0))
    }
}

/// Dispatch oriented-2 features (caller-supplied or synthesized) to the
/// topological square assembler. The single dispatch point shared by the
/// `Oriented2`, `Positions`, and `Oriented1` arms so the three input kinds
/// reach identical strategy code.
fn run_square_oriented2(
    features: &[OrientedFeature<2>],
    request: &DetectionRequest<'_>,
    axis_provenance: SquareAxisProvenance,
) -> Result<Vec<GridSolution>> {
    crate::topological::detect_square_oriented2_all(
        features,
        request.dimensions(),
        request.params(),
        axis_provenance,
    )
}

/// Dispatch hex triple-axis features (caller-supplied or synthesized) to the
/// hex topological path.
///
/// Hex detection is **topological-only** (see the support matrix on
/// [`detect_grid`]).
fn run_hex_oriented3(
    features: &[OrientedFeature<3>],
    request: &DetectionRequest<'_>,
) -> Result<Vec<GridSolution>> {
    crate::topological::detect_hex_oriented3_topological_all(
        features,
        request.dimensions(),
        request.params(),
    )
}

/// Multi-component variant of [`detect_grid`].
///
/// Returns one [`GridDetection`] per
/// qualifying connected component, ordered by labelled-count
/// descending (ties broken by smallest labelled `source_index`). The
/// topological assembler may return several solutions.
///
/// Rejected features and stage-level evidence belong to the opt-in
/// `diagnostics` feature, not to this mandatory-result contract.
///
/// The same `UnsupportedCombination` matrix applies as for
/// [`detect_grid`].
pub fn detect_grid_all(request: DetectionRequest<'_>) -> Result<Vec<GridDetection>> {
    Ok(detect_grid_all_internal(request)?
        .into_iter()
        .map(|solution| solution.detection)
        .collect())
}

pub(crate) fn detect_grid_all_internal(request: DetectionRequest<'_>) -> Result<Vec<GridSolution>> {
    validate_request(&request)?;
    let solutions = match (request.lattice(), request.evidence()) {
        (LatticeKind::Square, Evidence::Oriented2(features)) => {
            // Native two-axis evidence: no synthesis, so the recovery schedule
            // stays off under `RecoverySchedule::Auto` (byte-compat).
            run_square_oriented2(features, &request, SquareAxisProvenance::FullyMeasured)?
        }
        (LatticeKind::Square, Evidence::Positions(features)) => {
            // Orientation-free input: recover each corner's two local grid
            // directions from neighbour geometry, then run the chosen square
            // strategy. Both strategies consume `OrientedFeature<2>`, so the
            // synthesized axes feed either path unchanged. The axes are
            // synthesized, so `Auto` enables the recovery schedule.
            let oriented = crate::orient::synthesize_oriented2(features);
            run_square_oriented2(
                &oriented,
                &request,
                SquareAxisProvenance::IncludesSynthesized,
            )?
        }
        (LatticeKind::Square, Evidence::Oriented1(features)) => {
            // Single-axis input: keep the supplied axis and recover the second
            // local grid direction from neighbour geometry, then run the chosen
            // square strategy. Same Oriented2 back-half as the Positions path;
            // the second axis is synthesized, so `Auto` enables recovery.
            let oriented = crate::orient::synthesize_oriented2_from_oriented1(features);
            run_square_oriented2(
                &oriented,
                &request,
                SquareAxisProvenance::IncludesSynthesized,
            )?
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
        (LatticeKind::Hex, Evidence::Oriented1(features)) => {
            let oriented = crate::orient::synthesize_oriented3_from_oriented1(features);
            run_hex_oriented3(&oriented, &request)?
        }
        _ => {
            return Err(GridError::UnsupportedCombination {
                task: GridTask::Detection,
                lattice: request.lattice(),
                evidence: request.evidence().kind(),
            })
        }
    };
    Ok(solutions)
}

pub(crate) fn validate_request(request: &DetectionRequest<'_>) -> Result<()> {
    if let Some(dimensions) = request.dimensions() {
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err(GridError::InconsistentInput(
                "grid dimensions count feature positions and must be non-zero".to_owned(),
            ));
        }
    }

    let residual = request.params().max_residual_px();
    if residual.is_nan() || residual < 0.0 {
        return Err(GridError::InconsistentInput(
            "max_residual_px must be non-negative or +infinity".to_owned(),
        ));
    }
    validate_tuning(request.params().tuning())?;

    match request.evidence() {
        Evidence::Positions(features) => validate_points(features.iter()),
        Evidence::Oriented1(features) => validate_oriented(features),
        Evidence::Oriented2(features) => validate_oriented(features),
        Evidence::Oriented3(features) => validate_oriented(features),
    }
}

fn validate_points<'a>(points: impl Iterator<Item = &'a PointFeature>) -> Result<()> {
    let mut source_indices = HashSet::new();
    for point in points {
        if !point.position.x.is_finite() || !point.position.y.is_finite() {
            return Err(GridError::InconsistentInput(format!(
                "feature {} has a non-finite image position",
                point.source_index
            )));
        }
        if !source_indices.insert(point.source_index) {
            return Err(GridError::InconsistentInput(format!(
                "duplicate feature source_index {}",
                point.source_index
            )));
        }
    }
    Ok(())
}

fn validate_oriented<const N: usize>(features: &[OrientedFeature<N>]) -> Result<()> {
    validate_points(features.iter().map(|feature| &feature.point))?;
    for feature in features {
        for (slot, axis) in feature.axes.iter().enumerate() {
            if !axis.angle_rad.is_finite() {
                return Err(GridError::InconsistentInput(format!(
                    "feature {} axis {slot} has a non-finite angle",
                    feature.point.source_index
                )));
            }
            if axis
                .sigma_rad
                .is_some_and(|sigma| !sigma.is_finite() || sigma < 0.0)
            {
                return Err(GridError::InconsistentInput(format!(
                    "feature {} axis {slot} has an invalid sigma",
                    feature.point.source_index
                )));
            }
        }
    }
    Ok(())
}

fn validate_tuning(tuning: &DetectionTuning) -> Result<()> {
    let topo = &tuning.topological;
    let finite_positive = |value: f32| value.is_finite() && value > 0.0;
    if !finite_positive(topo.axis_align_tol_rad)
        || !finite_positive(topo.max_axis_sigma_rad)
        || !finite_positive(topo.cluster_axis_tol_rad)
        || !topo.opposing_edge_ratio_max.is_finite()
        || topo.opposing_edge_ratio_max < 1.0
        || !topo.edge_length_min_rel.is_finite()
        || topo.edge_length_min_rel < 0.0
        || topo.edge_length_max_rel.is_nan()
        || topo.edge_length_max_rel <= 0.0
        || topo.edge_length_min_rel > topo.edge_length_max_rel
        || topo.min_corners_for_component < 4
        || topo.min_quads_per_component == 0
        || topo
            .axis_cluster_centers
            .is_some_and(|centers| centers.iter().any(|center| !center.is_finite()))
    {
        return Err(GridError::InconsistentInput(
            "invalid expert topological tuning".to_owned(),
        ));
    }

    let validation = &tuning.validation;
    let non_negative_or_inf = |value: f32| !value.is_nan() && value >= 0.0;
    if !non_negative_or_inf(validation.line_tol_rel)
        || validation.line_min_members < 2
        || !non_negative_or_inf(validation.local_h_tol_rel)
        || !validation.step_deviation_thresh_rel.is_finite()
        || validation.step_deviation_thresh_rel < 0.0
    {
        return Err(GridError::InconsistentInput(
            "invalid expert validation tuning".to_owned(),
        ));
    }
    Ok(())
}
