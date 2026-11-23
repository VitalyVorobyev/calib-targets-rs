//! Component validation, projective fit, residual drop, and triangulation helpers.

use std::collections::HashSet;

use nalgebra::Point2;

use crate::cluster::angular_dist_pi;
use crate::detect::DetectionParams;
use crate::error::{GridError, Result};
use crate::feature::OrientedFeature;
use crate::lattice::{Coord, GridDimensions, LatticeKind};
use crate::result::{
    GridEntry, GridSolution, LabelledGrid, LatticeFit, RejectedFeature, RejectionReason,
};
use crate::shared::cell_size::mean_cardinal_edge_length;
use crate::shared::validate as pg_validate;
use crate::shared::{fit_component, FitComponentResult};
use crate::topological::axis::AxisCache;
use crate::topological::{classify, delaunay};

use super::{TopologicalParams, MIN_USABLE_FOR_DELAUNAY};

/// Build the global "unlabelled" set and assemble the per-component
/// solutions, attributing every globally-unseen feature to the largest
/// component so callers that read solely `solutions[0].rejected` see the
/// same shape as the single-solution path.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        name = "topological_assembly",
        level = "debug",
        skip_all,
        fields(num_components = component_outputs.len()),
    )
)]
pub(super) fn assemble_solutions(
    component_outputs: Vec<ComponentOutput>,
    features: &[OrientedFeature<2>],
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
    global_unlabelled.sort_by_key(|rejected| rejected.source_index);

    let mut solutions: Vec<GridSolution> = Vec::with_capacity(component_outputs.len());
    for (idx, out) in component_outputs.into_iter().enumerate() {
        let ComponentOutput {
            entries,
            fit,
            dimensions,
            mut rejected,
            ..
        } = out;
        if idx == 0 {
            rejected.extend(global_unlabelled.iter().copied());
        }
        sort_rejections(&mut rejected);
        let grid = LabelledGrid::new(LatticeKind::Square, entries, dimensions);
        solutions.push(GridSolution::new(grid, fit, rejected));
    }
    solutions
}

pub(in crate::topological) struct ComponentOutput {
    pub(in crate::topological) entries: Vec<GridEntry>,
    pub(in crate::topological) fit: LatticeFit,
    pub(in crate::topological) dimensions: Option<GridDimensions>,
    pub(in crate::topological) rejected: Vec<RejectedFeature>,
    pub(in crate::topological) kept_source_indices: HashSet<usize>,
    pub(in crate::topological) validation_drop_source_indices: HashSet<usize>,
    pub(in crate::topological) min_source_index: usize,
}

pub(super) fn build_usable_mask(
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

pub(super) fn build_component_solution(
    labelled: &std::collections::HashMap<Coord, usize>,
    features: &[OrientedFeature<2>],
    positions: &[Point2<f32>],
    dimensions: Option<GridDimensions>,
    params: &DetectionParams,
) -> Result<Option<ComponentOutput>> {
    // Reuse the shared advanced validate post-stage (the same module the
    // chessboard topological adapter and the recovery schedule consume).
    let mut validate_entries: Vec<pg_validate::LabelledEntry> = labelled
        .iter()
        .map(|(coord, &idx)| pg_validate::LabelledEntry {
            idx,
            pixel: features[idx].point.position,
            grid: (coord.u, coord.v),
        })
        .collect();
    validate_entries.sort_by_key(|entry| (entry.grid.1, entry.grid.0, entry.idx));
    let cell_size = mean_cardinal_edge_length(labelled, positions);
    #[cfg(feature = "tracing")]
    let validation = {
        let _span = tracing::debug_span!("topological_validation").entered();
        pg_validate::validate(&validate_entries, cell_size, &params.tuning().validation)
    };
    #[cfg(not(feature = "tracing"))]
    let validation =
        pg_validate::validate(&validate_entries, cell_size, &params.tuning().validation);

    // Working label set after the validation drop, keyed by Coord.
    let mut kept: Vec<(Coord, usize)> = labelled
        .iter()
        .filter(|(_, &idx)| !validation.blacklist.contains(&idx))
        .map(|(&coord, &idx)| (coord, idx))
        .collect();
    if kept.len() < 4 {
        return Ok(None);
    }

    // Canonicalize before fitting so the mandatory public transform and its
    // residuals are expressed in exactly the same coordinate frame as the
    // returned labels. Detector builders that need the original axis slots use
    // `expert::square::assemble_oriented2_components`, which stops before this
    // facade-only normalization.
    let provisional_entries: Vec<GridEntry> = kept
        .iter()
        .map(|&(coord, feature_index)| {
            let feature = &features[feature_index];
            GridEntry::new(
                coord,
                feature.point.source_index,
                feature.point.position,
                None,
            )
        })
        .collect();
    let mut grid = LabelledGrid::new(LatticeKind::Square, provisional_entries, dimensions);
    grid.normalize();
    if let (Some(dimensions), Some((min, max))) = (grid.dimensions(), grid.bbox()) {
        debug_assert_eq!(min, Coord::new(0, 0));
        let Some(span_u) = usize::try_from(max.u)
            .ok()
            .and_then(|value| value.checked_add(1))
        else {
            return Ok(None);
        };
        let Some(span_v) = usize::try_from(max.v)
            .ok()
            .and_then(|value| value.checked_add(1))
        else {
            return Ok(None);
        };
        if span_u > dimensions.width || span_v > dimensions.height {
            return Ok(None);
        }
    }
    let feature_index_by_source: std::collections::HashMap<usize, usize> = features
        .iter()
        .enumerate()
        .map(|(index, feature)| (feature.point.source_index, index))
        .collect();
    let canonical_kept = grid
        .entries()
        .iter()
        .map(|entry| {
            feature_index_by_source
                .get(&entry.source_index)
                .copied()
                .map(|feature_index| (entry.coord, feature_index))
        })
        .collect::<Option<Vec<_>>>();
    let Some(canonical_kept) = canonical_kept else {
        return Ok(None);
    };
    kept = canonical_kept;
    let Some(fit_result) =
        run_fit_with_residual_drop(&mut kept, features, positions, LatticeKind::Square, params)?
    else {
        return Ok(None);
    };
    let entries_out = fit_result.entries;
    let fit = fit_result.fit;
    let over_threshold = fit_result.over_threshold;
    let dimensions = grid.dimensions();

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
    sort_rejections(&mut rejected);

    let min_source_index = kept_source_indices
        .iter()
        .copied()
        .min()
        .unwrap_or(usize::MAX);

    Ok(Some(ComponentOutput {
        entries: entries_out,
        fit,
        dimensions,
        rejected,
        kept_source_indices,
        validation_drop_source_indices,
        min_source_index,
    }))
}

/// Run the shared `fit_component` helper, drop over-threshold entries
/// once, and refit on the remaining set. Mutates `kept` to the surviving
/// label set. Returns `None` when fewer than four entries survive.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        name = "topological_projective_fit",
        level = "debug",
        skip_all,
        fields(num_entries = kept.len()),
    )
)]
fn run_fit_with_residual_drop(
    kept: &mut Vec<(Coord, usize)>,
    features: &[OrientedFeature<2>],
    positions: &[Point2<f32>],
    lattice: LatticeKind,
    params: &DetectionParams,
) -> Result<Option<FitComponentResult>> {
    let first = fit_component(kept, features, positions, lattice, params)?;
    if first.over_threshold.is_empty() {
        return Ok(Some(first));
    }
    let drop: HashSet<usize> = first
        .over_threshold
        .iter()
        .map(|r| r.source_index)
        .collect();
    kept.retain(|&(_, idx)| !drop.contains(&features[idx].point.source_index));
    if kept.len() < 4 {
        return Ok(None);
    }
    // Dropping a border row or column can lift bbox-min above `(0, 0)`. Rebase
    // before the refit so both `entries` and `fit` satisfy the `(0, 0)` invariant.
    crate::lattice::rebase::rebase_slice_to_origin(kept, |item| &mut item.0);
    let refit = fit_component(kept, features, positions, lattice, params)?;
    // Preserve the first pass's over-threshold attribution.
    Ok(Some(FitComponentResult {
        entries: refit.entries,
        fit: refit.fit,
        over_threshold: first.over_threshold,
    }))
}

fn sort_rejections(rejected: &mut [RejectedFeature]) {
    rejected.sort_by_key(|item| {
        let reason = match item.reason {
            RejectionReason::ResidualTooHigh => 0_u8,
            RejectionReason::ValidationDropped => 1,
            RejectionReason::Unlabelled => 2,
        };
        (
            item.source_index,
            reason,
            item.coord.map(|coord| (coord.v, coord.u)),
        )
    });
}

/// Per-feature alignment check against the optional axis-cluster
/// centers in [`TopologicalParams`]. Returns `true` when the gate is
/// disabled (`axis_cluster_centers.is_none()`) or when at least one
/// informative axis is within `cluster_axis_tol_rad` of one of the
/// centers under undirected (mod π) distance.
pub(super) fn axes_pass_cluster_gate(
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

/// Triangulate the usable features and classify every edge, after pruning
/// features that cannot be a lattice-cell corner.
///
/// A feature none of whose Delaunay edges is a Grid edge belongs to no cell:
/// every cell corner has two Grid edges on the cell's perimeter. Such a
/// feature — typically clutter inside a cell, where its edges run to the
/// cell's corners at roughly the diagonal angle — still splits the
/// surrounding cells into fans of triangles that no longer pair into quads,
/// so a few of them along a narrow strip of cells can disconnect a board.
/// Pruning them and triangulating once more recovers those cells.
///
/// The pass cannot lose a cell. Removing a vertex from a Delaunay
/// triangulation leaves every triangle not incident to it in place (their
/// circumcircles stay empty), and no triangle of a quad is incident to a
/// pruned feature, since quad corners have Grid edges. Edges between the
/// remaining features are only added, so no remaining feature loses a Grid
/// edge, and a second pass would prune nothing. A pruned feature stays in the
/// input; only the triangulation leaves it out. The criterion is the cell
/// test's own alignment test, so it adds no parameter.
///
/// Returns the final usable mask with the triangulation and edge classes
/// built over it.
pub(super) fn triangulate_lattice_candidates(
    positions: &[Point2<f32>],
    axes: &[AxisCache],
    mut usable: Vec<bool>,
    axis_align_tol_rad: f32,
) -> Result<(Vec<bool>, delaunay::Triangulation, Vec<classify::EdgeClass>)> {
    let triangulate_and_classify = |usable: &[bool]| {
        let triangulation = triangulate_usable(positions, usable);
        let edge_kinds =
            classify::classify_all_edges(positions, axes, &triangulation, axis_align_tol_rad);
        (triangulation, edge_kinds)
    };
    let (triangulation, edge_kinds) = triangulate_and_classify(&usable);
    if triangulation.num_tri() == 0 {
        return Err(GridError::DegenerateGeometry);
    }

    let mut has_grid_edge = vec![false; positions.len()];
    for (edge, &kind) in edge_kinds.iter().enumerate() {
        if kind == classify::EdgeClass::Grid {
            has_grid_edge[triangulation.triangles[edge]] = true;
            has_grid_edge[triangulation.triangles[delaunay::Triangulation::next_edge(edge)]] = true;
        }
    }
    let mut pruned = false;
    for (usable, &has_grid_edge) in usable.iter_mut().zip(&has_grid_edge) {
        if *usable && !has_grid_edge {
            *usable = false;
            pruned = true;
        }
    }
    if !pruned {
        return Ok((usable, triangulation, edge_kinds));
    }
    if usable.iter().filter(|&&u| u).count() < MIN_USABLE_FOR_DELAUNAY {
        return Err(GridError::InsufficientEvidence);
    }
    let (triangulation, edge_kinds) = triangulate_and_classify(&usable);
    if triangulation.num_tri() == 0 {
        return Err(GridError::DegenerateGeometry);
    }
    Ok((usable, triangulation, edge_kinds))
}

/// Triangulate only the usable features and remap triangle vertex
/// indices back into the global feature index space.
pub(in crate::topological) fn triangulate_usable(
    positions: &[Point2<f32>],
    usable: &[bool],
) -> delaunay::Triangulation {
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
