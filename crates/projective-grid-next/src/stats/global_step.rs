//! Automatic global cell-size estimation for a 2D corner cloud.
//!
//! Given the positions of detected corners, finds the dominant pairwise
//! "nearest-neighbor step" — the most common spatial distance between
//! adjacent points. Pattern-agnostic: any grid-like layout (chessboard,
//! ChArUco, PuzzleBoard, hex) produces a peaked distribution of nearest
//! distances, and this module recovers its mode.
//!
//! Used by the graph-build layer to size absolute thresholds (KD-tree
//! radius, validator step bounds) automatically per-frame, so callers no
//! longer need to supply explicit `min_spacing_pix` / `max_spacing_pix` /
//! `step_fallback_pix` that have to match the image scale.
//!
//! # Algorithm
//!
//! 1. Build a KD-tree over the input positions.
//! 2. Per corner, take the closest non-self distance. Collect into a vector.
//! 3. Fit a 1-D mean-shift mode on the collected distances, seeded from the
//!    25th, 50th, and 75th percentile. Track the density (count of samples
//!    within bandwidth) at each mode; return the mode that maximises
//!    `support × cell_size` — this breaks ties in favour of larger cells
//!    when marker-internal corners produce a comparable sub-mode at small
//!    distances.
//!
//! # Dual-scale datasets (ChArUco, etc.)
//!
//! When marker-internal corners coexist with board corners, the nearest-
//! distance distribution becomes bimodal. Each marker-internal corner
//! contributes only its distance to its closest neighbour — often another
//! marker-internal corner, not a board corner — so the two modes remain
//! distinguishable; the `support × cell_size` score and the
//! `multimodal` flag together give callers enough information to fall back
//! to a self-consistent seed estimate.

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, RealField};

use crate::float::{lit, Float};

/// Estimated global grid step.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct GlobalStepEstimate<F: Float> {
    /// Dominant nearest-neighbour distance, in pixels.
    pub cell_size: F,
    /// Density at the returned mode: number of input points whose nearest-
    /// neighbour distance lies within `±bandwidth` of `cell_size`.
    pub support: usize,
    /// Total points whose nearest-neighbour distance was collected (= input
    /// length, minus isolated points and exact duplicates).
    pub sample_count: usize,
    /// `support / sample_count`, saturated to `[0, 1]`. A confident unimodal
    /// cloud sits near 1.0; noisy or multi-scale clouds sit lower.
    pub confidence: F,
    /// `true` when at least two of the three percentile-seeded mean-shift
    /// runs converged to *different* modes (separated by more than a
    /// bandwidth). Signals the underlying nearest-neighbour distance
    /// distribution is multi-modal.
    pub multimodal: bool,
}

/// Tuning knobs for [`estimate_global_cell_size`].
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct GlobalStepParams<F: Float> {
    /// Bandwidth for mean-shift, expressed as a fraction of the seed
    /// distance. Defaults to `0.15` (±15 % of the candidate step).
    pub bandwidth_rel: F,
    /// Maximum mean-shift iterations per seed before accepting the current
    /// centre. Defaults to `20`.
    pub max_iters: u32,
    /// Convergence threshold: mean-shift stops when the centre update
    /// falls below `bandwidth × convergence_rel`. Defaults to `1e-3`.
    pub convergence_rel: F,
}

impl<F: Float> Default for GlobalStepParams<F> {
    fn default() -> Self {
        Self {
            bandwidth_rel: lit::<F>(0.15_f32),
            max_iters: 20,
            convergence_rel: lit::<F>(1e-3_f32),
        }
    }
}

/// Estimate a single dominant grid-cell size from a cloud of 2D corners.
///
/// Returns `None` when the cloud is too small (≤ 1 point) or degenerate
/// (all nearest-neighbour distances are zero).
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_points = positions.len()),
    )
)]
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
        percentile_sorted(&nn_distances, lit::<F>(0.25_f32)),
        percentile_sorted(&nn_distances, lit::<F>(0.5_f32)),
        percentile_sorted(&nn_distances, lit::<F>(0.75_f32)),
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
            let score = lit::<F>(support as f32) * mode;
            if best.map(|b: (F, usize, F)| score > b.2).unwrap_or(true) {
                best = Some((mode, support, score));
            }
        }
    }
    let (cell_size, support, _) = best?;
    let confidence = RealField::max(
        RealField::min(
            lit::<F>(support as f32) / lit::<F>(sample_count as f32),
            F::one(),
        ),
        F::zero(),
    );

    // Multimodality: at least two seeds converged to modes that differ by
    // more than one bandwidth (computed from the winning cell_size).
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
    let idx_f = q * lit::<F>((len - 1) as f32);
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
    use crate::float::abs;

    fn rectangular_grid<F: Float>(rows: u32, cols: u32, spacing: F) -> Vec<Point2<F>> {
        let mut out = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                out.push(Point2::new(
                    lit::<F>(i as f32) * spacing,
                    lit::<F>(j as f32) * spacing,
                ));
            }
        }
        out
    }

    fn assert_recovers_regular_grid<F: Float + kiddo::float::kdtree::Axis>() {
        let params = GlobalStepParams::<F>::default();
        for spacing_f32 in [10.0_f32, 24.0, 50.0] {
            let spacing = lit::<F>(spacing_f32);
            let pts = rectangular_grid::<F>(5, 5, spacing);
            let est = estimate_global_cell_size(&pts, &params).expect("estimate");
            let rel_err = abs::<F>(est.cell_size - spacing) / spacing;
            assert!(rel_err < lit::<F>(0.02_f32), "spacing recovery >2 %");
            assert!(est.confidence > lit::<F>(0.9_f32));
        }
    }

    fn assert_sparse_noise_does_not_drag<F: Float + kiddo::float::kdtree::Axis>() {
        let mut pts = rectangular_grid::<F>(5, 5, lit::<F>(24.0_f32));
        for (dx, dy) in [(6.0_f32, 9.0), (43.0, 9.0), (9.0, 43.0), (81.0, 81.0)] {
            pts.push(Point2::new(lit::<F>(dx), lit::<F>(dy)));
        }
        let est = estimate_global_cell_size::<F>(&pts, &GlobalStepParams::default()).expect("est");
        assert!(abs::<F>(est.cell_size - lit::<F>(24.0_f32)) < lit::<F>(2.0_f32));
        assert!(est.support >= 10);
    }

    fn assert_bimodal_density_weights_cell_size<F: Float + kiddo::float::kdtree::Axis>() {
        let mut pts: Vec<Point2<F>> = Vec::new();
        for j in 0..4 {
            for i in 0..4 {
                pts.push(Point2::new(
                    lit::<F>(i as f32) * lit::<F>(4.0_f32),
                    lit::<F>(j as f32) * lit::<F>(4.0_f32),
                ));
            }
        }
        for j in 0..4 {
            for i in 0..4 {
                pts.push(Point2::new(
                    lit::<F>(1000.0_f32) + lit::<F>(i as f32) * lit::<F>(40.0_f32),
                    lit::<F>(1000.0_f32) + lit::<F>(j as f32) * lit::<F>(40.0_f32),
                ));
            }
        }
        let est = estimate_global_cell_size::<F>(&pts, &GlobalStepParams::default()).expect("est");
        assert!(abs::<F>(est.cell_size - lit::<F>(40.0_f32)) < lit::<F>(4.0_f32));
        assert!(est.multimodal);
    }

    fn assert_unimodal_grid_not_multimodal<F: Float + kiddo::float::kdtree::Axis>() {
        let pts = rectangular_grid::<F>(7, 7, lit::<F>(25.0_f32));
        let est = estimate_global_cell_size::<F>(&pts, &GlobalStepParams::default()).expect("est");
        assert!(!est.multimodal);
    }

    fn assert_too_small_input_returns_none<F: Float + kiddo::float::kdtree::Axis>() {
        let pts: Vec<Point2<F>> = vec![];
        assert!(estimate_global_cell_size::<F>(&pts, &GlobalStepParams::default()).is_none());
        let pts = vec![Point2::<F>::new(F::zero(), F::zero())];
        assert!(estimate_global_cell_size::<F>(&pts, &GlobalStepParams::default()).is_none());
    }

    fn assert_degenerate_duplicates_skipped<F: Float + kiddo::float::kdtree::Axis>() {
        let pts: Vec<Point2<F>> = vec![
            Point2::new(F::zero(), F::zero()),
            Point2::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(10.0_f32), F::zero()),
            Point2::new(F::zero(), lit::<F>(10.0_f32)),
            Point2::new(lit::<F>(10.0_f32), lit::<F>(10.0_f32)),
        ];
        let est = estimate_global_cell_size::<F>(&pts, &GlobalStepParams::default()).expect("est");
        assert!(abs::<F>(est.cell_size - lit::<F>(10.0_f32)) < lit::<F>(1.0_f32));
    }

    fn assert_mild_jitter_recovers_mode<F: Float + kiddo::float::kdtree::Axis>() {
        let pts: Vec<Point2<F>> = rectangular_grid::<F>(5, 5, lit::<F>(24.0_f32))
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let jx = lit::<F>(((i * 17 % 7) as f32 - 3.0) * 0.4);
                let jy = lit::<F>(((i * 23 % 9) as f32 - 4.0) * 0.4);
                Point2::new(p.x + jx, p.y + jy)
            })
            .collect();
        let est = estimate_global_cell_size::<F>(&pts, &GlobalStepParams::default()).expect("est");
        assert!(abs::<F>(est.cell_size - lit::<F>(24.0_f32)) < lit::<F>(2.0_f32));
    }

    #[test]
    fn recovers_regular_grid_f32() {
        assert_recovers_regular_grid::<f32>();
    }
    #[test]
    fn recovers_regular_grid_f64() {
        assert_recovers_regular_grid::<f64>();
    }
    #[test]
    fn sparse_noise_f32() {
        assert_sparse_noise_does_not_drag::<f32>();
    }
    #[test]
    fn sparse_noise_f64() {
        assert_sparse_noise_does_not_drag::<f64>();
    }
    #[test]
    fn bimodal_weights_f32() {
        assert_bimodal_density_weights_cell_size::<f32>();
    }
    #[test]
    fn bimodal_weights_f64() {
        assert_bimodal_density_weights_cell_size::<f64>();
    }
    #[test]
    fn unimodal_not_multimodal_f32() {
        assert_unimodal_grid_not_multimodal::<f32>();
    }
    #[test]
    fn unimodal_not_multimodal_f64() {
        assert_unimodal_grid_not_multimodal::<f64>();
    }
    #[test]
    fn too_small_input_f32() {
        assert_too_small_input_returns_none::<f32>();
    }
    #[test]
    fn too_small_input_f64() {
        assert_too_small_input_returns_none::<f64>();
    }
    #[test]
    fn degenerate_duplicates_f32() {
        assert_degenerate_duplicates_skipped::<f32>();
    }
    #[test]
    fn degenerate_duplicates_f64() {
        assert_degenerate_duplicates_skipped::<f64>();
    }
    #[test]
    fn mild_jitter_f32() {
        assert_mild_jitter_recovers_mode::<f32>();
    }
    #[test]
    fn mild_jitter_f64() {
        assert_mild_jitter_recovers_mode::<f64>();
    }
}
