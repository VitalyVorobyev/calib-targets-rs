/// Orientation clustering for ChESS-style angles in [0, π) (mod π).
///
/// Given a set of angles θ_k (diagonals of light squares) and optional
/// weights, this finds two dominant orientation clusters on the circle
/// and assigns each angle to cluster 0, 1, or None (outlier).
///
/// This is purely angular; no geometry or projective assumptions here.

use crate::Corner;
use std::f32::consts::{FRAC_PI_2, PI};

/// Parameters for orientation clustering.
#[derive(Clone, Debug)]
pub struct OrientationClusteringParams {
    /// Number of histogram bins on [0, π).
    pub num_bins: usize,
    /// Max k-means iterations.
    pub max_iters: usize,
    /// Minimal separation between initial peaks (radians).
    pub peak_min_separation: f32,
    /// Max allowed distance from both centers before marking as outlier (radians).
    pub outlier_threshold: f32,
    /// Minimal total weight required for a peak to be considered (fraction of total sum).
    pub min_peak_weight_fraction: f32,
    /// Whether to use weights when clustering.
    pub use_weights: bool,
}

impl Default for OrientationClusteringParams {
    fn default() -> Self {
        Self {
            num_bins: 90, // ~2° per bin
            max_iters: 10,
            peak_min_separation: 10f32.to_radians(),
            outlier_threshold: 30f32.to_radians(),
            min_peak_weight_fraction: 0.05, // 5% of total weight
            use_weights: true,
        }
    }
}

/// Result of orientation clustering.
#[derive(Clone, Debug)]
pub struct OrientationClusteringResult {
    /// Cluster centers in [0, π), indices 0 and 1.
    pub centers: [f32; 2],
    /// For each input angle, the assigned cluster (0 or 1) or None if outlier.
    pub labels: Vec<Option<usize>>,
    /// Sum of weights in each cluster (excluding outliers).
    pub cluster_weights: [f32; 2],
}

/// Cluster ChESS orientations into two dominant directions on [0, π).
///
/// `angles` must be radians; they will be wrapped to [0, π).
/// If `weights` is None, all weights are treated as 1.0.
pub fn cluster_orientations(
    corners: &[Corner],
    params: &OrientationClusteringParams,
) -> Option<OrientationClusteringResult> {
    let n = corners.len();
    if n == 0 || params.num_bins < 4 {
        return None;
    }

    // 1. Wrap angles and build histogram on [0, π).
    let mut hist = vec![0.0f32; params.num_bins];
    let mut total_weight = 0.0f32;

    for c in corners {
        let t = wrap_angle_pi(c.orientation);
        let bin = angle_to_bin(t, params.num_bins);
        let w = if params.use_weights { c.strength.max(0.0) } else { 1.0 };
        hist[bin] += w;
        total_weight += w;
    }

    if total_weight <= 0.0 {
        return None;
    }

    // 2. Smooth histogram (circular) with a small kernel.
    let hist_smoothed = smooth_circular_histogram(&hist);

    // 3. Find local maxima as peak candidates.
    let peaks = find_peaks(&hist_smoothed);

    if peaks.is_empty() {
        return None;
    }

    // 4. Pick top 2 peaks (by height), with minimal separation.
    let min_peak_weight = total_weight * params.min_peak_weight_fraction;
    let mut peaks_sorted = peaks;
    peaks_sorted.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Filter out very small peaks.
    peaks_sorted.retain(|p| p.value >= min_peak_weight);
    if peaks_sorted.len() < 2 {
        return None;
    }

    // Candidate centers from the two strongest peaks.
    let phi1 = bin_to_angle(peaks_sorted[0].bin, params.num_bins);
    let mut phi2 = None;

    for p in &peaks_sorted[1..] {
        let cand = bin_to_angle(p.bin, params.num_bins);
        let sep = angular_dist_pi(phi1, cand);
        if sep >= params.peak_min_separation {
            phi2 = Some(cand);
            break;
        }
    }

    let phi2 = match phi2 {
        Some(v) => v,
        None => return None, // only one strong mode or too close
    };

    let mut centers = [phi1, phi2];

    // 5. Circular 2-means refinement (with outliers).
    let mut labels: Vec<Option<usize>> = vec![None; n];

    for _ in 0..params.max_iters {
        // Assignment step.
        let mut changed = false;
        for (i, c) in corners.iter().enumerate() {
            let t = wrap_angle_pi(c.orientation);

            let d0 = angular_dist_pi(t, centers[0]);
            let d1 = angular_dist_pi(t, centers[1]);

            let (best_cluster, best_dist) = if d0 <= d1 { (0usize, d0) } else { (1usize, d1) };

            let new_label = if best_dist <= params.outlier_threshold {
                Some(best_cluster)
            } else {
                None
            };

            if labels[i] != new_label {
                labels[i] = new_label;
                changed = true;
            }
        }

        // Update step: recompute centers as circular means.
        let mut sum_vec = [[0.0f32; 2], [0.0f32; 2]];
        let mut sum_w = [0.0f32; 2];

        for (i, lbl) in labels.iter().enumerate() {
            if let Some(c) = lbl {
                let w = if params.use_weights { corners[i].strength.max(0.0) } else { 1.0 };
                let t = wrap_angle_pi(corners[i].orientation);
                let vx = t.cos();
                let vy = t.sin();
                sum_vec[*c][0] += w * vx;
                sum_vec[*c][1] += w * vy;
                sum_w[*c] += w;
            }
        }

        for c in 0..2 {
            if sum_w[c] > 0.0 {
                let vx = sum_vec[c][0] / sum_w[c];
                let vy = sum_vec[c][1] / sum_w[c];
                let new_center = vy.atan2(vx);
                centers[c] = wrap_angle_pi(new_center);
            }
        }

        if !changed {
            break;
        }
    }

    // Final cluster weights.
    let mut cluster_weights = [0.0f32; 2];
    for (i, lbl) in labels.iter().enumerate() {
        if let Some(c) = lbl {
            let w = if params.use_weights { corners[i].strength.max(0.0) } else { 1.0 };
            cluster_weights[*c] += w;
        }
    }

    Some(OrientationClusteringResult {
        centers,
        labels,
        cluster_weights,
    })
}

/// Wrap an angle to [0, π).
fn wrap_angle_pi(theta: f32) -> f32 {
    let mut t = theta % PI;
    if t < 0.0 {
        t += PI;
    }
    t
}

/// Smallest angular distance on the circle with period π (result in [0, π/2]).
fn angular_dist_pi(a: f32, b: f32) -> f32 {
    let mut d = a - b;
    // wrap to [-π/2, π/2]
    while d > FRAC_PI_2 {
        d -= PI;
    }
    while d < -FRAC_PI_2 {
        d += PI;
    }
    d.abs()
}

/// Convert angle in [0, π) to bin index.
fn angle_to_bin(theta: f32, num_bins: usize) -> usize {
    let t = wrap_angle_pi(theta);
    let x = t / PI * num_bins as f32;
    let mut idx = x.floor() as isize;
    if idx < 0 {
        idx = 0;
    }
    if idx as usize >= num_bins {
        idx = (num_bins - 1) as isize;
    }
    idx as usize
}

/// Convert bin index back to angle (center of bin).
fn bin_to_angle(bin: usize, num_bins: usize) -> f32 {
    let step = PI / num_bins as f32;
    (bin as f32 + 0.5) * step
}

/// Smooth a circular histogram with a small symmetric kernel.
fn smooth_circular_histogram(hist: &[f32]) -> Vec<f32> {
    let n = hist.len();
    if n == 0 {
        return Vec::new();
    }

    // Simple discrete Gaussian-like kernel: [1, 4, 6, 4, 1] / 16
    const K: [f32; 5] = [1.0, 4.0, 6.0, 4.0, 1.0];
    const K_SUM: f32 = 16.0;

    let mut out = vec![0.0f32; n];
    for i in 0..n {
        let mut acc = 0.0f32;
        for (k, &w) in K.iter().enumerate() {
            let offset = k as isize - 2;
            let j = ((i as isize + offset).rem_euclid(n as isize)) as usize;
            acc += w * hist[j];
        }
        out[i] = acc / K_SUM;
    }
    out
}

#[derive(Clone, Debug)]
struct Peak {
    bin: usize,
    value: f32,
}

/// Find local maxima on a circular 1D array.
fn find_peaks(hist: &[f32]) -> Vec<Peak> {
    let n = hist.len();
    let mut peaks = Vec::new();
    if n == 0 {
        return peaks;
    }

    for i in 0..n {
        let prev = hist[(i + n - 1) % n];
        let curr = hist[i];
        let next = hist[(i + 1) % n];

        if curr >= prev && curr >= next && curr > 0.0 {
            peaks.push(Peak {
                bin: i,
                value: curr,
            });
        }
    }

    peaks
}
