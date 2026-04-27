//! Generic 2×2 seed-quad finder.
//!
//! The detector's first non-local commitment is a *seed quad*: four
//! labelled corners at `(0, 0), (1, 0), (0, 1), (1, 1)` whose edge
//! lengths match each other within tolerance, whose angles match the
//! corners' own grid axes, and whose geometry is internally consistent
//! (no intermediate real corner sits between two seed corners — that
//! would mean the quad has skipped a row / column of the true grid).
//!
//! This module implements the **pattern-agnostic** half of that finder
//! — KD-tree neighbour search, axis-classification of chord directions,
//! parallelogram completion, edge-ratio match. Pattern-specific gates
//! (parity, axis-slot swap, marker rules, midpoint-violation rules)
//! plug in via the [`SeedQuadValidator`] trait. The chessboard detector
//! is the reference consumer.
//!
//! # Algorithm
//!
//! 1. The validator enumerates `A` candidates (sorted by quality —
//!    e.g., chessboard strength) and `BC` candidates (anything that
//!    can serve as B or C — chessboard's "Swapped" cluster).
//! 2. For each `A`: KD-tree-search the `BC` set for `K_BC` nearest
//!    neighbours, classify each by angular distance from `A.axes[0]` /
//!    `A.axes[1]` (closer to axes-0 → `B_cands`; closer to axes-1 →
//!    `C_cands`), enumerate `(B, C)` pairs among the shortest few in
//!    each list, predict `D = A + (B − A) + (C − A)` (parallelogram
//!    completion), find the nearest `A`-class candidate to `D` within
//!    `close_tol_rel × avg_edge`, verify all four edges agree pairwise
//!    on length within ratio tolerance, verify the validator's
//!    per-edge gate on every edge, and reject the quad if its
//!    midpoints / center carry a pattern-specific intermediate-corner
//!    violation.
//! 3. First quad passing every gate wins.

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};

use crate::circular_stats::{angular_dist_pi, wrap_pi};
use crate::topological::AxisHint;

use super::{Seed, SeedOutput};

/// Pattern-specific hooks used by [`find_quad`].
///
/// Implementations supply per-corner geometry (`position`, `axes`),
/// the eligibility split between A/D candidates and B/C candidates,
/// the per-edge invariant, and the midpoint-violation check.
pub trait SeedQuadValidator {
    /// Per-corner pixel position. Indices are stable across the call.
    fn position(&self, idx: usize) -> Point2<f32>;

    /// Per-corner two grid-axis directions. The finder uses only the
    /// `angle` field; `sigma` is available for pattern-specific gates
    /// but ignored by the generic find-quad logic.  The finder folds
    /// each angle into `[0, π)` before angular distance calculations,
    /// so callers may supply angles in any range.
    ///
    /// Use [`AxisHint::from_angle`] when you do not track per-axis
    /// uncertainty.
    fn axes(&self, idx: usize) -> [AxisHint; 2];

    /// Indices eligible to act as the seed's `A` (and `D`) corners,
    /// sorted in **descending preference order** — the finder
    /// enumerates them in order and returns the first quad passing
    /// every gate.
    fn a_candidates(&self) -> Vec<usize>;

    /// Indices eligible to act as the seed's `B` or `C` corners.
    /// Order is irrelevant (the KD-tree handles spatial search).
    fn bc_candidates(&self) -> Vec<usize>;

    /// True iff the directed edge `from → to` satisfies the pattern's
    /// per-edge invariant. For chessboard: axis-slot swap (the chord
    /// matches one slot at `from` and the other slot at `to`).
    ///
    /// Default: accept every edge — usable for patterns whose only
    /// constraint is geometric consistency.
    fn edge_ok(&self, _from: usize, _to: usize, _axis_tol_rad: f32) -> bool {
        true
    }

    /// True iff the seed quad has a pattern-specific midpoint /
    /// parallelogram-center violation that signals a 2×-spacing
    /// mislabel — e.g., on chessboard, a Swapped corner near an edge
    /// midpoint or a Canonical corner near the quad center.
    ///
    /// Default: never violate — usable for patterns with no
    /// intermediate-corner expectation.
    fn has_midpoint_violation(&self, _seed: Seed, _cell_size: f32) -> bool {
        false
    }
}

/// Tuning knobs for [`find_quad`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct SeedQuadParams {
    /// Angular tolerance (radians) on `axis_dist(chord, A.axes[k])`.
    /// Chord directions outside this tolerance from both axes are
    /// rejected.
    pub axis_tol_rad: f32,
    /// Edge-length ratio tolerance: `min_edge / max_edge ≥ 1 −
    /// edge_ratio_tol` for all four seed edges. Rejects quads whose
    /// edges differ in length by more than this fraction.
    pub edge_ratio_tol: f32,
    /// Search radius for `D` candidates around the predicted parallelo-
    /// gram corner, expressed as a fraction of the seed's mean
    /// `(|AB| + |AC|) / 2`.
    pub close_tol_rel: f32,
    /// `K` in the KD-tree query for B/C candidates. 32 is enough to
    /// capture board neighbours at unknown lattice scale.
    pub k_bc: usize,
    /// Per-axis cap on enumerated `B` / `C` candidates when running
    /// the inner pair search.
    pub top_per_axis: usize,
}

impl Default for SeedQuadParams {
    fn default() -> Self {
        Self {
            axis_tol_rad: 15.0_f32.to_radians(),
            edge_ratio_tol: 0.30,
            close_tol_rel: 0.30,
            k_bc: 32,
            top_per_axis: 6,
        }
    }
}

impl SeedQuadParams {
    /// Construct fully-specified params from the four caller-tunable
    /// values. The struct is `#[non_exhaustive]` (additional knobs may
    /// land in later releases) so this is the supported way to build
    /// one outside the crate.
    pub fn new(axis_tol_rad: f32, edge_ratio_tol: f32, close_tol_rel: f32) -> Self {
        Self {
            axis_tol_rad,
            edge_ratio_tol,
            close_tol_rel,
            ..Self::default()
        }
    }
}

/// Run the generic seed-quad search.
///
/// Returns the first quad — `(A, B, C, D)` plus mean edge length as the
/// `cell_size` — that passes every pattern-agnostic geometric check
/// AND every validator-supplied gate. Returns `None` when no quad
/// satisfies all constraints.
pub fn find_quad<V: SeedQuadValidator>(
    validator: &V,
    params: &SeedQuadParams,
) -> Option<SeedOutput> {
    let a_indices = validator.a_candidates();
    let bc_indices = validator.bc_candidates();
    if a_indices.is_empty() || bc_indices.is_empty() {
        return None;
    }

    let min_ratio = 1.0 - params.edge_ratio_tol;
    let max_ratio = 1.0 + params.edge_ratio_tol;
    let ratio_floor = min_ratio / max_ratio;

    // KD-trees over A and BC candidates.
    let mut a_tree: KdTree<f32, 2> = KdTree::new();
    for (slot, &idx) in a_indices.iter().enumerate() {
        let p = validator.position(idx);
        a_tree.add(&[p.x, p.y], slot as u64);
    }
    let mut bc_tree: KdTree<f32, 2> = KdTree::new();
    for (slot, &idx) in bc_indices.iter().enumerate() {
        let p = validator.position(idx);
        bc_tree.add(&[p.x, p.y], slot as u64);
    }

    for &a_idx in &a_indices {
        let a_pos = validator.position(a_idx);
        let a_axes = validator.axes(a_idx);
        let a_axis0 = wrap_pi(a_axes[0].angle);
        let a_axis1 = wrap_pi(a_axes[1].angle);

        // Nearest BC neighbours (sorted asc by distance).
        let mut neighbors: Vec<(usize, f32, Vector2<f32>)> = bc_tree
            .nearest_n::<SquaredEuclidean>(&[a_pos.x, a_pos.y], params.k_bc)
            .into_iter()
            .map(|nn| {
                let slot = nn.item as usize;
                let idx = bc_indices[slot];
                let p = validator.position(idx);
                let off = Vector2::new(p.x - a_pos.x, p.y - a_pos.y);
                (idx, nn.distance.sqrt(), off)
            })
            .filter(|(_, d, _)| d.is_finite() && *d > 1e-3)
            .collect();
        neighbors.sort_by(|a, b| a.1.total_cmp(&b.1));
        if neighbors.len() < 2 {
            continue;
        }

        // Classify each by which axis the chord aligns with.
        let mut b_cands: Vec<(usize, f32, Vector2<f32>)> = Vec::new();
        let mut c_cands: Vec<(usize, f32, Vector2<f32>)> = Vec::new();
        for (idx, dist, off) in &neighbors {
            let ang = wrap_pi(off.y.atan2(off.x));
            let d0 = angular_dist_pi(ang, a_axis0);
            let d1 = angular_dist_pi(ang, a_axis1);
            if d0 <= params.axis_tol_rad && d0 < d1 {
                b_cands.push((*idx, *dist, *off));
            } else if d1 <= params.axis_tol_rad && d1 < d0 {
                c_cands.push((*idx, *dist, *off));
            }
        }
        if b_cands.is_empty() || c_cands.is_empty() {
            continue;
        }

        // Enumerate (B, C) pairs among the shortest candidates.
        for (b_idx, b_dist, b_off) in b_cands.iter().take(params.top_per_axis) {
            for (c_idx, c_dist, c_off) in c_cands.iter().take(params.top_per_axis) {
                if b_idx == c_idx {
                    continue;
                }
                let ab = *b_dist;
                let ac = *c_dist;
                if ab.min(ac) / ab.max(ac) < ratio_floor {
                    continue;
                }

                // Predict D and search for the nearest A-class corner.
                let pred = a_pos + b_off + c_off;
                let avg_edge = (ab + ac) * 0.5;
                let close_px_sq = (params.close_tol_rel * avg_edge).powi(2);

                let mut best: Option<(usize, f32)> = None;
                for nn in a_tree
                    .within_unsorted::<SquaredEuclidean>(&[pred.x, pred.y], close_px_sq)
                    .into_iter()
                {
                    let slot = nn.item as usize;
                    let d_idx = a_indices[slot];
                    if d_idx == a_idx {
                        continue;
                    }
                    let d = nn.distance.sqrt();
                    if best.map(|b| d < b.1).unwrap_or(true) {
                        best = Some((d_idx, d));
                    }
                }
                let Some((d_idx, _gap)) = best else { continue };

                // All 4 edges must match pairwise within the ratio tolerance.
                let bd = (validator.position(d_idx) - validator.position(*b_idx)).norm();
                let cd = (validator.position(d_idx) - validator.position(*c_idx)).norm();
                let all = [ab, ac, bd, cd];
                let emin = all.iter().copied().fold(f32::INFINITY, f32::min);
                let emax = all.iter().copied().fold(0.0_f32, f32::max);
                if emax <= 0.0 || emin / emax < ratio_floor {
                    continue;
                }

                // Per-edge validator gate (chessboard: axis-slot swap).
                if !validator.edge_ok(a_idx, *b_idx, params.axis_tol_rad)
                    || !validator.edge_ok(a_idx, *c_idx, params.axis_tol_rad)
                    || !validator.edge_ok(*b_idx, d_idx, params.axis_tol_rad)
                    || !validator.edge_ok(*c_idx, d_idx, params.axis_tol_rad)
                {
                    continue;
                }

                let cell_size = (ab + ac + bd + cd) * 0.25;
                let seed = Seed {
                    a: a_idx,
                    b: *b_idx,
                    c: *c_idx,
                    d: d_idx,
                };

                if validator.has_midpoint_violation(seed, cell_size) {
                    continue;
                }

                return Some(SeedOutput { seed, cell_size });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Validator over a flat array of `(position, axes)` pairs. A and BC
    /// indices come from a parity bitmask the test fixture supplies.
    struct ToyValidator<'a> {
        positions: &'a [Point2<f32>],
        axes: &'a [[AxisHint; 2]],
        is_a: Vec<bool>,
    }

    impl<'a> SeedQuadValidator for ToyValidator<'a> {
        fn position(&self, idx: usize) -> Point2<f32> {
            self.positions[idx]
        }
        fn axes(&self, idx: usize) -> [AxisHint; 2] {
            self.axes[idx]
        }
        fn a_candidates(&self) -> Vec<usize> {
            (0..self.is_a.len()).filter(|&i| self.is_a[i]).collect()
        }
        fn bc_candidates(&self) -> Vec<usize> {
            (0..self.is_a.len()).filter(|&i| !self.is_a[i]).collect()
        }
    }

    fn checkerboard(rows: i32, cols: i32, s: f32) -> ToyValidator<'static> {
        // Build into Box::leak to keep lifetime simple in tests.
        let n = (rows * cols) as usize;
        let mut positions = Vec::with_capacity(n);
        let mut axes = Vec::with_capacity(n);
        let mut is_a = Vec::with_capacity(n);
        for j in 0..rows {
            for i in 0..cols {
                positions.push(Point2::new(i as f32 * s + 50.0, j as f32 * s + 50.0));
                axes.push([
                    AxisHint::from_angle(0.0_f32),
                    AxisHint::from_angle(std::f32::consts::FRAC_PI_2),
                ]);
                is_a.push((i + j).rem_euclid(2) == 0);
            }
        }
        ToyValidator {
            positions: Box::leak(positions.into_boxed_slice()),
            axes: Box::leak(axes.into_boxed_slice()),
            is_a,
        }
    }

    #[test]
    fn finds_quad_on_clean_grid() {
        let v = checkerboard(5, 5, 20.0);
        let out = find_quad(&v, &SeedQuadParams::default()).expect("seed");
        assert!((out.cell_size - 20.0).abs() < 0.5);
        // Verify A and D are A-class, B and C are BC-class.
        assert!(v.is_a[out.seed.a]);
        assert!(!v.is_a[out.seed.b]);
        assert!(!v.is_a[out.seed.c]);
        assert!(v.is_a[out.seed.d]);
    }

    #[test]
    fn returns_none_when_one_class_is_empty() {
        let axes_arr: &[[AxisHint; 2]] = &[
            [
                AxisHint::from_angle(0.0),
                AxisHint::from_angle(std::f32::consts::FRAC_PI_2),
            ],
            [
                AxisHint::from_angle(0.0),
                AxisHint::from_angle(std::f32::consts::FRAC_PI_2),
            ],
        ];
        let v = ToyValidator {
            positions: &[Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)],
            axes: axes_arr,
            is_a: vec![true, true],
        };
        assert!(find_quad(&v, &SeedQuadParams::default()).is_none());
    }

    #[test]
    fn rejects_seeds_that_violate_edge_ok() {
        // Validator that vetoes EVERY edge — no quad should pass.
        struct AlwaysReject<'a>(&'a ToyValidator<'a>);
        impl<'a> SeedQuadValidator for AlwaysReject<'a> {
            fn position(&self, idx: usize) -> Point2<f32> {
                self.0.position(idx)
            }
            fn axes(&self, idx: usize) -> [AxisHint; 2] {
                self.0.axes(idx)
            }
            fn a_candidates(&self) -> Vec<usize> {
                self.0.a_candidates()
            }
            fn bc_candidates(&self) -> Vec<usize> {
                self.0.bc_candidates()
            }
            fn edge_ok(&self, _: usize, _: usize, _: f32) -> bool {
                false
            }
        }
        let inner = checkerboard(5, 5, 20.0);
        let outer = AlwaysReject(&inner);
        assert!(find_quad(&outer, &SeedQuadParams::default()).is_none());
    }

    #[test]
    fn rejects_seeds_with_midpoint_violation() {
        struct MidpointViolator<'a>(&'a ToyValidator<'a>);
        impl<'a> SeedQuadValidator for MidpointViolator<'a> {
            fn position(&self, idx: usize) -> Point2<f32> {
                self.0.position(idx)
            }
            fn axes(&self, idx: usize) -> [AxisHint; 2] {
                self.0.axes(idx)
            }
            fn a_candidates(&self) -> Vec<usize> {
                self.0.a_candidates()
            }
            fn bc_candidates(&self) -> Vec<usize> {
                self.0.bc_candidates()
            }
            fn has_midpoint_violation(&self, _: Seed, _: f32) -> bool {
                true
            }
        }
        let inner = checkerboard(5, 5, 20.0);
        let outer = MidpointViolator(&inner);
        assert!(find_quad(&outer, &SeedQuadParams::default()).is_none());
    }
}
