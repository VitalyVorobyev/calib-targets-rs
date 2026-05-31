//! Local lattice-orientation synthesis from point positions alone.
//!
//! Both square strategies consume [`OrientedFeature<2>`] — each corner carries
//! two local grid directions. When the caller has no per-corner orientation
//! (`Evidence::Positions`: a dot grid, a circle grid, or a chessboard whose
//! corners carry no axis estimate), this module recovers those two directions
//! geometrically so the existing seed-and-grow / topological machinery runs
//! unchanged.
//!
//! # The perspective problem
//!
//! The grid is viewed in perspective. The two grid directions are therefore
//! **not** orthogonal in the image, and the angle between them **varies across
//! the image** (the grid-line families converge toward two vanishing points).
//! Any method that assumes a fixed 90° between the axes, or a single global
//! orientation, is wrong. The estimate must be *local* and must not constrain
//! the inter-axis angle.
//!
//! # What is perspective-invariant
//!
//! A projective map preserves straight lines, so three collinear grid points
//! `(i−1, j), (i, j), (i+1, j)` stay collinear in the image. Hence a corner's
//! `+u` and `−u` neighbour chords are **exactly antipodal** (180° apart) even
//! under arbitrary perspective. Folding chord orientation **modulo π**
//! therefore collapses each axis neighbour-pair onto a single direction
//! *exactly* — with no orthogonality assumption. The two grid directions show
//! up as two distinct clusters in `[0, π)`, separated by whatever angle the
//! local perspective dictates.
//!
//! A second fact makes the neighbour set reliable: for a grid cell the axis
//! step is shorter than the diagonal step (`√(a² + b²) > max(a, b)`), and mild
//! perspective scales both by roughly the same local factor, so a corner's
//! **four nearest neighbours are its four axis neighbours** (`±u`, `±v`). The
//! estimate uses those.
//!
//! # Algorithm
//!
//! 1. Per corner: fold the chord angles to its `k` nearest neighbours into
//!    `[0, π)`. Generically these are two antipodal pairs collapsing to two
//!    directions.
//! 2. Pool every corner's folded nearest-edge angles into a **global
//!    distribution** and pick its two dominant modes `(g0, g1)`. This is a
//!    robust, image-wide prior — used only to *seed* the per-corner estimate
//!    and as a fallback, never as the answer.
//! 3. Per corner: run an undirected (mod-π) 2-means over the corner's folded
//!    chords, seeded at `(g0, g1)`. The two resulting centers are the corner's
//!    two local grid directions — **not** constrained to be orthogonal, so they
//!    track the local perspective. An empty cluster falls back to its global
//!    seed.
//!
//! # Precision contract
//!
//! A corner whose synthesized axes are wrong is rejected downstream by the
//! seed / attach geometry gates (axis-alignment, edge-length, residual) — it
//! becomes a *missing* corner, never a *mislabelled* one. That is the correct
//! trade for the workspace precision contract: a missing corner is acceptable,
//! a wrong `(i, j)` label is not.
//!
//! # Undirected circular statistics
//!
//! Axis angles are undirected (period π): `θ` and `θ + π` are the same
//! direction. Every mean here accumulates `(cos 2θ, sin 2θ)` and halves the
//! `atan2` result; raw `(cos θ, sin θ)` accumulation would break at the 0/π
//! seam.

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::Point2;

use crate::feature::{LocalAxis, OrientedFeature, PointFeature};

/// Nearest neighbours used to estimate one corner's local grid directions.
/// Four is the principled count: an interior grid corner's four nearest
/// neighbours are exactly its axis neighbours (diagonals stay farther).
const K_AXIS_NEIGHBOURS: usize = 4;

/// Minimum chord length (pixels) for a neighbour edge to inform an estimate;
/// guards against coincident / duplicate points.
const MIN_CHORD_PX: f32 = 1e-3;

/// Histogram bins over `[0, π)` for the global mode search (2° per bin).
const GLOBAL_BINS: usize = 90;

/// Minimum separation (radians) required between the two global modes, and the
/// half-width suppressed around the first mode before searching for the second.
const MODE_MIN_SEPARATION: f32 = 0.349_065_85; // 20°

/// 2-means iterations per corner. The clusters are tiny (≈4 points) so this
/// converges immediately; a small fixed budget keeps it allocation-light.
const REFINE_ITERS: usize = 4;

/// Synthesize two local lattice axes for every point feature from neighbour
/// geometry, returning oriented-2 features that carry the same `source_index`
/// and position plus the recovered axes. The two axes are **not** constrained
/// to be orthogonal — they track the local projected grid directions.
///
/// The result is consumed by [`crate::seed_and_grow`] / [`crate::topological`]
/// exactly like caller-supplied oriented features.
pub fn synthesize_oriented2(features: &[PointFeature]) -> Vec<OrientedFeature<2>> {
    let positions: Vec<Point2<f32>> = features.iter().map(|f| f.position).collect();
    let n = positions.len();

    if n < 3 {
        // Not enough points to recover two directions; emit a benign default.
        // Such inputs cannot form a grid and are dropped downstream anyway.
        return features
            .iter()
            .map(|f| OrientedFeature::<2>::new(*f, ordered_axes(0.0, std::f32::consts::FRAC_PI_2)))
            .collect();
    }

    let mut tree: KdTree<f32, 2> = KdTree::new();
    for (i, p) in positions.iter().enumerate() {
        tree.add(&[p.x, p.y], i as u64);
    }

    // Per-corner folded chord angles to the k nearest neighbours, each
    // carrying an inverse-distance weight so a corner's closer (true axis)
    // neighbours dominate over a farther diagonal that sneaks into the k-set
    // at a grid boundary.
    let per_corner: Vec<Vec<(f32, f32)>> = (0..n)
        .map(|i| nearest_folded_chords(&tree, &positions, i))
        .collect();

    // Global two-mode prior over the pooled nearest-edge orientations.
    let (g0, g1) = global_two_modes(&per_corner);

    features
        .iter()
        .enumerate()
        .map(|(i, feat)| {
            let (a0, a1) = refine_axes(&per_corner[i], g0, g1);
            OrientedFeature::<2>::new(*feat, ordered_axes(a0, a1))
        })
        .collect()
}

/// Folded (`[0, π)`) chord angles from corner `i` to its `K_AXIS_NEIGHBOURS`
/// nearest neighbours, each paired with an inverse-distance weight.
fn nearest_folded_chords(
    tree: &KdTree<f32, 2>,
    positions: &[Point2<f32>],
    i: usize,
) -> Vec<(f32, f32)> {
    let p = positions[i];
    // `+ 1` because the nearest hit is the query point itself.
    let hits = tree.nearest_n::<SquaredEuclidean>(&[p.x, p.y], K_AXIS_NEIGHBOURS + 1);

    let mut out = Vec::with_capacity(K_AXIS_NEIGHBOURS);
    for nn in hits {
        let j = nn.item as usize;
        if j == i {
            continue;
        }
        let q = positions[j];
        let dx = q.x - p.x;
        let dy = q.y - p.y;
        let d = (dx * dx + dy * dy).sqrt();
        if d <= MIN_CHORD_PX {
            continue;
        }
        out.push((fold_pi(dy.atan2(dx)), 1.0 / d));
    }
    out
}

/// Find the two dominant grid-edge orientations across the whole image from a
/// smoothed circular (mod-π) histogram of every corner's nearest-edge angles.
///
/// Returns `(g0, g1)` with `g1` at least [`MODE_MIN_SEPARATION`] from `g0`.
/// Falls back to an orthogonal pair only when the data exposes a single
/// direction — and only as a *seed* for the per-corner refinement, which still
/// adapts to the true local angle.
fn global_two_modes(per_corner: &[Vec<(f32, f32)>]) -> (f32, f32) {
    let bin_w = std::f32::consts::PI / GLOBAL_BINS as f32;
    let mut hist = [0.0_f32; GLOBAL_BINS];
    let mut total = 0usize;
    for chords in per_corner {
        for &(a, w) in chords {
            let mut b = (a / bin_w) as usize;
            if b >= GLOBAL_BINS {
                b = GLOBAL_BINS - 1;
            }
            hist[b] += w;
            total += 1;
        }
    }
    if total == 0 {
        return (0.0, std::f32::consts::FRAC_PI_2);
    }

    let smoothed = smooth_circular(&hist);
    let g0_bin = argmax(&smoothed);
    let g0 = (g0_bin as f32 + 0.5) * bin_w;

    // Suppress a window around g0 (mod π) and find the next peak.
    let suppress = (MODE_MIN_SEPARATION / bin_w).ceil() as i32;
    let mut best_bin = None;
    let mut best_val = 0.0_f32;
    for (b, &v) in smoothed.iter().enumerate() {
        let circ = circular_bin_distance(b as i32, g0_bin as i32, GLOBAL_BINS as i32);
        if circ <= suppress {
            continue;
        }
        if v > best_val {
            best_val = v;
            best_bin = Some(b);
        }
    }
    let g1 = match best_bin {
        Some(b) if best_val > 0.0 => (b as f32 + 0.5) * bin_w,
        // Only one direction is globally visible. Seed the second axis
        // orthogonally; per-corner refinement still recovers the true local
        // (non-orthogonal) angle wherever the data supports it.
        _ => fold_pi(g0 + std::f32::consts::FRAC_PI_2),
    };
    (g0, g1)
}

/// Undirected (mod-π) 2-means over a corner's folded chord angles, seeded at
/// the global modes. Returns the two cluster centers — the corner's two local
/// grid directions, unconstrained in their separation.
fn refine_axes(folded: &[(f32, f32)], g0: f32, g1: f32) -> (f32, f32) {
    if folded.is_empty() {
        return (g0, g1);
    }
    let (mut c0, mut c1) = (g0, g1);
    for _ in 0..REFINE_ITERS {
        let mut acc0 = UndirectedMean::default();
        let mut acc1 = UndirectedMean::default();
        for &(a, w) in folded {
            if dist_pi(a, c0) <= dist_pi(a, c1) {
                acc0.push(a, w);
            } else {
                acc1.push(a, w);
            }
        }
        // An empty cluster keeps its global seed so the slot stays defined.
        c0 = acc0.mean().unwrap_or(c0);
        c1 = acc1.mean().unwrap_or(c1);
    }
    (c0, c1)
}

/// Order two directions into the workspace axis convention:
/// `axes[0] ∈ [0, π)`, `axes[1] ∈ (axes[0], axes[0] + π)`. Since both inputs
/// are folded to `[0, π)`, this is an ascending sort.
fn ordered_axes(a: f32, b: f32) -> [LocalAxis; 2] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    [LocalAxis::new(lo, None), LocalAxis::new(hi, None)]
}

/// Weighted undirected circular-mean accumulator over `[0, π)` via
/// `(Σ w cos 2θ, Σ w sin 2θ)`.
#[derive(Clone, Copy, Default)]
struct UndirectedMean {
    sum_cos: f32,
    sum_sin: f32,
    count: usize,
}

impl UndirectedMean {
    fn push(&mut self, theta: f32, weight: f32) {
        self.sum_cos += weight * (2.0 * theta).cos();
        self.sum_sin += weight * (2.0 * theta).sin();
        self.count += 1;
    }

    fn mean(&self) -> Option<f32> {
        if self.count == 0 || self.sum_cos.hypot(self.sum_sin) < 1e-6 {
            return None;
        }
        Some(fold_pi(0.5 * self.sum_sin.atan2(self.sum_cos)))
    }
}

/// Smallest undirected angular distance modulo π, in `[0, π/2]`.
#[inline]
fn dist_pi(a: f32, b: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let d = (a - b).abs() % pi;
    d.min(pi - d)
}

/// Fold an angle into `[0, π)`.
#[inline]
fn fold_pi(theta: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let mut t = theta % pi;
    if t < 0.0 {
        t += pi;
    }
    if t >= pi {
        t -= pi;
    }
    t
}

/// Circular box-smooth (mod the histogram length) with a ±2-bin window.
fn smooth_circular(hist: &[f32; GLOBAL_BINS]) -> [f32; GLOBAL_BINS] {
    let mut out = [0.0_f32; GLOBAL_BINS];
    let n = GLOBAL_BINS as i32;
    for (i, slot) in out.iter_mut().enumerate() {
        let mut s = 0.0_f32;
        for d in -2..=2 {
            let idx = ((i as i32 + d) % n + n) % n;
            s += hist[idx as usize];
        }
        *slot = s;
    }
    out
}

fn argmax(v: &[f32; GLOBAL_BINS]) -> usize {
    let mut best = 0usize;
    let mut best_val = v[0];
    for (i, &x) in v.iter().enumerate() {
        if x > best_val {
            best_val = x;
            best = i;
        }
    }
    best
}

/// Circular distance between two bin indices over `n` bins.
#[inline]
fn circular_bin_distance(a: i32, b: i32, n: i32) -> i32 {
    let d = (a - b).abs() % n;
    d.min(n - d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Matrix3;

    fn grid_features(rows: i32, cols: i32, s: f32) -> Vec<PointFeature> {
        let mut out = Vec::new();
        let mut idx = 0usize;
        for j in 0..rows {
            for i in 0..cols {
                out.push(PointFeature::new(
                    idx,
                    Point2::new(i as f32 * s + 40.0, j as f32 * s + 40.0),
                ));
                idx += 1;
            }
        }
        out
    }

    /// Both synthesized axes must align (undirected) with the two expected
    /// directions — in either slot order, and with NO orthogonality assumption.
    fn assert_axes_match(axes: [LocalAxis; 2], exp_a: f32, exp_b: f32, tol_deg: f32) {
        let tol = tol_deg.to_radians();
        // axes[0] matches one expected, axes[1] the other (some assignment).
        let direct = dist_pi(axes[0].angle_rad, exp_a).max(dist_pi(axes[1].angle_rad, exp_b));
        let swapped = dist_pi(axes[0].angle_rad, exp_b).max(dist_pi(axes[1].angle_rad, exp_a));
        let err = direct.min(swapped);
        assert!(
            err < tol,
            "axes {:?},{:?} don't match expected {exp_a},{exp_b} (err {err})",
            axes[0].angle_rad,
            axes[1].angle_rad
        );
    }

    #[test]
    fn axis_aligned_grid_recovers_horizontal_vertical() {
        let feats = grid_features(6, 6, 25.0);
        let oriented = synthesize_oriented2(&feats);
        // Interior corner (2, 2) -> flat index 2*6 + 2 = 14.
        assert_axes_match(oriented[14].axes, 0.0, std::f32::consts::FRAC_PI_2, 4.0);
    }

    #[test]
    fn rotated_grid_tracks_orientation() {
        for deg in [10.0_f32, 30.0, 47.0, 80.0] {
            let theta = deg.to_radians();
            let (c, s) = (theta.cos(), theta.sin());
            let feats: Vec<PointFeature> = grid_features(6, 6, 25.0)
                .iter()
                .map(|f| {
                    let (x, y) = (f.position.x, f.position.y);
                    PointFeature::new(f.source_index, Point2::new(c * x - s * y, s * x + c * y))
                })
                .collect();
            let oriented = synthesize_oriented2(&feats);
            // Pure rotation keeps axes orthogonal: expected theta and theta+90.
            assert_axes_match(
                oriented[14].axes,
                fold_pi(theta),
                fold_pi(theta + std::f32::consts::FRAC_PI_2),
                6.0,
            );
        }
    }

    #[test]
    fn perspective_grid_axes_are_non_orthogonal_and_correct() {
        // Project a canonical grid through a homography with a real
        // perspective term, then check each interior corner's recovered axes
        // match the LOCAL projected grid directions — which are NOT 90° apart.
        let h = Matrix3::new(
            1.0, 0.20, 0.0, //
            0.0, 1.0, 0.0, //
            0.0015, 0.0009, 1.0,
        );
        let project = |gx: f32, gy: f32| -> Point2<f32> {
            let v = h * nalgebra::Vector3::new(gx, gy, 1.0);
            Point2::new(v.x / v.z, v.y / v.z)
        };

        let rows = 9;
        let cols = 9;
        let s = 30.0_f32;
        let mut feats = Vec::new();
        let mut idx = 0usize;
        for j in 0..rows {
            for i in 0..cols {
                let p = project(i as f32 * s + 40.0, j as f32 * s + 40.0);
                feats.push(PointFeature::new(idx, p));
                idx += 1;
            }
        }
        let oriented = synthesize_oriented2(&feats);

        let mut saw_non_orthogonal = false;
        // Check several interior corners.
        for j in 2..rows - 2 {
            for i in 2..cols - 2 {
                let flat = (j * cols + i) as usize;
                let here = feats[flat].position;
                let pu = feats[(j * cols + (i + 1)) as usize].position;
                let pv = feats[((j + 1) * cols + i) as usize].position;
                let exp_u = fold_pi((pu.y - here.y).atan2(pu.x - here.x));
                let exp_v = fold_pi((pv.y - here.y).atan2(pv.x - here.x));
                assert_axes_match(oriented[flat].axes, exp_u, exp_v, 6.0);
                if dist_pi(exp_u, exp_v) < 80.0_f32.to_radians() {
                    saw_non_orthogonal = true;
                }
            }
        }
        // The whole point: the perspective view really does bend the inter-axis
        // angle away from 90° somewhere in the interior.
        assert!(
            saw_non_orthogonal,
            "test homography too weak to exercise non-orthogonal axes"
        );
    }

    #[test]
    fn preserves_source_index_and_position() {
        let feats = grid_features(3, 3, 20.0);
        let oriented = synthesize_oriented2(&feats);
        for (f, o) in feats.iter().zip(&oriented) {
            assert_eq!(o.point.source_index, f.source_index);
            assert_eq!(o.point.position, f.position);
        }
    }

    #[test]
    fn handles_degenerate_inputs() {
        assert!(synthesize_oriented2(&[]).is_empty());
        let one = vec![PointFeature::new(0, Point2::new(1.0, 2.0))];
        let got = synthesize_oriented2(&one);
        assert_eq!(got.len(), 1);
        assert!(got[0].axes[0].angle_rad.is_finite() && got[0].axes[1].angle_rad.is_finite());
    }
}
