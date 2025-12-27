/// Orientation clustering for ChESS-style angles in [0, π) (mod π).
///
/// Given a set of angles θ_k (diagonals of light squares) and optional
/// weights, this finds two dominant orientation clusters on the circle
/// and assigns each angle to cluster 0, 1, or None (outlier).
///
/// This is purely angular; no geometry or projective assumptions here.
use crate::Corner;
use log::warn;
use nalgebra::Vector2;
use serde::{Deserialize, Serialize};
use std::f32::consts::{FRAC_PI_2, PI};

/// Parameters for orientation clustering.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OrientationClusteringParams {
    /// Number of histogram bins on [0, π).
    pub num_bins: usize,
    /// Max k-means iterations.
    pub max_iters: usize,
    /// Minimal separation between initial peaks (degrees).
    pub peak_min_separation_deg: f32,
    /// Max allowed distance from both centers before marking as outlier (degrees).
    pub outlier_threshold_deg: f32,
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
            peak_min_separation_deg: 10f32,
            outlier_threshold_deg: 30f32,
            min_peak_weight_fraction: 0.05, // 5% of total weight
            use_weights: true,
        }
    }
}

/// Result of orientation clustering.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrientationClusteringResult {
    /// Cluster centers in [0, π), indices 0 and 1.
    pub centers: [f32; 2],
    /// For each input angle, the assigned cluster (0 or 1) or None if outlier.
    pub labels: Vec<Option<usize>>,
    /// Sum of weights in each cluster (excluding outliers).
    pub cluster_weights: [f32; 2],
    /// Smoothed histogram for debugging/visualization.
    pub histogram: Option<OrientationHistogram>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrientationHistogram {
    pub bin_centers: Vec<f32>,
    pub values: Vec<f32>,
}

/// Compute smoothed orientation histogram for debug/visualization.
pub fn compute_orientation_histogram(
    corners: &[Corner],
    params: &OrientationClusteringParams,
) -> Option<OrientationHistogram> {
    build_smoothed_histogram(corners, params).map(|h| OrientationHistogram {
        bin_centers: h.bin_centers,
        values: h.values,
    })
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
        warn!("n = {n} num_bins = {}", params.num_bins);
        return None;
    }

    // 1. Wrap angles and build histogram on [0, π) once (shared with debug output).
    let SmoothedHistogramData {
        values: hist_smoothed,
        bin_centers,
        total_weight,
        corner_bins,
    } = build_smoothed_histogram(corners, params)?;

    // 3. Find local maxima as peak candidates.
    let peaks = find_peaks(&hist_smoothed);
    if peaks.is_empty() {
        warn!("Orientation peaks not found");
        return None;
    }

    // 4. Group peaks (contiguous bins) and pick top 2 by total weight, with minimal separation.
    let mut supports: Vec<PeakSupport> = peaks
        .into_iter()
        .map(|p| build_peak_support(&hist_smoothed, p.bin, &bin_centers))
        .collect();

    let min_peak_weight = total_weight * params.min_peak_weight_fraction;
    supports.retain(|p| p.weight >= min_peak_weight);
    supports.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if supports.len() < 2 {
        warn!(
            "{} grouped peaks, total {total_weight:.2}, min {min_peak_weight:.2}",
            supports.len()
        );
        return None;
    }

    let mut phi1 = None;
    let mut phi2 = None;

    for sup in &supports {
        if phi1.is_none() {
            phi1 = Some(sup.angle_from_corners(corners, &corner_bins, params));
            continue;
        }
        let c1 = phi1.unwrap();
        let cand = sup.angle_from_corners(corners, &corner_bins, params);
        if angular_dist_pi(c1, cand) >= params.peak_min_separation_deg.to_radians() {
            phi2 = Some(cand);
            break;
        }
    }

    let (phi1, phi2) = match (phi1, phi2) {
        (Some(a), Some(b)) => (a, b),
        _ => return None,
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

            let new_label = if best_dist <= params.outlier_threshold_deg.to_radians() {
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
                let w = if params.use_weights {
                    corners[i].strength.max(0.0)
                } else {
                    1.0
                };
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
            let w = if params.use_weights {
                corners[i].strength.max(0.0)
            } else {
                1.0
            };
            cluster_weights[*c] += w;
        }
    }

    Some(OrientationClusteringResult {
        centers,
        labels,
        cluster_weights,
        histogram: Some(OrientationHistogram {
            bin_centers,
            values: hist_smoothed,
        }),
    })
}

struct SmoothedHistogramData {
    values: Vec<f32>,
    bin_centers: Vec<f32>,
    total_weight: f32,
    corner_bins: Vec<usize>,
}

fn build_smoothed_histogram(
    corners: &[Corner],
    params: &OrientationClusteringParams,
) -> Option<SmoothedHistogramData> {
    if params.num_bins < 1 {
        return None;
    }

    let mut hist = vec![0.0f32; params.num_bins];
    let mut total_weight = 0.0f32;
    let mut corner_bins = Vec::with_capacity(corners.len());

    for c in corners {
        let t = wrap_angle_pi(c.orientation);
        let bin = angle_to_bin(t, params.num_bins);
        let w = if params.use_weights {
            c.strength.max(0.0)
        } else {
            1.0
        };
        hist[bin] += w;
        total_weight += w;
        corner_bins.push(bin);
    }

    if total_weight <= 0.0 {
        return None;
    }

    let values = smooth_circular_histogram(&hist);
    let bin_centers: Vec<f32> = (0..params.num_bins)
        .map(|b| bin_to_angle(b, params.num_bins))
        .collect();

    Some(SmoothedHistogramData {
        values,
        bin_centers,
        total_weight,
        corner_bins,
    })
}

#[derive(Clone, Debug)]
struct PeakSupport {
    bins: Vec<usize>,
    weight: f32,
    weighted_angle: f32,
}

impl PeakSupport {
    fn angle_from_corners(
        &self,
        corners: &[Corner],
        corner_bins: &[usize],
        params: &OrientationClusteringParams,
    ) -> f32 {
        let mut sum = [0.0f32; 2];
        let mut w_sum = 0.0f32;
        for (corner, &bin) in corners.iter().zip(corner_bins.iter()) {
            if self.bins.contains(&bin) {
                let w = if params.use_weights {
                    corner.strength.max(0.0)
                } else {
                    1.0
                };
                let t = wrap_angle_pi(corner.orientation);
                sum[0] += w * t.cos();
                sum[1] += w * t.sin();
                w_sum += w;
            }
        }

        if w_sum > 0.0 {
            wrap_angle_pi(sum[1].atan2(sum[0]))
        } else {
            self.weighted_angle
        }
    }
}

fn build_peak_support(hist: &[f32], peak_bin: usize, bin_centers: &[f32]) -> PeakSupport {
    let n = hist.len();
    let mut bins = vec![peak_bin];

    // expand left
    let mut i = (peak_bin + n - 1) % n;
    while hist[i] <= hist[(i + 1) % n] && hist[i] > 0.0 {
        bins.push(i);
        i = (i + n - 1) % n;
        if i == peak_bin {
            break;
        }
    }

    // expand right
    let mut i = (peak_bin + 1) % n;
    while hist[i] <= hist[(i + n - 1) % n] && hist[i] > 0.0 {
        bins.push(i);
        i = (i + 1) % n;
        if i == peak_bin {
            break;
        }
    }

    bins.sort();
    bins.dedup();

    let mut weight = 0.0f32;
    let mut sum = [0.0f32; 2];
    for &b in &bins {
        let w = hist[b];
        weight += w;
        let t = bin_centers[b];
        sum[0] += w * t.cos();
        sum[1] += w * t.sin();
    }
    let weighted_angle = wrap_angle_pi(sum[1].atan2(sum[0]));

    PeakSupport {
        bins,
        weight,
        weighted_angle,
    }
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
    for (i, item) in out.iter_mut().enumerate() {
        let mut acc = 0.0f32;
        for (k, &w) in K.iter().enumerate() {
            let offset = k as isize - 2;
            let j = ((i as isize + offset).rem_euclid(n as isize)) as usize;
            acc += w * hist[j];
        }
        *item = acc / K_SUM;
    }
    out
}

#[derive(Clone, Debug)]
struct Peak {
    bin: usize,
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
            peaks.push(Peak { bin: i });
        }
    }

    peaks
}

/// Estimate a dominant grid axis from orientations using a double-angle mean.
pub fn estimate_grid_axes_from_orientations(corners: &[Corner]) -> Option<f32> {
    if corners.is_empty() {
        return None;
    }

    // Accumulate in double-angle space to handle θ ≡ θ + π.
    let mut sum = Vector2::<f32>::zeros();
    let mut weight_sum = 0.0f32;

    for c in corners {
        let theta = c.orientation;
        let w = c.strength.max(0.0);
        if w <= 0.0 {
            continue;
        }

        let two_theta = 2.0 * theta;
        let v = Vector2::new(two_theta.cos(), two_theta.sin());
        sum += w * v;
        weight_sum += w;
    }

    if weight_sum <= 0.0 {
        return None;
    }

    let mean = sum / weight_sum;
    if mean.norm_squared() < 1e-6 {
        return None;
    }

    let mean_two_angle = mean.y.atan2(mean.x);
    let mean_theta = 0.5 * mean_two_angle;
    Some(mean_theta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Point2;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

    fn make_corner(theta: f32, strength: f32) -> Corner {
        Corner {
            position: Point2::new(0.0, 0.0),
            orientation: theta,
            orientation_cluster: None,
            strength,
        }
    }

    #[test]
    fn clusters_two_dominant_modes() {
        let cluster_a = [
            FRAC_PI_4 - 0.05,
            FRAC_PI_4,
            FRAC_PI_4 + 0.04,
            FRAC_PI_4 + 0.02,
        ];
        let cluster_b = [
            3.0 * FRAC_PI_4 - 0.03,
            3.0 * FRAC_PI_4,
            3.0 * FRAC_PI_4 + 0.02,
            3.0 * FRAC_PI_4 + 0.04,
        ];

        let mut corners = Vec::new();
        for &theta in &cluster_a {
            corners.push(make_corner(theta, 1.0));
        }
        for &theta in &cluster_b {
            corners.push(make_corner(theta, 1.5));
        }

        let params = OrientationClusteringParams {
            max_iters: 5,
            ..Default::default()
        };

        let result = cluster_orientations(&corners, &params).expect("expected two clusters");
        assert_eq!(corners.len(), result.labels.len());

        let center_a = if angular_dist_pi(result.centers[0], cluster_a[0])
            < angular_dist_pi(result.centers[1], cluster_a[0])
        {
            0
        } else {
            1
        };
        let center_b = 1 - center_a;

        for lbl in result.labels.iter().take(cluster_a.len()) {
            assert_eq!(Some(center_a), *lbl);
        }
        for lbl in result.labels.iter().skip(cluster_a.len()) {
            assert_eq!(Some(center_b), *lbl);
        }

        let separation = angular_dist_pi(result.centers[0], result.centers[1]);
        assert!((separation - FRAC_PI_2).abs() < 0.2);
    }

    #[test]
    fn marks_far_angles_as_outliers() {
        let mut corners = Vec::new();
        for _ in 0..5 {
            corners.push(make_corner(FRAC_PI_4, 1.0));
        }
        for _ in 0..5 {
            corners.push(make_corner(3.0 * FRAC_PI_4, 1.0));
        }
        corners.push(make_corner(0.0, 1.0)); // outlier

        let result = cluster_orientations(&corners, &OrientationClusteringParams::default())
            .expect("clustering should succeed");

        assert_eq!(corners.len(), result.labels.len());
        assert_eq!(
            corners.len() - 1,
            result.labels.iter().filter(|l| l.is_some()).count()
        );
        assert!(result.labels.last().unwrap().is_none());
    }

    #[test]
    fn returns_none_when_only_one_peak() {
        let corners = vec![make_corner(0.1, 1.0); 6];
        assert!(cluster_orientations(&corners, &OrientationClusteringParams::default()).is_none());
    }
}
