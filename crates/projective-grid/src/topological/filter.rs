//! Topological + geometric filtering of merged quads.
//!
//! - **Topological filter**: a corner with quad-mesh degree > 4 is illegal
//!   (a regular grid has max degree 4). Quads with two or more illegal
//!   corners are dropped.
//! - **Parallelogram filter**: opposing edges of a quad whose lengths
//!   differ by more than `opposing_edge_ratio_max` indicate an extreme
//!   parallelogram — reject.
//! - **Per-component cell-size filter**: after topology + parallelogram,
//!   compute connected quad-mesh components and their per-component
//!   median edge length; reject quads whose perimeter edges fall outside
//!   `[edge_length_min_rel, edge_length_max_rel] × component_median`.
//!   This catches quads formed across missing corners (long edges) or
//!   across spurious within-cell features (short edges) — failure modes
//!   that the parallelogram test admits when both opposing pairs scale
//!   together. Passing `min_rel = 0.0` expresses an upper-only edge-length
//!   band.

use std::collections::HashMap;

use nalgebra::Point2;

use super::quads::Quad;
use super::walk::{build_edge_index, connected_components};

#[inline]
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
    let eps = 1e-6_f32;
    let safe_min = |a: f32, b: f32| {
        let m = if a < b { a } else { b };
        if m < eps {
            eps
        } else {
            m
        }
    };
    let safe_max = |a: f32, b: f32| if a > b { a } else { b };
    let r1 = safe_max(l01, l23) / safe_min(l01, l23);
    let r2 = safe_max(l12, l30) / safe_min(l12, l30);
    if r1 > r2 {
        r1
    } else {
        r2
    }
}

fn quad_degrees(quads: &[Quad]) -> HashMap<usize, u32> {
    // Per-corner degree in the quad mesh: count incidence over all
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

/// Apply topology + parallelogram + per-component cell-size filters and
/// return the surviving quads in input order.
pub(super) fn filter_quads(
    quads: Vec<Quad>,
    positions: &[Point2<f32>],
    opposing_edge_ratio_max: f32,
    edge_length_min_rel: f32,
    edge_length_max_rel: f32,
) -> Vec<Quad> {
    let degree = quad_degrees(&quads);

    #[cfg(feature = "tracing")]
    let topology_filtered = {
        let _span =
            tracing::debug_span!("topological_quad_filter", num_quads_in = quads.len()).entered();
        apply_topological_quad_filter(quads, &degree)
    };
    #[cfg(not(feature = "tracing"))]
    let topology_filtered = apply_topological_quad_filter(quads, &degree);

    #[cfg(feature = "tracing")]
    let geometry_filtered = {
        let _span = tracing::debug_span!(
            "geometry_quad_filter",
            num_quads_in = topology_filtered.len()
        )
        .entered();
        apply_geometry_quad_filter(topology_filtered, positions, opposing_edge_ratio_max)
    };
    #[cfg(not(feature = "tracing"))]
    let geometry_filtered =
        apply_geometry_quad_filter(topology_filtered, positions, opposing_edge_ratio_max);

    #[cfg(feature = "tracing")]
    {
        let _span = tracing::debug_span!(
            "cell_size_quad_filter",
            num_quads_in = geometry_filtered.len()
        )
        .entered();
        apply_per_component_cell_size_filter(
            geometry_filtered,
            positions,
            edge_length_min_rel,
            edge_length_max_rel,
        )
    }
    #[cfg(not(feature = "tracing"))]
    apply_per_component_cell_size_filter(
        geometry_filtered,
        positions,
        edge_length_min_rel,
        edge_length_max_rel,
    )
}

fn apply_topological_quad_filter(quads: Vec<Quad>, degree: &HashMap<usize, u32>) -> Vec<Quad> {
    quads
        .into_iter()
        .filter(|q| {
            let illegal_count = q
                .vertices
                .iter()
                .copied()
                .filter(|v| degree.get(v).copied().unwrap_or(0) > 8)
                .count();
            illegal_count < 2
        })
        .collect()
}

fn apply_geometry_quad_filter(
    quads: Vec<Quad>,
    positions: &[Point2<f32>],
    opposing_edge_ratio_max: f32,
) -> Vec<Quad> {
    quads
        .into_iter()
        .filter(|q| max_opposing_edge_ratio(q, positions) <= opposing_edge_ratio_max)
        .collect()
}

/// Reject quads whose perimeter edges fall outside
/// `[edge_length_min_rel, edge_length_max_rel] * component_median_edge_length`.
/// Component is the connected quad-mesh component the quad lives in
/// (per-component, not global, so a frame with two boards at different
/// scales doesn't reject one).
///
/// `edge_length_min_rel = 0.0` disables the lower bound; the entire
/// filter is skipped when both bounds are effectively inactive
/// (`min_rel <= 0.0` AND `max_rel` is non-finite).
fn apply_per_component_cell_size_filter(
    quads: Vec<Quad>,
    positions: &[Point2<f32>],
    edge_length_min_rel: f32,
    edge_length_max_rel: f32,
) -> Vec<Quad> {
    if quads.is_empty() {
        return quads;
    }
    // Skip the entire filter when both bounds are disabled.
    let lower_active = edge_length_min_rel > 0.0;
    let upper_active = edge_length_max_rel.is_finite();
    if !lower_active && !upper_active {
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
            if lower_active && lo_e < edge_length_min_rel * median {
                return None;
            }
            if upper_active && hi_e > edge_length_max_rel * median {
                return None;
            }
            Some(q)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32) -> Point2<f32> {
        Point2::new(x, y)
    }

    #[test]
    fn max_opposing_edge_ratio_on_square() {
        let positions = vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)];
        let q = Quad {
            vertices: [0, 1, 2, 3],
        };
        let r = max_opposing_edge_ratio(&q, &positions);
        let eps = 1e-3_f32;
        assert!((r - 1.0).abs() < eps, "ratio {r:?}");
    }

    #[test]
    fn filter_keeps_clean_quad() {
        let positions = vec![pt(0.0, 0.0), pt(1.0, 0.0), pt(1.0, 1.0), pt(0.0, 1.0)];
        let q = Quad {
            vertices: [0, 1, 2, 3],
        };
        let kept = filter_quads(vec![q], &positions, 1.5, 0.4, 2.5);
        assert_eq!(kept.len(), 1);
    }
}
