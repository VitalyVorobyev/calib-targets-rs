//! Axis-driven topological grid finder (Shu/Brunton/Fiala 2009) for the
//! `(LatticeKind::Square, Evidence::Oriented2)` slot.
//!
//! Pipeline overview:
//!
//! 1. Pre-filter features whose both axes are uninformative under
//!    [`TopologicalParams::max_axis_sigma_rad`].
//! 2. Delaunay-triangulate the surviving feature positions.
//! 3. Classify every Delaunay half-edge as `Grid`, `Diagonal`, or
//!    `Spurious` via the per-corner axes (no image-color sampling).
//! 4. Merge triangle pairs sharing a `Diagonal` edge into quads (one
//!    quad per lattice cell).
//! 5. Drop quads with two illegal corners (quad-mesh degree > 4),
//!    extreme parallelograms, or out-of-band edge lengths against the
//!    per-component median.
//! 6. Flood-fill integer `(u, v)` labels through the surviving quad
//!    mesh and rebase each connected component to `(0, 0)`.
//! 7. Reunite the labelled components in label space with the shared
//!    [`crate::shared::merge::merge_components_local`]
//!    pass (local geometry only, radial-distortion safe), so the topological
//!    path no longer leaves an un-merged quad-mesh component per disconnected
//!    patch.
//! 8. Reuse the shared advanced [`validate`](crate::shared::validate)
//!    post-stage to drop labelled corners flagged by line-collinearity and
//!    local-H checks.
//! 9. Fit a projective transform on the surviving labels and report
//!    per-corner residuals.
//!
//! Multi-component output is represented directly: the orchestrator returns
//! one [`GridSolution`] per qualifying component, ordered by labelled count
//! descending.

use std::collections::HashMap;

use nalgebra::Point2;

use crate::detect::DetectionParams;
use crate::error::{GridError, Result};
use crate::feature::OrientedFeature;
use crate::lattice::{Coord, GridDimensions};
use crate::result::GridSolution;
use crate::shared::merge::{merge_components_local, LocalMergeParams};
use crate::shared::recovery_schedule::SquareAxisProvenance;

use super::axis::build_axis_caches;
use super::{classify, delaunay, filter, quads, walk};

mod solution;

use solution::{
    assemble_solutions, build_component_solution, build_usable_mask, triangulate_lattice_candidates,
};
pub(in crate::topological) use solution::{triangulate_usable, ComponentOutput};

/// Minimum number of usable features for Delaunay triangulation.
pub(in crate::topological) const MIN_USABLE_FOR_DELAUNAY: usize = 3;

/// Exact stage snapshots collected only by the diagnostics entry point.
#[derive(Debug, Default)]
pub(super) struct SquarePipelineTrace {
    pub(super) usable: Vec<bool>,
    pub(super) triangles: Vec<[usize; 3]>,
    pub(super) edges: Vec<(usize, usize, classify::EdgeClass)>,
    pub(super) raw_quads: Vec<[usize; 4]>,
    pub(super) topology_quads: Vec<[usize; 4]>,
    pub(super) geometry_quads: Vec<[usize; 4]>,
    pub(super) scale_quads: Vec<[usize; 4]>,
    pub(super) walk_components: Vec<Vec<(Coord, usize)>>,
    pub(super) merged_components: Vec<Vec<(Coord, usize)>>,
}

type LabelledComponent = HashMap<Coord, usize>;
type LabelledComponents = Vec<LabelledComponent>;

struct SquareTopology {
    positions: Vec<Point2<f32>>,
    components: LabelledComponents,
}

/// Tuning knobs for the axis-driven topological pipeline.
///
/// Defaults are conservative values pinned by the crate's regression tests.
/// Adding new fields is non-breaking via `#[non_exhaustive]`;
/// literal-construction from outside the crate goes through [`Self::default`]
/// + struct-update syntax or [`Self::new`].
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[non_exhaustive]
pub struct TopologicalParams {
    /// Maximum angular distance, in radians, between an edge's
    /// direction and a corner's axis for the edge to classify as a
    /// grid edge at that corner. Default: 15° = 0.262 rad.
    #[cfg_attr(feature = "schemars", schemars(extend("exclusiveMinimum" = 0.0)))]
    #[cfg_attr(feature = "schemars", schemars(extend("x-unit" = "rad")))]
    pub axis_align_tol_rad: f32,
    /// Maximum 1σ axis uncertainty (radians) for a feature axis to be
    /// considered informative. Features whose both axes have
    /// `sigma_rad ≥ max_axis_sigma_rad` are excluded from Delaunay;
    /// classification skips individual axes above the threshold.
    /// Default: `0.6 ≈ 34°`. `sigma_rad = None` is treated as informative.
    #[cfg_attr(feature = "schemars", schemars(extend("exclusiveMinimum" = 0.0)))]
    #[cfg_attr(feature = "schemars", schemars(extend("x-unit" = "rad")))]
    pub max_axis_sigma_rad: f32,
    /// Reject quads whose opposing edges differ in length by more than
    /// this factor (paper's parallelogram test). Default: `1.5`.
    #[cfg_attr(feature = "schemars", schemars(range(min = 1.0)))]
    pub opposing_edge_ratio_max: f32,
    /// Lower bound on a quad's perimeter edge length, expressed as a
    /// fraction of the per-component median quad edge length. Quads
    /// with any edge shorter than `edge_length_min_rel * component_median`
    /// are rejected as "below local cell scale". Default: `0.4`.
    /// Set to `0.0` to disable the lower bound entirely.
    #[cfg_attr(feature = "schemars", schemars(range(min = 0.0)))]
    pub edge_length_min_rel: f32,
    /// Upper bound on a quad's perimeter edge length, expressed as a
    /// fraction of the per-component median quad edge length. Quads
    /// with any edge longer than `edge_length_max_rel * component_median`
    /// are rejected as "above local cell scale" (typically a quad formed
    /// across a missing corner). Default: `2.5`. Set to `+inf` to
    /// disable the upper bound entirely (JSON has no infinity: pass a very
    /// large finite value such as `1e30` instead).
    #[cfg_attr(feature = "schemars", schemars(extend("exclusiveMinimum" = 0.0)))]
    pub edge_length_max_rel: f32,
    /// Discard labelled components with fewer than this many corners.
    /// Default: `4` (one quad of four corners).
    #[cfg_attr(feature = "schemars", schemars(range(min = 4)))]
    pub min_corners_for_component: usize,
    /// Discard connected quad-mesh components below this size. Default:
    /// `1` (keep all). Set higher to reject isolated noise quads.
    #[cfg_attr(feature = "schemars", schemars(range(min = 1)))]
    pub min_quads_per_component: usize,
    /// Optional global grid-direction centers, in radians, interpreted
    /// modulo π. When `Some([θ₀, θ₁])`, a feature is admitted to
    /// Delaunay only if at least one of its informative axes is within
    /// [`Self::cluster_axis_tol_rad`] of one of the centers. When
    /// `None`, the gate is skipped.
    #[cfg_attr(feature = "schemars", schemars(extend("x-unit" = "rad")))]
    pub axis_cluster_centers: Option<[f32; 2]>,
    /// Per-axis admission tolerance against
    /// [`Self::axis_cluster_centers`], in radians. Only consulted when
    /// `axis_cluster_centers.is_some()`. Default: `16° = 0.279`.
    #[cfg_attr(feature = "schemars", schemars(extend("exclusiveMinimum" = 0.0)))]
    #[cfg_attr(feature = "schemars", schemars(extend("x-unit" = "rad")))]
    pub cluster_axis_tol_rad: f32,
}

impl Default for TopologicalParams {
    fn default() -> Self {
        Self {
            axis_align_tol_rad: 15.0_f32.to_radians(),
            max_axis_sigma_rad: 0.6,
            opposing_edge_ratio_max: 1.5,
            edge_length_min_rel: 0.4,
            edge_length_max_rel: 2.5,
            min_corners_for_component: 4,
            min_quads_per_component: 1,
            axis_cluster_centers: None,
            cluster_axis_tol_rad: 16.0_f32.to_radians(),
        }
    }
}

impl TopologicalParams {
    /// Construct topological params from the two most commonly tuned
    /// knobs; the remaining fields take their defaults.
    pub fn new(axis_align_tol_rad: f32, max_axis_sigma_rad: f32) -> Self {
        Self {
            axis_align_tol_rad,
            max_axis_sigma_rad,
            ..Self::default()
        }
    }

    /// Builder-style override for [`Self::axis_align_tol_rad`].
    pub fn with_axis_align_tol_rad(mut self, value: f32) -> Self {
        self.axis_align_tol_rad = value;
        self
    }

    /// Builder-style override for [`Self::max_axis_sigma_rad`].
    pub fn with_max_axis_sigma_rad(mut self, value: f32) -> Self {
        self.max_axis_sigma_rad = value;
        self
    }

    /// Builder-style override for [`Self::opposing_edge_ratio_max`].
    pub fn with_opposing_edge_ratio_max(mut self, value: f32) -> Self {
        self.opposing_edge_ratio_max = value;
        self
    }

    /// Builder-style override for [`Self::edge_length_min_rel`].
    pub fn with_edge_length_min_rel(mut self, value: f32) -> Self {
        self.edge_length_min_rel = value;
        self
    }

    /// Builder-style override for [`Self::edge_length_max_rel`].
    pub fn with_edge_length_max_rel(mut self, value: f32) -> Self {
        self.edge_length_max_rel = value;
        self
    }

    /// Set both edge-length bounds in one call. Equivalent to
    /// `.with_edge_length_min_rel(min_rel).with_edge_length_max_rel(max_rel)`.
    ///
    /// Pass `min_rel = 0.0` to disable the lower bound; pass
    /// `max_rel = f32::INFINITY` to disable the upper bound.
    pub fn with_edge_length_band(mut self, min_rel: f32, max_rel: f32) -> Self {
        self.edge_length_min_rel = min_rel;
        self.edge_length_max_rel = max_rel;
        self
    }

    /// Builder-style override for [`Self::min_corners_for_component`].
    pub fn with_min_corners_for_component(mut self, value: usize) -> Self {
        self.min_corners_for_component = value;
        self
    }

    /// Builder-style override for [`Self::min_quads_per_component`].
    pub fn with_min_quads_per_component(mut self, value: usize) -> Self {
        self.min_quads_per_component = value;
        self
    }

    /// Builder-style override for [`Self::axis_cluster_centers`]. The
    /// two centers are stored as supplied; the caller is responsible
    /// for wrapping them into `[0, π)` if their source might emit
    /// signed angles. The internal alignment check works modulo π so
    /// either convention is accepted.
    pub fn with_axis_cluster_centers(mut self, centers: [f32; 2]) -> Self {
        self.axis_cluster_centers = Some(centers);
        self
    }

    /// Builder-style override for [`Self::cluster_axis_tol_rad`].
    pub fn with_cluster_axis_tol_rad(mut self, tol_rad: f32) -> Self {
        self.cluster_axis_tol_rad = tol_rad;
        self
    }
}

/// Multi-component axis-driven topological grid detector for
/// `(Square, Oriented2)`.
///
/// Returns one [`GridSolution`] per qualifying connected quad-mesh
/// component, ordered by component size descending. Features that no
/// component admitted (uninformative axes, gated by the cluster prior,
/// not picked up by Delaunay, etc.) appear in the **first** solution's
/// `rejected` vector tagged [`RejectionReason::Unlabelled`]; features
/// dropped by the per-component validation stage appear in that
/// component's own `rejected` vector tagged
/// [`RejectionReason::ValidationDropped`].
///
/// Too little usable evidence returns [`GridError::InsufficientEvidence`];
/// topology or fit degeneracy returns the underlying typed geometry error.
pub(crate) fn detect_square_oriented2_all(
    features: &[OrientedFeature<2>],
    dimensions: Option<GridDimensions>,
    params: &DetectionParams,
    axis_provenance: SquareAxisProvenance,
) -> Result<Vec<GridSolution>> {
    detect_square_oriented2_all_observed(features, dimensions, params, axis_provenance, None)
}

pub(super) fn detect_square_oriented2_all_observed(
    features: &[OrientedFeature<2>],
    dimensions: Option<GridDimensions>,
    params: &DetectionParams,
    axis_provenance: SquareAxisProvenance,
    trace: Option<&mut SquarePipelineTrace>,
) -> Result<Vec<GridSolution>> {
    let tuning = params.tuning();
    let SquareTopology {
        positions,
        components: merged,
    } = assemble_square_oriented2_components_observed(features, &tuning.topological, trace)?;

    // Geometry-only recovery schedule for facade-synthesized axes. The native
    // Oriented2 path stays off under `RecoverySchedule::Auto`; explicit `On`
    // still applies to either provenance. The chessboard adapter selects `Off`
    // because it owns its pattern-specific recovery downstream.
    let merged = if let Some(rec_params) = tuning.recovery.resolve(axis_provenance) {
        let ij_in: Vec<std::collections::HashMap<(i32, i32), usize>> = merged
            .iter()
            .map(|m| m.iter().map(|(c, &idx)| ((c.u, c.v), idx)).collect())
            .collect();
        let local_pitch = crate::shared::recovery_schedule::local_pitch_of(&positions);
        let recovered = crate::shared::recovery_schedule::recover_components(
            ij_in,
            crate::shared::recovery_schedule::RecoveryInputs {
                features,
                positions: &positions,
                local_pitch: &local_pitch,
                params: &rec_params,
                validate_params: &tuning.validation,
            },
        );
        recovered
            .into_iter()
            .map(|m| {
                m.into_iter()
                    .map(|((u, v), idx)| (Coord::new(u, v), idx))
                    .collect()
            })
            .collect()
    } else {
        merged
    };

    // Process each merged component independently; preserve the labelled
    // source-indices of every component that yielded a valid solution
    // so the orchestrator can build the global "unlabelled" set afterwards.
    let mut component_outputs: Vec<ComponentOutput> = Vec::new();
    for labelled in &merged {
        if labelled.len() < 4 {
            continue;
        }
        match build_component_solution(labelled, features, &positions, dimensions, params)? {
            Some(out) => component_outputs.push(out),
            None => continue,
        }
    }

    if component_outputs.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }

    // Sort components by labelled count descending; ties broken by the
    // smallest source_index seen so the order is deterministic.
    component_outputs.sort_by(|a, b| {
        b.kept_source_indices
            .len()
            .cmp(&a.kept_source_indices.len())
            .then_with(|| a.min_source_index.cmp(&b.min_source_index))
    });

    let solutions = assemble_solutions(component_outputs, features);
    Ok(solutions)
}

/// Assemble square topology through the component-merge checkpoint.
///
/// Coordinates intentionally remain in the walk's axis-slot frame. Public
/// detections canonicalize them later, after validation and before fitting;
/// pattern-specific detector builders may need the original slots because
/// their admission and recovery policies attach semantics to those axes.
pub(crate) fn assemble_square_oriented2_components(
    features: &[OrientedFeature<2>],
    topo: &TopologicalParams,
) -> Result<Vec<std::collections::HashMap<Coord, usize>>> {
    assemble_square_oriented2_components_observed(features, topo, None)
        .map(|topology| topology.components)
}

fn assemble_square_oriented2_components_observed(
    features: &[OrientedFeature<2>],
    topo: &TopologicalParams,
    mut trace: Option<&mut SquarePipelineTrace>,
) -> Result<SquareTopology> {
    if features.len() < MIN_USABLE_FOR_DELAUNAY {
        return Err(GridError::InsufficientEvidence);
    }

    let axes = build_axis_caches(features, topo.max_axis_sigma_rad);
    // Apply the optional axis-cluster gate. When no centers are supplied
    // the predicate is the identity and `usable` matches the ungated
    // behaviour exactly.
    #[cfg(feature = "tracing")]
    let usable: Vec<bool> = {
        let _span = tracing::debug_span!("usable_mask", num_features = features.len()).entered();
        build_usable_mask(features, &axes, topo)
    };
    #[cfg(not(feature = "tracing"))]
    let usable: Vec<bool> = build_usable_mask(features, &axes, topo);
    let n_usable = usable.iter().filter(|&&b| b).count();
    if n_usable < MIN_USABLE_FOR_DELAUNAY {
        return Err(GridError::InsufficientEvidence);
    }

    let positions: Vec<Point2<f32>> = features.iter().map(|f| f.point.position).collect();
    let (usable, triangulation, edge_kinds) =
        triangulate_lattice_candidates(&positions, &axes, usable, topo.axis_align_tol_rad)?;
    if let Some(trace) = trace.as_mut() {
        trace.usable.clone_from(&usable);
        trace.triangles = triangulation.triangles.as_chunks::<3>().0.to_vec();
    }
    if let Some(trace) = trace.as_mut() {
        trace.edges = edge_kinds
            .iter()
            .enumerate()
            .map(|(edge, &kind)| {
                (
                    triangulation.triangles[edge],
                    triangulation.triangles[delaunay::Triangulation::next_edge(edge)],
                    kind,
                )
            })
            .collect();
    }
    let raw_quads = quads::merge_triangle_pairs(&triangulation, &edge_kinds, &positions);
    if let Some(trace) = trace.as_mut() {
        trace.raw_quads = raw_quads.iter().map(|quad| quad.vertices).collect();
    }
    let kept_quads = if let Some(trace) = trace.as_mut() {
        let mut observer = |stage: filter::FilterStage, quads: &[quads::Quad]| {
            let snapshot = quads.iter().map(|quad| quad.vertices).collect();
            match stage {
                filter::FilterStage::Topology => trace.topology_quads = snapshot,
                filter::FilterStage::Geometry => trace.geometry_quads = snapshot,
                filter::FilterStage::CellScale => trace.scale_quads = snapshot,
            }
        };
        filter::filter_quads_observed(
            raw_quads,
            &positions,
            topo.opposing_edge_ratio_max,
            topo.edge_length_min_rel,
            topo.edge_length_max_rel,
            Some(&mut observer),
        )
    } else {
        filter::filter_quads(
            raw_quads,
            &positions,
            topo.opposing_edge_ratio_max,
            topo.edge_length_min_rel,
            topo.edge_length_max_rel,
        )
    };
    let components = walk::label_components(
        &kept_quads,
        &positions,
        topo.min_quads_per_component,
        topo.min_corners_for_component,
    );
    if let Some(trace) = trace.as_mut() {
        trace.walk_components = components
            .iter()
            .map(|component| sorted_labels(&component.labelled))
            .collect();
    }

    if components.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }

    // Reunite the labelled components in label space with the shared
    // `merge_components_local` step. The topological walk leaves one quad-mesh
    // component per disconnected patch; hosting the merge here lets the
    // chessboard adapter consume a single already-merged output (see
    // `calib-targets-chessboard::topological`).
    let merged = merge_walk_components(&components, &positions);
    if let Some(trace) = trace.as_mut() {
        trace.merged_components = merged.iter().map(sorted_labels).collect();
    }
    if merged.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }
    Ok(SquareTopology {
        positions,
        components: merged,
    })
}

fn sorted_labels(labelled: &std::collections::HashMap<Coord, usize>) -> Vec<(Coord, usize)> {
    let mut labels: Vec<(Coord, usize)> = labelled
        .iter()
        .map(|(&coord, &feature_index)| (coord, feature_index))
        .collect();
    labels.sort_by_key(|&(coord, feature_index)| (coord.v, coord.u, feature_index));
    labels
}

/// Reunite the walk's labelled components in label space via the shared
/// local-geometry merge, then return one `Coord`-keyed map per surviving
/// merged component.
///
/// The merge input is ordered exactly as the per-component solutions were
/// historically presented to consumers: by labelled count descending, ties
/// broken by the smallest feature index. The previous architecture ran the
/// per-component validate + fit first and sorted the resulting solutions by
/// `(kept_source_indices.len() desc, min_source_index asc)`; with this
/// facade-hosted merge the validate/fit run *after* the merge, so the
/// pre-merge ordering is reconstructed directly from the walk components
/// (validate is membership-preserving for the merge-input ordering keys —
/// labelled count and minimum feature index — so the two orderings agree).
///
/// `merge_components_local` re-sorts its working set by size on every
/// iteration and rebases its output, so this ordering only fixes the
/// tie-break among equal-size components; pinning it keeps the merge
/// deterministic and byte-compatible with the prior chessboard-side merge.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        name = "topological_component_merge",
        level = "debug",
        skip_all,
        fields(num_components = components.len()),
    )
)]
fn merge_walk_components(
    components: &[walk::TopologicalComponent],
    positions: &[Point2<f32>],
) -> Vec<std::collections::HashMap<Coord, usize>> {
    // Order the walk components by the historical solution-presentation key.
    let mut ordered: Vec<&walk::TopologicalComponent> = components.iter().collect();
    ordered.sort_by(|a, b| {
        b.labelled
            .len()
            .cmp(&a.labelled.len())
            .then_with(|| min_feature_index(a).cmp(&min_feature_index(b)))
    });

    // Convert each `Coord`-keyed walk map into the `(i32, i32)`-keyed shape
    // the shared merge consumes. Hold the owned maps alive so the
    let owned: Vec<std::collections::HashMap<(i32, i32), usize>> = ordered
        .iter()
        .map(|c| {
            c.labelled
                .iter()
                .map(|(coord, &idx)| ((coord.u, coord.v), idx))
                .collect()
        })
        .collect();
    let merged = merge_components_local(&owned, positions, &LocalMergeParams::default()).components;

    merged
        .into_iter()
        .map(|m| {
            m.into_iter()
                .map(|((u, v), idx)| (Coord::new(u, v), idx))
                .collect()
        })
        .collect()
}

/// Smallest feature index referenced by a walk component, used as the
/// tie-break in [`merge_walk_components`]'s ordering.
fn min_feature_index(component: &walk::TopologicalComponent) -> usize {
    component
        .labelled
        .values()
        .copied()
        .min()
        .unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
