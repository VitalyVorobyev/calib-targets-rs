//! Topological dispatch path for the chessboard detector.
//!
//! Wraps [`projective_grid::build_grid_topological`] and the shared
//! component merger so the detector's `detect_all` entry point can
//! select between the historical seed-and-grow pipeline and the new
//! topological pipeline at runtime via
//! [`crate::params::GraphBuildAlgorithm`].
//!
//! The topological pipeline is image-free: it consumes only the corner
//! positions and the per-corner ChESS axes, and produces the same
//! `(i, j) → corner_idx` labelling as the seed-and-grow pipeline so the
//! detector's existing [`build_detection`](crate::detector) helper can
//! finalise the output unchanged.

use std::collections::HashMap;

use calib_targets_core::Corner;
use nalgebra::{Point2, Vector2};
use projective_grid::{
    build_grid_topological, merge_components_local, AxisHint, ComponentInput, TopologicalGrid,
};

use crate::cluster::ClusterCenters;
use crate::corner::{CornerAug, CornerStage};
use crate::detector::{build_detection_from_grow, Detection};
use crate::grow::GrowResult;
use crate::params::DetectorParams;

/// Adapt a `Corner.axes: [AxisEstimate; 2]` slot to projective-grid's
/// equivalent [`AxisHint`].
#[inline]
fn axis_hint_from(c: &Corner) -> [AxisHint; 2] {
    [
        AxisHint {
            angle: c.axes[0].angle,
            sigma: c.axes[0].sigma,
        },
        AxisHint {
            angle: c.axes[1].angle,
            sigma: c.axes[1].sigma,
        },
    ]
}

/// Filter corners by strength and fit-quality, mirroring the chessboard-v2
/// pre-filter exactly so the two pipelines share the same eligibility set.
fn prefilter(corners: &[Corner], params: &DetectorParams) -> Vec<bool> {
    corners
        .iter()
        .map(|c| {
            let strong = c.strength >= params.min_corner_strength;
            let fit_ok = !params.max_fit_rms_ratio.is_finite()
                || c.contrast <= 0.0
                || c.fit_rms <= params.max_fit_rms_ratio * c.contrast;
            strong && fit_ok
        })
        .collect()
}

/// Estimate the global cell size of a labelled component as the median
/// nearest-neighbour pixel distance along the labelled `i` and `j` axes.
fn estimate_cell_size_from_labels(
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
) -> f32 {
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

/// Mean step vectors along the labelled `i` and `j` axes. Returns
/// `(grid_u, grid_v)` where `grid_u` is the mean pixel displacement
/// from `(i, j)` to `(i + 1, j)` and `grid_v` to `(i, j + 1)`.
fn estimate_grid_steps(
    labelled: &HashMap<(i32, i32), usize>,
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

/// Synthetic [`ClusterCenters`] derived from labelled grid step
/// directions. Used purely to populate `Detection::grid_directions` —
/// none of the downstream consumers compare the topological pipeline's
/// `theta0/theta1` against the chessboard-v2's clustered axes.
fn cluster_centers_from_grid(grid_u: Vector2<f32>, grid_v: Vector2<f32>) -> ClusterCenters {
    let theta0 = grid_u.y.atan2(grid_u.x);
    let theta1 = grid_v.y.atan2(grid_v.x);
    ClusterCenters { theta0, theta1 }
}

/// Run the topological pipeline and return one [`Detection`] per
/// surviving labelled component.
pub fn detect_all_topological(corners: &[Corner], params: &DetectorParams) -> Vec<Detection> {
    if corners.is_empty() {
        return Vec::new();
    }
    let mask = prefilter(corners, params);
    if mask.iter().filter(|&&b| b).count() < params.min_labeled_corners {
        return Vec::new();
    }

    // Adapt to projective-grid inputs. Corners that fail the pre-filter
    // get a "no info" axis pair so the topological classifier excludes
    // them naturally without needing to reindex the position slice.
    let positions: Vec<Point2<f32>> = corners.iter().map(|c| c.position).collect();
    let axes: Vec<[AxisHint; 2]> = corners
        .iter()
        .zip(mask.iter())
        .map(|(c, ok)| {
            if *ok {
                axis_hint_from(c)
            } else {
                [AxisHint::default(); 2]
            }
        })
        .collect();

    let topo: TopologicalGrid = match build_grid_topological(&positions, &axes, &params.topological)
    {
        Ok(g) => g,
        Err(_) => return Vec::new(),
    };
    if topo.components.is_empty() {
        return Vec::new();
    }

    let component_views: Vec<ComponentInput<'_>> = topo
        .components
        .iter()
        .map(|c| ComponentInput {
            labelled: &c.labelled,
            positions: &positions,
        })
        .collect();
    let merged = merge_components_local(&component_views, &params.component_merge);

    // For each surviving component, build a Detection. We need a fresh
    // `CornerAug` slice per component so `build_detection`'s
    // canonicalisation can flag labels via stage updates without
    // bleeding state across components.
    let mut out: Vec<Detection> = Vec::new();
    for labelled in &merged.components {
        if labelled.len() < params.min_labeled_corners {
            continue;
        }
        let cell_size = estimate_cell_size_from_labels(labelled, &positions);
        let (grid_u, grid_v) = estimate_grid_steps(labelled, &positions);
        let centers = cluster_centers_from_grid(grid_u, grid_v);

        let mut augs: Vec<CornerAug> = corners
            .iter()
            .enumerate()
            .map(|(i, c)| CornerAug::from_corner(i, c))
            .collect();
        for (&at, &idx) in labelled.iter() {
            augs[idx].stage = CornerStage::Labeled {
                at,
                local_h_residual_px: None,
            };
        }

        let grow = GrowResult {
            labelled: labelled.clone(),
            by_corner: labelled.iter().map(|(&k, &v)| (v, k)).collect(),
            ambiguous: Default::default(),
            holes: Default::default(),
            grid_u,
            grid_v,
        };

        out.push(build_detection_from_grow(&augs, &grow, centers, cell_size));
    }

    // Sort by labelled count desc so callers see the most populous
    // component first, then cap by `max_components`.
    out.sort_by_key(|d| std::cmp::Reverse(d.target.corners.len()));
    out.truncate(params.max_components.max(1) as usize);
    out
}
