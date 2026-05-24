//! Float-generic circular-histogram + plateau-aware peak picking +
//! double-angle 2-means helpers.
//!
//! Closes Gap 2 from `docs/algorithmic_gaps.md` — the legacy
//! `projective_grid::circular_stats` module was hardcoded to `f32`. This is
//! the same algorithm Float-generic throughout.
//!
//! ## Undirected circular mean
//!
//! Axes are *undirected lines* — equivalent under `θ ≡ θ + π`. Any function
//! computing a circular mean of axis angles MUST accumulate
//! `(cos 2θ, sin 2θ)` and halve the resulting `atan2`. Accumulating raw
//! `(cos θ, sin θ)` silently returns garbage centres at the 0° / 180° seam
//! — this was the v1 Phase-4 regression root cause (CLAUDE.md "Corner
//! orientation contract (axes-only)"). [`refine_2means_double_angle`]
//! implements the discipline.
//!
//! ## Primitive set
//!
//! * [`wrap_pi`] / [`angular_dist_pi`] — angle helpers over the
//!   undirected mod-π circle.
//! * [`angle_to_bin`] / [`bin_to_angle`] — convert between an angle in
//!   `[0, π)` and an equal-width circular-histogram bin index.
//! * [`smooth_circular_5`] — one-pass `[1, 4, 6, 4, 1] / 16` circular
//!   convolution. The kernel is the discrete-binomial 5-tap low-pass; it
//!   preserves the histogram's total mass exactly.
//! * [`pick_two_peaks`] — plateau-aware local-maxima detection on a
//!   smoothed circular histogram.
//! * [`refine_2means_double_angle`] — 2-means refinement using the
//!   double-angle accumulation.

use crate::float::{lit, Float};

/// Wrap an angle to `[0, π)`. Works for any finite input.
#[inline]
pub fn wrap_pi<F: Float>(theta: F) -> F {
    let pi = F::pi();
    let mut t = theta % pi;
    if t < F::zero() {
        t += pi;
    }
    // Guard against `t == π` after FP wobble on the boundary.
    if t >= pi {
        t -= pi;
    }
    t
}

/// Smallest angular distance on the circle with period π. Result in
/// `[0, π/2]`.
#[inline]
pub fn angular_dist_pi<F: Float>(a: F, b: F) -> F {
    let pi = F::pi();
    let mut diff = ((a - b) % pi + pi) % pi;
    let comp = pi - diff;
    if comp < diff {
        diff = comp;
    }
    diff
}

/// Map an angle in `[0, π)` to the bin index in a histogram of `n`
/// equal-width bins over that range. Idempotent under prior [`wrap_pi`];
/// inputs outside `[0, π)` are wrapped first.
#[inline]
pub fn angle_to_bin<F: Float>(theta: F, n: usize) -> usize {
    let t = wrap_pi(theta);
    let pi = F::pi();
    let n_f = lit::<F>(n as f32);
    let x = t / pi * n_f;
    let floor = x.floor();
    // Floor-to-usize via i32 round-trip; the value is bounded by n.
    let mut idx: i32 = floor.to_subset().unwrap_or(0.0) as i32;
    if idx < 0 {
        idx = 0;
    }
    if (idx as usize) >= n {
        idx = (n as i32) - 1;
    }
    idx as usize
}

/// Inverse of [`angle_to_bin`]: bin center angle in `[0, π)`.
#[inline]
pub fn bin_to_angle<F: Float>(bin: usize, n: usize) -> F {
    let pi = F::pi();
    let n_f = lit::<F>(n as f32);
    let step = pi / n_f;
    (lit::<F>(bin as f32) + lit::<F>(0.5_f32)) * step
}

/// Smooth a circular histogram with a one-pass `[1, 4, 6, 4, 1] / 16`
/// kernel (the 5-tap discrete-binomial low-pass; sum-preserving). Handles
/// the wrap boundary with `rem_euclid`. Empty input returns empty output.
pub fn smooth_circular_5<F: Float>(hist: &[F]) -> Vec<F> {
    let n = hist.len();
    if n == 0 {
        return Vec::new();
    }
    // Reify the binomial kernel for the generic Float. Origin: 5-tap
    // discrete-binomial approximation to a Gaussian, sum = 16.
    let k: [F; 5] = [
        lit::<F>(1.0_f32),
        lit::<F>(4.0_f32),
        lit::<F>(6.0_f32),
        lit::<F>(4.0_f32),
        lit::<F>(1.0_f32),
    ];
    let k_sum = lit::<F>(16.0_f32);
    let mut out = vec![F::zero(); n];
    let n_i = n as isize;
    for (i, bin) in out.iter_mut().enumerate() {
        let mut acc = F::zero();
        for (k_idx, &w) in k.iter().enumerate() {
            let offset = k_idx as isize - 2;
            let j = ((i as isize + offset).rem_euclid(n_i)) as usize;
            acc += w * hist[j];
        }
        *bin = acc / k_sum;
    }
    out
}

/// Options for [`pick_two_peaks`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct PeakPickOptions<F: Float> {
    /// Minimum fraction of `total_weight` a peak must carry to be
    /// considered.
    pub min_peak_weight_fraction: F,
    /// Minimum angular separation (radians, mod-π circle) between the two
    /// returned peaks.
    pub min_separation: F,
}

impl<F: Float> PeakPickOptions<F> {
    /// Construct options from the minimum peak-weight fraction and the
    /// minimum angular separation (radians, mod-π).
    pub fn new(min_peak_weight_fraction: F, min_separation: F) -> Self {
        Self {
            min_peak_weight_fraction,
            min_separation,
        }
    }
}

/// Pick the two strongest plateau-aware peaks from a smoothed circular
/// histogram, subject to a minimum-weight floor and minimum angular
/// separation.
///
/// Returns `Some((theta0, theta1))` (bin-centre angles in `[0, π)`, no
/// ordering guarantee) or `None` when fewer than two qualifying peaks
/// exist or no two peaks are far enough apart.
///
/// "Plateau-aware" means a run of equal-valued bins bordered on both
/// sides by strictly lower bins reports the plateau's midpoint as the
/// peak — important when smoothing spreads a direction's vote mass across
/// adjacent bins.
pub fn pick_two_peaks<F: Float>(
    smoothed: &[F],
    total_weight: F,
    opts: &PeakPickOptions<F>,
) -> Option<(F, F)> {
    let n = smoothed.len();
    if n == 0 {
        return None;
    }
    let min_w = total_weight * opts.min_peak_weight_fraction;

    let mut peaks: Vec<(usize, F)> = Vec::new();
    let mut visited = vec![false; n];
    for start in 0..n {
        if visited[start] {
            continue;
        }
        let here = smoothed[start];
        if here < min_w {
            visited[start] = true;
            continue;
        }
        let mut len = 1usize;
        while len < n {
            let next_idx = (start + len) % n;
            if smoothed[next_idx] != here {
                break;
            }
            len += 1;
        }
        for k in 0..len {
            visited[(start + k) % n] = true;
        }
        if len == n {
            continue;
        }
        let left = smoothed[(start + n - 1) % n];
        let right = smoothed[(start + len) % n];
        if here > left && here > right {
            let mid = (start + len / 2) % n;
            peaks.push((mid, here));
        }
    }

    peaks.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    if peaks.is_empty() {
        return None;
    }
    let theta_of = |bin: usize| bin_to_angle::<F>(bin, n);
    let first = theta_of(peaks[0].0);
    for (bin, _w) in peaks.iter().skip(1) {
        let cand = theta_of(*bin);
        if angular_dist_pi(first, cand) >= opts.min_separation {
            return Some((first, cand));
        }
    }
    None
}

/// A single weighted vote over the mod-π circle.
#[derive(Clone, Copy, Debug)]
pub struct AngleVote<F: Float> {
    /// Vote direction in radians, interpreted on the mod-π circle.
    pub angle: F,
    /// Non-negative weight this vote contributes to its histogram bin.
    pub weight: F,
}

/// Refine two cluster centres via weighted 2-means on mod-π-circular vote
/// angles using the **double-angle** discipline (the V1 Phase-4 regression
/// fix called out in CLAUDE.md): accumulate `(w·cos 2θ, w·sin 2θ)` per
/// cluster and halve the resulting `atan2`.
///
/// Returns the refined `(center0, center1)`. Stops early when both centres
/// stabilise to within `1e-5` radians, or after `max_iters`. With zero
/// votes this returns `seed` unchanged.
pub fn refine_2means_double_angle<F: Float>(
    votes: &[AngleVote<F>],
    seed: [F; 2],
    max_iters: usize,
) -> (F, F) {
    if votes.is_empty() {
        return (seed[0], seed[1]);
    }

    let mut centers = seed;
    let two = lit::<F>(2.0_f32);
    let half = lit::<F>(0.5_f32);
    let tol = lit::<F>(1e-5_f32);

    for _ in 0..max_iters {
        let mut sum_2cos = [F::zero(); 2];
        let mut sum_2sin = [F::zero(); 2];
        let mut sum_w = [F::zero(); 2];
        for v in votes {
            let d0 = angular_dist_pi(v.angle, centers[0]);
            let d1 = angular_dist_pi(v.angle, centers[1]);
            let k = if d0 <= d1 { 0 } else { 1 };
            let two_theta = two * v.angle;
            sum_2cos[k] += v.weight * two_theta.cos();
            sum_2sin[k] += v.weight * two_theta.sin();
            sum_w[k] += v.weight;
        }
        let mut new_centers = centers;
        for k in 0..2 {
            if sum_w[k] > F::zero() {
                let two_theta = sum_2sin[k].atan2(sum_2cos[k]);
                new_centers[k] = wrap_pi(two_theta * half);
            }
        }
        if (new_centers[0] - centers[0]).abs() < tol && (new_centers[1] - centers[1]).abs() < tol {
            return (new_centers[0], new_centers[1]);
        }
        centers = new_centers;
    }
    (centers[0], centers[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_wrap_pi_handles_boundary<F: Float>() {
        let pi = F::pi();
        let tol = lit::<F>(1e-5_f32);
        assert!((wrap_pi::<F>(F::zero())).abs() < tol);
        assert!(wrap_pi::<F>(pi) < tol);
        assert!((wrap_pi::<F>(pi + lit::<F>(0.1_f32)) - lit::<F>(0.1_f32)).abs() < tol);
        assert!((wrap_pi::<F>(-lit::<F>(0.1_f32)) - (pi - lit::<F>(0.1_f32))).abs() < tol);
    }

    fn assert_angular_dist_pi_wraps<F: Float>() {
        let pi = F::pi();
        let tol = lit::<F>(1e-5_f32);
        assert!(
            (angular_dist_pi::<F>(lit::<F>(0.1_f32), pi - lit::<F>(0.1_f32)) - lit::<F>(0.2_f32))
                .abs()
                < tol
        );
        let frac_pi_2 = pi * lit::<F>(0.5_f32);
        assert!((angular_dist_pi::<F>(F::zero(), frac_pi_2) - frac_pi_2).abs() < tol);
    }

    fn assert_smooth_5_preserves_total<F: Float>() {
        let hist: Vec<F> = vec![
            F::zero(),
            F::zero(),
            lit::<F>(16.0_f32),
            F::zero(),
            F::zero(),
        ];
        let out = smooth_circular_5::<F>(&hist);
        let mut sum = F::zero();
        for v in &out {
            sum += *v;
        }
        assert!((sum - lit::<F>(16.0_f32)).abs() < lit::<F>(1e-4_f32));
    }

    fn assert_pick_two_peaks_orthogonal<F: Float>() {
        // 18 bins of 10°, peaks at 0° (bin 0) and 90° (bin 9).
        let mut hist: Vec<F> = vec![F::zero(); 18];
        hist[0] = lit::<F>(100.0_f32);
        hist[9] = lit::<F>(100.0_f32);
        let smoothed = smooth_circular_5::<F>(&hist);
        let frac_pi_2 = F::pi() * lit::<F>(0.5_f32);
        let separation = F::pi() / lit::<F>(3.0_f32); // 60°
        let peaks = pick_two_peaks::<F>(
            &smoothed,
            lit::<F>(200.0_f32),
            &PeakPickOptions::new(lit::<F>(0.02_f32), separation),
        )
        .expect("two peaks");
        let lo = if peaks.0 < peaks.1 { peaks.0 } else { peaks.1 };
        let hi = if peaks.0 < peaks.1 { peaks.1 } else { peaks.0 };
        assert!(lo.abs() < lit::<F>(0.1_f32));
        assert!((hi - frac_pi_2).abs() < lit::<F>(0.1_f32));
    }

    fn assert_pick_two_peaks_handles_plateau<F: Float>() {
        // Mass split across bins 0 and n-1 — the near-π wrap scenario.
        let n = 18;
        let mut hist: Vec<F> = vec![F::zero(); n];
        hist[0] = lit::<F>(50.0_f32);
        hist[n - 1] = lit::<F>(50.0_f32);
        hist[9] = lit::<F>(100.0_f32);
        let smoothed = smooth_circular_5::<F>(&hist);
        let frac_pi_2 = F::pi() * lit::<F>(0.5_f32);
        let pi = F::pi();
        let separation = pi / lit::<F>(3.0_f32);
        let peaks = pick_two_peaks::<F>(
            &smoothed,
            lit::<F>(200.0_f32),
            &PeakPickOptions::new(lit::<F>(0.02_f32), separation),
        )
        .expect("plateau peaks");
        let lo = if peaks.0 < peaks.1 { peaks.0 } else { peaks.1 };
        let hi = if peaks.0 < peaks.1 { peaks.1 } else { peaks.0 };
        let tol = lit::<F>(0.2_f32);
        assert!(lo.abs() < tol || (pi - lo).abs() < tol);
        assert!((hi - frac_pi_2).abs() < tol);
    }

    fn assert_two_means_converges<F: Float>() {
        let pi = F::pi();
        let frac_pi_2 = pi * lit::<F>(0.5_f32);
        let votes: Vec<AngleVote<F>> = (0..50)
            .flat_map(|_| {
                [
                    AngleVote {
                        angle: lit::<F>(0.1_f32),
                        weight: F::one(),
                    },
                    AngleVote {
                        angle: frac_pi_2 - lit::<F>(0.1_f32),
                        weight: F::one(),
                    },
                ]
            })
            .collect();
        let seed = [lit::<F>(0.2_f32), frac_pi_2 - lit::<F>(0.2_f32)];
        let (c0, c1) = refine_2means_double_angle::<F>(&votes, seed, 10);
        let tol = lit::<F>(0.05_f32);
        assert!((c0 - lit::<F>(0.1_f32)).abs() < tol || (c1 - lit::<F>(0.1_f32)).abs() < tol);
        assert!(
            (c0 - (frac_pi_2 - lit::<F>(0.1_f32))).abs() < tol
                || (c1 - (frac_pi_2 - lit::<F>(0.1_f32))).abs() < tol
        );
    }

    fn assert_two_means_empty_returns_seed<F: Float>() {
        let seed = [lit::<F>(0.1_f32), lit::<F>(1.2_f32)];
        let (c0, c1) = refine_2means_double_angle::<F>(&[], seed, 5);
        assert_eq!((c0, c1), (seed[0], seed[1]));
    }

    fn assert_two_means_at_zero_seam<F: Float>() {
        // Votes at angles 0.05 and π-0.05 are the SAME undirected line
        // (separated by π). Their double-angle circular mean should land
        // near 0 (or equivalently near π, but `wrap_pi` returns the
        // [0, π) representative — so near 0). Accumulating raw
        // (cos θ, sin θ) would give ~π/2, which is the seam bug we
        // explicitly guard against.
        let pi = F::pi();
        let votes: Vec<AngleVote<F>> = (0..40)
            .flat_map(|_| {
                [
                    AngleVote {
                        angle: lit::<F>(0.05_f32),
                        weight: F::one(),
                    },
                    AngleVote {
                        angle: pi - lit::<F>(0.05_f32),
                        weight: F::one(),
                    },
                ]
            })
            .collect();
        // Seed close to 0 with both centres on the same side to force
        // every vote into cluster 0.
        let seed = [lit::<F>(0.0_f32), pi * lit::<F>(0.5_f32)];
        let (c0, _) = refine_2means_double_angle::<F>(&votes, seed, 20);
        // The double-angle mean of {0.05, π-0.05} on the mod-π circle
        // lands at 0 (the two votes are the same undirected line). Allow
        // a wide tolerance because the [0, π) wrap can map the seam
        // representative to either edge.
        let dist_to_zero = wrap_pi::<F>(c0);
        let near_zero = dist_to_zero < lit::<F>(0.1_f32) || (pi - dist_to_zero) < lit::<F>(0.1_f32);
        assert!(
            near_zero,
            "double-angle 2-means recovered seam centre; raw (cos, sin) would land near π/2",
        );
    }

    #[test]
    fn wrap_pi_handles_boundary_f32() {
        assert_wrap_pi_handles_boundary::<f32>();
    }
    #[test]
    fn wrap_pi_handles_boundary_f64() {
        assert_wrap_pi_handles_boundary::<f64>();
    }
    #[test]
    fn angular_dist_pi_wraps_f32() {
        assert_angular_dist_pi_wraps::<f32>();
    }
    #[test]
    fn angular_dist_pi_wraps_f64() {
        assert_angular_dist_pi_wraps::<f64>();
    }
    #[test]
    fn smooth_5_preserves_total_f32() {
        assert_smooth_5_preserves_total::<f32>();
    }
    #[test]
    fn smooth_5_preserves_total_f64() {
        assert_smooth_5_preserves_total::<f64>();
    }
    #[test]
    fn pick_two_peaks_orthogonal_f32() {
        assert_pick_two_peaks_orthogonal::<f32>();
    }
    #[test]
    fn pick_two_peaks_orthogonal_f64() {
        assert_pick_two_peaks_orthogonal::<f64>();
    }
    #[test]
    fn pick_two_peaks_plateau_f32() {
        assert_pick_two_peaks_handles_plateau::<f32>();
    }
    #[test]
    fn pick_two_peaks_plateau_f64() {
        assert_pick_two_peaks_handles_plateau::<f64>();
    }
    #[test]
    fn two_means_converges_f32() {
        assert_two_means_converges::<f32>();
    }
    #[test]
    fn two_means_converges_f64() {
        assert_two_means_converges::<f64>();
    }
    #[test]
    fn two_means_empty_seed_f32() {
        assert_two_means_empty_returns_seed::<f32>();
    }
    #[test]
    fn two_means_empty_seed_f64() {
        assert_two_means_empty_returns_seed::<f64>();
    }
    #[test]
    fn two_means_seam_f32() {
        assert_two_means_at_zero_seam::<f32>();
    }
    #[test]
    fn two_means_seam_f64() {
        assert_two_means_at_zero_seam::<f64>();
    }
}
