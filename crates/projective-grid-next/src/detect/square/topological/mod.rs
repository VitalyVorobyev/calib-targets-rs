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
//!    quad per chessboard cell).
//! 5. Drop quads with two illegal corners (quad-mesh degree > 4),
//!    extreme parallelograms, or out-of-band edge lengths against the
//!    per-component median.
//! 6. Flood-fill integer `(u, v)` labels through the surviving quad
//!    mesh and rebase each connected component to `(0, 0)`.
//! 7. Reuse the shared [`crate::validate::square`] post-stage to drop
//!    labelled corners flagged by the line-collinearity, local-H,
//!    edge-length, and (opt-in) axis-slot-swap parity checks.
//! 8. Fit a projective transform on the surviving labels and report
//!    per-corner residuals.
//!
//! ## Algorithmic origin
//!
//! Ported near-verbatim from the legacy `projective_grid::topological`
//! module, with the following deltas:
//!
//! * The `TopologicalContext<F>` trait (eligibility / policy / axis
//!   overrides) is gone. The Phase C `(Square, Oriented2)` seed-grow
//!   path dropped its analogous `SquareGrowContext` trait without
//!   losing functionality; this module does the same.
//! * The legacy crate's optional `cluster_centers` axis-prior gate is
//!   dropped. Per-edge axis alignment already filters off-axis noise
//!   (the salvage's "no_gate" path passes the same 5×5 + noiser
//!   integration test that the gated path does). No caller currently
//!   supplies a prior, and adding the knob back is a non-breaking
//!   change.
//! * Diagnostic event emission (`DiagnosticSink<F>`, `EdgeClass`/
//!   `QuadRejectReason` events) is dropped. Phase E may reintroduce
//!   counter collection through the same trait the seed-grow path
//!   uses if a consumer needs it.
//! * `Coord` is the new struct, not a tuple — all `(i, j)` arithmetic
//!   goes through `Coord::new` / `.u` / `.v`.

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
/// Defaults are the regression-pinned values from the legacy
/// `projective_grid::topological::TopologicalParams`. Adding new fields
/// is non-breaking via `#[non_exhaustive]`; literal-construction from
/// outside the crate goes through [`Self::default`] + struct-update
/// syntax or [`Self::new`].
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
    /// Default: `0.6 ≈ 34°` (matches the legacy regression-pinned
    /// value). `sigma_rad = None` is treated as informative.
    pub max_axis_sigma_rad: F,
    /// Reject quads whose opposing edges differ in length by more than
    /// this factor (paper's parallelogram test). Default: `1.5`.
    pub opposing_edge_ratio_max: F,
    /// Reject quads whose perimeter edges fall outside
    /// `[1.0 / edge_length_ratio_max, edge_length_ratio_max] *
    /// component_median_edge_length`. Default: `2.5`. Set to `+inf`
    /// to disable.
    pub edge_length_ratio_max: F,
    /// Discard labelled components with fewer than this many corners.
    /// Default: `4` (one quad of four corners).
    pub min_corners_for_component: usize,
    /// Discard connected quad-mesh components below this size. Default:
    /// `1` (keep all). Set higher to reject isolated noise quads.
    pub min_quads_per_component: usize,
}

impl<F: Float> Default for TopologicalParams<F> {
    fn default() -> Self {
        Self {
            axis_align_tol_rad: lit::<F>(15.0_f32.to_radians()),
            max_axis_sigma_rad: lit::<F>(0.6_f32),
            opposing_edge_ratio_max: lit::<F>(1.5_f32),
            edge_length_ratio_max: lit::<F>(2.5_f32),
            min_corners_for_component: 4,
            min_quads_per_component: 1,
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

    /// Builder-style override for [`Self::edge_length_ratio_max`].
    pub fn with_edge_length_ratio_max(mut self, value: F) -> Self {
        self.edge_length_ratio_max = value;
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
}

/// Axis-driven topological grid detector for `(Square, Oriented2)`.
///
/// Returns the same [`GridSolution<F>`] shape as the seed-and-grow
/// variant so consumers can treat both selectors uniformly.
pub(in crate::detect) fn detect_square_oriented2_topological<F: Float>(
    features: &[OrientedFeature<F, 2>],
    dimensions: Option<GridDimensions>,
    params: &DetectionParams<F>,
) -> Result<GridSolution<F>> {
    if features.len() < MIN_USABLE_FOR_DELAUNAY {
        return Err(GridError::InsufficientEvidence);
    }

    let topo = &params.topological;
    let axes = build_axis_caches(features, topo.max_axis_sigma_rad);
    let usable: Vec<bool> = axes.iter().map(AxisCache::any_informative).collect();
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
        topo.edge_length_ratio_max,
    );
    let components = walk::label_components(
        &kept_quads,
        topo.min_quads_per_component,
        topo.min_corners_for_component,
    );

    // Pick the largest component as the primary grid. Other components
    // become `RejectionReason::SecondaryComponent` rejections so
    // callers can see them without losing the integer-coord guarantee.
    let Some((primary_index, primary)) = components.iter().enumerate().max_by_key(|(_, c)| c.len())
    else {
        return Err(GridError::DegenerateGeometry);
    };

    if primary.len() < 4 {
        return Err(GridError::DegenerateGeometry);
    }

    // Stage entries for validate + fit.
    let mut labelled_entries: Vec<LabelledEntryRaw<F>> = primary
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
    let validation =
        crate::validate::validate(&validate_entries, features, cell_size, &params.validate);
    if !validation.blacklist.is_empty() {
        labelled_entries.retain(|e| !validation.blacklist.contains(&e.idx));
    }
    if labelled_entries.len() < 4 {
        return Err(GridError::DegenerateGeometry);
    }

    let lattice = LatticeKind::Square;
    let mut fit_outcome = fit_and_residuals(&labelled_entries, features, lattice, params)?;

    if !fit_outcome.over_threshold.is_empty() {
        // The drop set is keyed by the caller's `source_index` (matching
        // the wire shape consumers see), so the slice-position filter has
        // to translate through `features[e.idx].point.source_index`.
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
            return Err(GridError::DegenerateGeometry);
        }
        let refit = fit_and_residuals(&entries_kept, features, lattice, params)?;
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
    let validation_drop_indices: HashSet<usize> = validation
        .blacklist
        .iter()
        .map(|&idx| features[idx].point.source_index)
        .collect();
    let secondary_source_indices: HashSet<usize> = components
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != primary_index)
        .flat_map(|(_, c)| c.labelled.values().copied())
        .map(|idx| features[idx].point.source_index)
        .collect();

    let mut rejected: Vec<RejectedFeature<F>> = Vec::new();
    for feature in features {
        let src = feature.point.source_index;
        if kept_source_indices.contains(&src) {
            continue;
        }
        let reason = if validation_drop_indices.contains(&src) {
            RejectionReason::ValidationDropped
        } else if secondary_source_indices.contains(&src) {
            RejectionReason::SecondaryComponent
        } else {
            RejectionReason::Unlabelled
        };
        rejected.push(RejectedFeature::new(src, None, None, reason));
    }
    for r in over_threshold {
        rejected.push(r);
    }

    let entries_out_sorted = sorted_entries(entries_out);
    let grid = LabelledGrid::new(lattice, entries_out_sorted, dimensions);
    Ok(GridSolution::new(grid, Some(fit), rejected))
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
        assert!((p.edge_length_ratio_max - 2.5).abs() < 1e-5);
        assert_eq!(p.min_corners_for_component, 4);
        assert_eq!(p.min_quads_per_component, 1);
    }

    #[test]
    fn clean_5x5_grid_is_fully_labelled() {
        let features = axis_aligned_features(5, 5, 20.0);
        let params = DetectionParams::<f32>::default();
        let solution = detect_square_oriented2_topological(&features, None, &params).unwrap();
        assert_eq!(solution.grid.entries.len(), 25);
        let fit = solution.fit.unwrap();
        assert!(fit.residuals.max_px < 0.01, "{}", fit.residuals.max_px);
    }

    #[test]
    fn fewer_than_three_features_errors() {
        let features = axis_aligned_features(1, 2, 20.0);
        let params = DetectionParams::<f32>::default();
        let err = detect_square_oriented2_topological(&features, None, &params).unwrap_err();
        assert_eq!(err, GridError::InsufficientEvidence);
    }
}
