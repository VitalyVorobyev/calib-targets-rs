//! Parameters for the v2 detector.
//!
//! All spatial tolerances are **multiplicative with respect to `s`**
//! (the global cell size) — the pipeline is scale-invariant once `s`
//! is known. All angular tolerances are absolute degrees.
//!
//! Default values follow spec §6.

use serde::{Deserialize, Serialize};

/// Top-level detector configuration.
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DetectorParams {
    // --- Stage 1: pre-filter -------------------------------------------------
    /// Minimum corner strength (ChESS response). `0.0` disables the filter.
    pub min_corner_strength: f32,
    /// Corners are dropped when `c.fit_rms > max_fit_rms_ratio * c.contrast`
    /// (and `c.contrast > 0`). `f32::INFINITY` disables the filter.
    pub max_fit_rms_ratio: f32,

    // --- Stage 2 + 3: clustering --------------------------------------------
    /// Number of histogram bins on `[0, π)` for axis-direction clustering.
    pub num_bins: usize,
    /// Max 2-means refinement iterations over axis votes.
    pub max_iters_2means: usize,
    /// Per-axis absolute tolerance for a corner's axis to count as matching a
    /// cluster center.
    pub cluster_tol_deg: f32,
    /// Minimal angular separation (degrees) between the two peaks. Guards
    /// against seed-peak collisions; true grid axes are `~90°` apart.
    pub peak_min_separation_deg: f32,
    /// Minimal fraction of total axis-vote weight required for a peak to be
    /// considered.
    pub min_peak_weight_fraction: f32,

    // --- Stage 4: cell size --------------------------------------------------
    /// Optional caller hint. When provided and close to the estimate, the
    /// hint may tighten Stage-5/6 search windows. See `cell_size.rs`.
    pub cell_size_hint: Option<f32>,

    // --- Stage 5: seed -------------------------------------------------------
    /// Seed edge length window: `[1 - t, 1 + t] × s`.
    pub seed_edge_tol: f32,
    /// Angular tolerance (degrees) for seed-edge direction vs matched axis.
    pub seed_axis_tol_deg: f32,
    /// Parallelogram-closure tolerance (fraction of `s`) for seed quad `D`.
    pub seed_close_tol: f32,

    // --- Stage 6: grow -------------------------------------------------------
    /// Candidate-search radius (fraction of `s`) around predicted `(i, j)`.
    pub attach_search_rel: f32,
    /// Axis alignment tolerance at attachment time (degrees).
    pub attach_axis_tol_deg: f32,
    /// Ambiguity factor: if the second-nearest candidate is within
    /// `factor × nearest_distance`, the attachment is skipped.
    pub attach_ambiguity_factor: f32,
    /// Edge-length window (fraction of `s`) enforced when admitting edges
    /// from the new corner to its labelled neighbors.
    pub step_tol: f32,
    /// Edge axis-direction tolerance (degrees) enforced at admission time.
    pub edge_axis_tol_deg: f32,

    // --- Stage 7: validate ---------------------------------------------------
    /// Straight-line-fit collinearity tolerance (fraction of `s`).
    pub line_tol_rel: f32,
    /// Projective-line-fit collinearity tolerance (fraction of `s`). Looser
    /// than `line_tol_rel` to accommodate lens distortion.
    pub projective_line_tol_rel: f32,
    /// Minimum members required to fit a line / column for collinearity
    /// checks.
    pub line_min_members: usize,
    /// Local-H prediction tolerance (fraction of `s`).
    pub local_h_tol_rel: f32,
    /// Blacklist-retry cap.
    pub max_validation_iters: u32,

    // --- Stage 8: recall boosters -------------------------------------------
    pub enable_line_extrapolation: bool,
    pub enable_gap_fill: bool,
    pub enable_component_merge: bool,
    pub enable_weak_cluster_rescue: bool,
    /// Cluster tolerance for "weakly clustered" corners eligible as recall-
    /// booster candidates. Must be ≥ `cluster_tol_deg`.
    pub weak_cluster_tol_deg: f32,
    /// Minimum boundary-pair count required to attempt a component merge.
    pub component_merge_min_boundary_pairs: usize,
    /// Cap on the outer booster loop.
    pub max_booster_iters: u32,

    // --- Stage 9: output ----------------------------------------------------
    /// Minimum labelled corners for a Detection to be emitted.
    pub min_labeled_corners: usize,

    // --- Multi-component (same-board, disconnected pieces) ------------------
    /// Maximum number of components returned by [`crate::Detector::detect_all`].
    ///
    /// A chessboard can split into multiple disconnected pieces on ChArUco
    /// scenes where markers break contiguity. Each iteration peels off one
    /// grown grid from the unconsumed corners and re-runs seed → grow →
    /// validate. Default `3`.
    ///
    /// Does NOT claim to support scenes with two separate physical boards —
    /// one target per frame is the contract.
    pub max_components: u32,
}

impl Default for DetectorParams {
    fn default() -> Self {
        Self {
            min_corner_strength: 0.0,
            max_fit_rms_ratio: 0.5,

            num_bins: 90,
            max_iters_2means: 10,
            cluster_tol_deg: 12.0,
            peak_min_separation_deg: 60.0,
            min_peak_weight_fraction: 0.05,

            cell_size_hint: None,

            seed_edge_tol: 0.25,
            seed_axis_tol_deg: 15.0,
            seed_close_tol: 0.25,

            attach_search_rel: 0.35,
            attach_axis_tol_deg: 15.0,
            attach_ambiguity_factor: 1.5,
            step_tol: 0.25,
            edge_axis_tol_deg: 15.0,

            line_tol_rel: 0.15,
            projective_line_tol_rel: 0.25,
            line_min_members: 3,
            local_h_tol_rel: 0.20,
            max_validation_iters: 3,

            enable_line_extrapolation: true,
            enable_gap_fill: true,
            enable_component_merge: true,
            enable_weak_cluster_rescue: true,
            weak_cluster_tol_deg: 18.0,
            component_merge_min_boundary_pairs: 2,
            max_booster_iters: 5,

            min_labeled_corners: 8,

            max_components: 3,
        }
    }
}

impl DetectorParams {
    /// Three-config sweep preset: default + tighter + looser angular tolerances.
    ///
    /// Intended for `detect_chessboard_best`-style flows that try multiple
    /// configurations and return the result with the most labelled corners.
    /// All three configurations preserve the detector's
    /// precision-by-construction invariants; only recall-affecting
    /// tolerances are varied.
    pub fn sweep_default() -> Vec<Self> {
        let base = Self::default();
        let tight = Self {
            cluster_tol_deg: 9.0,
            seed_edge_tol: 0.18,
            attach_axis_tol_deg: 12.0,
            ..base.clone()
        };
        let loose = Self {
            cluster_tol_deg: 16.0,
            seed_edge_tol: 0.32,
            attach_axis_tol_deg: 18.0,
            ..base.clone()
        };
        vec![base, tight, loose]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_default_has_three_configs() {
        let configs = DetectorParams::sweep_default();
        assert_eq!(configs.len(), 3);
        let base = &configs[0];
        let tight = &configs[1];
        let loose = &configs[2];
        assert!(tight.cluster_tol_deg < base.cluster_tol_deg);
        assert!(loose.cluster_tol_deg > base.cluster_tol_deg);
        assert!(tight.seed_edge_tol < base.seed_edge_tol);
        assert!(loose.seed_edge_tol > base.seed_edge_tol);
    }
}
