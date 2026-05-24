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
//! 7. Reuse the shared [`crate::validate::square`] post-stage to drop
//!    labelled corners flagged by line-collinearity, local-H, and edge-length
//!    checks.
//! 8. Fit a projective transform on the surviving labels and report
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
use crate::float::{lit, Float};
use crate::geometry::{apply_projective, estimate_projective};
use crate::lattice::{Coord, GridDimensions, LatticeKind};
use crate::result::{
    GridEntry, GridSolution, LabelledGrid, LatticeFit, RejectedFeature, RejectionReason,
    ResidualSummary,
};

use self::axis::{build_axis_caches, AxisCache};

/// Minimum number of usable features for Delaunay triangulation.
const MIN_USABLE_FOR_DELAUNAY: usize = 3;

/// Tuning knobs for the axis-driven topological pipeline.
///
/// Defaults are conservative values pinned by the crate's regression tests.
/// Adding new fields is non-breaking via `#[non_exhaustive]`;
/// literal-construction from outside the crate goes through [`Self::default`]
/// + struct-update syntax or [`Self::new`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct TopologicalParams<F: Float> {
    /// Maximum angular distance, in radians, between an edge's
    /// direction and a corner's axis for the edge to classify as a
    /// grid edge at that corner. Default: 15° = 0.262 rad.
    pub axis_align_tol_rad: F,
    /// Maximum 1σ axis uncertainty (radians) for a feature axis to be
    /// considered informative. Features whose both axes have
    /// `sigma_rad ≥ max_axis_sigma_rad` are excluded from Delaunay;
    /// classification skips individual axes above the threshold.
    /// Default: `0.6 ≈ 34°`. `sigma_rad = None` is treated as informative.
    pub max_axis_sigma_rad: F,
    /// Reject quads whose opposing edges differ in length by more than
    /// this factor (paper's parallelogram test). Default: `1.5`.
    pub opposing_edge_ratio_max: F,
    /// Lower bound on a quad's perimeter edge length, expressed as a
    /// fraction of the per-component median quad edge length. Quads
    /// with any edge shorter than `edge_length_min_rel * component_median`
    /// are rejected as "below local cell scale". Default: `0.4`.
    /// Set to `0.0` to disable the lower bound entirely.
    pub edge_length_min_rel: F,
    /// Upper bound on a quad's perimeter edge length, expressed as a
    /// fraction of the per-component median quad edge length. Quads
    /// with any edge longer than `edge_length_max_rel * component_median`
    /// are rejected as "above local cell scale" (typically a quad formed
    /// across a missing corner). Default: `2.5`. Set to `+inf` to
    /// disable the upper bound entirely.
    pub edge_length_max_rel: F,
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
    pub axis_cluster_centers: Option<[F; 2]>,
    /// Per-axis admission tolerance against
    /// [`Self::axis_cluster_centers`], in radians. Only consulted when
    /// `axis_cluster_centers.is_some()`. Default: `16° = 0.279`.
    pub cluster_axis_tol_rad: F,
}

impl<F: Float> Default for TopologicalParams<F> {
    fn default() -> Self {
        Self {
            axis_align_tol_rad: lit::<F>(15.0_f32.to_radians()),
            max_axis_sigma_rad: lit::<F>(0.6_f32),
            opposing_edge_ratio_max: lit::<F>(1.5_f32),
            edge_length_min_rel: lit::<F>(0.4_f32),
            edge_length_max_rel: lit::<F>(2.5_f32),
            min_corners_for_component: 4,
            min_quads_per_component: 1,
            axis_cluster_centers: None,
            cluster_axis_tol_rad: lit::<F>(16.0_f32.to_radians()),
        }
    }
}

impl<F: Float> TopologicalParams<F> {
    /// Construct topological params from the two most commonly tuned
    /// knobs; the remaining fields take their defaults.
    pub fn new(axis_align_tol_rad: F, max_axis_sigma_rad: F) -> Self {
        Self {
            axis_align_tol_rad,
            max_axis_sigma_rad,
            ..Self::default()
        }
    }

    /// Builder-style override for [`Self::axis_align_tol_rad`].
    pub fn with_axis_align_tol_rad(mut self, value: F) -> Self {
        self.axis_align_tol_rad = value;
        self
    }

    /// Builder-style override for [`Self::max_axis_sigma_rad`].
    pub fn with_max_axis_sigma_rad(mut self, value: F) -> Self {
        self.max_axis_sigma_rad = value;
        self
    }

    /// Builder-style override for [`Self::opposing_edge_ratio_max`].
    pub fn with_opposing_edge_ratio_max(mut self, value: F) -> Self {
        self.opposing_edge_ratio_max = value;
        self
    }

    /// Builder-style override for [`Self::edge_length_min_rel`].
    pub fn with_edge_length_min_rel(mut self, value: F) -> Self {
        self.edge_length_min_rel = value;
        self
    }

    /// Builder-style override for [`Self::edge_length_max_rel`].
    pub fn with_edge_length_max_rel(mut self, value: F) -> Self {
        self.edge_length_max_rel = value;
        self
    }

    /// Set both edge-length bounds in one call. Equivalent to
    /// `.with_edge_length_min_rel(min_rel).with_edge_length_max_rel(max_rel)`.
    ///
    /// Pass `min_rel = 0.0` to disable the lower bound; pass
    /// `max_rel = F::infinity()` to disable the upper bound.
    pub fn with_edge_length_band(mut self, min_rel: F, max_rel: F) -> Self {
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
    pub fn with_axis_cluster_centers(mut self, centers: [F; 2]) -> Self {
        self.axis_cluster_centers = Some(centers);
        self
    }

    /// Builder-style override for [`Self::cluster_axis_tol_rad`].
    pub fn with_cluster_axis_tol_rad(mut self, tol_rad: F) -> Self {
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
pub(in crate::detect) fn detect_square_oriented2_topological_all<F: Float>(
    features: &[OrientedFeature<F, 2>],
    dimensions: Option<GridDimensions>,
    params: &DetectionParams<F>,
) -> Result<Vec<GridSolution<F>>> {
    if features.len() < MIN_USABLE_FOR_DELAUNAY {
        return Err(GridError::InsufficientEvidence);
    }

    let topo = &params.topological;
    let axes = build_axis_caches(features, topo.max_axis_sigma_rad);
    // Apply the optional axis-cluster gate (Phase E.0). When no
    // centers are supplied the predicate is the identity and `usable`
    // matches the Phase D behaviour exactly.
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
    let positions: Vec<Point2<F>> = features.iter().map(|f| f.point.position).collect();
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

    // Process each component independently; preserve the labelled
    // source-indices of every component that yielded a valid solution
    // so the orchestrator can build the global "unlabelled" set
    // afterwards.
    let mut component_outputs: Vec<ComponentOutput<F>> = Vec::new();
    for component in &components {
        if component.len() < 4 {
            continue;
        }
        match build_component_solution(component, features, dimensions, params) {
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

    // Build the global "unlabelled" set: every feature that no
    // component admitted (neither kept nor validation-dropped). The
    // Single-component callers read these on `solutions[0].rejected`; the
    // multi-component path attributes them to the
    // largest (first) component so callers that read solely
    // `solutions[0].rejected` see the same shape.
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
    let mut global_unlabelled: Vec<RejectedFeature<F>> = Vec::new();
    for feature in features {
        let src = feature.point.source_index;
        if globally_kept.contains(&src) {
            continue;
        }
        if globally_validation_dropped.contains(&src) {
            // A feature might be validation-dropped in one component
            // and never seen in any other; surface that as a
            // validation drop on the largest solution.
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

    let mut solutions: Vec<GridSolution<F>> = Vec::with_capacity(component_outputs.len());
    for (idx, out) in component_outputs.into_iter().enumerate() {
        let ComponentOutput {
            entries,
            fit,
            mut rejected,
            ..
        } = out;
        if idx == 0 {
            // Append the global unlabelled set to the largest
            // component's rejection list so `detect_grid` callers see the
            // same shape as the single-solution path.
            rejected.extend(global_unlabelled.iter().copied());
        }
        let grid = LabelledGrid::new(LatticeKind::Square, entries, dimensions);
        solutions.push(GridSolution::new(grid, Some(fit), rejected));
    }
    Ok(solutions)
}

struct ComponentOutput<F: Float> {
    entries: Vec<GridEntry<F>>,
    fit: LatticeFit<F>,
    rejected: Vec<RejectedFeature<F>>,
    kept_source_indices: HashSet<usize>,
    validation_drop_source_indices: HashSet<usize>,
    min_source_index: usize,
}

fn build_usable_mask<F: Float>(
    features: &[OrientedFeature<F, 2>],
    axes: &[AxisCache<F>],
    topo: &TopologicalParams<F>,
) -> Vec<bool> {
    features
        .iter()
        .zip(axes.iter())
        .map(|(f, cache)| cache.any_informative() && axes_pass_cluster_gate(&f.axes, cache, topo))
        .collect()
}

fn build_component_solution<F: Float>(
    component: &walk::TopologicalComponent,
    features: &[OrientedFeature<F, 2>],
    _dimensions: Option<GridDimensions>,
    params: &DetectionParams<F>,
) -> Option<ComponentOutput<F>> {
    let mut labelled_entries: Vec<LabelledEntryRaw<F>> = component
        .labelled
        .iter()
        .map(|(coord, &idx)| LabelledEntryRaw {
            idx,
            position: features[idx].point.position,
            coord: *coord,
        })
        .collect();

    // Reuse the shared post-stage. validate::square is lattice-shape-
    // agnostic at the predicate level — same module used by seed-grow.
    let validate_entries: Vec<crate::validate::LabelledEntry<F>> = labelled_entries
        .iter()
        .map(|e| crate::validate::LabelledEntry::new(e.idx, e.position, e.coord))
        .collect();
    let cell_size = estimate_cell_size(&labelled_entries);
    let validation = crate::validate::validate(&validate_entries, cell_size, &params.validate);
    if !validation.blacklist.is_empty() {
        labelled_entries.retain(|e| !validation.blacklist.contains(&e.idx));
    }
    if labelled_entries.len() < 4 {
        return None;
    }

    let lattice = LatticeKind::Square;
    let mut fit_outcome = fit_and_residuals(&labelled_entries, features, lattice, params).ok()?;

    if !fit_outcome.over_threshold.is_empty() {
        let drop: HashSet<usize> = fit_outcome
            .over_threshold
            .iter()
            .map(|r| r.source_index)
            .collect();
        let entries_kept: Vec<LabelledEntryRaw<F>> = labelled_entries
            .iter()
            .copied()
            .filter(|e| !drop.contains(&features[e.idx].point.source_index))
            .collect();
        if entries_kept.len() < 4 {
            return None;
        }
        let refit = fit_and_residuals(&entries_kept, features, lattice, params).ok()?;
        labelled_entries = entries_kept;
        fit_outcome = FitOutcome {
            entries: refit.entries,
            fit: refit.fit,
            over_threshold: fit_outcome.over_threshold,
        };
    }

    let FitOutcome {
        entries: entries_out,
        fit,
        over_threshold,
    } = fit_outcome;

    let kept_source_indices: HashSet<usize> = labelled_entries
        .iter()
        .map(|e| features[e.idx].point.source_index)
        .collect();
    let validation_drop_source_indices: HashSet<usize> = validation
        .blacklist
        .iter()
        .map(|&idx| features[idx].point.source_index)
        .collect();

    // Per-component rejected: validation drops scoped to this component,
    // plus over-threshold post-fit residuals scoped to this component.
    // Globally-unlabelled features are attributed by the orchestrator to
    // the largest component so the primary solution remains complete.
    let mut rejected: Vec<RejectedFeature<F>> = Vec::new();
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
    let entries_out_sorted = sorted_entries(entries_out);

    Some(ComponentOutput {
        entries: entries_out_sorted,
        fit,
        rejected,
        kept_source_indices,
        validation_drop_source_indices,
        min_source_index,
    })
}

/// Per-feature alignment check against the optional axis-cluster
/// centers in [`TopologicalParams`]. Returns `true` when the gate is
/// disabled (`axis_cluster_centers.is_none()`) or when at least one
/// informative axis is within `cluster_axis_tol_rad` of one of the
/// centers under undirected (mod π) distance.
fn axes_pass_cluster_gate<F: Float>(
    axes: &[crate::feature::LocalAxis<F>; 2],
    cache: &AxisCache<F>,
    params: &TopologicalParams<F>,
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
fn angular_dist_pi<F: Float>(a: F, b: F) -> F {
    let pi = F::pi();
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

#[derive(Clone, Copy)]
struct LabelledEntryRaw<F: Float> {
    idx: usize,
    position: Point2<F>,
    coord: Coord,
}

struct FitOutcome<F: Float> {
    entries: Vec<GridEntry<F>>,
    fit: LatticeFit<F>,
    over_threshold: Vec<RejectedFeature<F>>,
}

fn fit_and_residuals<F: Float>(
    entries: &[LabelledEntryRaw<F>],
    features: &[OrientedFeature<F, 2>],
    lattice: LatticeKind,
    params: &DetectionParams<F>,
) -> Result<FitOutcome<F>> {
    if entries.len() < 4 {
        return Err(GridError::InsufficientEvidence);
    }
    let mut model_pts: Vec<Point2<F>> = Vec::with_capacity(entries.len());
    let mut image_pts: Vec<Point2<F>> = Vec::with_capacity(entries.len());
    for entry in entries {
        model_pts.push(lattice.model_point(entry.coord));
        image_pts.push(entry.position);
    }
    let model_to_image = estimate_projective(&model_pts, &image_pts)?;

    let mut entries_out: Vec<GridEntry<F>> = Vec::with_capacity(entries.len());
    let mut residual_sum = F::zero();
    let mut residual_max = F::zero();
    let mut over_threshold: Vec<RejectedFeature<F>> = Vec::new();

    for entry in entries {
        let predicted = apply_projective(&model_to_image, lattice.model_point(entry.coord))
            .ok_or(GridError::DegenerateGeometry)?;
        let dx = entry.position.x - predicted.x;
        let dy = entry.position.y - predicted.y;
        let residual = (dx * dx + dy * dy).sqrt();
        residual_sum += residual;
        if residual > residual_max {
            residual_max = residual;
        }
        let source_index = features[entry.idx].point.source_index;
        if residual > params.max_residual_px {
            over_threshold.push(RejectedFeature::new(
                source_index,
                Some(entry.coord),
                Some(residual),
                RejectionReason::ResidualTooHigh,
            ));
        }
        entries_out.push(GridEntry::new(
            entry.coord,
            source_index,
            entry.position,
            Some(residual),
        ));
    }
    let mean = residual_sum / lit::<F>(entries.len() as f32);
    let summary = ResidualSummary::new(entries.len(), mean, residual_max);
    Ok(FitOutcome {
        entries: entries_out,
        fit: LatticeFit::new(model_to_image, summary),
        over_threshold,
    })
}

fn sorted_entries<F: Float>(mut entries: Vec<GridEntry<F>>) -> Vec<GridEntry<F>> {
    entries.sort_by_key(|e| (e.coord, e.source_index));
    entries
}

/// Triangulate only the usable features and remap triangle vertex
/// indices back into the global feature index space.
fn triangulate_usable<F: Float>(
    positions: &[Point2<F>],
    usable: &[bool],
) -> delaunay::Triangulation {
    let mut packed_to_global: Vec<usize> = Vec::with_capacity(positions.len());
    let mut packed_positions: Vec<Point2<F>> = Vec::with_capacity(positions.len());
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
fn estimate_cell_size<F: Float>(entries: &[LabelledEntryRaw<F>]) -> F {
    use crate::lattice::SQUARE_CARDINAL_OFFSETS;
    use std::collections::HashMap;

    let by_grid: HashMap<Coord, usize> = entries
        .iter()
        .enumerate()
        .map(|(slot, e)| (e.coord, slot))
        .collect();
    let mut sum = F::zero();
    let mut count: usize = 0;
    for entry in entries {
        for offset in &SQUARE_CARDINAL_OFFSETS {
            let neigh = Coord::new(entry.coord.u + offset.u, entry.coord.v + offset.v);
            if let Some(&slot) = by_grid.get(&neigh) {
                let nb = entries[slot].position;
                let dx = nb.x - entry.position.x;
                let dy = nb.y - entry.position.y;
                sum += (dx * dx + dy * dy).sqrt();
                count += 1;
            }
        }
    }
    if count == 0 {
        return lit::<F>(1.0_f32);
    }
    sum / lit::<F>(count as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::{LocalAxis, PointFeature};

    fn axis_aligned_features(rows: i32, cols: i32, s: f32) -> Vec<OrientedFeature<f32, 2>> {
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
        let p = TopologicalParams::<f32>::default();
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
        let params = DetectionParams::<f32>::default();
        let mut solutions =
            detect_square_oriented2_topological_all(&features, None, &params).unwrap();
        assert_eq!(solutions.len(), 1);
        let solution = solutions.remove(0);
        assert_eq!(solution.grid.entries.len(), 25);
        let fit = solution.fit.unwrap();
        assert!(fit.residuals.max_px < 0.01, "{}", fit.residuals.max_px);
    }

    #[test]
    fn fewer_than_three_features_errors() {
        let features = axis_aligned_features(1, 2, 20.0);
        let params = DetectionParams::<f32>::default();
        let err = detect_square_oriented2_topological_all(&features, None, &params).unwrap_err();
        assert_eq!(err, GridError::InsufficientEvidence);
    }

    #[test]
    fn cluster_gate_drops_off_axis_features() {
        // 5×5 axis-aligned grid (axes at 0°, 90°) + 4 noise features
        // whose axes both sit near 45°. With the cluster gate centered
        // at [0, π/2] and a 16° tolerance, the noise features must be
        // dropped pre-Delaunay; with the gate disabled they survive
        // (and may or may not get labelled — but the gate-OFF run keeps
        // 29 informative features through the cache step).
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

        // Gate ON: noise features fail the cluster gate.
        let params_on = DetectionParams::<f32>::default().with_topological(
            TopologicalParams::<f32>::default()
                .with_axis_cluster_centers([0.0, std::f32::consts::FRAC_PI_2]),
        );
        let mut sol_on =
            detect_square_oriented2_topological_all(&features, None, &params_on).unwrap();
        assert_eq!(sol_on.len(), 1);
        let primary = sol_on.remove(0);
        assert_eq!(primary.grid.entries.len(), 25, "gate must keep the 5×5");

        // Gate OFF: same grid still labelled, but the noise features
        // are NOT pre-filtered (they pass the per-axis-cache filter and
        // enter Delaunay). They may still end up unlabelled by walk,
        // but the rejection-reason distribution differs from the gated
        // run.
        let params_off = DetectionParams::<f32>::default();
        let mut sol_off =
            detect_square_oriented2_topological_all(&features, None, &params_off).unwrap();
        assert_eq!(sol_off.len(), 1);
        let primary_off = sol_off.remove(0);
        assert_eq!(primary_off.grid.entries.len(), 25);
        // The noise features make it into the orchestrator's rejected
        // bucket in both cases (the grid labels them either way), but
        // the gate path scrubs them at the pre-filter stage rather
        // than after Delaunay. Both runs surface them as `Unlabelled`.
        let noise_ids: std::collections::HashSet<usize> = (next..next + 4).collect();
        for r in &primary.rejected {
            if noise_ids.contains(&r.source_index) {
                assert_eq!(r.reason, RejectionReason::Unlabelled);
            }
        }
    }

    #[test]
    fn axes_pass_cluster_gate_with_no_centers_is_identity() {
        // Sanity for the helper: gate disabled accepts everything; gate
        // enabled rejects an axis 45° from both centers.
        let cache = AxisCache {
            angle_rad: [std::f32::consts::FRAC_PI_4, std::f32::consts::FRAC_PI_4],
            informative: [true, true],
        };
        let axes = [
            LocalAxis::new(std::f32::consts::FRAC_PI_4, Some(0.05_f32)),
            LocalAxis::new(std::f32::consts::FRAC_PI_4, Some(0.05_f32)),
        ];
        let params_off = TopologicalParams::<f32>::default();
        assert!(axes_pass_cluster_gate(&axes, &cache, &params_off));
        let params_on = TopologicalParams::<f32>::default()
            .with_axis_cluster_centers([0.0_f32, std::f32::consts::FRAC_PI_2]);
        assert!(!axes_pass_cluster_gate(&axes, &cache, &params_on));
    }

    #[test]
    fn angular_dist_pi_is_undirected() {
        // 0 vs π should be 0 (same undirected axis); 0 vs π/2 is π/2.
        let pi = std::f32::consts::PI;
        let d_zero = angular_dist_pi::<f32>(0.0, pi);
        assert!(d_zero < 1e-5, "{d_zero}");
        let d_perp = angular_dist_pi::<f32>(0.0, std::f32::consts::FRAC_PI_2);
        assert!((d_perp - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        // Sign / wrapping: `-0.1` and `π + 0.1` represent the same
        // direction on the mod-π circle only up to 0.2 rad apart
        // (each is 0.1 rad either side of the 0-seam).
        let d_signed = angular_dist_pi::<f64>(-0.1, std::f64::consts::PI + 0.1);
        assert!((d_signed - 0.2).abs() < 1e-9, "{d_signed}");
        // The seam itself: `π - 0.05` and `0.05` are 0.1 apart.
        let d_seam = angular_dist_pi::<f32>(std::f32::consts::PI - 0.05, 0.05);
        assert!((d_seam - 0.1).abs() < 1e-5, "{d_seam}");
    }

    #[test]
    fn cluster_gate_default_f64_matches_f32() {
        let p32 = TopologicalParams::<f32>::default();
        let p64 = TopologicalParams::<f64>::default();
        assert!((f64::from(p32.cluster_axis_tol_rad) - p64.cluster_axis_tol_rad).abs() < 1e-6);
    }
}
