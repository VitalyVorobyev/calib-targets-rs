//! Hex orchestration for the topological grid finder: the
//! `(LatticeKind::Hex, Evidence::Oriented3 | Positions)` entry point and its
//! component merge / fit / assembly helpers.
//!
//! This module owns the hex *pipeline wiring* — triangulate usable features,
//! classify hex cells against the three axis families, flood-fill axial
//! `(q, r)` labels, reunite components under the D6 symmetry group, and fit
//! each component through [`LatticeKind::Hex`]. The hex lattice math
//! (axis caches, cell classification, component labelling, D6) lives in
//! [`super::hex`]; the shared fit / merge engines live in
//! [`crate::shared`]. What does NOT belong here: any square-specific stage
//! (there is no diagonal class, no triangle-pair merge, and no
//! square-oriented validate on the hex path) and any image sampling.
//! Tier: advanced engine (semver-exempt pre-1.0).

use std::collections::HashSet;

use nalgebra::Point2;

use crate::detect::DetectionParams;
use crate::error::{GridError, Result};
use crate::lattice::{Coord, GridDimensions, LatticeKind};
use crate::result::{GridSolution, LabelledGrid, RejectedFeature, RejectionReason};
use crate::shared::fit_component;
use crate::shared::merge::{ComponentInput, LocalMergeParams};
use crate::shared::FitComponentResult;

use super::{hex, triangulate_usable, ComponentOutput, MIN_USABLE_FOR_DELAUNAY};

/// Multi-component axis-driven topological grid detector for
/// `(Hex, Oriented3)`.
///
/// Mirrors [`super::detect_square_oriented2_topological_all`] but on a hex
/// point lattice: the Delaunay triangles *are* the unit cells (no diagonal
/// class, no triangle-pair-to-quad merge), so classification keeps triangles
/// whose three edges align with three distinct axis families, and the walk
/// labels axial `(q, r)` coordinates by parallelogram completion. The shared
/// back-half (fit + residual drop) runs through [`LatticeKind::Hex`] / its
/// [`LatticeKind::model_point`].
///
/// Hex is **topological-only** with **no recovery schedule** (the recovery
/// machinery is seed-and-grow-coupled); the geometric fit residual drop is the
/// precision gate. Components are returned largest-first.
pub(crate) fn detect_hex_oriented3_topological_all(
    features: &[crate::feature::OrientedFeature<3>],
    dimensions: Option<GridDimensions>,
    params: &DetectionParams,
) -> Result<Vec<GridSolution>> {
    if features.len() < MIN_USABLE_FOR_DELAUNAY {
        return Err(GridError::InsufficientEvidence);
    }
    let topo = &params.topological;
    let caches = hex::build_hex_axis_caches(features, topo.max_axis_sigma_rad);
    let usable: Vec<bool> = caches
        .iter()
        .map(|c| c.informative[0] || c.informative[1] || c.informative[2])
        .collect();
    let n_usable = usable.iter().filter(|&&b| b).count();
    if n_usable < MIN_USABLE_FOR_DELAUNAY {
        return Err(GridError::InsufficientEvidence);
    }

    let positions: Vec<Point2<f32>> = features.iter().map(|f| f.point.position).collect();
    let triangulation = triangulate_usable(&positions, &usable);
    if triangulation.num_tri() == 0 {
        return Err(GridError::DegenerateGeometry);
    }

    let cells =
        hex::classify_hex_cells(&positions, &caches, &triangulation, topo.axis_align_tol_rad);
    let components = hex::label_components(&cells, topo.min_corners_for_component);
    if components.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }

    // Reunite the labelled hex components in axial label space under the D6
    // symmetry group (the hex analogue of the square facade's D4
    // `merge_components_local`). Ordered largest-first, ties by smallest feature
    // index, for determinism.
    let merged = merge_hex_components(&components, &positions);
    if merged.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }

    let mut component_outputs: Vec<ComponentOutput> = Vec::new();
    for labelled in &merged {
        if labelled.len() < 4 {
            continue;
        }
        match build_hex_component_solution(labelled, features, &positions, params) {
            Some(out) => component_outputs.push(out),
            None => continue,
        }
    }
    if component_outputs.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }

    component_outputs.sort_by(|a, b| {
        b.kept_source_indices
            .len()
            .cmp(&a.kept_source_indices.len())
            .then_with(|| a.min_source_index.cmp(&b.min_source_index))
    });

    Ok(assemble_hex_solutions(
        component_outputs,
        features,
        dimensions,
    ))
}

/// Reunite the hex walk's labelled components in axial label space via the
/// shared local-geometry merge under the D6 symmetry group, returning one
/// `Coord`-keyed map per surviving merged component.
fn merge_hex_components(
    components: &[hex::HexComponent],
    positions: &[Point2<f32>],
) -> Vec<std::collections::HashMap<Coord, usize>> {
    let mut ordered: Vec<&hex::HexComponent> = components.iter().collect();
    ordered.sort_by(|a, b| {
        b.labelled.len().cmp(&a.labelled.len()).then_with(|| {
            a.labelled
                .values()
                .copied()
                .min()
                .unwrap_or(usize::MAX)
                .cmp(&b.labelled.values().copied().min().unwrap_or(usize::MAX))
        })
    });

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

    let merged = crate::shared::merge::merge_components_local_for(
        &views,
        &LocalMergeParams::default(),
        LatticeKind::Hex,
    );
    let merged = if merged.components.is_empty() {
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

/// Fit one hex component through [`LatticeKind::Hex`] with a single
/// over-residual drop + refit; no square-specific validate stage runs (the
/// shared validate's row/column model is square-oriented — hex precision rests
/// on the projective fit residual).
fn build_hex_component_solution(
    labelled: &std::collections::HashMap<Coord, usize>,
    features: &[crate::feature::OrientedFeature<3>],
    positions: &[Point2<f32>],
    params: &DetectionParams,
) -> Option<ComponentOutput> {
    let mut kept: Vec<(Coord, usize)> = labelled.iter().map(|(&c, &idx)| (c, idx)).collect();
    if kept.len() < 4 {
        return None;
    }
    // Deterministic order before the fit (HashMap iteration is unordered).
    kept.sort_by_key(|&(c, idx)| (c, idx));

    let fit_result = hex_fit_with_residual_drop(&mut kept, features, positions, params)?;
    let FitComponentResult {
        entries: entries_out,
        fit,
        over_threshold,
    } = fit_result;

    let kept_source_indices: HashSet<usize> = kept
        .iter()
        .map(|&(_, idx)| features[idx].point.source_index)
        .collect();

    let mut rejected: Vec<RejectedFeature> = Vec::new();
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
        validation_drop_source_indices: HashSet::new(),
        min_source_index,
    })
}

/// Hex variant of [`run_fit_with_residual_drop`]: the shared `fit_component`
/// is generic over [`LatticeKind`], but it expects `OrientedFeature<2>`, so
/// the hex features are projected down to their first two axes for the fit
/// (the fit uses positions + coords only; the axes are unused inside
/// `fit_component`). Returns `None` when fewer than four entries survive.
fn hex_fit_with_residual_drop(
    kept: &mut Vec<(Coord, usize)>,
    features: &[crate::feature::OrientedFeature<3>],
    positions: &[Point2<f32>],
    params: &DetectionParams,
) -> Option<FitComponentResult> {
    let two: Vec<crate::feature::OrientedFeature<2>> = features
        .iter()
        .map(|f| crate::feature::OrientedFeature::<2>::new(f.point, [f.axes[0], f.axes[1]]))
        .collect();
    let first = fit_component(kept, &two, positions, LatticeKind::Hex, params).ok()?;
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
    let refit = fit_component(kept, &two, positions, LatticeKind::Hex, params).ok()?;
    Some(FitComponentResult {
        entries: refit.entries,
        fit: refit.fit,
        over_threshold: first.over_threshold,
    })
}

/// Assemble hex solutions, attributing globally-unlabelled features to the
/// first (largest) solution — mirroring [`assemble_solutions`] but tagged with
/// [`LatticeKind::Hex`].
fn assemble_hex_solutions(
    component_outputs: Vec<ComponentOutput>,
    features: &[crate::feature::OrientedFeature<3>],
    dimensions: Option<GridDimensions>,
) -> Vec<GridSolution> {
    let mut globally_kept: HashSet<usize> = HashSet::new();
    for out in &component_outputs {
        for &src in &out.kept_source_indices {
            globally_kept.insert(src);
        }
    }
    let mut global_unlabelled: Vec<RejectedFeature> = Vec::new();
    for feature in features {
        let src = feature.point.source_index;
        if !globally_kept.contains(&src) {
            global_unlabelled.push(RejectedFeature::new(
                src,
                None,
                None,
                RejectionReason::Unlabelled,
            ));
        }
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
        let grid = LabelledGrid::new(LatticeKind::Hex, entries, dimensions);
        solutions.push(GridSolution::new(grid, Some(fit), rejected));
    }
    solutions
}
