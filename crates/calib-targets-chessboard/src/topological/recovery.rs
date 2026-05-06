//! Chessboard-specific recovery after the image-free topological walk.
//!
//! `projective-grid` stops at connected labelled components. This module
//! adapts those components back into the chessboard detector's existing
//! booster and canonicalisation machinery.

use std::collections::{HashMap, HashSet};

use calib_targets_core::Corner;
use nalgebra::{Point2, Vector2};
use projective_grid::{merge_components_local, ComponentInput, GridTransform, GRID_TRANSFORMS_D4};

use crate::boosters::apply_boosters;
use crate::cluster::{cluster_axes, ClusterCenters};
use crate::corner::{CornerAug, CornerStage};
use crate::detector::{build_detection_from_grow, Detection};
use crate::grow::GrowResult;
use crate::params::DetectorParams;

pub(super) type LabelledComponent = HashMap<(i32, i32), usize>;

/// Estimate the global cell size of a labelled component as the median
/// nearest-neighbour pixel distance along the labelled `i` and `j` axes.
fn estimate_cell_size_from_labels(labelled: &LabelledComponent, positions: &[Point2<f32>]) -> f32 {
    let mut dists: Vec<f32> = Vec::new();
    for (&(i, j), &idx) in labelled.iter() {
        let p = positions[idx];
        if let Some(&right) = labelled.get(&(i + 1, j)) {
            let q = positions[right];
            dists.push(((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt());
        }
        if let Some(&down) = labelled.get(&(i, j + 1)) {
            let q = positions[down];
            dists.push(((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt());
        }
    }
    if dists.is_empty() {
        return 0.0;
    }
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    dists[dists.len() / 2]
}

fn median(mut values: Vec<f32>) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(values[values.len() / 2])
}

fn estimate_recovery_cell_size_from_labels(
    labelled: &LabelledComponent,
    positions: &[Point2<f32>],
) -> f32 {
    // The topological walk can produce components whose visible axes are
    // strongly anisotropic under perspective. Using the median over both
    // axes as the booster scale makes the longer axis fail the shared
    // edge-length gate, so use the larger directional median for recovery
    // while keeping the final reported cell_size on the conservative
    // all-edge median.
    let mut i_dists: Vec<f32> = Vec::new();
    let mut j_dists: Vec<f32> = Vec::new();
    for (&(i, j), &idx) in labelled.iter() {
        let p = positions[idx];
        if let Some(&right) = labelled.get(&(i + 1, j)) {
            let q = positions[right];
            i_dists.push(((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt());
        }
        if let Some(&down) = labelled.get(&(i, j + 1)) {
            let q = positions[down];
            j_dists.push(((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt());
        }
    }
    match (median(i_dists), median(j_dists)) {
        (Some(i), Some(j)) => i.max(j),
        (Some(v), None) | (None, Some(v)) => v,
        (None, None) => 0.0,
    }
}

/// Mean step vectors along the labelled `i` and `j` axes.
fn estimate_grid_steps(
    labelled: &LabelledComponent,
    positions: &[Point2<f32>],
) -> (Vector2<f32>, Vector2<f32>) {
    let mut u_sum = Vector2::zeros();
    let mut u_n = 0u32;
    let mut v_sum = Vector2::zeros();
    let mut v_n = 0u32;
    for (&(i, j), &idx) in labelled.iter() {
        let p = positions[idx];
        if let Some(&right) = labelled.get(&(i + 1, j)) {
            let q = positions[right];
            u_sum += Vector2::new(q.x - p.x, q.y - p.y);
            u_n += 1;
        }
        if let Some(&down) = labelled.get(&(i, j + 1)) {
            let q = positions[down];
            v_sum += Vector2::new(q.x - p.x, q.y - p.y);
            v_n += 1;
        }
    }
    let u = if u_n > 0 {
        u_sum / u_n as f32
    } else {
        Vector2::new(1.0, 0.0)
    };
    let v = if v_n > 0 {
        v_sum / v_n as f32
    } else {
        Vector2::new(0.0, 1.0)
    };
    (u, v)
}

#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        name = "topological_clustered_augs",
        level = "debug",
        skip_all,
        fields(num_corners = corners.len()),
    )
)]
pub(super) fn clustered_augs(
    corners: &[Corner],
    params: &DetectorParams,
) -> (Vec<CornerAug>, Option<ClusterCenters>) {
    let mut augs: Vec<CornerAug> = corners
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut aug = CornerAug::from_corner(i, c);
            let strong = c.strength >= params.min_corner_strength;
            let fit_ok = !params.max_fit_rms_ratio.is_finite()
                || c.contrast <= 0.0
                || c.fit_rms <= params.max_fit_rms_ratio * c.contrast;
            if strong && fit_ok {
                aug.stage = CornerStage::Strong;
            }
            aug
        })
        .collect();
    let centers = cluster_axes(&mut augs, params);
    (augs, centers)
}

fn align_label_parity(labelled: &mut LabelledComponent, augs: &[CornerAug]) {
    let mut matches = 0usize;
    let mut mismatches = 0usize;
    for (&(i, j), &idx) in labelled.iter() {
        let Some(label) = augs.get(idx).and_then(|c| c.label) else {
            continue;
        };
        let expected = (i + j).rem_euclid(2) as u8;
        if label.as_u8() == expected {
            matches += 1;
        } else {
            mismatches += 1;
        }
    }
    if mismatches > matches {
        let shifted = labelled
            .drain()
            .map(|((i, j), idx)| ((i + 1, j), idx))
            .collect();
        *labelled = shifted;
    }
}

fn mark_labelled(augs: &mut [CornerAug], labelled: &LabelledComponent) {
    for (&at, &idx) in labelled {
        if let Some(aug) = augs.get_mut(idx) {
            aug.stage = CornerStage::Labeled {
                at,
                local_h_residual_px: None,
            };
        }
    }
}

fn transform_label(t: GridTransform, ij: (i32, i32), delta: (i32, i32)) -> (i32, i32) {
    let mapped = t.apply(ij.0, ij.1);
    (mapped.i + delta.0, mapped.j + delta.1)
}

fn shared_corner_alignment(
    dst: &LabelledComponent,
    src: &LabelledComponent,
    min_overlap: usize,
) -> Option<(GridTransform, (i32, i32))> {
    let dst_by_corner: HashMap<usize, (i32, i32)> =
        dst.iter().map(|(&ij, &idx)| (idx, ij)).collect();
    let mut best: Option<(usize, usize, (i32, i32))> = None;
    for (t_idx, t) in GRID_TRANSFORMS_D4.iter().copied().enumerate() {
        let mut votes: HashMap<(i32, i32), usize> = HashMap::new();
        for (&ij_src, &idx) in src {
            let Some(&ij_dst) = dst_by_corner.get(&idx) else {
                continue;
            };
            let mapped = t.apply(ij_src.0, ij_src.1);
            let delta = (ij_dst.0 - mapped.i, ij_dst.1 - mapped.j);
            *votes.entry(delta).or_default() += 1;
        }
        for (delta, overlap) in votes {
            let rank = (overlap, usize::MAX - t_idx, delta);
            if best.map(|b| rank > b).unwrap_or(true) {
                best = Some((overlap, t_idx, delta));
            }
        }
    }
    let (overlap, t_idx, delta) = best?;
    if overlap < min_overlap {
        return None;
    }
    let t = GRID_TRANSFORMS_D4[t_idx];
    for (&ij_src, &idx) in src {
        let mapped = transform_label(t, ij_src, delta);
        if let Some(&existing) = dst.get(&mapped) {
            if existing != idx {
                return None;
            }
        }
    }
    Some((t, delta))
}

fn merge_components_with_shared_corners(
    components: Vec<LabelledComponent>,
    min_overlap: usize,
) -> Vec<LabelledComponent> {
    let mut out: Vec<LabelledComponent> = Vec::new();
    for component in components {
        let mut merged = false;
        for dst in out.iter_mut() {
            if let Some((t, delta)) = shared_corner_alignment(dst, &component, min_overlap) {
                for (&ij_src, &idx) in &component {
                    dst.insert(transform_label(t, ij_src, delta), idx);
                }
                merged = true;
                break;
            }
        }
        if !merged {
            out.push(component);
        }
    }
    out
}

/// Synthetic [`ClusterCenters`] derived from labelled grid step directions.
fn cluster_centers_from_grid(grid_u: Vector2<f32>, grid_v: Vector2<f32>) -> ClusterCenters {
    let theta0 = grid_u.y.atan2(grid_u.x);
    let theta1 = grid_v.y.atan2(grid_v.x);
    ClusterCenters { theta0, theta1 }
}

#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_components = merged_components.len()),
    )
)]
pub(super) fn recover_topological_components(
    merged_components: &[LabelledComponent],
    positions: &[Point2<f32>],
    base_augs: &[CornerAug],
    clustered_centers: Option<ClusterCenters>,
    params: &DetectorParams,
) -> Vec<LabelledComponent> {
    let mut boosted_components: Vec<LabelledComponent> = Vec::new();
    for component_labels in merged_components {
        let blacklist = HashSet::new();
        let mut labelled = component_labels.clone();
        let cell_size = estimate_cell_size_from_labels(&labelled, positions);
        let (grid_u, grid_v) = estimate_grid_steps(&labelled, positions);
        let centers =
            clustered_centers.unwrap_or_else(|| cluster_centers_from_grid(grid_u, grid_v));
        let mut augs = base_augs.to_vec();
        if clustered_centers.is_some() {
            align_label_parity(&mut labelled, &augs);
        }
        mark_labelled(&mut augs, &labelled);

        let mut grow = GrowResult {
            labelled,
            by_corner: Default::default(),
            ambiguous: Default::default(),
            holes: Default::default(),
            grid_u,
            grid_v,
        };
        grow.by_corner = grow.labelled.iter().map(|(&k, &v)| (v, k)).collect();

        if clustered_centers.is_some() && cell_size > 0.0 {
            let recovery_cell_size =
                estimate_recovery_cell_size_from_labels(&grow.labelled, positions);
            let _ = apply_boosters(
                &mut augs,
                &mut grow,
                centers,
                recovery_cell_size.max(cell_size),
                &blacklist,
                params,
            );
        }

        if grow.labelled.len() >= 4 {
            boosted_components.push(grow.labelled);
        }
    }

    let boosted_components = merge_components_with_shared_corners(
        boosted_components,
        params.component_merge.min_overlap.max(2),
    );
    if boosted_components.is_empty() {
        return Vec::new();
    }

    let boosted_views: Vec<ComponentInput<'_>> = boosted_components
        .iter()
        .map(|labelled| ComponentInput {
            labelled,
            positions,
        })
        .collect();

    #[cfg(feature = "tracing")]
    let _span = tracing::debug_span!(
        "topological_post_recovery_component_merge",
        num_components = boosted_views.len()
    )
    .entered();

    merge_components_local(&boosted_views, &params.component_merge).components
}

#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        name = "build_topological_detections",
        level = "debug",
        skip_all,
        fields(num_components = final_components.len()),
    )
)]
pub(super) fn build_topological_detections(
    final_components: Vec<LabelledComponent>,
    positions: &[Point2<f32>],
    base_augs: &[CornerAug],
    clustered_centers: Option<ClusterCenters>,
    params: &DetectorParams,
) -> Vec<Detection> {
    let mut out: Vec<Detection> = Vec::new();
    for labelled in final_components {
        if labelled.len() < params.min_labeled_corners {
            continue;
        }
        let (grid_u, grid_v) = estimate_grid_steps(&labelled, positions);
        let centers =
            clustered_centers.unwrap_or_else(|| cluster_centers_from_grid(grid_u, grid_v));
        let mut augs = base_augs.to_vec();
        mark_labelled(&mut augs, &labelled);
        let cell_size = estimate_cell_size_from_labels(&labelled, positions);
        let mut grow = GrowResult {
            by_corner: labelled.iter().map(|(&k, &v)| (v, k)).collect(),
            labelled,
            ambiguous: Default::default(),
            holes: Default::default(),
            grid_u,
            grid_v,
        };

        // Geometry verification (Phase B). The chessboard-v2 path runs
        // this gate unconditionally before shipping a detection; the
        // topological dispatch used to skip it. The check can only drop
        // labelled corners (line collinearity / local-H residual /
        // largest cardinal component) — it never adds wrong labels —
        // and it sets `detection_refused` if too few survive. Skip when
        // cell_size is degenerate (would divide-by-zero in validate).
        if cell_size > 0.0 {
            let mut blacklist: HashSet<usize> = HashSet::new();
            let trace = crate::detector::run_geometry_check(
                &mut augs,
                &mut grow,
                centers,
                cell_size,
                &mut blacklist,
                params,
            );
            if trace.detection_refused {
                continue;
            }
        }
        if grow.labelled.len() < params.min_labeled_corners {
            continue;
        }

        out.push(build_detection_from_grow(&augs, &grow, centers, cell_size));
    }

    out.sort_by_key(|d| std::cmp::Reverse(d.target.corners.len()));
    out.truncate(params.max_components.max(1) as usize);
    out
}
