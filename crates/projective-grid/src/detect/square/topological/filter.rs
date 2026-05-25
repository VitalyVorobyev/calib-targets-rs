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
use crate::float::{lit, Float};

#[inline]
fn edge_len<F: Float>(positions: &[Point2<F>], u: usize, v: usize) -> F {
    let pu = positions[u];
    let pv = positions[v];
    ((pv.x - pu.x).powi(2) + (pv.y - pu.y).powi(2)).sqrt()
}

fn max_opposing_edge_ratio<F: Float>(quad: &Quad, positions: &[Point2<F>]) -> F {
    let v = quad.vertices;
    let l01 = edge_len(positions, v[0], v[1]);
    let l12 = edge_len(positions, v[1], v[2]);
    let l23 = edge_len(positions, v[2], v[3]);
    let l30 = edge_len(positions, v[3], v[0]);
    let eps = lit::<F>(1e-6_f32);
    let safe_min = |a: F, b: F| {
        let m = if a < b { a } else { b };
        if m < eps {
            eps
        } else {
            m
        }
    };
    let safe_max = |a: F, b: F| if a > b { a } else { b };
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
fn quad_min_max_edge<F: Float>(quad: &Quad, positions: &[Point2<F>]) -> (F, F) {
    let inf = F::max_value().unwrap_or_else(|| lit::<F>(1e30_f32));
    let mut lo = inf;
    let mut hi = F::zero();
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
pub(super) fn filter_quads<F: Float>(
    quads: Vec<Quad>,
    positions: &[Point2<F>],
    opposing_edge_ratio_max: F,
    edge_length_min_rel: F,
    edge_length_max_rel: F,
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

fn apply_geometry_quad_filter<F: Float>(
    quads: Vec<Quad>,
    positions: &[Point2<F>],
    opposing_edge_ratio_max: F,
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
fn apply_per_component_cell_size_filter<F: Float>(
    quads: Vec<Quad>,
    positions: &[Point2<F>],
    edge_length_min_rel: F,
    edge_length_max_rel: F,
) -> Vec<Quad> {
    if quads.is_empty() {
        return quads;
    }
    // Skip the entire filter when both bounds are disabled.
    let lower_active = edge_length_min_rel > F::zero();
    let upper_active = edge_length_max_rel.is_finite();
    if !lower_active && !upper_active {
        return quads;
    }
    let edge_index = build_edge_index(&quads);
    let (comp_of, n_comps) = connected_components(&quads, &edge_index);
    let mut comp_edges: Vec<Vec<F>> = vec![Vec::new(); n_comps as usize];
    for (qi, q) in quads.iter().enumerate() {
        let cid = comp_of[qi] as usize;
        for (u, v) in q.perimeter_edges() {
            comp_edges[cid].push(edge_len(positions, u, v));
        }
    }
    let mut comp_median: Vec<Option<F>> = Vec::with_capacity(comp_edges.len());
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
            if median <= F::zero() {
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
    use crate::float::lit;

    fn pt<F: Float>(x: f32, y: f32) -> Point2<F> {
        Point2::new(lit::<F>(x), lit::<F>(y))
    }

    fn assert_max_opposing_edge_ratio_on_square<F: Float>() {
        let positions = vec![
            pt::<F>(0.0, 0.0),
            pt::<F>(1.0, 0.0),
            pt::<F>(1.0, 1.0),
            pt::<F>(0.0, 1.0),
        ];
        let q = Quad {
            vertices: [0, 1, 2, 3],
        };
        let r = max_opposing_edge_ratio(&q, &positions);
        let one = F::one();
        let eps = lit::<F>(1e-3_f32);
        assert!(crate::float::abs::<F>(r - one) < eps, "ratio {r:?}");
    }

    fn assert_filter_keeps_clean_quad<F: Float>() {
        let positions = vec![
            pt::<F>(0.0, 0.0),
            pt::<F>(1.0, 0.0),
            pt::<F>(1.0, 1.0),
            pt::<F>(0.0, 1.0),
        ];
        let q = Quad {
            vertices: [0, 1, 2, 3],
        };
        let kept = filter_quads(
            vec![q],
            &positions,
            lit::<F>(1.5_f32),
            lit::<F>(0.4_f32),
            lit::<F>(2.5_f32),
        );
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn max_opposing_edge_ratio_on_square_f32() {
        assert_max_opposing_edge_ratio_on_square::<f32>();
    }
    #[test]
    fn max_opposing_edge_ratio_on_square_f64() {
        assert_max_opposing_edge_ratio_on_square::<f64>();
    }
    #[test]
    fn filter_keeps_clean_quad_f32() {
        assert_filter_keeps_clean_quad::<f32>();
    }
    #[test]
    fn filter_keeps_clean_quad_f64() {
        assert_filter_keeps_clean_quad::<f64>();
    }
}
