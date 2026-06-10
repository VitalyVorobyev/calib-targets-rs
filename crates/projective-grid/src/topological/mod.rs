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
//!    pass (local geometry only, radial-distortion safe), mirroring the
//!    seed-and-grow facade. This makes the two algorithm facades expose
//!    identical multi-component semantics: the topological path no longer
//!    leaves an un-merged quad-mesh component per disconnected patch.
//! 8. Reuse the shared advanced [`validate`](crate::shared::validate)
//!    post-stage to drop labelled corners flagged by line-collinearity and
//!    local-H checks.
//! 9. Fit a projective transform on the surviving labels and report
//!    per-corner residuals.
//!
//! Multi-component output is represented directly: the orchestrator returns
//! one [`GridSolution`] per qualifying component, ordered by labelled count
//! descending.

mod axis;
mod classify;
mod delaunay;
mod filter;
mod quads;
mod walk;

use std::collections::HashSet;

use nalgebra::Point2;

use crate::detect::DetectionParams;
use crate::error::{GridError, Result};
use crate::feature::OrientedFeature;
use crate::lattice::{Coord, GridDimensions, LatticeKind};
use crate::result::{
    GridEntry, GridSolution, LabelledGrid, LatticeFit, RejectedFeature, RejectionReason,
};
use crate::shared::merge::{merge_components_local, ComponentInput, LocalMergeParams};
use crate::shared::validate as pg_validate;

use self::axis::{build_axis_caches, AxisCache};
use super::shared::{fit_component, FitComponentResult};

/// Minimum number of usable features for Delaunay triangulation.
const MIN_USABLE_FOR_DELAUNAY: usize = 3;

/// Tuning knobs for the axis-driven topological pipeline.
///
/// Defaults are conservative values pinned by the crate's regression tests.
/// Adding new fields is non-breaking via `#[non_exhaustive]`;
/// literal-construction from outside the crate goes through [`Self::default`]
/// + struct-update syntax or [`Self::new`].
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub struct TopologicalParams {
    /// Maximum angular distance, in radians, between an edge's
    /// direction and a corner's axis for the edge to classify as a
    /// grid edge at that corner. Default: 15° = 0.262 rad.
    pub axis_align_tol_rad: f32,
    /// Maximum 1σ axis uncertainty (radians) for a feature axis to be
    /// considered informative. Features whose both axes have
    /// `sigma_rad ≥ max_axis_sigma_rad` are excluded from Delaunay;
    /// classification skips individual axes above the threshold.
    /// Default: `0.6 ≈ 34°`. `sigma_rad = None` is treated as informative.
    pub max_axis_sigma_rad: f32,
    /// Reject quads whose opposing edges differ in length by more than
    /// this factor (paper's parallelogram test). Default: `1.5`.
    pub opposing_edge_ratio_max: f32,
    /// Lower bound on a quad's perimeter edge length, expressed as a
    /// fraction of the per-component median quad edge length. Quads
    /// with any edge shorter than `edge_length_min_rel * component_median`
    /// are rejected as "below local cell scale". Default: `0.4`.
    /// Set to `0.0` to disable the lower bound entirely.
    pub edge_length_min_rel: f32,
    /// Upper bound on a quad's perimeter edge length, expressed as a
    /// fraction of the per-component median quad edge length. Quads
    /// with any edge longer than `edge_length_max_rel * component_median`
    /// are rejected as "above local cell scale" (typically a quad formed
    /// across a missing corner). Default: `2.5`. Set to `+inf` to
    /// disable the upper bound entirely.
    pub edge_length_max_rel: f32,
    /// Discard labelled components with fewer than this many corners.
    /// Default: `4` (one quad of four corners).
    pub min_corners_for_component: usize,
    /// Discard connected quad-mesh components below this size. Default:
    /// `1` (keep all). Set higher to reject isolated noise quads.
    pub min_quads_per_component: usize,
    /// Optional global grid-direction centers, in radians, interpreted
    /// modulo π. When `Some([θ₀, θ₁])`, a feature is admitted to
    /// Delaunay only if at least one of its informative axes is within
    /// [`Self::cluster_axis_tol_rad`] of one of the centers. When
    /// `None`, the gate is skipped.
    pub axis_cluster_centers: Option<[f32; 2]>,
    /// Per-axis admission tolerance against
    /// [`Self::axis_cluster_centers`], in radians. Only consulted when
    /// `axis_cluster_centers.is_some()`. Default: `16° = 0.279`.
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
/// The empty-result case maps to `Vec::new()`, *not* an error; the
/// caller-facing `detect_grid_all` wrapper turns an empty solutions
/// vector into [`GridError::InsufficientEvidence`] when the request
/// reached the orchestrator with enough features.
pub(crate) fn detect_square_oriented2_topological_all(
    features: &[OrientedFeature<2>],
    dimensions: Option<GridDimensions>,
    params: &DetectionParams,
    synthesized_axes: bool,
) -> Result<Vec<GridSolution>> {
    if features.len() < MIN_USABLE_FOR_DELAUNAY {
        return Err(GridError::InsufficientEvidence);
    }

    let topo = &params.topological;
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

    // Triangulate over the packed usable set; remap triangles back into
    // the global feature index space so the downstream stages share
    // indices with `features` / `axes`.
    let positions: Vec<Point2<f32>> = features.iter().map(|f| f.point.position).collect();
    let triangulation = triangulate_usable(&positions, &usable);
    if triangulation.num_tri() == 0 {
        return Err(GridError::DegenerateGeometry);
    }

    let edge_kinds =
        classify::classify_all_edges(&positions, &axes, &triangulation, topo.axis_align_tol_rad);
    let raw_quads = quads::merge_triangle_pairs(&triangulation, &edge_kinds, &positions);
    let kept_quads = filter::filter_quads(
        raw_quads,
        &positions,
        topo.opposing_edge_ratio_max,
        topo.edge_length_min_rel,
        topo.edge_length_max_rel,
    );
    let components = walk::label_components(
        &kept_quads,
        topo.min_quads_per_component,
        topo.min_corners_for_component,
    );

    if components.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }

    // Reunite the labelled components in label space, mirroring the
    // seed-and-grow facade's `merge_components_local` step. Until now the
    // topological facade left one quad-mesh component per disconnected
    // patch; the chessboard adapter compensated by running this same merge
    // itself. Hosting it here unifies the two facades' multi-component
    // semantics and lets the chessboard adapter consume a single
    // already-merged output (see `calib-targets-chessboard::topological`).
    let merged = merge_walk_components(&components, &positions);
    if merged.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }

    // Geometry-only recovery schedule for the synthesized-axis path (enabled
    // under `RecoverySchedule::Auto` when `synthesized_axes`). Disabled for the
    // chessboard topological adapter, which sets `RecoverySchedule::Off` and
    // runs its own `CornerStage`-coupled recovery — so its production output
    // stays byte-identical. The recovery operates on the `(i32, i32)`-keyed
    // shape, so convert to/from `Coord`.
    let merged = if let Some(rec_params) = params.recovery.resolve(synthesized_axes) {
        let ij_in: Vec<std::collections::HashMap<(i32, i32), usize>> = merged
            .iter()
            .map(|m| m.iter().map(|(c, &idx)| ((c.u, c.v), idx)).collect())
            .collect();
        let local_pitch = crate::seed_and_grow::recovery::local_pitch_of(&positions);
        let recovered = crate::seed_and_grow::recovery::recover_components(
            ij_in,
            crate::seed_and_grow::recovery::RecoveryInputs {
                features,
                positions: &positions,
                local_pitch: &local_pitch,
                params: &rec_params,
                validate_params: &params.validate,
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
    // so the orchestrator can build the global "unlabelled" set
    // afterwards.
    let mut component_outputs: Vec<ComponentOutput> = Vec::new();
    for labelled in &merged {
        if labelled.len() < 4 {
            continue;
        }
        match build_component_solution(labelled, features, &positions, params) {
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

    let solutions = assemble_solutions(component_outputs, features, dimensions);
    Ok(solutions)
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
    // `ComponentInput` borrows stay valid for the merge call.
    let owned: Vec<std::collections::HashMap<(i32, i32), usize>> = ordered
        .iter()
        .map(|c| {
            c.labelled
                .iter()
                .map(|(coord, &idx)| ((coord.u, coord.v), idx))
                .collect()
        })
        .collect();
    let views: Vec<ComponentInput<'_>> = owned
        .iter()
        .map(|labelled| ComponentInput {
            labelled,
            positions,
        })
        .collect();

    let merged = merge_components_local(&views, &LocalMergeParams::default());
    let merged = if merged.components.is_empty() {
        // Defensive: an empty merge result means no component qualified.
        // Fall back to the (rebased) input maps so a degenerate merge can't
        // silently drop everything.
        owned
    } else {
        merged.components
    };

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

/// Build the global "unlabelled" set and assemble the per-component
/// solutions, attributing every globally-unseen feature to the largest
/// component so callers that read solely `solutions[0].rejected` see the
/// same shape as the single-solution path.
fn assemble_solutions(
    component_outputs: Vec<ComponentOutput>,
    features: &[OrientedFeature<2>],
    dimensions: Option<GridDimensions>,
) -> Vec<GridSolution> {
    let mut globally_kept: HashSet<usize> = HashSet::new();
    let mut globally_validation_dropped: HashSet<usize> = HashSet::new();
    for out in &component_outputs {
        for &src in &out.kept_source_indices {
            globally_kept.insert(src);
        }
        for &src in &out.validation_drop_source_indices {
            globally_validation_dropped.insert(src);
        }
    }
    let mut global_unlabelled: Vec<RejectedFeature> = Vec::new();
    for feature in features {
        let src = feature.point.source_index;
        if globally_kept.contains(&src) {
            continue;
        }
        if globally_validation_dropped.contains(&src) {
            global_unlabelled.push(RejectedFeature::new(
                src,
                None,
                None,
                RejectionReason::ValidationDropped,
            ));
            continue;
        }
        global_unlabelled.push(RejectedFeature::new(
            src,
            None,
            None,
            RejectionReason::Unlabelled,
        ));
    }

    let mut solutions: Vec<GridSolution> = Vec::with_capacity(component_outputs.len());
    for (idx, out) in component_outputs.into_iter().enumerate() {
        let ComponentOutput {
            entries,
            fit,
            mut rejected,
            ..
        } = out;
        if idx == 0 {
            rejected.extend(global_unlabelled.iter().copied());
        }
        let grid = LabelledGrid::new(LatticeKind::Square, entries, dimensions);
        solutions.push(GridSolution::new(grid, Some(fit), rejected));
    }
    solutions
}

struct ComponentOutput {
    entries: Vec<GridEntry>,
    fit: LatticeFit,
    rejected: Vec<RejectedFeature>,
    kept_source_indices: HashSet<usize>,
    validation_drop_source_indices: HashSet<usize>,
    min_source_index: usize,
}

fn build_usable_mask(
    features: &[OrientedFeature<2>],
    axes: &[AxisCache],
    topo: &TopologicalParams,
) -> Vec<bool> {
    features
        .iter()
        .zip(axes.iter())
        .map(|(f, cache)| cache.any_informative() && axes_pass_cluster_gate(&f.axes, cache, topo))
        .collect()
}

fn build_component_solution(
    labelled: &std::collections::HashMap<Coord, usize>,
    features: &[OrientedFeature<2>],
    positions: &[Point2<f32>],
    params: &DetectionParams,
) -> Option<ComponentOutput> {
    // Reuse the advanced post-stage. The advanced validate is the same
    // module that backs the chessboard seed-and-grow path.
    let validate_entries: Vec<pg_validate::LabelledEntry> = labelled
        .iter()
        .map(|(coord, &idx)| pg_validate::LabelledEntry {
            idx,
            pixel: features[idx].point.position,
            grid: (coord.u, coord.v),
        })
        .collect();
    let cell_size = estimate_cell_size(labelled, positions);
    let validation = pg_validate::validate(&validate_entries, cell_size, &params.validate);

    // Working label set after the validation drop, keyed by Coord.
    let mut kept: Vec<(Coord, usize)> = labelled
        .iter()
        .filter(|(_, &idx)| !validation.blacklist.contains(&idx))
        .map(|(&coord, &idx)| (coord, idx))
        .collect();
    if kept.len() < 4 {
        return None;
    }

    let lattice = LatticeKind::Square;
    let fit_result = run_fit_with_residual_drop(&mut kept, features, positions, lattice, params)?;
    let FitComponentResult {
        entries: entries_out,
        fit,
        over_threshold,
    } = fit_result;

    let kept_source_indices: HashSet<usize> = kept
        .iter()
        .map(|&(_, idx)| features[idx].point.source_index)
        .collect();
    let validation_drop_source_indices: HashSet<usize> = validation
        .blacklist
        .iter()
        .map(|&idx| features[idx].point.source_index)
        .collect();

    let mut rejected: Vec<RejectedFeature> = Vec::new();
    for &src in &validation_drop_source_indices {
        rejected.push(RejectedFeature::new(
            src,
            None,
            None,
            RejectionReason::ValidationDropped,
        ));
    }
    for r in over_threshold {
        rejected.push(r);
    }

    let min_source_index = kept_source_indices
        .iter()
        .copied()
        .min()
        .unwrap_or(usize::MAX);

    Some(ComponentOutput {
        entries: entries_out,
        fit,
        rejected,
        kept_source_indices,
        validation_drop_source_indices,
        min_source_index,
    })
}

/// Run the shared `fit_component` helper, drop over-threshold entries
/// once, and refit on the remaining set. Mutates `kept` to the surviving
/// label set. Returns `None` when fewer than four entries survive.
fn run_fit_with_residual_drop(
    kept: &mut Vec<(Coord, usize)>,
    features: &[OrientedFeature<2>],
    positions: &[Point2<f32>],
    lattice: LatticeKind,
    params: &DetectionParams,
) -> Option<FitComponentResult> {
    let first = fit_component(kept, features, positions, lattice, params).ok()?;
    if first.over_threshold.is_empty() {
        return Some(first);
    }
    let drop: HashSet<usize> = first
        .over_threshold
        .iter()
        .map(|r| r.source_index)
        .collect();
    kept.retain(|&(_, idx)| !drop.contains(&features[idx].point.source_index));
    if kept.len() < 4 {
        return None;
    }
    let refit = fit_component(kept, features, positions, lattice, params).ok()?;
    // Preserve the first pass's over-threshold attribution.
    Some(FitComponentResult {
        entries: refit.entries,
        fit: refit.fit,
        over_threshold: first.over_threshold,
    })
}

/// Per-feature alignment check against the optional axis-cluster
/// centers in [`TopologicalParams`]. Returns `true` when the gate is
/// disabled (`axis_cluster_centers.is_none()`) or when at least one
/// informative axis is within `cluster_axis_tol_rad` of one of the
/// centers under undirected (mod π) distance.
fn axes_pass_cluster_gate(
    axes: &[crate::feature::LocalAxis; 2],
    cache: &AxisCache,
    params: &TopologicalParams,
) -> bool {
    let Some(centers) = params.axis_cluster_centers else {
        return true;
    };
    let tol = params.cluster_axis_tol_rad;
    for (axis, &informative) in axes.iter().zip(cache.informative.iter()) {
        if !informative {
            continue;
        }
        let angle = axis.angle_rad;
        let d0 = angular_dist_pi(angle, centers[0]);
        let d1 = angular_dist_pi(angle, centers[1]);
        if d0 < tol || d1 < tol {
            return true;
        }
    }
    false
}

/// Smallest angular distance on the circle with period π. Result in
/// `[0, π/2]`. The topological cluster gate is the only consumer in this
/// crate, so the helper is local.
#[inline]
fn angular_dist_pi(a: f32, b: f32) -> f32 {
    let pi = std::f32::consts::PI;
    // `(diff % π + π) % π` keeps the result in `[0, π)` for any sign
    // of `diff`. Bare `%` in Rust is the truncated remainder, so the
    // double `+ π) % π` is necessary.
    let diff_raw = (a - b) % pi;
    let positive = (diff_raw + pi) % pi;
    let complement = pi - positive;
    if positive < complement {
        positive
    } else {
        complement
    }
}

/// Triangulate only the usable features and remap triangle vertex
/// indices back into the global feature index space.
fn triangulate_usable(positions: &[Point2<f32>], usable: &[bool]) -> delaunay::Triangulation {
    let mut packed_to_global: Vec<usize> = Vec::with_capacity(positions.len());
    let mut packed_positions: Vec<Point2<f32>> = Vec::with_capacity(positions.len());
    for (i, (&u, &p)) in usable.iter().zip(positions.iter()).enumerate() {
        if u {
            packed_to_global.push(i);
            packed_positions.push(p);
        }
    }
    let mut triangulation = delaunay::triangulate(&packed_positions);
    // After triangulation, indices reference the packed slice. We remap
    // `triangles` to global indices; `halfedges` stay valid because
    // half-edges are offsets into `triangles`, not vertex indices.
    for v in triangulation.triangles.iter_mut() {
        *v = packed_to_global[*v];
    }
    triangulation
}

/// Mean labelled-pair edge length over cardinal lattice neighbours.
/// Used as the `cell_size` input to the shared validate post-stage.
///
/// Falls back to `1.0` when no cardinal pair exists, in which case the
/// validate caller's relative tolerances reduce to absolute thresholds.
/// In practice the topological pipeline only reaches this helper with
/// at least one labelled quad, so the fallback is defensive.
fn estimate_cell_size(
    labelled: &std::collections::HashMap<Coord, usize>,
    positions: &[Point2<f32>],
) -> f32 {
    use crate::lattice::SQUARE_CARDINAL_OFFSETS;

    let mut sum = 0.0_f32;
    let mut count: usize = 0;
    for (&coord, &idx) in labelled {
        let here = positions[idx];
        for offset in &SQUARE_CARDINAL_OFFSETS {
            let neigh = Coord::new(coord.u + offset.u, coord.v + offset.v);
            if let Some(&n_idx) = labelled.get(&neigh) {
                let nb = positions[n_idx];
                let dx = nb.x - here.x;
                let dy = nb.y - here.y;
                sum += (dx * dx + dy * dy).sqrt();
                count += 1;
            }
        }
    }
    if count == 0 {
        return 1.0;
    }
    sum / count as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::{LocalAxis, PointFeature};

    fn axis_aligned_features(rows: i32, cols: i32, s: f32) -> Vec<OrientedFeature<2>> {
        let origin = 50.0_f32;
        let mut out = Vec::with_capacity((rows * cols) as usize);
        let mut idx = 0_usize;
        for j in 0..rows {
            for i in 0..cols {
                let x = (i as f32) * s + origin;
                let y = (j as f32) * s + origin;
                let point = PointFeature::new(idx, Point2::new(x, y));
                let axes = [
                    LocalAxis::new(0.0_f32, Some(0.05)),
                    LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(0.05)),
                ];
                out.push(OrientedFeature::new(point, axes));
                idx += 1;
            }
        }
        out
    }

    #[test]
    fn default_params_match_regression_values() {
        let p = TopologicalParams::default();
        assert!((p.axis_align_tol_rad - 15.0_f32.to_radians()).abs() < 1e-5);
        assert!((p.max_axis_sigma_rad - 0.6).abs() < 1e-5);
        assert!((p.opposing_edge_ratio_max - 1.5).abs() < 1e-5);
        assert!((p.edge_length_min_rel - 0.4).abs() < 1e-5);
        assert!((p.edge_length_max_rel - 2.5).abs() < 1e-5);
        assert_eq!(p.min_corners_for_component, 4);
        assert_eq!(p.min_quads_per_component, 1);
        assert!(p.axis_cluster_centers.is_none());
        assert!((p.cluster_axis_tol_rad - 16.0_f32.to_radians()).abs() < 1e-5);
    }

    #[test]
    fn clean_5x5_grid_is_fully_labelled() {
        let features = axis_aligned_features(5, 5, 20.0);
        let params = DetectionParams::default();
        let mut solutions =
            detect_square_oriented2_topological_all(&features, None, &params, false).unwrap();
        assert_eq!(solutions.len(), 1);
        let solution = solutions.remove(0);
        assert_eq!(solution.grid.entries.len(), 25);
        let fit = solution.fit.unwrap();
        assert!(fit.residuals.max_px < 0.01, "{}", fit.residuals.max_px);
    }

    #[test]
    fn fewer_than_three_features_errors() {
        let features = axis_aligned_features(1, 2, 20.0);
        let params = DetectionParams::default();
        let err =
            detect_square_oriented2_topological_all(&features, None, &params, false).unwrap_err();
        assert_eq!(err, GridError::InsufficientEvidence);
    }

    #[test]
    fn cluster_gate_drops_off_axis_features() {
        // 5×5 axis-aligned grid (axes at 0°, 90°) + 4 noise features
        // whose axes both sit near 45°. With the cluster gate centered
        // at [0, π/2] and a 16° tolerance, the noise features must be
        // dropped pre-Delaunay; with the gate disabled they survive.
        let mut features = axis_aligned_features(5, 5, 20.0);
        let extra: [(f32, f32); 4] = [(40.0, 40.0), (180.0, 40.0), (40.0, 180.0), (180.0, 180.0)];
        let next = features.len();
        for (i, &(x, y)) in extra.iter().enumerate() {
            let point = PointFeature::new(next + i, Point2::new(x, y));
            let off_axis = std::f32::consts::FRAC_PI_4;
            let axes = [
                LocalAxis::new(off_axis, Some(0.05)),
                LocalAxis::new(off_axis + std::f32::consts::FRAC_PI_2, Some(0.05)),
            ];
            features.push(OrientedFeature::new(point, axes));
        }

        let params_on = DetectionParams::default().with_topological(
            TopologicalParams::default()
                .with_axis_cluster_centers([0.0, std::f32::consts::FRAC_PI_2]),
        );
        let mut sol_on =
            detect_square_oriented2_topological_all(&features, None, &params_on, false).unwrap();
        assert_eq!(sol_on.len(), 1);
        let primary = sol_on.remove(0);
        assert_eq!(primary.grid.entries.len(), 25, "gate must keep the 5×5");

        let params_off = DetectionParams::default();
        let mut sol_off =
            detect_square_oriented2_topological_all(&features, None, &params_off, false).unwrap();
        assert_eq!(sol_off.len(), 1);
        let primary_off = sol_off.remove(0);
        assert_eq!(primary_off.grid.entries.len(), 25);
        let noise_ids: std::collections::HashSet<usize> = (next..next + 4).collect();
        for r in &primary.rejected {
            if noise_ids.contains(&r.source_index) {
                assert_eq!(r.reason, RejectionReason::Unlabelled);
            }
        }
    }

    #[test]
    fn axes_pass_cluster_gate_with_no_centers_is_identity() {
        let cache = AxisCache {
            angle_rad: [std::f32::consts::FRAC_PI_4, std::f32::consts::FRAC_PI_4],
            informative: [true, true],
        };
        let axes = [
            LocalAxis::new(std::f32::consts::FRAC_PI_4, Some(0.05_f32)),
            LocalAxis::new(std::f32::consts::FRAC_PI_4, Some(0.05_f32)),
        ];
        let params_off = TopologicalParams::default();
        assert!(axes_pass_cluster_gate(&axes, &cache, &params_off));
        let params_on = TopologicalParams::default()
            .with_axis_cluster_centers([0.0_f32, std::f32::consts::FRAC_PI_2]);
        assert!(!axes_pass_cluster_gate(&axes, &cache, &params_on));
    }

    #[test]
    fn angular_dist_pi_is_undirected() {
        let pi = std::f32::consts::PI;
        let d_zero = angular_dist_pi(0.0, pi);
        assert!(d_zero < 1e-5, "{d_zero}");
        let d_perp = angular_dist_pi(0.0, std::f32::consts::FRAC_PI_2);
        assert!((d_perp - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        let d_signed = angular_dist_pi(-0.1, std::f32::consts::PI + 0.1);
        assert!((d_signed - 0.2).abs() < 1e-4, "{d_signed}");
        let d_seam = angular_dist_pi(std::f32::consts::PI - 0.05, 0.05);
        assert!((d_seam - 0.1).abs() < 1e-5, "{d_seam}");
    }
}

// Relocated from detect/advanced/square/topological_trace.rs.
pub mod trace;
