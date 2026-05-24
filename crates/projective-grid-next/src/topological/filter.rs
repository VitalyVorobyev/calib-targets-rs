//! Topological + geometric filtering of merged quads.
//!
//! - **Topological filter**: a corner with quad-mesh degree > 4 is illegal
//!   (a regular grid has max degree 4). Quads with two or more illegal
//!   corners are dropped.
//! - **Geometric filter (parallelogram)**: opposing edges of a quad whose
//!   lengths differ by more than `opposing_edge_ratio_max` indicate an
//!   extreme parallelogram — reject.
//! - **Geometric filter (per-component cell-size)**: after topology +
//!   parallelogram, compute connected quad-mesh components and their
//!   per-component median edge length; reject quads whose perimeter
//!   edges fall outside `[1.0 / edge_length_ratio_max,
//!   edge_length_ratio_max] × component_median`. This catches quads
//!   formed across missing corners (long edges) or across spurious
//!   within-cell features (short edges) — failure modes that the
//!   parallelogram test admits when both opposing pairs scale together.
//!
//! Renamed from the legacy `topo_filter.rs` per the Phase 3 brief; the
//! algorithm is identical, ported to `F: Float`.

use std::collections::{HashMap, HashSet};

use nalgebra::Point2;

use super::quads::Quad;
use super::walk::{build_edge_index, connected_components};
use crate::diagnostics::events::QuadRejectReason;
use crate::float::{lit, Float};

/// Per-quad filtering decision used to feed the event stream.
///
/// The legacy crate also kept the measured `max_opposing_edge_ratio` on
/// the decision for trace overlays. In the new design that metric is
/// recoverable from `Quad.vertices + positions`, so we omit it here to
/// keep the struct minimal; the bench harness (Phase 6d) recomputes the
/// ratio when needed.
#[derive(Clone, Debug)]
pub(crate) struct QuadFilterDecision<F: Float> {
    pub(crate) quad: Quad,
    /// `None` when kept; `Some(reason)` when dropped, carrying the first
    /// rejection reason the filter cascade reported.
    pub(crate) rejection: Option<QuadRejectReason>,
    pub(crate) kept: bool,
    pub(crate) _phantom: std::marker::PhantomData<F>,
}

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

fn evaluate_quad<F: Float>(
    quad: Quad,
    positions: &[Point2<F>],
    opposing_edge_ratio_max: F,
    degree: &HashMap<usize, u32>,
) -> QuadFilterDecision<F> {
    // Topological: drop if ≥ 2 illegal corners (degree > 4).
    // We measure perimeter incidence above, but the paper's notion of
    // "edge-degree" is the number of distinct quad-mesh edges meeting at the
    // corner. Counting unique perimeter endpoints across all quads is
    // equivalent up to a factor of 2 (each shared edge appears in two quads).
    // A regular grid interior corner has 4 incident edges → 8 incidences. We
    // conservatively flag corners with > 8 incidences.
    let illegal_count = quad
        .vertices
        .iter()
        .copied()
        .filter(|v| degree.get(v).copied().unwrap_or(0) > 8)
        .count();
    let topology_pass = illegal_count < 2;
    let max_ratio = max_opposing_edge_ratio(&quad, positions);
    let geometry_pass = max_ratio <= opposing_edge_ratio_max;
    let rejection = if !topology_pass {
        Some(QuadRejectReason::Topology)
    } else if !geometry_pass {
        Some(QuadRejectReason::OpposingEdgeRatio)
    } else {
        None
    };
    let _ = max_ratio;
    QuadFilterDecision {
        quad,
        rejection,
        kept: rejection.is_none(),
        _phantom: std::marker::PhantomData,
    }
}

/// Apply topology + parallelogram + per-component cell-size filters and
/// return one decision per input quad. Used by the orchestrator to emit
/// per-quad `TopologicalQuad` events.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_quads_in = quads.len()),
    )
)]
pub(crate) fn filter_quad_decisions<F: Float>(
    quads: &[Quad],
    positions: &[Point2<F>],
    opposing_edge_ratio_max: F,
    edge_length_ratio_max: F,
) -> Vec<QuadFilterDecision<F>> {
    let degree = quad_degrees(quads);
    let mut decisions: Vec<QuadFilterDecision<F>> = quads
        .iter()
        .copied()
        .map(|q| evaluate_quad(q, positions, opposing_edge_ratio_max, &degree))
        .collect();
    let candidate_quads: Vec<Quad> = decisions
        .iter()
        .filter(|d| d.kept)
        .map(|d| d.quad)
        .collect();
    let after_cell_size =
        apply_per_component_cell_size_filter(candidate_quads, positions, edge_length_ratio_max);
    let surviving: HashSet<[usize; 4]> = after_cell_size.into_iter().map(|q| q.vertices).collect();
    for d in decisions.iter_mut() {
        if d.kept && !surviving.contains(&d.quad.vertices) {
            d.kept = false;
            d.rejection = Some(QuadRejectReason::EdgeLengthBounds);
        }
    }
    decisions
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

/// Reject quads whose perimeter edges fall outside
/// `[1.0 / edge_length_ratio_max, edge_length_ratio_max] *
/// component_median_edge_length`. Component is the connected
/// quad-mesh component the quad lives in (per-component, not global,
/// so a frame with two boards at different scales doesn't reject one).
///
/// Disabled when `edge_length_ratio_max` is non-finite (`+inf`).
fn apply_per_component_cell_size_filter<F: Float>(
    quads: Vec<Quad>,
    positions: &[Point2<F>],
    edge_length_ratio_max: F,
) -> Vec<Quad> {
    if quads.is_empty() {
        return quads;
    }
    if !edge_length_ratio_max.is_finite() {
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
    let lo_rel = F::one() / edge_length_ratio_max;
    let hi_rel = edge_length_ratio_max;
    quads
        .into_iter()
        .enumerate()
        .filter_map(|(qi, q)| {
            let median = comp_median[comp_of[qi] as usize]?;
            if median <= F::zero() {
                return Some(q);
            }
            let (lo_e, hi_e) = quad_min_max_edge(&q, positions);
            let lo_band = lo_rel * median;
            let hi_band = hi_rel * median;
            if lo_e < lo_band || hi_e > hi_band {
                None
            } else {
                Some(q)
            }
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
        // Unit square — opposing edges are equal.
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
        // r should be 1.0 (within fp precision).
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
        let decisions =
            filter_quad_decisions(&[q], &positions, lit::<F>(1.5_f32), lit::<F>(2.5_f32));
        assert_eq!(decisions.len(), 1);
        assert!(decisions[0].kept);
        assert!(decisions[0].rejection.is_none());
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
