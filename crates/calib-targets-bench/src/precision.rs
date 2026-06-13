//! Baseline-free structural precision audit over a labelled corner set.
//!
//! Ported from the chessboard test helper `audit_wrong_label_edges`
//! (`crates/calib-targets-chessboard/tests/private_130puzzle.rs`). It flags the
//! two wrong-label signatures the topological geometry gate must remove —
//! skipped-corner / diagonal-boundary edges and duplicate-pixel folds — using
//! the per-snap **global** median cardinal-edge length as the reference, so it
//! stays independent of the detector's own local geometry predicate.
//!
//! This lives in the bench crate rather than the library on purpose: lifting it
//! into a reusable library function (and surfacing it as a per-snap product
//! signal) is a separate, deferred algorithm-surface change. Here it is purely
//! a measurement-campaign metric — it never gates detection.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::baseline::BaselineCorner;

/// Baseline-free structural precision signals for one snap's labelled corners.
///
/// Both counters are wrong-label signatures: they should be `0` on a clean
/// grid and grow when the grid builder skips corners (overlong edges) or folds
/// two labels onto one physical corner (collapsed pairs).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StructuralPrecision {
    /// Cardinal edges longer than `1.6×` the per-snap global median
    /// cardinal-edge length — a skipped-corner / diagonal-boundary signature.
    pub overlong_edges: usize,
    /// Pairs of distinct `(i, j)` labels closer than `0.2×` that median in
    /// pixels — the duplicate-pixel fold the topological grid can produce in
    /// defocused bands. Complements
    /// [`crate::diff::BaselineDiff::duplicate_run_positions`], which quantises
    /// at a fixed pixel epsilon rather than scale-relative.
    pub collapsed_pairs: usize,
    /// Per-snap global median cardinal-edge length, pixels. `0.0` when there is
    /// fewer than one cardinal edge. Carried so a comparison table can
    /// sanity-check scale across snaps.
    pub median_edge_px: f32,
}

/// Compute the structural precision audit from a snap's labelled corners.
///
/// Thresholds match the source helper: overlong = edge `> 1.6×` the global
/// median cardinal-edge length; collapsed = a distinct `(i, j)` pair `< 0.2×`
/// that median apart. The collapsed-pair scan is `O(n²)` in the corner count;
/// acceptable because the bench is a measurement tool, not a hot path.
pub fn structural_precision(corners: &[BaselineCorner]) -> StructuralPrecision {
    // (i, j) → pixel position. Mirrors the source helper: a duplicate label
    // (same (i, j)) collapses here, which the chessboard grid contract forbids
    // within a component anyway.
    let by_grid: HashMap<(i32, i32), (f32, f32)> =
        corners.iter().map(|c| ((c.i, c.j), (c.x, c.y))).collect();

    // Cardinal-edge lengths: probe each corner's +i and +j neighbour.
    let mut lens: Vec<f32> = Vec::new();
    for (&(i, j), &(x, y)) in &by_grid {
        for (di, dj) in [(1, 0), (0, 1)] {
            if let Some(&(nx, ny)) = by_grid.get(&(i + di, j + dj)) {
                lens.push(((nx - x).powi(2) + (ny - y).powi(2)).sqrt());
            }
        }
    }
    if lens.is_empty() {
        return StructuralPrecision::default();
    }
    lens.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = lens[lens.len() / 2];
    let overlong_edges = lens.iter().filter(|&&l| l > 1.6 * median).count();

    let pts: Vec<(f32, f32)> = by_grid.values().copied().collect();
    let eps2 = (0.2 * median).powi(2);
    let mut collapsed_pairs = 0usize;
    for (a, &(ax, ay)) in pts.iter().enumerate() {
        for &(bx, by) in &pts[a + 1..] {
            if (ax - bx).powi(2) + (ay - by).powi(2) < eps2 {
                collapsed_pairs += 1;
            }
        }
    }

    StructuralPrecision {
        overlong_edges,
        collapsed_pairs,
        median_edge_px: median,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corner(i: i32, j: i32, x: f32, y: f32) -> BaselineCorner {
        BaselineCorner {
            i,
            j,
            x,
            y,
            id: None,
            score: 0.0,
        }
    }

    #[test]
    fn clean_grid_has_no_violations() {
        // 3×3 grid, 10 px spacing — every cardinal edge is exactly the median.
        let mut corners = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                corners.push(corner(i, j, 10.0 * i as f32, 10.0 * j as f32));
            }
        }
        let p = structural_precision(&corners);
        assert_eq!(p.overlong_edges, 0);
        assert_eq!(p.collapsed_pairs, 0);
        assert_eq!(p.median_edge_px, 10.0);
    }

    #[test]
    fn one_overlong_edge_and_one_collapsed_pair() {
        // Same 3×3 / 10 px grid (12 edges of length 10, keeps median = 10)…
        let mut corners = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                corners.push(corner(i, j, 10.0 * i as f32, 10.0 * j as f32));
            }
        }
        // …plus a (3, 0) corner far from (2, 0): edge (2,0)->(3,0) = 40 px > 1.6×10.
        corners.push(corner(3, 0, 60.0, 0.0));
        // …plus a corner with labels that have no cardinal neighbour in the set
        // (so it adds no edge), sitting ~0.7 px from (1, 0) — a collapsed pair.
        corners.push(corner(10, 10, 10.5, 0.5));

        let p = structural_precision(&corners);
        assert_eq!(p.median_edge_px, 10.0, "median must stay at the 10 px grid");
        assert_eq!(p.overlong_edges, 1, "only the 40 px edge is overlong");
        assert_eq!(
            p.collapsed_pairs, 1,
            "only the (1,0)/(10,10) pair collapses"
        );
    }

    #[test]
    fn empty_input_is_default() {
        assert_eq!(structural_precision(&[]), StructuralPrecision::default());
    }
}
