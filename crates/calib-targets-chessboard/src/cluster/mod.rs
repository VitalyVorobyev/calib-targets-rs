//! Axes-based orientation clustering for the detector.
//!
//! Computes the two global grid-direction centers `(Θ₀, Θ₁)` from the
//! per-corner `axes[0]` and `axes[1]` angles, then labels every
//! corner by matching its two axes against those two centers.
//!
//! # Why this differs from the workspace-level `cluster_orientations`
//!
//! `calib_targets_core::cluster_orientations` (post Phase-0 migration)
//! also clusters using axes. This module reuses its per-corner
//! `axes[0]` / `axes[1]` contributions but is **self-contained** —
//! This module keeps its own histogram + 2-means implementation so its
//! per-stage debug surface is decoupled from the shared helper. The
//! algorithm is identical (double-angle circular mean over per-axis
//! votes; the double-angle trick is mandatory for undirected angles
//! modulo π).
//!
//! # Inputs / outputs
//!
//! * Input: a slice of [`CornerAug`] whose `axes` field is
//!   populated. Axes with sigma equal to the no-info sentinel (π)
//!   are skipped.
//! * Output:
//!   - `ClusterCenters { theta0, theta1 }` in `[0, π)` with
//!     `theta0 < theta1`.
//!   - A per-corner [`AxisCluster`] assignment.
//!
//! # Algorithm
//!
//! 1. Build a smoothed circular histogram on `[0, π)` with
//!    `num_bins` bins. For every corner and every axis `k ∈ {0, 1}`,
//!    add a vote at `wrap_pi(axes[k].angle)` with weight
//!    `strength / (1 + axes[k].sigma)`.
//! 2. Smooth with a `[1, 4, 6, 4, 1] / 16` circular kernel.
//! 3. Find local maxima. Keep peaks with total weight ≥
//!    `min_peak_weight_fraction × total`. Pick the two strongest
//!    peaks separated by at least `peak_min_separation_deg`.
//! 4. Refine centers via **double-angle** 2-means over per-axis
//!    votes. Each axis vote `θ` is mapped to `2θ` before averaging;
//!    the average is halved back — this is the correct undirected-
//!    angle (mod π) circular mean. Iterate up to `max_iters`.
//! 5. Per-corner label: for each corner, compute the two possible
//!    axis assignments (canonical vs swapped) and pick the cheaper.
//!    Require the LARGER distance in the winning assignment to be
//!    within `cluster_tol_deg`; otherwise the corner is unclustered.

use crate::circular_stats as cs;
use crate::corner::{CornerAug, CornerStage};
use crate::params::DetectorParams;
use serde::Serialize;
use std::f32::consts::PI;

mod assign;
mod slot_coherence;

pub(crate) use assign::effective_tol_rad;
pub use assign::{assign_corner, refit_centers_from_labelled, AxisCluster};
use slot_coherence::fix_axis_slot_coherence;
pub(crate) use slot_coherence::fix_partial_slot_flips_post_stage6;

// Re-export the hoisted angle helpers under their old local names so
// sibling modules (`seed`, `grow`, `boosters`) keep their existing
// `use crate::cluster::{angular_dist_pi, wrap_pi, ...}` imports.
pub(crate) use crate::circular_stats::{angular_dist_pi, wrap_pi};

/// Result of clustering: two grid-direction centers in `[0, π)`
/// with `theta0 < theta1`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct ClusterCenters {
    pub theta0: f32,
    pub theta1: f32,
}

/// Stage-3 introspection captured during a single `cluster_axes_debug`
/// run. Surfaced through `DebugFrame` so an offline tool can plot the
/// histogram and check whether 2-means refinement walked off the
/// visible peaks. Local-only: never serialized into a public report.
#[derive(Clone, Debug, Serialize)]
pub struct ClusterDebug {
    /// Number of histogram bins spanning the `[0, π)` axis-angle range.
    pub num_bins: usize,
    /// Raw per-bin weighted vote counts before smoothing.
    pub histogram: Vec<f32>,
    /// The histogram after circular smoothing — the curve peak-picking runs on.
    pub smoothed: Vec<f32>,
    /// Sum of all bin weights — the normalizer for the peak-weight floor.
    pub total_weight: f32,
    /// Peak seeds picked from the smoothed histogram, in radians (`[0, π)`),
    /// before 2-means refinement. `None` when peak picking failed.
    pub peak_seeds_rad: Option<[f32; 2]>,
    /// Centers after 2-means refinement, in radians. `None` when peak
    /// picking failed (refinement isn't run).
    pub refined_centers_rad: Option<[f32; 2]>,
}

/// Run clustering over a slice of [`CornerAug`]. Mutates each
/// corner's `stage` and `label` fields in place.
///
/// Returns `Some(centers)` on success, `None` when fewer than two
/// qualifying peaks were found (the detector should return no
/// detection in that case).
///
/// Thin wrapper over [`cluster_axes_debug`]; callers wanting the
/// histogram + peak seeds should call `cluster_axes_debug` directly.
pub fn cluster_axes(corners: &mut [CornerAug], params: &DetectorParams) -> Option<ClusterCenters> {
    cluster_axes_debug(corners, params).0
}

/// Same as [`cluster_axes`] but also returns a [`ClusterDebug`] payload
/// with the smoothed histogram and the peak seeds — useful for offline
/// triage of clustering failures. The caller pays the cost of carrying
/// the histogram (a few KB).
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "debug", skip_all, fields(num_corners = corners.len()))
)]
pub fn cluster_axes_debug(
    corners: &mut [CornerAug],
    params: &DetectorParams,
) -> (Option<ClusterCenters>, ClusterDebug) {
    let tuning = params.effective_tuning();
    let mut debug = ClusterDebug {
        num_bins: tuning.num_bins,
        histogram: Vec::new(),
        smoothed: Vec::new(),
        total_weight: 0.0,
        peak_seeds_rad: None,
        refined_centers_rad: None,
    };

    if corners.is_empty() || tuning.num_bins < 4 {
        return (None, debug);
    }

    let hist = build_histogram(corners, params);
    debug.histogram = hist.bins.clone();
    debug.total_weight = hist.total_weight;
    if hist.total_weight <= 0.0 {
        return (None, debug);
    }

    let smoothed = cs::smooth_circular_5(&hist.bins);
    debug.smoothed = smoothed.clone();

    let peak_opts = cs::PeakPickOptions::new(
        tuning.min_peak_weight_fraction,
        tuning.peak_min_separation_deg.to_radians(),
    );
    let Some((theta0_seed, theta1_seed)) =
        cs::pick_two_peaks(&smoothed, hist.total_weight, &peak_opts)
    else {
        return (None, debug);
    };
    debug.peak_seeds_rad = Some([theta0_seed, theta1_seed]);

    let votes = collect_axis_votes(corners);
    let (theta0, theta1) =
        cs::refine_2means_double_angle(&votes, [theta0_seed, theta1_seed], tuning.max_iters_2means);
    debug.refined_centers_rad = Some([theta0, theta1]);

    let (a, b) = if theta0 <= theta1 {
        (theta0, theta1)
    } else {
        (theta1, theta0)
    };
    let centers = ClusterCenters {
        theta0: a,
        theta1: b,
    };

    // Assign per-corner label. The effective per-corner tolerance is
    // `cluster_tol_rad + cluster_sigma_k * max(σ_a0, σ_a1)` so noisy
    // axes get proportional slack — see `AdvancedTuning::cluster_sigma_k`.
    let base_tol_rad = tuning.cluster_tol_deg.to_radians();
    let sigma_k = tuning.cluster_sigma_k;
    for corner in corners.iter_mut() {
        if !matches!(corner.stage, CornerStage::Strong) {
            continue;
        }
        let tol_rad = effective_tol_rad(corner, base_tol_rad, sigma_k);
        let assign = assign_corner(corner, centers, tol_rad);
        match assign {
            AxisCluster::Labeled { label, .. } => {
                corner.label = Some(label);
                corner.stage = CornerStage::Clustered { label };
            }
            AxisCluster::Unclustered { max_d_rad } => {
                corner.label = None;
                corner.stage = CornerStage::NoCluster {
                    max_d_deg: max_d_rad.to_degrees(),
                };
            }
        }
    }

    // Spatial-coherence pass: chess-corners 0.9's DiskFit can pick the
    // wrong antipodal dark sector for some chessboard corners, leaving
    // adjacent corners with the SAME axis-slot ordering instead of the
    // alternating pattern the BFS / seed / edge-invariant relies on. The
    // bug shows up as a same-label cluster of neighbours where a
    // chessboard demands opposite labels. Detect the offenders by
    // spatial majority vote and recover by swapping their two
    // `AxisEstimate` slots (which also flips the cluster label).
    //
    // Gated on a heavy label imbalance (one class < ~22% of the
    // total). RingFit produces ~50/50 balanced labels by construction
    // and is unaffected by this gate. DiskFit produces ~50/50 on
    // ChArUco-style images (small0..small5, target_7) where the
    // existing parity convention is fine — also unaffected by the
    // gate. The gate fires on clean-chessboard scenes where DiskFit's
    // antipodal-sector pick collapses to the same physical axis for
    // most corners (mid.png 62/15 = 80% Canonical pre-fix).
    fix_axis_slot_coherence(corners);

    (Some(centers), debug)
}
// --- internals ------------------------------------------------------------

struct Histogram {
    bins: Vec<f32>,
    total_weight: f32,
}

fn build_histogram(corners: &[CornerAug], params: &DetectorParams) -> Histogram {
    let n = params.effective_tuning().num_bins;
    let mut bins = vec![0.0_f32; n];
    let mut total = 0.0_f32;
    for corner in corners {
        if !matches!(corner.stage, CornerStage::Strong) {
            continue;
        }
        for axis in &corner.axes {
            if !axis.sigma.is_finite() || axis.sigma >= PI - f32::EPSILON {
                // No-info sentinel → skip this axis.
                continue;
            }
            let w = weight(corner.strength, axis.sigma);
            if w <= 0.0 {
                continue;
            }
            let bin = cs::angle_to_bin(cs::wrap_pi(axis.angle), n);
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
fn weight(strength: f32, sigma: f32) -> f32 {
    let s = strength.max(0.0);
    let base = if s > 0.0 { s } else { 1.0 };
    base / (1.0 + sigma.max(0.0))
}

/// Materialise per-axis votes in the shape expected by the hoisted
/// [`cs::refine_2means_double_angle`] helper.
fn collect_axis_votes(corners: &[CornerAug]) -> Vec<cs::AngleVote> {
    let mut votes: Vec<cs::AngleVote> = Vec::new();
    for corner in corners {
        if !matches!(corner.stage, CornerStage::Strong) {
            continue;
        }
        for axis in &corner.axes {
            if !axis.sigma.is_finite() || axis.sigma >= PI - f32::EPSILON {
                continue;
            }
            let w = weight(corner.strength, axis.sigma);
            if w <= 0.0 {
                continue;
            }
            votes.push(cs::AngleVote {
                angle: cs::wrap_pi(axis.angle),
                weight: w,
            });
        }
    }
    votes
}

// --- tests ----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corner::{ChessCorner, ClusterLabel};
    use calib_targets_core::AxisEstimate;
    use nalgebra::Point2;

    fn make_corner(
        input_index: usize,
        x: f32,
        y: f32,
        axis0_deg: f32,
        sigma_deg: f32,
        strength: f32,
    ) -> CornerAug {
        let a0 = axis0_deg.to_radians();
        let a1 = a0 + std::f32::consts::FRAC_PI_2;
        let sigma = sigma_deg.to_radians();
        let c = ChessCorner {
            position: Point2::new(x, y),
            axes: [
                AxisEstimate {
                    angle: wrap_pi(a0),
                    sigma,
                },
                AxisEstimate {
                    angle: wrap_pi(a1),
                    sigma,
                },
            ],
            contrast: 10.0,
            fit_rms: 1.0,
            strength,
        };
        let mut aug = CornerAug::from_chess_corner(input_index, &c);
        aug.stage = CornerStage::Strong;
        aug
    }

    // Deterministic pseudo-random jitter without pulling in `rand` as a
    // test dep — a small wrapping-linear-congruential generator is
    // plenty for tests that just need symmetric noise.
    fn jitter(i: usize, amp_deg: f32) -> f32 {
        // Hash-ish: multiply, shift, wrap to [-0.5, 0.5], scale.
        let x = (i as u32).wrapping_mul(2_654_435_761);
        let frac = ((x >> 8) as f32) / ((1u32 << 24) as f32); // [0,1)
        (frac - 0.5) * amp_deg
    }

    #[test]
    fn recovers_centers_30_120() {
        let mut corners = Vec::new();
        // Half parity-0 corners (axes[0] ≈ 30°, axes[1] ≈ 120°).
        for i in 0..50 {
            let j = jitter(i, 10.0);
            corners.push(make_corner(
                i,
                i as f32,
                0.0,
                30.0 + j,
                0.05_f32.to_radians(),
                1.0,
            ));
        }
        // Half parity-1 corners (axes[0] ≈ 120°, axes[1] ≈ 30°ish).
        for i in 0..50 {
            let j = jitter(i + 1000, 10.0);
            corners.push(make_corner(
                50 + i,
                i as f32,
                1.0,
                120.0 + j,
                0.05_f32.to_radians(),
                1.0,
            ));
        }

        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        // Expect peaks near 30° and 120° (Θ₀ < Θ₁ sort), with the
        // tightness of the jitter.
        let expected_low = 30.0_f32.to_radians();
        let expected_high = 120.0_f32.to_radians();
        assert!(
            angular_dist_pi(centers.theta0, expected_low) < 2.0_f32.to_radians(),
            "Θ₀ = {:.2}° off from 30°",
            centers.theta0.to_degrees()
        );
        assert!(
            angular_dist_pi(centers.theta1, expected_high) < 2.0_f32.to_radians(),
            "Θ₁ = {:.2}° off from 120°",
            centers.theta1.to_degrees()
        );
        // All strong corners should get a label.
        assert!(corners
            .iter()
            .all(|c| matches!(c.stage, CornerStage::Clustered { .. })));
    }

    #[test]
    fn parity_0_gets_canonical_parity_1_gets_swapped() {
        let mut corners = Vec::new();
        for i in 0..30 {
            // Parity-0: axes[0] at 0°, axes[1] at 90°.
            corners.push(make_corner(i, i as f32, 0.0, 0.0, 0.01, 1.0));
        }
        for i in 0..30 {
            // Parity-1: axes[0] at 90°, axes[1] at 180°→0°.
            corners.push(make_corner(30 + i, i as f32, 1.0, 90.0, 0.01, 1.0));
        }
        let params = DetectorParams::default();
        cluster_axes(&mut corners, &params).expect("centers");

        // Half Canonical, half Swapped.
        let canon = corners
            .iter()
            .filter(|c| matches!(c.label, Some(ClusterLabel::Canonical)))
            .count();
        let swap = corners
            .iter()
            .filter(|c| matches!(c.label, Some(ClusterLabel::Swapped)))
            .count();
        assert_eq!(canon + swap, 60, "every corner labeled");
        // At least half in each bucket — the exact split depends on
        // which peak sorts as Θ₀ (smaller angle wins).
        assert!(canon >= 25 && swap >= 25);
    }

    #[test]
    fn corner_far_from_both_centers_is_unclustered() {
        let mut corners = Vec::new();
        // 40 corners at 0°/90°.
        for i in 0..40 {
            corners.push(make_corner(i, i as f32, 0.0, 0.0, 0.01, 1.0));
        }
        // 1 misaligned corner — axes[0] at 25° (not matching any
        // cluster center within 12°).
        corners.push(make_corner(99, 0.0, 0.0, 25.0, 0.01, 1.0));

        let params = DetectorParams::default();
        cluster_axes(&mut corners, &params).expect("centers");

        let last = corners.last().expect("corners is non-empty");
        match &last.stage {
            CornerStage::NoCluster { .. } => {}
            other => unreachable!(
                "a corner with axes 25° off both centers must end in NoCluster, got {other:?}"
            ),
        }
        assert!(last.label.is_none());
    }

    #[test]
    fn empty_input_returns_none() {
        let mut corners: Vec<CornerAug> = Vec::new();
        let params = DetectorParams::default();
        assert!(cluster_axes(&mut corners, &params).is_none());
    }
}

#[cfg(test)]
mod plateau_peak_regression {
    //! Regression: when a physical direction falls on the `π` wrap
    //! boundary (ChESS reports `3.1415925 ≈ π − ε`, which `wrap_pi`
    //! leaves near `π` instead of folding to 0), the smoothed
    //! histogram gains two equal-height adjacent bins at 0 and
    //! `n − 1`. A strict `here > prev && here > next` peak check
    //! misses the flat-top plateau and `cluster_axes` returns
    //! `None`. This happens in practice on perfectly rectilinear
    //! synthetic puzzleboards (testdata example8/example9).
    //!
    //! See the plateau-aware branch in `pick_two_peaks`.
    use super::*;
    use crate::corner::{ChessCorner, CornerAug};
    use calib_targets_core::AxisEstimate;
    use nalgebra::Point2;

    #[test]
    fn near_pi_wrap_still_clusters() {
        // Use 3.1415925 (what the real ChESS adapter reports on the
        // synthetic puzzleboard) rather than f32::consts::PI, so the
        // wrap-boundary bug is reproduced exactly.
        const NEAR_PI: f32 = 3.1415925;
        let mut augs: Vec<CornerAug> = Vec::new();
        for j in 0..10_i32 {
            for i in 0..10_i32 {
                let swapped = (i + j).rem_euclid(2) == 1;
                let (a0, a1) = if swapped {
                    (std::f32::consts::FRAC_PI_2, NEAR_PI)
                } else {
                    (0.0_f32, std::f32::consts::FRAC_PI_2)
                };
                let c = ChessCorner {
                    position: Point2::new(i as f32 * 100.0 + 50.0, j as f32 * 100.0 + 50.0),
                    axes: [
                        AxisEstimate {
                            angle: a0,
                            sigma: 0.008,
                        },
                        AxisEstimate {
                            angle: a1,
                            sigma: 0.008,
                        },
                    ],
                    contrast: 136.0,
                    fit_rms: 4.7,
                    strength: 612.0,
                };
                let mut aug = CornerAug::from_chess_corner(augs.len(), &c);
                aug.stage = CornerStage::Strong;
                augs.push(aug);
            }
        }
        let params = DetectorParams::default();
        let centers =
            cluster_axes(&mut augs, &params).expect("near-π plateau must still yield two peaks");
        // Centers should settle at ≈0 and ≈π/2 after 2-means.
        assert!(
            angular_dist_pi(centers.theta0, 0.0) < 1.0_f32.to_radians(),
            "Θ₀ = {:.3}° too far from 0°",
            centers.theta0.to_degrees()
        );
        assert!(
            angular_dist_pi(centers.theta1, std::f32::consts::FRAC_PI_2) < 1.0_f32.to_radians(),
            "Θ₁ = {:.3}° too far from 90°",
            centers.theta1.to_degrees()
        );
        // Every input corner should now be clustered — on a perfect
        // grid there should be no stragglers.
        let n_clustered = augs
            .iter()
            .filter(|a| matches!(a.stage, CornerStage::Clustered { .. }))
            .count();
        assert_eq!(n_clustered, 100);
    }
}
