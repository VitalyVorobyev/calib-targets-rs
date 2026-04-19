/// Orientation clustering for ChESS-style per-corner axes.
///
/// Each input corner carries two local grid-axis directions
/// ([`Corner::axes`]). This module groups those axes into two dominant
/// cluster centers on [0, π) and assigns each corner a cluster label based
/// on which axis at the corner matches which cluster:
///
/// * `labels[i] = Some(0)` — `axes[0]` went to cluster 0 and `axes[1]` to
///   cluster 1 ("canonical" assignment).
/// * `labels[i] = Some(1)` — `axes[0]` went to cluster 1 and `axes[1]` to
///   cluster 0 ("swapped" assignment). Adjacent corners on a chessboard
///   canonicalize to opposite labels because the bright/dark sectors flip.
/// * `labels[i] = None` — the *larger* of the two within-assignment
///   angular distances exceeded `outlier_threshold_deg` (one of the axes
///   is too far from its chosen center to trust the pair).
///
/// The clustering is purely angular; no geometry or projective assumptions.
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
    /// Whether to use per-corner `strength` as part of each axis vote weight
    /// (multiplicative). When false, each axis votes with `1 / (1 + σ)` only.
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
    /// Per-corner label. `Some(0)` if the canonical pairing
    /// (`axes[0]`→c0, `axes[1]`→c1) is best *and* within tolerance;
    /// `Some(1)` if the swapped pairing is best; `None` if the larger
    /// residual of the winning assignment is above
    /// `outlier_threshold_deg`.
    pub labels: Vec<Option<usize>>,
    /// Sum of vote weights that landed in each cluster (including both
    /// axes across all corners that produced a non-outlier label).
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

/// One axis vote feeding into the histogram and the 2-means refinement.
#[derive(Clone, Copy, Debug)]
struct AxisVote {
    /// Angle wrapped to [0, π).
    angle: f32,
    /// Voting weight (`strength / (1 + σ)` when strength>0 and
    /// `use_weights`, or `1 / (1 + σ)` otherwise).
    weight: f32,
}

/// Cluster per-axis orientations into two dominant directions on [0, π).
///
/// Each corner contributes TWO votes (one per `Corner::axes[k]`). The
/// 2-means step iterates over the full 2×N vote set. After convergence,
/// each corner is labelled by comparing the "canonical" vs "swapped"
/// axis-to-center pairing; see [`OrientationClusteringResult`] for what
/// `Some(0)` / `Some(1)` / `None` mean.
pub fn cluster_orientations(
    corners: &[Corner],
    params: &OrientationClusteringParams,
) -> Option<OrientationClusteringResult> {
    let n = corners.len();
    if n == 0 || params.num_bins < 4 {
        warn!("n = {n} num_bins = {}", params.num_bins);
        return None;
    }

    // 1. Build per-axis votes, the smoothed histogram, and running totals
    //    in one pass.
    let SmoothedHistogramData {
        values: hist_smoothed,
        bin_centers,
        total_weight,
        votes,
    } = build_smoothed_histogram(corners, params)?;

    // 2. Find local maxima and group contiguous bins into support regions.
    let peaks = find_peaks(&hist_smoothed);
    if peaks.is_empty() {
        warn!("Orientation peaks not found");
        return None;
    }

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

    // 3. Pick the top-2 peak centers with sufficient angular separation.
    let mut phi1 = None;
    let mut phi2 = None;

    for sup in &supports {
        if phi1.is_none() {
            phi1 = Some(sup.refined_angle(&votes));
            continue;
        }
        let c1 = phi1.unwrap();
        let cand = sup.refined_angle(&votes);
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

    // 4. Circular 2-means refinement over per-axis votes (2N points, not N).
    //    Uses double-angle accumulation so undirected axes at the
    //    0°/180° seam combine without cancellation. This is the bug fix
    //    vs. v1: the previous implementation iterated per corner using
    //    a single `orientation` angle, which meant only one of the two
    //    axes at each corner influenced the centers.
    for _ in 0..params.max_iters {
        let mut sum_vec = [[0.0f32; 2], [0.0f32; 2]];
        let mut sum_w = [0.0f32; 2];
        let mut changed = false;

        for vote in &votes {
            let d0 = angular_dist_pi(vote.angle, centers[0]);
            let d1 = angular_dist_pi(vote.angle, centers[1]);
            let best = if d0 <= d1 { 0usize } else { 1usize };
            let best_dist = if best == 0 { d0 } else { d1 };

            // Only use votes within the outlier band for center updates —
            // otherwise far-off axes drag the centers away from the real
            // modes and the label assignment breaks. (See the
            // `marks_far_angles_as_outliers` test.)
            if best_dist > params.outlier_threshold_deg.to_radians() {
                continue;
            }

            // Double-angle: each vote contributes `e^{i·2θ}` so axes that
            // sit on opposite halves of the circle (θ and θ+π) add
            // coherently instead of cancelling.
            let two_theta = 2.0 * vote.angle;
            let vx = two_theta.cos();
            let vy = two_theta.sin();
            sum_vec[best][0] += vote.weight * vx;
            sum_vec[best][1] += vote.weight * vy;
            sum_w[best] += vote.weight;
        }

        for c in 0..2 {
            if sum_w[c] > 0.0 {
                let vx = sum_vec[c][0] / sum_w[c];
                let vy = sum_vec[c][1] / sum_w[c];
                let new_center = wrap_angle_pi(0.5 * vy.atan2(vx));
                if (new_center - centers[c]).abs() > 1e-6 {
                    changed = true;
                }
                centers[c] = new_center;
            }
        }

        if !changed {
            break;
        }
    }

    // 5. Per-corner labels: canonical vs swapped pairing. Both axes must
    //    agree (the *larger* of the two assignment distances must be
    //    within the outlier band); otherwise the corner is an outlier.
    let mut labels: Vec<Option<usize>> = vec![None; n];
    let mut cluster_weights = [0.0f32; 2];
    let outlier_rad = params.outlier_threshold_deg.to_radians();

    for i in 0..n {
        let corner = &corners[i];
        let a0 = wrap_angle_pi(corner.axes[0].angle);
        let a1 = wrap_angle_pi(corner.axes[1].angle);

        // Canonical: axes[0] -> centers[0], axes[1] -> centers[1].
        let d_can_0 = angular_dist_pi(a0, centers[0]);
        let d_can_1 = angular_dist_pi(a1, centers[1]);
        let worst_canonical = d_can_0.max(d_can_1);
        let total_canonical = d_can_0 + d_can_1;

        // Swapped: axes[0] -> centers[1], axes[1] -> centers[0].
        let d_sw_0 = angular_dist_pi(a0, centers[1]);
        let d_sw_1 = angular_dist_pi(a1, centers[0]);
        let worst_swapped = d_sw_0.max(d_sw_1);
        let total_swapped = d_sw_0 + d_sw_1;

        // Pick the assignment with smaller sum-of-distances (ties broken
        // by canonical, i.e. `<=`).
        let (label, worst) = if total_canonical <= total_swapped {
            (0usize, worst_canonical)
        } else {
            (1usize, worst_swapped)
        };

        if worst <= outlier_rad {
            labels[i] = Some(label);
            // Accumulate cluster weights from the two axis votes that
            // contributed (matching the canonical-vs-swapped decision).
            let w0 = axis_vote_weight(corner, 0, params.use_weights);
            let w1 = axis_vote_weight(corner, 1, params.use_weights);
            if label == 0 {
                cluster_weights[0] += w0;
                cluster_weights[1] += w1;
            } else {
                cluster_weights[1] += w0;
                cluster_weights[0] += w1;
            }
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

/// Per-axis voting weight. `strength` may be absent (defaulted to 0) on
/// legacy corners that never passed through the 0.6 adapter; in that case
/// we fall back to `1 / (1 + σ)` so those corners still influence the
/// histogram. When `use_weights` is false the `strength` multiplier is
/// disabled entirely.
fn axis_vote_weight(corner: &Corner, axis_slot: usize, use_weights: bool) -> f32 {
    let axis = &corner.axes[axis_slot];
    let sigma_term = 1.0 / (1.0 + axis.sigma.max(0.0));
    if use_weights {
        let s = corner.strength;
        if s > 0.0 {
            s * sigma_term
        } else {
            sigma_term
        }
    } else {
        sigma_term
    }
}

struct SmoothedHistogramData {
    values: Vec<f32>,
    bin_centers: Vec<f32>,
    total_weight: f32,
    votes: Vec<AxisVote>,
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
    let mut votes: Vec<AxisVote> = Vec::with_capacity(corners.len() * 2);

    for c in corners.iter() {
        for slot in 0..2 {
            let angle = wrap_angle_pi(c.axes[slot].angle);
            let weight = axis_vote_weight(c, slot, params.use_weights);
            if weight <= 0.0 {
                continue;
            }
            let bin = angle_to_bin(angle, params.num_bins);
            hist[bin] += weight;
            total_weight += weight;
            votes.push(AxisVote { angle, weight });
        }
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
        votes,
    })
}

#[derive(Clone, Debug)]
struct PeakSupport {
    bins: Vec<usize>,
    weight: f32,
    weighted_angle: f32,
    num_bins: usize,
}

impl PeakSupport {
    /// Weighted circular mean of every per-axis vote whose bin falls in
    /// the peak's support region. Uses double-angle accumulation so that
    /// votes near the 0°/180° seam (same undirected axis) do not cancel.
    fn refined_angle(&self, votes: &[AxisVote]) -> f32 {
        let mut sum = [0.0f32; 2];
        let mut w_sum = 0.0f32;
        for vote in votes {
            let bin = angle_to_bin(vote.angle, self.num_bins);
            if self.bins.contains(&bin) {
                let two_theta = 2.0 * vote.angle;
                sum[0] += vote.weight * two_theta.cos();
                sum[1] += vote.weight * two_theta.sin();
                w_sum += vote.weight;
            }
        }
        if w_sum > 0.0 {
            wrap_angle_pi(0.5 * sum[1].atan2(sum[0]))
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
        // Double-angle accumulation so the seam at 0°/180° does not
        // cancel the bins on opposite sides of the axis.
        let two_t = 2.0 * bin_centers[b];
        sum[0] += w * two_t.cos();
        sum[1] += w * two_t.sin();
    }
    let weighted_angle = wrap_angle_pi(0.5 * sum[1].atan2(sum[0]));

    PeakSupport {
        bins,
        weight,
        weighted_angle,
        num_bins: n,
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

/// Estimate a dominant grid axis from per-corner two-axis descriptors.
///
/// Uses double-angle accumulation on `axes[0]` (`axes[1]` is nominally
/// orthogonal by construction in chess-corners 0.6, so double-angle
/// space collapses the pair to a single mode). Weights are
/// `strength / (1 + σ₀)` with a fallback to `1 / (1 + σ₀)` when strength
/// is missing.
pub fn estimate_grid_axes_from_orientations(corners: &[Corner]) -> Option<f32> {
    if corners.is_empty() {
        return None;
    }

    let mut sum = Vector2::<f32>::zeros();
    let mut weight_sum = 0.0f32;

    for c in corners {
        let theta = c.axes[0].angle;
        let sigma_term = 1.0 / (1.0 + c.axes[0].sigma.max(0.0));
        let s = c.strength;
        let w = if s > 0.0 { s * sigma_term } else { sigma_term };
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
    use crate::AxisEstimate;
    use nalgebra::Point2;
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

    /// Test helper: build a corner whose axis 0 points along `theta` and
    /// whose axis 1 is orthogonal (theta + π/2). Both axes use the same
    /// small sigma so weights are equal.
    fn make_corner(theta: f32, strength: f32) -> Corner {
        Corner {
            position: Point2::new(0.0, 0.0),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: theta,
                    sigma: 0.05,
                },
                AxisEstimate {
                    angle: theta + FRAC_PI_2,
                    sigma: 0.05,
                },
            ],
            strength,
            ..Corner::default()
        }
    }

    /// Test helper: build a corner with axes explicitly swapped (axis 0
    /// at `theta + π/2`, axis 1 at `theta`). Used to assert that the
    /// "canonical vs swapped" label machinery flips in response to slot
    /// swaps at the corner level.
    fn make_corner_swapped(theta: f32, strength: f32) -> Corner {
        Corner {
            position: Point2::new(0.0, 0.0),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: theta + FRAC_PI_2,
                    sigma: 0.05,
                },
                AxisEstimate {
                    angle: theta,
                    sigma: 0.05,
                },
            ],
            strength,
            ..Corner::default()
        }
    }

    #[test]
    fn clusters_two_dominant_modes() {
        // Under the axes-only contract, clustering places its two
        // centers near the two grid-axis directions. All corners are
        // contributing votes to both peaks; the per-corner label flips
        // between `Some(0)` and `Some(1)` depending on which axis slot
        // lands on which center (i.e. the chessboard parity flip).
        //
        // Canonical corners have axes = [θ, θ+π/2] near [π/4, 3π/4].
        // Swapped corners have axes = [θ+π/2, θ] near [3π/4, π/4] —
        // same underlying directions, but with the slots swapped.
        let canonical_primaries = [FRAC_PI_4 - 0.05, FRAC_PI_4, FRAC_PI_4 + 0.04];
        let swapped_primaries = [FRAC_PI_4 - 0.03, FRAC_PI_4, FRAC_PI_4 + 0.02];

        let mut corners = Vec::new();
        for &theta in &canonical_primaries {
            corners.push(make_corner(theta, 1.0));
        }
        for &theta in &swapped_primaries {
            corners.push(make_corner_swapped(theta, 1.5));
        }

        let params = OrientationClusteringParams {
            max_iters: 5,
            ..Default::default()
        };

        let result = cluster_orientations(&corners, &params).expect("expected two clusters");
        assert_eq!(corners.len(), result.labels.len());

        // Centers must be roughly 90° apart and near the two injected
        // directions (modulo π).
        let separation = angular_dist_pi(result.centers[0], result.centers[1]);
        assert!(
            (separation - FRAC_PI_2).abs() < 0.2,
            "expected cluster centers ~90° apart, got {}",
            separation.to_degrees()
        );

        // Split the label vector into the canonical and swapped halves
        // — each half must share a single label, and the two halves
        // must disagree (parity flip).
        let labels_a: Vec<Option<usize>> = result.labels[..canonical_primaries.len()].to_vec();
        let labels_b: Vec<Option<usize>> = result.labels[canonical_primaries.len()..].to_vec();
        let same_a = labels_a.iter().all(|l| l.is_some() && *l == labels_a[0]);
        let same_b = labels_b.iter().all(|l| l.is_some() && *l == labels_b[0]);
        assert!(
            same_a,
            "canonical-order corners must share a label: {:?}",
            labels_a
        );
        assert!(
            same_b,
            "swapped-order corners must share a label: {:?}",
            labels_b
        );
        assert_ne!(
            labels_a[0], labels_b[0],
            "swapped-order corners must flip label relative to canonical"
        );
    }

    #[test]
    fn marks_far_angles_as_outliers() {
        // Two populations 90° apart plus one rogue corner whose axes sit
        // at (0, π/3) — 0 aligns with one of the peaks but π/3 is ~30° off
        // the other, placing the worst-of-two distance beyond the
        // outlier band.
        let mut corners = Vec::new();
        for _ in 0..5 {
            corners.push(make_corner(FRAC_PI_4, 1.0));
        }
        for _ in 0..5 {
            corners.push(make_corner_swapped(FRAC_PI_4, 1.0));
        }
        corners.push(Corner {
            position: Point2::new(0.0, 0.0),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: 0.0,
                    sigma: 0.05,
                },
                AxisEstimate {
                    angle: std::f32::consts::FRAC_PI_3,
                    sigma: 0.05,
                },
            ],
            strength: 1.0,
            ..Corner::default()
        });

        let result = cluster_orientations(&corners, &OrientationClusteringParams::default())
            .expect("clustering should succeed");

        assert_eq!(corners.len(), result.labels.len());
        // The first 10 should all be labelled; the rogue corner is the
        // only outlier.
        assert_eq!(
            corners.len() - 1,
            result.labels.iter().filter(|l| l.is_some()).count(),
            "labels = {:?}",
            result.labels
        );
        assert!(result.labels.last().unwrap().is_none());
    }

    #[test]
    fn returns_none_when_only_one_peak() {
        // All corners point at the same pair (0.1 rad, 0.1 + π/2 rad) so
        // the histogram has two strong peaks. Inject noise makes the
        // histogram collapse to a single wide peak instead — so we build
        // corners whose axes share a SINGLE angle (both slots 0 and 1 at
        // 0.1 rad) to force one histogram peak.
        let corners: Vec<Corner> = (0..6)
            .map(|_| Corner {
                position: Point2::new(0.0, 0.0),
                orientation_cluster: None,
                axes: [
                    AxisEstimate {
                        angle: 0.1,
                        sigma: 0.05,
                    },
                    AxisEstimate {
                        angle: 0.1,
                        sigma: 0.05,
                    },
                ],
                strength: 1.0,
                ..Corner::default()
            })
            .collect();
        assert!(cluster_orientations(&corners, &OrientationClusteringParams::default()).is_none());
    }
}
