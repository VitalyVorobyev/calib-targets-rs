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

fn edge_len(positions: &[Point2<f32>], u: usize, v: usize) -> f32 {
    let pu = positions[u];
    let pv = positions[v];
    ((pv.x - pu.x).powi(2) + (pv.y - pu.y).powi(2)).sqrt()
}

fn passes_geometric(quad: &Quad, positions: &[Point2<f32>], params: &TopologicalParams) -> bool {
    let v = quad.vertices;
    let l01 = edge_len(positions, v[0], v[1]);
    let l12 = edge_len(positions, v[1], v[2]);
    let l23 = edge_len(positions, v[2], v[3]);
    let l30 = edge_len(positions, v[3], v[0]);
    let r1 = l01.max(l23) / l01.min(l23).max(1e-6);
    let r2 = l12.max(l30) / l12.min(l30).max(1e-6);
    r1.max(r2) <= params.edge_ratio_max
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
pub(crate) fn filter_quads(
    quads: &[Quad],
    positions: &[Point2<f32>],
    params: &TopologicalParams,
) -> Vec<Quad> {
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

    quads
        .iter()
        .copied()
        .filter(|q| {
            // Topological: drop if ≥ 2 illegal corners (degree > 4).
            // We measure perimeter incidence above, but the paper's
            // notion of "edge-degree" is the number of distinct quad-mesh
            // edges meeting at the corner. Counting unique perimeter
            // endpoints across all quads is equivalent up to a factor of
            // 2 (each shared edge appears in two quads). A regular grid
            // interior corner has 4 incident edges → 8 incidences. We
            // conservatively flag corners with > 8 incidences.
            let illegal = q
                .vertices
                .iter()
                .filter(|v| degree.get(v).copied().unwrap_or(0) > 8)
                .count();
            illegal < 2
        })
        .filter(|q| passes_geometric(q, positions, params))
        .collect()
}
