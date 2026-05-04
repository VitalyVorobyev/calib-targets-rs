//! Serializable diagnostics for the topological grid builder.
//!
//! The trace path is intentionally allocation-heavy: it records every corner,
//! classified Delaunay half-edge, merged quad decision, and walked component
//! for overlays and JSON diagnostics. It shares the same stage functions as
//! [`super::build_grid_topological`] so visualizations do not drift from
//! production behavior.

use nalgebra::Point2;
use serde::{Deserialize, Serialize};

use super::{
    classify, delaunay, quads, topo_filter, triangle_class, update_edge_stats,
    update_triangle_stats, usable_mask, walk, AxisHint, EdgeKind, TopologicalComponent,
    TopologicalError, TopologicalParams, TopologicalStats, TriangleClass,
};

/// One corner as seen by the topological pipeline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopologicalCornerTrace {
    pub index: usize,
    pub position: [f32; 2],
    pub axes: [AxisHint; 2],
    pub usable: bool,
}

/// Diagnostic angular distances for one classified half-edge.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TopologicalEdgeMetricTrace {
    /// Largest endpoint angular distance to a usable grid axis.
    pub grid_distance_rad: Option<f32>,
    /// Largest endpoint angular distance to the nearest diagonal direction.
    pub diagonal_distance_rad: Option<f32>,
    /// `axis_align_tol_rad - grid_distance_rad`; positive means the half-edge
    /// passed the grid-angle gate at both endpoints.
    pub grid_margin_rad: Option<f32>,
    /// `diagonal_angle_tol_rad - diagonal_distance_rad`; positive means the
    /// half-edge passed the diagonal-angle gate at both endpoints.
    pub diagonal_margin_rad: Option<f32>,
}

/// One Delaunay triangle plus its classified half-edges.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopologicalTriangleTrace {
    pub index: usize,
    pub vertices: [usize; 3],
    /// Half-edge buddy indices. `None` means convex-hull edge.
    pub halfedges: [Option<usize>; 3],
    pub edge_kinds: [EdgeKind; 3],
    pub edge_metrics: [TopologicalEdgeMetricTrace; 3],
    pub class: TriangleClass,
}

/// One merged quad candidate and the decisions made by the two filters.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopologicalQuadTrace {
    pub index: usize,
    /// Quad vertices in TL, TR, BR, BL order in image coordinates.
    pub vertices: [usize; 4],
    pub illegal_vertices: Vec<usize>,
    pub topology_pass: bool,
    pub geometry_pass: bool,
    pub max_opposing_edge_ratio: f32,
    pub kept: bool,
}

/// One final `(i, j) -> corner_idx` label.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TopologicalLabelTrace {
    pub i: i32,
    pub j: i32,
    pub corner_idx: usize,
}

/// One connected labelled component after topological walking and bbox rebase.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopologicalComponentTrace {
    pub index: usize,
    pub labels: Vec<TopologicalLabelTrace>,
}

/// Full trace of the topological pipeline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TopologicalTrace {
    pub params: TopologicalParams,
    pub corners: Vec<TopologicalCornerTrace>,
    pub triangles: Vec<TopologicalTriangleTrace>,
    pub quads: Vec<TopologicalQuadTrace>,
    pub components: Vec<TopologicalComponentTrace>,
    pub diagnostics: TopologicalStats,
}

fn component_trace(components: &[TopologicalComponent]) -> Vec<TopologicalComponentTrace> {
    components
        .iter()
        .enumerate()
        .map(|(index, component)| {
            let mut labels: Vec<TopologicalLabelTrace> = component
                .labelled
                .iter()
                .map(|(&(i, j), &corner_idx)| TopologicalLabelTrace { i, j, corner_idx })
                .collect();
            labels.sort_by_key(|l| (l.j, l.i, l.corner_idx));
            TopologicalComponentTrace { index, labels }
        })
        .collect()
}

/// Build labelled grid components and return every intermediate stage needed
/// for diagnostics and visualization.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_corners = positions.len()),
    )
)]
pub fn build_grid_topological_trace(
    positions: &[Point2<f32>],
    axes: &[[AxisHint; 2]],
    params: &TopologicalParams,
) -> Result<TopologicalTrace, TopologicalError> {
    if positions.len() != axes.len() {
        return Err(TopologicalError::LengthMismatch {
            positions: positions.len(),
            axes: axes.len(),
        });
    }
    let mut stats = TopologicalStats {
        corners_in: positions.len(),
        ..Default::default()
    };

    let usable_mask = usable_mask(axes, params);
    stats.corners_used = usable_mask.iter().filter(|&&b| b).count();
    if stats.corners_used < 3 {
        return Err(TopologicalError::NotEnoughCorners {
            usable: stats.corners_used,
        });
    }

    let triangulation = delaunay::triangulate(positions);
    stats.triangles = triangulation.triangles.len() / 3;
    let edge_kinds =
        classify::classify_all_edges(positions, axes, &usable_mask, &triangulation, params);
    update_edge_stats(&mut stats, &edge_kinds);
    update_triangle_stats(&mut stats, &edge_kinds);

    let raw_quads = quads::merge_triangle_pairs(&triangulation, &edge_kinds, positions);
    stats.quads_merged = raw_quads.len();

    let quad_decisions = topo_filter::filter_quad_decisions(&raw_quads, positions, params);
    let kept_quads: Vec<_> = quad_decisions
        .iter()
        .filter(|d| d.kept)
        .map(|d| d.quad)
        .collect();
    stats.quads_kept = kept_quads.len();

    let components = walk::label_components(&kept_quads, params.min_quads_per_component);
    stats.components = components.len();

    let corners = positions
        .iter()
        .zip(axes.iter())
        .zip(usable_mask.iter())
        .enumerate()
        .map(|(index, ((p, a), &usable))| TopologicalCornerTrace {
            index,
            position: [p.x, p.y],
            axes: *a,
            usable,
        })
        .collect();

    let triangles = (0..stats.triangles)
        .map(|index| {
            let base = 3 * index;
            let halfedge = |k: usize| {
                let e = triangulation.halfedges[base + k];
                if e == delaunator::EMPTY {
                    None
                } else {
                    Some(e)
                }
            };
            TopologicalTriangleTrace {
                index,
                vertices: [
                    triangulation.triangles[base],
                    triangulation.triangles[base + 1],
                    triangulation.triangles[base + 2],
                ],
                halfedges: [halfedge(0), halfedge(1), halfedge(2)],
                edge_kinds: [edge_kinds[base], edge_kinds[base + 1], edge_kinds[base + 2]],
                edge_metrics: [0, 1, 2].map(|k| {
                    let metric = classify::classify_edge_metric(
                        positions,
                        axes,
                        &usable_mask,
                        &triangulation,
                        base + k,
                        params,
                    );
                    TopologicalEdgeMetricTrace {
                        grid_distance_rad: metric.grid_distance_rad,
                        diagonal_distance_rad: metric.diagonal_distance_rad,
                        grid_margin_rad: metric.grid_margin_rad,
                        diagonal_margin_rad: metric.diagonal_margin_rad,
                    }
                }),
                class: triangle_class(&edge_kinds, index),
            }
        })
        .collect();

    let quads = quad_decisions
        .iter()
        .enumerate()
        .map(|(index, decision)| TopologicalQuadTrace {
            index,
            vertices: decision.quad.vertices,
            illegal_vertices: decision.illegal_vertices.clone(),
            topology_pass: decision.topology_pass,
            geometry_pass: decision.geometry_pass,
            max_opposing_edge_ratio: decision.max_opposing_edge_ratio,
            kept: decision.kept,
        })
        .collect();

    Ok(TopologicalTrace {
        params: *params,
        corners,
        triangles,
        quads,
        components: component_trace(&components),
        diagnostics: stats,
    })
}
