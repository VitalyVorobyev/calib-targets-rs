//! Topological + geometric filtering of merged quads (paper §4 + Phase D2).
//!
//! - **Topological filter**: a corner with quad-mesh degree > 4 is illegal
//!   (a regular grid has max degree 4). Quads with two or more illegal
//!   corners are dropped.
//! - **Geometric filter (parallelogram)**: opposing edges of a quad whose
//!   lengths differ by more than `edge_ratio_max` indicate an extreme
//!   parallelogram — reject.
//! - **Geometric filter (per-component cell-size)**: after topology +
//!   parallelogram, compute connected quad-mesh components and their
//!   per-component median edge length; reject quads whose perimeter
//!   edges fall outside `[quad_edge_min_rel, quad_edge_max_rel] ×
//!   component_median`. This catches quads formed across missing
//!   corners (long edges) or across spurious within-cell features
//!   (short edges) — failure modes that the parallelogram test admits
//!   when both opposing pairs scale together.

use std::collections::HashMap;

use nalgebra::Point2;

use super::quads::Quad;
use super::walk::{build_edge_index, connected_components};
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
    let mut decisions: Vec<QuadFilterDecision> = quads
        .iter()
        .copied()
        .map(|q| evaluate_quad(q, positions, params, &degree))
        .collect();
    // Apply the per-component cell-size filter on the topology+parallelogram
    // survivors. Decisions for quads that get dropped here have their
    // `kept` field cleared so the trace stays consistent with production.
    let candidate_quads: Vec<Quad> = decisions
        .iter()
        .filter(|d| d.kept)
        .map(|d| d.quad)
        .collect();
    let after_cell_size = apply_per_component_cell_size_filter(candidate_quads, positions, params);
    let surviving: std::collections::HashSet<[usize; 4]> =
        after_cell_size.into_iter().map(|q| q.vertices).collect();
    for d in decisions.iter_mut() {
        if d.kept && !surviving.contains(&d.quad.vertices) {
            d.kept = false;
        }
    }
    decisions
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
    let initial: Vec<Quad> = quads
        .iter()
        .copied()
        .map(|q| evaluate_quad(q, positions, params, &degree))
        .filter(|d| d.kept)
        .map(|d| d.quad)
        .collect();
    apply_per_component_cell_size_filter(initial, positions, params)
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

    let initial: Vec<Quad> = {
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
    };

    let _span =
        tracing::debug_span!("cell_size_quad_filter", num_quads_in = initial.len()).entered();
    apply_per_component_cell_size_filter(initial, positions, params)
}

#[inline]
fn quad_min_max_edge(quad: &Quad, positions: &[Point2<f32>]) -> (f32, f32) {
    let mut lo = f32::INFINITY;
    let mut hi = 0.0_f32;
    for (u, v) in quad.perimeter_edges() {
        let l = edge_len(positions, u, v);
        if l < lo {
            lo = l;
        }
        if l > hi {
            hi = l;
        }
    }
    (lo, hi)
}

/// Reject quads whose perimeter edges fall outside `[min_rel, max_rel] *
/// component_median_edge_length`. Component is the connected
/// quad-mesh component the quad lives in (per-component, not global,
/// so a frame with two boards at different scales doesn't reject one).
///
/// Disabled when both bounds are degenerate (`min_rel <= 0.0` and
/// `max_rel.is_infinite()`).
fn apply_per_component_cell_size_filter(
    quads: Vec<Quad>,
    positions: &[Point2<f32>],
    params: &TopologicalParams,
) -> Vec<Quad> {
    if quads.is_empty() {
        return quads;
    }
    if params.quad_edge_min_rel <= 0.0 && !params.quad_edge_max_rel.is_finite() {
        return quads;
    }
    let edge_index = build_edge_index(&quads);
    let (comp_of, n_comps) = connected_components(&quads, &edge_index);
    let mut comp_edges: Vec<Vec<f32>> = vec![Vec::new(); n_comps as usize];
    for (qi, q) in quads.iter().enumerate() {
        let cid = comp_of[qi] as usize;
        for (u, v) in q.perimeter_edges() {
            comp_edges[cid].push(edge_len(positions, u, v));
        }
    }
    let mut comp_median: Vec<Option<f32>> = Vec::with_capacity(comp_edges.len());
    for v in comp_edges.iter_mut() {
        if v.is_empty() {
            comp_median.push(None);
            continue;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        comp_median.push(Some(v[v.len() / 2]));
    }
    quads
        .into_iter()
        .enumerate()
        .filter_map(|(qi, q)| {
            let median = comp_median[comp_of[qi] as usize]?;
            if median <= 0.0 {
                return Some(q);
            }
            let (lo_e, hi_e) = quad_min_max_edge(&q, positions);
            let lo_band = params.quad_edge_min_rel * median;
            let hi_band = params.quad_edge_max_rel * median;
            if lo_e < lo_band || hi_e > hi_band {
                None
            } else {
                Some(q)
            }
        })
        .collect()
}
