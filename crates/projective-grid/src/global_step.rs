//! Automatic global cell-size estimation for a 2D corner cloud.
//!
//! Given the positions of detected corners, finds the dominant pairwise
//! "nearest-neighbor step" — the most common spatial distance between adjacent
//! points. Pattern-agnostic: any grid-like layout (chessboard, ChArUco,
//! PuzzleBoard, hex) produces a peaked distribution of nearest distances, and
//! this module recovers its mode.
//!
//! Used by the graph-build layer to size absolute thresholds (KD-tree radius,
//! validator step bounds) automatically per-frame, so callers no longer need
//! to supply `min_spacing_pix` / `max_spacing_pix` / `step_fallback_pix` that
//! have to match the image scale.
//!
//! # Algorithm
//!
//! 1. Build a KD-tree over the input positions.
//! 2. Per corner, take the closest non-self distance. Collect into a vector.
//! 3. Fit a 1-D mean-shift mode on the collected distances, seeded from the
//!    25th, 50th, and 75th percentile. Track the density (count of samples
//!    within bandwidth) at each mode; return the densest mode.
//!
//! # Dual-scale datasets (ChArUco, etc.)
//!
//! When marker-internal corners coexist with board corners, the nearest-
//! distance distribution becomes bimodal: a sub-mode at the marker spacing
//! (~0.2× board step) and a dominant mode at the board step. Because we use
//! *nearest-neighbor* distance (not all pairs), each marker-internal corner
//! contributes only its distance to its closest neighbor — often another
//! marker-internal corner, not a board corner — so the two modes remain
//! distinguishable and the board mode retains the higher total support on
//! typical real-world captures.

use crate::Float;
use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, RealField};

/// Estimated global grid step.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct GlobalStepEstimate<F: Float = f32> {
    /// Dominant nearest-neighbor distance, in pixels.
    pub cell_size: F,
    /// Density at the returned mode: number of input points whose nearest-
    /// neighbor distance lies within `±bandwidth` of `cell_size`. Useful for
    /// downstream sanity checks.
    pub support: usize,
    /// Total points whose nearest-neighbor distance was collected (= input
    /// length, minus isolated points that have no reachable neighbors in the
    /// KD-tree search).
    pub sample_count: usize,
    /// `support / sample_count`, saturated to `[0, 1]`. A confident
    /// unimodal cloud sits near 1.0; noisy or multi-scale clouds sit lower.
    pub confidence: F,
    /// `true` when at least two of the three percentile-seeded mean-shift
    /// runs converged to *different* modes (separated by more than a
    /// bandwidth). Signals the underlying nearest-neighbour distance
    /// distribution is multi-modal — typical for ChArUco frames where
    /// marker-internal corners coexist with board corners. Downstream
    /// callers may want to fall back to a self-consistent seed estimate
    /// when this is set.
    pub multimodal: bool,
}

/// Tuning knobs for [`estimate_global_cell_size`].
#[derive(Clone, Copy, Debug)]
pub struct GlobalStepParams<F: Float = f32> {
    /// Bandwidth for mean-shift, expressed as a fraction of the seed value.
    /// Defaults to `0.15` (±15 % of the candidate step).
    pub bandwidth_rel: F,
    /// Maximum mean-shift iterations per seed before accepting the current
    /// center. Defaults to `20`.
    pub max_iters: u32,
    /// Convergence threshold: mean-shift stops when the center update falls
    /// below `bandwidth × convergence_rel`. Defaults to `1e-3`.
    pub convergence_rel: F,
}

impl<F: Float> Default for GlobalStepParams<F> {
    fn default() -> Self {
        Self {
            bandwidth_rel: F::from_subset(&0.15),
            max_iters: 20,
            convergence_rel: F::from_subset(&1e-3),
        }
    }
}

/// Estimate a single dominant grid-cell size from a cloud of 2D corners.
///
/// Returns `None` when the cloud is too small (≤ 1 point) or degenerate
/// (all nearest-neighbor distances are zero).
pub fn estimate_global_cell_size<F: Float + kiddo::float::kdtree::Axis>(
    positions: &[Point2<F>],
    params: &GlobalStepParams<F>,
) -> Option<GlobalStepEstimate<F>> {
    if positions.len() < 2 {
        return None;
    }

    let coords: Vec<[F; 2]> = positions.iter().map(|p| [p.x, p.y]).collect();
    let tree: KdTree<F, 2> = (&coords).into();

    let mut nn_distances: Vec<F> = Vec::with_capacity(positions.len());
    for (i, p) in positions.iter().enumerate() {
        let hits = tree.nearest_n::<SquaredEuclidean>(&[p.x, p.y], 2);
        for hit in hits {
            let j = hit.item as usize;
            if j == i {
                continue;
            }
            let d2 = hit.distance;
            if d2 > F::zero() {
                nn_distances.push(d2.sqrt());
            }
            break;
        }
    }

    if nn_distances.is_empty() {
        return None;
    }
    nn_distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let sample_count = nn_distances.len();
    let seeds = [
        percentile_sorted(&nn_distances, F::from_subset(&0.25)),
        percentile_sorted(&nn_distances, F::from_subset(&0.5)),
        percentile_sorted(&nn_distances, F::from_subset(&0.75)),
    ];

    // Score each mode candidate by `support × cell_size`. This breaks ties in
    // favour of larger cell sizes when a minority sub-mode from marker-internal
    // corners has comparable support — we always want the lattice step, not
    // the within-cell step.
    let mut best: Option<(F, usize, F)> = None; // (mode, support, score)
    let mut converged_modes: Vec<F> = Vec::new();
    for seed in seeds {
        if let Some((mode, support_u32)) = mean_shift_mode(&nn_distances, seed, params) {
            if support_u32 == 0 {
                continue;
            }
            let support = support_u32 as usize;
            converged_modes.push(mode);
            let score = F::from_subset(&(support as f64)) * mode;
            if best.map(|b: (F, usize, F)| score > b.2).unwrap_or(true) {
                best = Some((mode, support, score));
            }
        }
    }
    let (cell_size, support, _) = best?;
    let confidence = RealField::max(
        RealField::min(
            F::from_subset(&(support as f64)) / F::from_subset(&(sample_count as f64)),
            F::one(),
        ),
        F::zero(),
    );

    // Multimodality: at least two seeds converged to modes that
    // differ by more than one bandwidth. Bandwidth here is computed
    // from the winning `cell_size`.
    let bandwidth = cell_size * params.bandwidth_rel;
    let multimodal = converged_modes.iter().any(|&m| {
        let diff: F = m - cell_size;
        let abs_diff: F = if diff < F::zero() { -diff } else { diff };
        abs_diff > bandwidth
    });

    Some(GlobalStepEstimate {
        cell_size,
        support,
        sample_count,
        confidence,
        multimodal,
    })
}

fn percentile_sorted<F: Float>(sorted: &[F], q: F) -> F {
    let len = sorted.len();
    if len == 0 {
        return F::zero();
    }
    let idx_f = q * F::from_subset(&((len - 1) as f64));
    let idx = idx_f.floor();
    let i = idx.to_subset().unwrap_or(0.0) as usize;
    let i = i.min(len - 1);
    sorted[i]
}

fn mean_shift_mode<F: Float>(
    sorted: &[F],
    seed: F,
    params: &GlobalStepParams<F>,
) -> Option<(F, u32)> {
    if seed <= F::zero() {
        return None;
    }
    let bandwidth = seed * params.bandwidth_rel;
    if bandwidth <= F::zero() {
        return Some((seed, 0));
    }
    let convergence = bandwidth * params.convergence_rel;

    let mut center = seed;
    for _ in 0..params.max_iters {
        let mut sum = F::zero();
        let mut weight = F::zero();
        let mut count_in_band = 0u32;
        for &v in sorted {
            let diff = v - center;
            if diff.abs() > bandwidth {
                continue;
            }
            let t = diff / bandwidth;
            let w = F::one() - t * t;
            let w = if w < F::zero() { F::zero() } else { w };
            if w > F::zero() {
                sum += v * w;
                weight += w;
                count_in_band += 1;
            }
        }
        if weight <= F::zero() {
            return Some((center, 0));
        }
        let next = sum / weight;
        if (next - center).abs() <= convergence {
            return Some((next, count_in_band));
        }
        center = next;
    }
    // Did not converge: fall back to the last center and its in-band count.
    let mut in_band = 0u32;
    for &v in sorted {
        if (v - center).abs() <= bandwidth {
            in_band += 1;
        }
    }
    Some((center, in_band))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangular_grid(rows: u32, cols: u32, spacing: f32) -> Vec<Point2<f32>> {
        let mut out = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                out.push(Point2::new(i as f32 * spacing, j as f32 * spacing));
            }
        }
        out
    }

    #[test]
    fn recovers_regular_grid_spacing() {
        let params = GlobalStepParams::<f32>::default();
        for &spacing in &[10.0_f32, 24.0, 50.0] {
            let pts = rectangular_grid(5, 5, spacing);
            let est = estimate_global_cell_size(&pts, &params).expect("estimate");
            assert!(
                (est.cell_size - spacing).abs() / spacing < 0.02,
                "spacing {spacing}: estimate {} off >2 %",
                est.cell_size
            );
            assert!(est.confidence > 0.9, "confidence {}", est.confidence);
        }
    }

    #[test]
    fn sparse_noise_does_not_drag_mode() {
        // 5×5 board at spacing=24 plus a small sprinkling of noise corners
        // offset by random non-grid distances. The dominant mode should stay
        // on the board spacing.
        let mut pts = rectangular_grid(5, 5, 24.0);
        // 4 noise points at positions that do not land on the 24 px lattice.
        for (dx, dy) in [(6.0, 9.0), (43.0, 9.0), (9.0, 43.0), (81.0, 81.0)] {
            pts.push(Point2::new(dx, dy));
        }
        let est =
            estimate_global_cell_size(&pts, &GlobalStepParams::<f32>::default()).expect("estimate");
        assert!(
            (est.cell_size - 24.0).abs() < 2.0,
            "expected board step ~24 but got {}",
            est.cell_size
        );
        assert!(est.support >= 10); // most of the 25 board corners contribute.
    }

    #[test]
    fn bimodal_density_weights_by_cell_size() {
        // Two disjoint grids: a 4×4 "small" grid at spacing=4 and a 4×4
        // "big" grid at spacing=40. Equal point counts. The `support ×
        // cell_size` score biases us toward the larger spacing.
        let mut pts = Vec::new();
        for j in 0..4 {
            for i in 0..4 {
                pts.push(Point2::new(i as f32 * 4.0, j as f32 * 4.0));
            }
        }
        for j in 0..4 {
            for i in 0..4 {
                pts.push(Point2::new(
                    1000.0 + i as f32 * 40.0,
                    1000.0 + j as f32 * 40.0,
                ));
            }
        }
        let est =
            estimate_global_cell_size(&pts, &GlobalStepParams::<f32>::default()).expect("estimate");
        assert!(
            (est.cell_size - 40.0).abs() < 4.0,
            "expected larger-grid cell ~40 but got {}",
            est.cell_size
        );
        // The two clusters' cell-size modes are an order of magnitude
        // apart — multiple percentile seeds converge to different modes,
        // so the multimodal flag fires.
        assert!(est.multimodal, "expected multimodal=true on bimodal cloud");
    }

    #[test]
    fn unimodal_grid_has_multimodal_false() {
        let pts = rectangular_grid(7, 7, 25.0);
        let est =
            estimate_global_cell_size(&pts, &GlobalStepParams::<f32>::default()).expect("estimate");
        assert!(!est.multimodal, "expected multimodal=false on a clean grid");
    }

    #[test]
    fn too_small_input_returns_none() {
        let pts: Vec<Point2<f32>> = vec![];
        assert!(estimate_global_cell_size(&pts, &GlobalStepParams::<f32>::default()).is_none());
        let pts = vec![Point2::new(0.0, 0.0)];
        assert!(estimate_global_cell_size(&pts, &GlobalStepParams::<f32>::default()).is_none());
    }

    #[test]
    fn degenerate_duplicate_points_are_skipped() {
        let pts = vec![
            Point2::new(0.0, 0.0),
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(0.0, 10.0),
            Point2::new(10.0, 10.0),
        ];
        let est =
            estimate_global_cell_size(&pts, &GlobalStepParams::<f32>::default()).expect("estimate");
        assert!((est.cell_size - 10.0).abs() < 1.0);
    }

    #[test]
    fn mild_jitter_still_recovers_mode() {
        // 5×5 grid at 24 px spacing with 5 % positional jitter.
        let pts: Vec<Point2<f32>> = rectangular_grid(5, 5, 24.0)
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let jitter_x = ((i * 17 % 7) as f32 - 3.0) * 0.4;
                let jitter_y = ((i * 23 % 9) as f32 - 4.0) * 0.4;
                Point2::new(p.x + jitter_x, p.y + jitter_y)
            })
            .collect();
        let est =
            estimate_global_cell_size(&pts, &GlobalStepParams::<f32>::default()).expect("estimate");
        assert!(
            (est.cell_size - 24.0).abs() < 2.0,
            "expected ~24 got {}",
            est.cell_size
        );
    }
}
