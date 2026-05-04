//! Topological + geometric filtering of merged quads (paper §4).
//!
//! - **Topological filter**: a corner with quad-mesh degree > 4 is illegal
//!   (a regular grid has max degree 4). Quads with two or more illegal
//!   corners are dropped.
//! - **Geometric filter**: opposing edges of a quad whose lengths differ
//!   by more than `edge_ratio_max` indicate an extreme parallelogram —
//!   reject.

use std::collections::HashMap;

use nalgebra::Point2;

use super::quads::Quad;
use super::TopologicalParams;

/// Per-quad filtering decision used by the trace API.
#[derive(Clone, Debug)]
pub(crate) struct QuadFilterDecision {
    pub(crate) quad: Quad,
    pub(crate) illegal_vertices: Vec<usize>,
    pub(crate) topology_pass: bool,
    pub(crate) geometry_pass: bool,
    pub(crate) max_opposing_edge_ratio: f32,
    pub(crate) kept: bool,
}

fn edge_len(positions: &[Point2<f32>], u: usize, v: usize) -> f32 {
    let pu = positions[u];
    let pv = positions[v];
    ((pv.x - pu.x).powi(2) + (pv.y - pu.y).powi(2)).sqrt()
}

fn max_opposing_edge_ratio(quad: &Quad, positions: &[Point2<f32>]) -> f32 {
    let v = quad.vertices;
    let l01 = edge_len(positions, v[0], v[1]);
    let l12 = edge_len(positions, v[1], v[2]);
    let l23 = edge_len(positions, v[2], v[3]);
    let l30 = edge_len(positions, v[3], v[0]);
    let r1 = l01.max(l23) / l01.min(l23).max(1e-6);
    let r2 = l12.max(l30) / l12.min(l30).max(1e-6);
    r1.max(r2)
}

fn quad_degrees(quads: &[Quad]) -> HashMap<usize, u32> {
    // Compute per-corner degree in the quad mesh: count incidence over all
    // perimeter edges. Each quad contributes 4 edges; an edge incidence
    // bumps both endpoints by 1.
    let mut degree: HashMap<usize, u32> = HashMap::new();
    for q in quads {
        for (u, v) in q.perimeter_edges() {
            *degree.entry(u).or_default() += 1;
            *degree.entry(v).or_default() += 1;
        }
    }
    degree
}

fn evaluate_quad(
    quad: Quad,
    positions: &[Point2<f32>],
    params: &TopologicalParams,
    degree: &HashMap<usize, u32>,
) -> QuadFilterDecision {
    // Topological: drop if ≥ 2 illegal corners (degree > 4).
    // We measure perimeter incidence above, but the paper's notion of
    // "edge-degree" is the number of distinct quad-mesh edges meeting at the
    // corner. Counting unique perimeter endpoints across all quads is
    // equivalent up to a factor of 2 (each shared edge appears in two quads).
    // A regular grid interior corner has 4 incident edges → 8 incidences. We
    // conservatively flag corners with > 8 incidences.
    let illegal_vertices: Vec<usize> = quad
        .vertices
        .iter()
        .copied()
        .filter(|v| degree.get(v).copied().unwrap_or(0) > 8)
        .collect();
    let topology_pass = illegal_vertices.len() < 2;
    let max_opposing_edge_ratio = max_opposing_edge_ratio(&quad, positions);
    let geometry_pass = max_opposing_edge_ratio <= params.edge_ratio_max;
    let kept = topology_pass && geometry_pass;
    QuadFilterDecision {
        quad,
        illegal_vertices,
        topology_pass,
        geometry_pass,
        max_opposing_edge_ratio,
        kept,
    }
}

#[cfg(feature = "tracing")]
fn illegal_vertices_for_quad(quad: &Quad, degree: &HashMap<usize, u32>) -> Vec<usize> {
    quad.vertices
        .iter()
        .copied()
        .filter(|v| degree.get(v).copied().unwrap_or(0) > 8)
        .collect()
}

/// Apply topological + geometric filtering and return one decision per input quad.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_quads_in = quads.len()),
    )
)]
pub(crate) fn filter_quad_decisions(
    quads: &[Quad],
    positions: &[Point2<f32>],
    params: &TopologicalParams,
) -> Vec<QuadFilterDecision> {
    let degree = quad_degrees(quads);
    quads
        .iter()
        .copied()
        .map(|q| evaluate_quad(q, positions, params, &degree))
        .collect()
}

/// Apply topological + geometric filtering and return the surviving quads.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_quads_in = quads.len()),
    )
)]
#[cfg(not(feature = "tracing"))]
pub(crate) fn filter_quads(
    quads: &[Quad],
    positions: &[Point2<f32>],
    params: &TopologicalParams,
) -> Vec<Quad> {
    let degree = quad_degrees(quads);
    quads
        .iter()
        .copied()
        .map(|q| evaluate_quad(q, positions, params, &degree))
        .filter(|d| d.kept)
        .map(|d| d.quad)
        .collect()
}

/// Apply filtering with separate tracing spans for the topological and
/// geometric decisions. The extra allocation is compiled only for the
/// diagnostic tracing feature.
#[cfg(feature = "tracing")]
pub(crate) fn filter_quads(
    quads: &[Quad],
    positions: &[Point2<f32>],
    params: &TopologicalParams,
) -> Vec<Quad> {
    let topology = {
        let _span =
            tracing::debug_span!("topological_quad_filter", num_quads_in = quads.len()).entered();
        let degree = quad_degrees(quads);
        quads
            .iter()
            .copied()
            .map(|quad| {
                let illegal_vertices = illegal_vertices_for_quad(&quad, &degree);
                let topology_pass = illegal_vertices.len() < 2;
                (quad, topology_pass)
            })
            .collect::<Vec<_>>()
    };

    let _span =
        tracing::debug_span!("geometry_quad_filter", num_quads_in = topology.len()).entered();
    topology
        .into_iter()
        .filter_map(|(quad, topology_pass)| {
            let max_opposing_edge_ratio = max_opposing_edge_ratio(&quad, positions);
            let geometry_pass = max_opposing_edge_ratio <= params.edge_ratio_max;
            (topology_pass && geometry_pass).then_some(quad)
        })
        .collect()
}
