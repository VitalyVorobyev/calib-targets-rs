//! Global grid-direction recovery from per-feature dual-axis estimates.
//!
//! Given a set of features that each carry **two** undirected local
//! lattice-axis estimates (e.g. chess-corner x-junctions whose two
//! tanh-fit ridges approximate the local grid directions), this module
//! recovers the two global grid-direction centres `(Θ₀, Θ₁)` and
//! labels every feature by which of its two axes matches `Θ₀` vs `Θ₁`.
//!
//! It is the orientation-prior stage shared by every grid pipeline that
//! ingests dual-oriented features. Target-specific glue (which features
//! are eligible, how to map the canonical/swapped assignment onto a
//! caller's own label type, parity-coherence repair) stays caller-side;
//! this module is the pure direction-clustering math.
//!
//! **Tier:** stable facade — [`cluster_axes`] and its
//! [`AxisClusterCenters`] / [`AxisAssignment`] types are re-exported at the
//! crate root and follow normal semver intent.
//!
//! # Inputs / outputs
//!
//! * Input: a slice of [`AxisFeature`], each carrying its two
//!   [`AxisObservation`]s `(angle, sigma)` and a detector `strength`.
//!   Axes whose sigma is the no-info sentinel (≥ `π`) or non-finite are
//!   skipped. Callers pre-filter to the features they want to vote
//!   (e.g. chessboard passes only its `Strong` corners).
//! * Output:
//!   - [`AxisClusterCenters`] `{ theta0, theta1 }` in `[0, π)` with
//!     `theta0 ≤ theta1`.
//!   - A per-feature [`AxisAssignment`].
//!
//! # Algorithm
//!
//! 1. Build a smoothed circular histogram on `[0, π)` with
//!    `num_bins` bins. For every feature and every axis `k ∈ {0, 1}`,
//!    add a vote at `wrap_pi(axes[k].angle)` with weight
//!    `strength / (1 + axes[k].sigma)`.
//! 2. Smooth with a `[1, 4, 6, 4, 1] / 16` circular kernel.
//! 3. Find local maxima. Keep peaks with total weight ≥
//!    `min_peak_weight_fraction × total`. Pick the two strongest
//!    peaks separated by at least `peak_min_separation_rad`.
//! 4. Refine centres via **double-angle** 2-means over per-axis
//!    votes. Each axis vote `θ` is mapped to `2θ` before averaging;
//!    the average is halved back — this is the correct undirected-
//!    angle (mod π) circular mean. Iterate up to `max_iters_2means`.
//! 5. Per-feature label: for each feature compute the two possible
//!    axis assignments (canonical vs swapped) and pick the cheaper.
//!    Require the LARGER distance in the winning assignment to be
//!    within the per-feature tolerance; otherwise the feature is
//!    unassigned.

mod circular;

use std::f32::consts::PI;

use serde::Serialize;

pub use circular::{
    angle_to_bin, angular_dist_pi, bin_to_angle, pick_two_peaks, refine_2means_double_angle,
    smooth_circular_5, wrap_pi, AngleVote, PeakPickOptions,
};

/// One undirected local lattice-axis estimate feeding the clustering
/// stage: an angle in radians and its 1σ angular uncertainty.
///
/// Sigma at the no-info sentinel (≥ `π`) or non-finite marks an axis
/// with no usable direction information; such axes are skipped.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct AxisObservation {
    /// Axis angle in radians (interpreted on the mod-π circle).
    pub angle: f32,
    /// 1σ angular uncertainty in radians.
    pub sigma: f32,
}

impl AxisObservation {
    /// Construct an axis observation from its angle and 1σ uncertainty.
    pub fn new(angle: f32, sigma: f32) -> Self {
        Self { angle, sigma }
    }
}

/// A feature with two undirected local lattice-axis estimates plus a
/// detector strength used to weight its histogram votes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct AxisFeature {
    /// The feature's two local lattice-axis estimates.
    pub axes: [AxisObservation; 2],
    /// Detector response used as the histogram-vote base weight.
    pub strength: f32,
}

impl AxisFeature {
    /// Construct an axis feature from its two axes and detector strength.
    pub fn new(axes: [AxisObservation; 2], strength: f32) -> Self {
        Self { axes, strength }
    }
}

/// Tuning for [`cluster_axes`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct ClusterParams {
    /// Number of histogram bins spanning the `[0, π)` axis-angle range.
    pub num_bins: usize,
    /// Minimum fraction of total vote weight a peak must carry.
    pub min_peak_weight_fraction: f32,
    /// Minimum angular separation between the two peaks (radians, mod-π).
    pub peak_min_separation_rad: f32,
    /// Maximum 2-means refinement iterations.
    pub max_iters_2means: usize,
    /// Base per-feature admission tolerance in radians.
    pub base_tol_rad: f32,
    /// Multiplier on per-feature axis sigma added to [`Self::base_tol_rad`].
    pub cluster_sigma_k: f32,
}

impl ClusterParams {
    /// Construct clustering parameters from the individual knobs.
    pub fn new(
        num_bins: usize,
        min_peak_weight_fraction: f32,
        peak_min_separation_rad: f32,
        max_iters_2means: usize,
        base_tol_rad: f32,
        cluster_sigma_k: f32,
    ) -> Self {
        Self {
            num_bins,
            min_peak_weight_fraction,
            peak_min_separation_rad,
            max_iters_2means,
            base_tol_rad,
            cluster_sigma_k,
        }
    }
}

/// The two recovered global grid-direction centres in `[0, π)` with
/// `theta0 ≤ theta1`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub struct AxisClusterCenters {
    /// The smaller of the two direction centres (radians, `[0, π)`).
    pub theta0: f32,
    /// The larger of the two direction centres (radians, `[0, π)`).
    pub theta1: f32,
}

impl AxisClusterCenters {
    /// Construct cluster centres. The two centres are stored as given;
    /// callers that need the `theta0 ≤ theta1` ordering should sort
    /// first (e.g. via [`Self::sorted`]).
    pub fn new(theta0: f32, theta1: f32) -> Self {
        Self { theta0, theta1 }
    }

    /// Construct cluster centres, ordering them so `theta0 ≤ theta1`.
    pub fn sorted(a: f32, b: f32) -> Self {
        if a <= b {
            Self {
                theta0: a,
                theta1: b,
            }
        } else {
            Self {
                theta0: b,
                theta1: a,
            }
        }
    }
}

/// Per-feature assignment produced by [`cluster_axes`].
///
/// The variants record which slot ordering matched and the worst
/// per-axis distance in the winning assignment.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[non_exhaustive]
pub enum AxisAssignment {
    /// `axes[0] → Θ₀`, `axes[1] → Θ₁`; both within tolerance.
    Canonical {
        /// Worst per-axis distance to its matched centre (radians).
        max_d_rad: f32,
    },
    /// `axes[0] → Θ₁`, `axes[1] → Θ₀`; both within tolerance.
    Swapped {
        /// Worst per-axis distance to its matched centre (radians).
        max_d_rad: f32,
    },
    /// The best assignment still left one axis outside tolerance.
    None {
        /// Worst per-axis distance to its matched centre (radians).
        max_d_rad: f32,
    },
}

/// Stage introspection captured during a single [`cluster_axes`] run.
///
/// Surfaced so an offline tool can plot the histogram and check whether
/// 2-means refinement walked off the visible peaks.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct AxisClusterDebug {
    /// Number of histogram bins spanning the `[0, π)` axis-angle range.
    pub num_bins: usize,
    /// Raw per-bin weighted vote counts before smoothing.
    pub histogram: Vec<f32>,
    /// The histogram after circular smoothing — the curve peak-picking runs on.
    pub smoothed: Vec<f32>,
    /// Sum of all bin weights — the normaliser for the peak-weight floor.
    pub total_weight: f32,
    /// Peak seeds picked from the smoothed histogram, in radians (`[0, π)`),
    /// before 2-means refinement. `None` when peak picking failed.
    pub peak_seeds_rad: Option<[f32; 2]>,
    /// Centres after 2-means refinement, in radians. `None` when peak
    /// picking failed (refinement isn't run).
    pub refined_centers_rad: Option<[f32; 2]>,
}

impl AxisClusterDebug {
    fn empty(num_bins: usize) -> Self {
        Self {
            num_bins,
            histogram: Vec::new(),
            smoothed: Vec::new(),
            total_weight: 0.0,
            peak_seeds_rad: None,
            refined_centers_rad: None,
        }
    }
}

/// Recover the two global grid-direction centres from a slice of
/// dual-axis features and assign each feature to a slot ordering.
///
/// Returns `(centres, per_feature_assignments, debug)`. `centres` is
/// `None` when fewer than two qualifying peaks were found; in that case
/// the assignment vector is empty. Otherwise the assignment vector has
/// one [`AxisAssignment`] per input feature, in input order.
///
/// The histogram and votes iterate the features in the given order;
/// callers wanting byte-stable results across runs must pass features
/// in a stable order.
pub fn cluster_axes(
    features: &[AxisFeature],
    params: &ClusterParams,
) -> (
    Option<AxisClusterCenters>,
    Vec<AxisAssignment>,
    AxisClusterDebug,
) {
    let mut debug = AxisClusterDebug::empty(params.num_bins);

    if features.is_empty() || params.num_bins < 4 {
        return (None, Vec::new(), debug);
    }

    let hist = build_histogram(features, params.num_bins);
    debug.histogram = hist.bins.clone();
    debug.total_weight = hist.total_weight;
    if hist.total_weight <= 0.0 {
        return (None, Vec::new(), debug);
    }

    let smoothed = smooth_circular_5(&hist.bins);
    debug.smoothed = smoothed.clone();

    let peak_opts = PeakPickOptions::new(
        params.min_peak_weight_fraction,
        params.peak_min_separation_rad,
    );
    let Some((theta0_seed, theta1_seed)) = pick_two_peaks(&smoothed, hist.total_weight, &peak_opts)
    else {
        return (None, Vec::new(), debug);
    };
    debug.peak_seeds_rad = Some([theta0_seed, theta1_seed]);

    let votes = collect_axis_votes(features);
    let (theta0, theta1) =
        refine_2means_double_angle(&votes, [theta0_seed, theta1_seed], params.max_iters_2means);
    debug.refined_centers_rad = Some([theta0, theta1]);

    let centers = AxisClusterCenters::sorted(theta0, theta1);

    let mut assignments = Vec::with_capacity(features.len());
    for feature in features {
        let tol_rad = effective_tol_rad(&feature.axes, params.base_tol_rad, params.cluster_sigma_k);
        assignments.push(assign_axes(&feature.axes, centers, tol_rad));
    }

    (Some(centers), assignments, debug)
}

// --- internals ------------------------------------------------------------

struct Histogram {
    bins: Vec<f32>,
    total_weight: f32,
}

fn build_histogram(features: &[AxisFeature], num_bins: usize) -> Histogram {
    let mut bins = vec![0.0_f32; num_bins];
    let mut total = 0.0_f32;
    for feature in features {
        for axis in &feature.axes {
            if !axis.sigma.is_finite() || axis.sigma >= PI - f32::EPSILON {
                // No-info sentinel → skip this axis.
                continue;
            }
            let w = vote_weight(feature.strength, axis.sigma);
            if w <= 0.0 {
                continue;
            }
            let bin = angle_to_bin(wrap_pi(axis.angle), num_bins);
            bins[bin] += w;
            total += w;
        }
    }
    Histogram {
        bins,
        total_weight: total,
    }
}

#[inline]
fn vote_weight(strength: f32, sigma: f32) -> f32 {
    let s = strength.max(0.0);
    let base = if s > 0.0 { s } else { 1.0 };
    base / (1.0 + sigma.max(0.0))
}

/// Materialise per-axis votes in the shape expected by the
/// [`refine_2means_double_angle`] helper.
fn collect_axis_votes(features: &[AxisFeature]) -> Vec<AngleVote> {
    let mut votes: Vec<AngleVote> = Vec::new();
    for feature in features {
        for axis in &feature.axes {
            if !axis.sigma.is_finite() || axis.sigma >= PI - f32::EPSILON {
                continue;
            }
            let w = vote_weight(feature.strength, axis.sigma);
            if w <= 0.0 {
                continue;
            }
            votes.push(AngleVote {
                angle: wrap_pi(axis.angle),
                weight: w,
            });
        }
    }
    votes
}

/// Per-feature cluster admission threshold in radians.
///
/// `base_tol_rad + min(sigma_k · max(σ_a0, σ_a1), 3°)` — sigma bonus is
/// capped so a single noisy feature cannot blow open the gate. Sigmas
/// at the no-info sentinel (≈ π) are clamped to a finite ceiling (10°).
#[inline]
pub fn effective_tol_rad(axes: &[AxisObservation; 2], base_tol_rad: f32, sigma_k: f32) -> f32 {
    if sigma_k <= 0.0 {
        return base_tol_rad;
    }
    let sigma_cap = 10.0_f32.to_radians();
    let s0 = axes[0].sigma.clamp(0.0, sigma_cap);
    let s1 = axes[1].sigma.clamp(0.0, sigma_cap);
    let bonus = sigma_k * s0.max(s1);
    let max_bonus = 3.0_f32.to_radians();
    base_tol_rad + bonus.min(max_bonus)
}

/// Assign one feature's two axes to a slot ordering given known centres.
///
/// Computes the canonical (`axes[0]→Θ₀, axes[1]→Θ₁`) and swapped costs,
/// picks the cheaper (ties → canonical), and admits it when the worst
/// per-axis distance in the winning assignment is within `tol_rad`.
pub fn assign_axes(
    axes: &[AxisObservation; 2],
    centers: AxisClusterCenters,
    tol_rad: f32,
) -> AxisAssignment {
    let a0 = wrap_pi(axes[0].angle);
    let a1 = wrap_pi(axes[1].angle);

    let d_a0_t0 = angular_dist_pi(a0, centers.theta0);
    let d_a0_t1 = angular_dist_pi(a0, centers.theta1);
    let d_a1_t0 = angular_dist_pi(a1, centers.theta0);
    let d_a1_t1 = angular_dist_pi(a1, centers.theta1);

    let canon_cost = d_a0_t0 + d_a1_t1;
    let canon_max = d_a0_t0.max(d_a1_t1);
    let swap_cost = d_a0_t1 + d_a1_t0;
    let swap_max = d_a0_t1.max(d_a1_t0);

    let (canonical, max_d) = if canon_cost <= swap_cost {
        (true, canon_max)
    } else {
        (false, swap_max)
    };

    if max_d <= tol_rad {
        if canonical {
            AxisAssignment::Canonical { max_d_rad: max_d }
        } else {
            AxisAssignment::Swapped { max_d_rad: max_d }
        }
    } else {
        AxisAssignment::None { max_d_rad: max_d }
    }
}

/// Refit cluster centres from a labelled subset's axes only.
///
/// For each feature in `axes`, pick the slot assignment (canonical /
/// swapped) that minimises the cost under `old_centers` — same tie-break
/// as [`assign_axes`] — to decide which of its two axes belongs to slot 0
/// vs slot 1. Accumulate `(cos 2θ, sin 2θ)` per slot (undirected circular
/// mean), halve the atan2, wrap to `[0, π)`, and order so `θ0 ≤ θ1`.
///
/// Returns `None` if `axes.len() < min_samples` (the caller should keep
/// the original centres).
pub fn refit_centers(
    axes: &[[AxisObservation; 2]],
    old_centers: AxisClusterCenters,
    min_samples: usize,
) -> Option<AxisClusterCenters> {
    if axes.len() < min_samples {
        return None;
    }
    let mut s0_re = 0.0_f32;
    let mut s0_im = 0.0_f32;
    let mut s1_re = 0.0_f32;
    let mut s1_im = 0.0_f32;
    for pair in axes {
        let a0 = wrap_pi(pair[0].angle);
        let a1 = wrap_pi(pair[1].angle);
        let d_a0_t0 = angular_dist_pi(a0, old_centers.theta0);
        let d_a0_t1 = angular_dist_pi(a0, old_centers.theta1);
        let d_a1_t0 = angular_dist_pi(a1, old_centers.theta0);
        let d_a1_t1 = angular_dist_pi(a1, old_centers.theta1);
        let canon_cost = d_a0_t0 + d_a1_t1;
        let swap_cost = d_a0_t1 + d_a1_t0;
        let (a_to_t0, a_to_t1) = if canon_cost <= swap_cost {
            (a0, a1)
        } else {
            (a1, a0)
        };
        s0_re += (2.0 * a_to_t0).cos();
        s0_im += (2.0 * a_to_t0).sin();
        s1_re += (2.0 * a_to_t1).cos();
        s1_im += (2.0 * a_to_t1).sin();
    }
    let mut t0 = 0.5 * s0_im.atan2(s0_re);
    let mut t1 = 0.5 * s1_im.atan2(s1_re);
    while t0 < 0.0 {
        t0 += PI;
    }
    while t0 >= PI {
        t0 -= PI;
    }
    while t1 < 0.0 {
        t1 += PI;
    }
    while t1 >= PI {
        t1 -= PI;
    }
    if t0 > t1 {
        std::mem::swap(&mut t0, &mut t1);
    }
    Some(AxisClusterCenters {
        theta0: t0,
        theta1: t1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(angle: f32, sigma: f32) -> AxisObservation {
        AxisObservation::new(angle, sigma)
    }

    fn feat(a0_deg: f32, sigma_deg: f32, strength: f32) -> AxisFeature {
        let a0 = a0_deg.to_radians();
        let a1 = a0 + std::f32::consts::FRAC_PI_2;
        let sigma = sigma_deg.to_radians();
        AxisFeature::new([obs(wrap_pi(a0), sigma), obs(wrap_pi(a1), sigma)], strength)
    }

    fn default_params() -> ClusterParams {
        ClusterParams::new(
            90,
            0.02,
            60.0_f32.to_radians(),
            10,
            12.0_f32.to_radians(),
            0.5,
        )
    }

    fn jitter(i: usize, amp_deg: f32) -> f32 {
        let x = (i as u32).wrapping_mul(2_654_435_761);
        let frac = ((x >> 8) as f32) / ((1u32 << 24) as f32);
        (frac - 0.5) * amp_deg
    }

    #[test]
    fn recovers_centers_30_120() {
        let mut features = Vec::new();
        for i in 0..50 {
            features.push(feat(30.0 + jitter(i, 10.0), 0.05, 1.0));
        }
        for i in 0..50 {
            features.push(feat(120.0 + jitter(i + 1000, 10.0), 0.05, 1.0));
        }
        let params = default_params();
        let (centers, assignments, _dbg) = cluster_axes(&features, &params);
        let centers = centers.expect("centers");
        let expected_low = 30.0_f32.to_radians();
        let expected_high = 120.0_f32.to_radians();
        assert!(angular_dist_pi(centers.theta0, expected_low) < 2.0_f32.to_radians());
        assert!(angular_dist_pi(centers.theta1, expected_high) < 2.0_f32.to_radians());
        assert!(assignments
            .iter()
            .all(|a| !matches!(a, AxisAssignment::None { .. })));
    }

    #[test]
    fn far_feature_is_unassigned() {
        let mut features = Vec::new();
        for _ in 0..40 {
            features.push(feat(0.0, 0.01_f32.to_degrees(), 1.0));
        }
        features.push(feat(25.0, 0.01_f32.to_degrees(), 1.0));
        let params = default_params();
        let (_centers, assignments, _dbg) = cluster_axes(&features, &params);
        assert!(matches!(
            assignments.last().expect("non-empty"),
            AxisAssignment::None { .. }
        ));
    }

    #[test]
    fn empty_input_returns_none() {
        let params = default_params();
        let (centers, assignments, _dbg) = cluster_axes(&[], &params);
        assert!(centers.is_none());
        assert!(assignments.is_empty());
    }
}
