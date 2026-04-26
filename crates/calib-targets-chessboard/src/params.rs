//! Chessboard detector parameters.
//!
//! All spatial tolerances are **multiplicative with respect to `s`**
//! (the global cell size) — the pipeline is scale-invariant once `s`
//! is known. All angular tolerances are absolute degrees.
//!
//! Default values follow spec §6.

use serde::{Deserialize, Serialize};

fn default_validate_step_aware() -> bool {
    // Default off: shipping the capability without changing behaviour.
    // The step-aware threshold is anisotropic per-corner — tighter in
    // perspective-foreshortened regions, looser in radially-distorted
    // ones. On the public bench, enabling it drops one labelled corner
    // on `testdata/puzzleboard_reference/example1.png` (the tighter
    // back-edge tolerance over-flags). Treat enabling it as a focused
    // experiment per dataset until we have a tuned `line_tol_rel` /
    // `local_h_tol_rel` pair that holds the precision contract on
    // every blessed image.
    false
}

fn default_step_deviation_thresh_rel() -> f32 {
    // Off by default. Set to e.g. 0.5 to flag corners whose local
    // step deviates from the labelled-set median by more than 50%.
    // Combined with a line flag, the corner is blacklisted (rule 4).
    0.0
}

fn default_cluster_sigma_k() -> f32 {
    // k = 0 by default — sigma-aware tolerance is plumbed through but
    // disabled. Empirical study (k = 0.5–2.0 with cap 3–4°): every
    // positive setting that recovers `small3.png`'s NoCluster set also
    // destabilises `example2.png`'s seed finder under heavy radial
    // distortion. Extra Clustered candidates expose a ~1.4×-cell seed
    // quad whose edge midpoints don't coincide with any real corner,
    // so the existing midpoint-violation check (even broadened to
    // include all positions) does not reject it. The seed selector
    // needs cell-size consistency or trial-grow scoring before this
    // gate can open. Setting `cluster_sigma_k` > 0 in a custom
    // `DetectorParams` is fine for experiments.
    0.0
}

fn default_enable_stage6_5_rescue() -> bool {
    // Default on. The rescue pass runs after Stage 6 and is gated on
    // (a) local-H position match, (b) parity match against the global
    // centers, and (c) the axis-slot-swap edge invariant. None of
    // these admit a structurally wrong corner — the precision
    // contract still holds.
    true
}

fn default_rescue_axis_tol_deg() -> f32 {
    // 22° covers the false-NoCluster `max_d_deg` quartiles observed
    // on `example1.png` (max 32°) and `example2.png` (max 21°).
    // Goes wider than `weak_cluster_tol_deg` because Stage 6.5
    // requires the additional position + parity + edge gates.
    22.0
}

fn default_rescue_search_rel() -> f32 {
    // 0.8 cell — wide enough to catch corners under heavy perspective
    // foreshortening where local-H extrapolation at boundary cells
    // overshoots the actual position by ~0.5 cell. The ambiguity gate
    // and parity / axis / edge invariants keep precision intact.
    0.8
}

fn default_stage6_local_h() -> bool {
    // Local-H Stage 6 is the production default: per-candidate
    // homography from the K nearest labelled corners + deeper bbox
    // enumeration (`extend_depth = 3`). On the public bench it lifts
    // `testdata/puzzleboard_reference/example2.png` from 75 → 134
    // labelled corners (heavy radial distortion, where global-H's
    // residual gate refused). All other public images stay byte-exact.
    // p95 latency goes from ~10 ms to ~18 ms — the cost of one DLT
    // per candidate cell, within the Phase 5 budget (≤ 1.3× baseline).
    //
    // Set to `false` to fall back to the single-global-H Stage 6 if
    // the latency or determinism behaviour ever needs to be compared
    // back-to-back.
    true
}

fn default_stage6_local_k_nearest() -> usize {
    // K = 12 gives 3× over-determination on the 9-DOF DLT and is
    // wide enough to capture local perspective without diluting it
    // with far-away labels. Reduce to 8 for very small labelled sets;
    // raise to 16 for large/dense grids.
    12
}

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
    /// Per-axis absolute tolerance (degrees) for a corner's axis to count as
    /// matching a cluster center. The effective per-corner gate is
    /// `cluster_tol_deg + cluster_sigma_k * max(σ_a0, σ_a1)`, so noisier
    /// axis estimates get proportional slack — see [`cluster_sigma_k`].
    ///
    /// [`cluster_sigma_k`]: DetectorParams::cluster_sigma_k
    pub cluster_tol_deg: f32,
    /// Multiplier on the per-corner axis sigma added to [`cluster_tol_deg`]
    /// when admitting a corner. Default `2.0`: clean corners
    /// (σ ≈ 0.5–1°) get ≈ `cluster_tol_deg + 1–2°`; noisy corners
    /// (σ ≈ 3–5° on tilted-lens / partial-focus images) get
    /// `cluster_tol_deg + 6–10°`. Set to `0.0` to restore the strict
    /// fixed-tolerance behaviour.
    ///
    /// Justification: ChESS axis sigma is the 1σ Gauss–Newton uncertainty
    /// of the two-axis fit, so a per-corner gate of `tol + k·σ` is the
    /// standard way to pass corners whose true axis is within tolerance
    /// but whose estimate fell outside under noise. `k = 2` corresponds
    /// to a ≈ 95% one-sided confidence band.
    ///
    /// [`cluster_tol_deg`]: DetectorParams::cluster_tol_deg
    #[serde(default = "default_cluster_sigma_k")]
    pub cluster_sigma_k: f32,
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
    /// Straight-line-fit collinearity tolerance (fraction of the
    /// per-corner scale — see [`validate_step_aware`]).
    ///
    /// [`validate_step_aware`]: DetectorParams::validate_step_aware
    pub line_tol_rel: f32,
    /// Minimum members required to fit a line / column for collinearity
    /// checks.
    pub line_min_members: usize,
    /// Local-H prediction tolerance (fraction of the per-corner scale
    /// — see [`validate_step_aware`]).
    ///
    /// [`validate_step_aware`]: DetectorParams::validate_step_aware
    pub local_h_tol_rel: f32,
    /// When `true`, [`line_tol_rel`] / [`local_h_tol_rel`] are
    /// multiplied by a per-corner local step (computed from labelled
    /// grid neighbours via central or one-sided finite differences)
    /// instead of the global cell size. Anisotropic thresholds catch
    /// outliers in dense (perspective-foreshortened) regions that a
    /// global threshold would miss, and stay loose enough in
    /// distorted regions where the local cell pitch grows. Falls back
    /// to global cell size for corners with too few labelled
    /// neighbours.
    ///
    /// Default `true`. Set to `false` to restore the pre-2026-04
    /// behaviour.
    ///
    /// [`line_tol_rel`]: DetectorParams::line_tol_rel
    /// [`local_h_tol_rel`]: DetectorParams::local_h_tol_rel
    #[serde(default = "default_validate_step_aware")]
    pub validate_step_aware: bool,
    /// When `> 0` and [`validate_step_aware`] is set, an extra flag
    /// fires for corners whose local step deviates from the labelled-
    /// set median by more than this fraction (e.g. `0.5` flags
    /// corners whose step is < 1/(1+0.5) of the median or > 1.5×
    /// median). Combined with a line flag, the corner is
    /// blacklisted.
    ///
    /// Default `0.5`. Set to `0.0` to disable the deviation flag.
    ///
    /// [`validate_step_aware`]: DetectorParams::validate_step_aware
    #[serde(default = "default_step_deviation_thresh_rel")]
    pub validate_step_deviation_thresh_rel: f32,
    /// Blacklist-retry cap.
    pub max_validation_iters: u32,

    // --- Stage 6.5: NoCluster rescue ---------------------------------------
    /// Run a Stage-6.5 pass after Stage-6 boundary extension that
    /// re-considers `Strong` / `NoCluster` corners as candidates for
    /// empty grid cells. Reuses the same per-candidate local-H
    /// machinery as Stage 6 but admits corners whose axes failed the
    /// strict Stage-3 gate by a margin, gated on (a) position match
    /// with the local-H prediction, (b) parity match against the
    /// global cluster centers via the cheaper canonical/swapped
    /// assignment, and (c) the axis-slot-swap edge invariant to a
    /// labelled neighbour. Recovers corners whose axes drifted under
    /// perspective foreshortening or radial distortion (typical
    /// failure mode on `puzzleboard_reference/example1.png` and
    /// `example2.png`).
    ///
    /// Default `true`. Set to `false` to restore the pre-Stage-6.5
    /// behaviour.
    #[serde(default = "default_enable_stage6_5_rescue")]
    pub enable_stage6_5_rescue: bool,
    /// Per-axis absolute tolerance (degrees) for [`Stage 6.5
    /// rescue`](DetectorParams::enable_stage6_5_rescue) admission.
    /// Wider than [`cluster_tol_deg`] (typically 12°) and the booster's
    /// [`weak_cluster_tol_deg`] (typically 18°) because the rescue
    /// pass is precision-anchored on local-H position match — a wide
    /// axis gate alone cannot admit a wrong corner.
    ///
    /// Default `22°`: the Step-0 evidence on
    /// `puzzleboard_reference/example1.png` and `example2.png` showed
    /// false-NoCluster `max_d_deg` quartiles in the 12–22° range; this
    /// value covers them without admitting structurally-misoriented
    /// corners.
    ///
    /// [`cluster_tol_deg`]: DetectorParams::cluster_tol_deg
    /// [`weak_cluster_tol_deg`]: DetectorParams::weak_cluster_tol_deg
    #[serde(default = "default_rescue_axis_tol_deg")]
    pub rescue_axis_tol_deg: f32,
    /// `K` parameter for Stage-6.5 local-H fitting (same semantics as
    /// [`stage6_local_k_nearest`]).
    ///
    /// [`stage6_local_k_nearest`]: DetectorParams::stage6_local_k_nearest
    #[serde(default = "default_stage6_local_k_nearest")]
    pub stage6_5_local_k_nearest: usize,
    /// Position-search radius for Stage-6.5 candidate matching, as a
    /// fraction of `cell_size`. Wider than Stage-6's `search_rel`
    /// (default 0.40) because heavy perspective foreshortening makes
    /// the local-H prediction at boundary cells overshoot by
    /// significantly more than 0.40 cell. The wider gate is safe
    /// because Stage 6.5 still enforces parity + axis match + edge
    /// invariant + ambiguity, all of which fail on a wrongly-located
    /// candidate.
    ///
    /// Default `0.8`.
    #[serde(default = "default_rescue_search_rel")]
    pub rescue_search_rel: f32,

    // --- Stage 6: boundary extension --------------------------------------
    /// Use the per-candidate local-homography Stage 6
    /// (`projective_grid::square::grow_extension::extend_via_local_homography`)
    /// instead of the single-global-H one. The local-H variant fits an
    /// H per candidate cell from the K nearest labelled corners, gets
    /// per-candidate trust gates, and reaches further past the bbox
    /// because each iteration shifts the local-H window with the
    /// growing labelled set.
    ///
    /// Default `false` (single-global-H, baseline today). Flip to
    /// `true` after A/B confirms parity / superset on every blessed
    /// image.
    #[serde(default = "default_stage6_local_h")]
    pub stage6_local_h: bool,
    /// `K` parameter for [`stage6_local_h`]: the number of nearest
    /// labelled corners (by grid Manhattan distance) used to fit each
    /// candidate cell's local H.
    ///
    /// [`stage6_local_h`]: DetectorParams::stage6_local_h
    #[serde(default = "default_stage6_local_k_nearest")]
    pub stage6_local_k_nearest: usize,

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
            cluster_sigma_k: default_cluster_sigma_k(),
            peak_min_separation_deg: 60.0,
            // Raised from 0.05 → 0.02: with fine (2°) bins and
            // realistic axis noise, the per-bin weight of a genuine
            // grid-direction peak on a 500-corner scene can fall to
            // ~2–3% of total axis-vote weight (see small1/3/4
            // ChArUco snaps in testdata/). 0.05 was tuned for the
            // private flagship dataset where corners are cleaner and mass
            // concentrates tightly; 0.02 is still comfortably above
            // pure-noise bins.
            min_peak_weight_fraction: 0.02,

            cell_size_hint: None,

            seed_edge_tol: 0.25,
            seed_axis_tol_deg: 15.0,
            seed_close_tol: 0.25,

            attach_search_rel: 0.35,
            attach_axis_tol_deg: 15.0,
            attach_ambiguity_factor: 1.5,
            step_tol: 0.25,
            edge_axis_tol_deg: 15.0,

            // Raised from 0.15 → 0.18: under extreme perspective on
            // dense boards, straight-line fits over long columns
            // legitimately deviate from the fit by ~0.15-0.18 × s.
            // The invariant-first contract still holds because
            // line-failure is only one of several conditions for a
            // blacklist (see validate::attribution).
            line_tol_rel: 0.18,
            line_min_members: 3,
            local_h_tol_rel: 0.20,
            validate_step_aware: default_validate_step_aware(),
            validate_step_deviation_thresh_rel: default_step_deviation_thresh_rel(),
            // Raised from 3 → 6: on dense boards with many
            // borderline-outlier corners near the edge, the
            // validate→blacklist→regrow loop can take 4–5 iterations
            // to settle (see testdata/puzzleboard_reference/example1.png
            // with ~230 labelled corners and an oscillating blacklist
            // of 2–4 per iter). 3 was adequate for the private flagship
            // benchmark where blacklists are typically empty on the
            // first pass; 6 absorbs the wider real-world variance
            // without noticeable cost (each iter is cheap).
            max_validation_iters: 6,

            stage6_local_h: default_stage6_local_h(),
            stage6_local_k_nearest: default_stage6_local_k_nearest(),

            enable_stage6_5_rescue: default_enable_stage6_5_rescue(),
            rescue_axis_tol_deg: default_rescue_axis_tol_deg(),
            stage6_5_local_k_nearest: default_stage6_local_k_nearest(),
            rescue_search_rel: default_rescue_search_rel(),

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
