//! Circular-histogram + plateau-aware peak picking + double-angle
//! 2-means helpers.
//!
//! These primitives are pattern-agnostic pieces of the chessboard
//! detector's **global grid-direction** stage. The chessboard-
//! specific wrapper in `calib_targets_chessboard::cluster` owns the
//! histogram-accumulation step (iterates corners, reads per-axis
//! `(angle, sigma, strength)`) and the final per-corner label
//! assignment; this module provides the generic math underneath:
//!
//! * [`wrap_pi`] / [`angular_dist_pi`] — angle helpers over the
//!   undirected mod-π circle.
//! * [`smooth_circular_5`] — a 1-pass `[1, 4, 6, 4, 1] / 16` circular
//!   convolution.
//! * [`pick_two_peaks`] — plateau-aware local-maxima detection on a
//!   smoothed circular histogram. Handles the edge case where a
//!   physical direction's mass lands on both sides of a bin boundary
//!   and the peak flat-tops across two adjacent bins.
//! * [`refine_2means_double_angle`] — 2-means refinement over
//!   mod-π-circular votes using the standard double-angle trick so
//!   the circular mean stays correct across the `0 ≈ π` seam.
//!
//! # When to use this
//!
//! Any grid-detection pipeline that needs to identify "two dominant
//! directions" from noisy axis-angle votes (chessboard x-junctions,
//! line grids, woven lattices) can build a histogram of `(angle,
//! weight)` pairs, run the peak + 2-means steps here, and consume
//! the two centers as its grid axes.

use std::f32::consts::PI;

/// Wrap an angle to `[0, π)`. Works for any finite input.
#[inline]
pub fn wrap_pi(theta: f32) -> f32 {
    let mut t = theta % PI;
    if t < 0.0 {
        t += PI;
    }
    // Guard against `t == PI` after FP wobble on the boundary.
    if t >= PI {
        t -= PI;
    }
    t
}

/// Smallest angular distance on the circle with period π. Result in
/// `[0, π/2]`.
#[inline]
pub fn angular_dist_pi(a: f32, b: f32) -> f32 {
    let diff = ((a - b) % PI + PI) % PI;
    diff.min(PI - diff)
}

/// Map an angle in `[0, π)` to the bin index in a histogram of `n`
/// equal-width bins over that range. Idempotent under prior
/// [`wrap_pi`]; inputs outside `[0, π)` are wrapped first.
#[inline]
pub fn angle_to_bin(theta: f32, n: usize) -> usize {
    let t = wrap_pi(theta);
    let x = t / PI * n as f32;
    let mut idx = x.floor() as isize;
    if idx < 0 {
        idx = 0;
    }
    if idx as usize >= n {
        idx = (n - 1) as isize;
    }
    idx as usize
}

/// Inverse of [`angle_to_bin`]: bin center angle in `[0, π)`.
#[inline]
pub fn bin_to_angle(bin: usize, n: usize) -> f32 {
    let step = PI / n as f32;
    (bin as f32 + 0.5) * step
}

/// Smooth a circular histogram with a one-pass `[1, 4, 6, 4, 1] / 16`
/// kernel. Handles the wrap boundary with `rem_euclid`. Empty input
/// returns empty output.
pub fn smooth_circular_5(hist: &[f32]) -> Vec<f32> {
    let n = hist.len();
    if n == 0 {
        return Vec::new();
    }
    const K: [f32; 5] = [1.0, 4.0, 6.0, 4.0, 1.0];
    const K_SUM: f32 = 16.0;
    let mut out = vec![0.0_f32; n];
    for (i, bin) in out.iter_mut().enumerate() {
        let mut acc = 0.0_f32;
        for (k, &w) in K.iter().enumerate() {
            let offset = k as isize - 2;
            let j = ((i as isize + offset).rem_euclid(n as isize)) as usize;
            acc += w * hist[j];
        }
        *bin = acc / K_SUM;
    }
    out
}

/// Options for [`pick_two_peaks`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct PeakPickOptions {
    /// Minimum fraction of `total_weight` a peak must carry to be
    /// considered. Rejects histogram noise.
    pub min_peak_weight_fraction: f32,
    /// Minimum angular separation between the two returned peaks
    /// (in radians, on the mod-π circle). Ensures the two peaks
    /// represent genuinely distinct directions rather than two
    /// ridges of one cluster.
    pub min_separation: f32,
}

impl PeakPickOptions {
    pub fn new(min_peak_weight_fraction: f32, min_separation: f32) -> Self {
        Self {
            min_peak_weight_fraction,
            min_separation,
        }
    }
}

/// Pick the two strongest plateau-aware peaks from a smoothed
/// circular histogram, subject to a minimum-weight floor and minimum
/// angular separation.
///
/// Returns `Some((theta0, theta1))` (bin-center angles in `[0, π)`,
/// no ordering guarantee) or `None` when fewer than two qualifying
/// peaks exist or no two peaks are far enough apart.
///
/// "Plateau-aware" means the peak detector handles a run of equal-
/// valued bins bordered on both sides by strictly lower bins: the
/// plateau's midpoint bin is reported as the peak. This is important
/// when a direction's vote mass lands at a bin boundary and
/// smoothing pushes symmetric mass into the two adjacent bins.
pub fn pick_two_peaks(
    smoothed: &[f32],
    total_weight: f32,
    opts: &PeakPickOptions,
) -> Option<(f32, f32)> {
    let n = smoothed.len();
    if n == 0 {
        return None;
    }
    let min_w = total_weight * opts.min_peak_weight_fraction;

    let mut peaks: Vec<(usize, f32)> = Vec::new();
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
            // Completely flat histogram — no peak.
            continue;
        }
        let left = smoothed[(start + n - 1) % n];
        let right = smoothed[(start + len) % n];
        if here > left && here > right {
            let mid = (start + len / 2) % n;
            peaks.push((mid, here));
        }
    }

    peaks.sort_by(|a, b| b.1.total_cmp(&a.1));
    if peaks.is_empty() {
        return None;
    }
    let theta_of = |bin: usize| bin_to_angle(bin, n);
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
pub struct AngleVote {
    pub angle: f32,
    pub weight: f32,
}

/// Refine two cluster centers via weighted 2-means on mod-π-circular
/// vote angles using the **double-angle** trick: accumulate
/// `(w·cos 2θ, w·sin 2θ)` per cluster and halve the resulting atan2.
/// This is the correct circular mean for undirected angles (mod π);
/// accumulating raw `(cos θ, sin θ)` silently returns garbage near
/// the 0°/180° seam.
///
/// Returns the refined `(center0, center1)`. Stops early when both
/// centers stabilise to within `1e-5` radians or after `max_iters`.
///
/// With zero votes this returns `seed` unchanged.
pub fn refine_2means_double_angle(
    votes: &[AngleVote],
    seed: [f32; 2],
    max_iters: usize,
) -> (f32, f32) {
    if votes.is_empty() {
        return (seed[0], seed[1]);
    }

    let mut centers = seed;

    for _ in 0..max_iters {
        let mut sum_2cos = [0.0_f32; 2];
        let mut sum_2sin = [0.0_f32; 2];
        let mut sum_w = [0.0_f32; 2];
        for v in votes {
            let d0 = angular_dist_pi(v.angle, centers[0]);
            let d1 = angular_dist_pi(v.angle, centers[1]);
            let k = if d0 <= d1 { 0 } else { 1 };
            let two_theta = 2.0 * v.angle;
            sum_2cos[k] += v.weight * two_theta.cos();
            sum_2sin[k] += v.weight * two_theta.sin();
            sum_w[k] += v.weight;
        }
        let mut new_centers = centers;
        for k in 0..2 {
            if sum_w[k] > 0.0 {
                let two_theta = sum_2sin[k].atan2(sum_2cos[k]);
                new_centers[k] = wrap_pi(two_theta * 0.5);
            }
        }
        if (new_centers[0] - centers[0]).abs() < 1e-5 && (new_centers[1] - centers[1]).abs() < 1e-5
        {
            return (new_centers[0], new_centers[1]);
        }
        centers = new_centers;
    }
    (centers[0], centers[1])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn wrap_pi_handles_boundary() {
        assert!((wrap_pi(0.0) - 0.0).abs() < 1e-6);
        assert!((wrap_pi(PI) - 0.0).abs() < 1e-6);
        assert!((wrap_pi(PI + 0.1) - 0.1).abs() < 1e-5);
        assert!((wrap_pi(-0.1) - (PI - 0.1)).abs() < 1e-5);
    }

    #[test]
    fn angular_dist_pi_wraps() {
        assert!((angular_dist_pi(0.1, PI - 0.1) - 0.2).abs() < 1e-5);
        assert!((angular_dist_pi(0.0, FRAC_PI_2) - FRAC_PI_2).abs() < 1e-6);
    }

    #[test]
    fn smooth_5_preserves_total() {
        let hist = vec![0.0, 0.0, 16.0, 0.0, 0.0];
        let out = smooth_circular_5(&hist);
        let sum: f32 = out.iter().sum();
        assert!((sum - 16.0).abs() < 1e-4, "got {sum}");
    }

    #[test]
    fn pick_two_peaks_separates_orthogonal_peaks() {
        // 18 bins of 10°, two peaks at 0° (bin 0) and 90° (bin 9).
        let mut hist = vec![0.0_f32; 18];
        hist[0] = 100.0;
        hist[9] = 100.0;
        let smoothed = smooth_circular_5(&hist);
        let peaks = pick_two_peaks(
            &smoothed,
            200.0,
            &PeakPickOptions::new(0.02, 60.0_f32.to_radians()),
        )
        .expect("two peaks");
        let lo = peaks.0.min(peaks.1);
        let hi = peaks.0.max(peaks.1);
        assert!((lo).abs() < 0.1, "lo too far from 0: {lo}");
        assert!((hi - FRAC_PI_2).abs() < 0.1, "hi too far from π/2: {hi}");
    }

    #[test]
    fn pick_two_peaks_handles_plateau_at_boundary() {
        // Simulate mass split across bins 0 and n-1 — the near-π wrap
        // scenario that killed example8/example9 in testdata.
        let n = 18;
        let mut hist = vec![0.0_f32; n];
        hist[0] = 50.0;
        hist[n - 1] = 50.0;
        hist[9] = 100.0;
        let smoothed = smooth_circular_5(&hist);
        // Expect a peak near angle 0 and a peak at ~90°.
        let peaks = pick_two_peaks(
            &smoothed,
            200.0,
            &PeakPickOptions::new(0.02, 60.0_f32.to_radians()),
        )
        .expect("should recover two peaks despite the boundary plateau");
        let lo = peaks.0.min(peaks.1);
        let hi = peaks.0.max(peaks.1);
        assert!(
            lo.abs() < 0.2 || (PI - lo).abs() < 0.2,
            "low peak at wrong angle: {lo}"
        );
        assert!((hi - FRAC_PI_2).abs() < 0.2, "hi at wrong angle: {hi}");
    }

    #[test]
    fn two_means_converges_on_orthogonal_votes() {
        let votes: Vec<AngleVote> = (0..50)
            .flat_map(|_| {
                [
                    AngleVote {
                        angle: 0.1,
                        weight: 1.0,
                    },
                    AngleVote {
                        angle: FRAC_PI_2 - 0.1,
                        weight: 1.0,
                    },
                ]
            })
            .collect();
        let (c0, c1) = refine_2means_double_angle(&votes, [0.2, FRAC_PI_2 - 0.2], 10);
        assert!((c0 - 0.1).abs() < 0.05 || (c1 - 0.1).abs() < 0.05);
        assert!((c0 - (FRAC_PI_2 - 0.1)).abs() < 0.05 || (c1 - (FRAC_PI_2 - 0.1)).abs() < 0.05);
    }

    #[test]
    fn two_means_empty_returns_seed() {
        let seed = [0.1_f32, 1.2];
        let (c0, c1) = refine_2means_double_angle(&[], seed, 5);
        assert_eq!((c0, c1), (seed[0], seed[1]));
    }
}
