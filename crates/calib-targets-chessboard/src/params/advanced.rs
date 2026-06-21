//! Advanced, unstable per-stage tuning knobs for the chessboard detector.
//!
//! [`AdvancedTuning`] is the opt-in sub-struct behind
//! [`DetectorParams::advanced`](super::DetectorParams::advanced). It holds the
//! tuning knobs for the live topological pipeline stages: the grid builder, the
//! shared component merge, the strength/fit pre-filter, axis clustering, the
//! recall boosters, and the mandatory final geometry check.
//!
//! **Stability.** Unlike the stable core on [`DetectorParams`], the fields of
//! `AdvancedTuning` are **NOT covered by semver**. They are named after, and
//! coupled to, internal pipeline stages, so they may be **renamed, retyped, or
//! removed between minor versions** as the detector evolves. Treat them as an
//! escape hatch for a specific failing input backed by evidence — not as part
//! of the public configuration contract. A calibration consumer has no basis
//! to set any of them and should leave the struct at [`Default`].
//!
//! When [`DetectorParams::advanced`](super::DetectorParams::advanced) is set,
//! the whole struct is serialized under a nested `"advanced"` JSON object — the
//! knobs are **not** flattened into the top-level config. When it is `None`,
//! no `"advanced"` key appears and the detector behaves exactly as if every
//! knob held its [`Default`] value.
//!
//! All spatial tolerances are **multiplicative with respect to `s`** (the
//! global cell size) — the pipeline is scale-invariant once `s` is known. All
//! angular tolerances are absolute degrees.

use projective_grid::shared::merge::LocalMergeParams;
use projective_grid::TopologicalParams;
use serde::{Deserialize, Serialize};

pub(super) fn default_topological_params() -> TopologicalParams {
    TopologicalParams::default()
        .with_opposing_edge_ratio_max(10.0)
        .with_edge_length_band(0.0, 1.8)
}

fn default_component_merge_params() -> LocalMergeParams {
    LocalMergeParams::default()
}

fn default_validate_step_aware() -> bool {
    // Default off: shipping the capability without changing behaviour.
    // The step-aware threshold is anisotropic per-corner — tighter in
    // perspective-foreshortened regions, looser in radially-distorted
    // ones. On the public bench, enabling it drops one labelled corner
    // on `testdata/puzzleboard_reference/example1.png` (the tighter
    // back-edge tolerance over-flags). Treat enabling it as a focused
    // experiment per dataset until we have a tuned tolerance pair that
    // holds the precision contract on every blessed image.
    false
}

fn default_cluster_sigma_k() -> f32 {
    // k = 0 by default — sigma-aware tolerance is plumbed through but
    // disabled. Empirical study (k = 0.5–2.0 with cap 3–4°): every
    // positive setting that recovers `small3.png`'s NoCluster set also
    // destabilises clustering under heavy radial distortion. Setting
    // `cluster_sigma_k` > 0 in a custom `AdvancedTuning` is fine for
    // experiments.
    0.0
}

fn default_geometry_check_line_tol_rel() -> f32 {
    // The final geometry check uses a much looser line-collinearity
    // tolerance than the grid builder's validation pass: its role is to
    // catch gross mislabels — full-cell or diagonal shifts (~1.4 cell
    // residual) — not the borderline perspective drift the builder
    // already accepted. A tight tolerance here produces catastrophic
    // recall regressions on heavy-radial-distortion boards.
    0.45
}

fn default_geometry_check_local_h_tol_rel() -> f32 {
    // Same logic as above for local-H residual. A diagonal mislabel
    // shifts a corner by ~1.4 cell from its predicted position; a
    // tolerance of 0.6 cell is well below that gap while leaving the
    // legitimate perspective-distorted corners alone.
    0.6
}

fn default_enable_final_edge_shape_check() -> bool {
    true
}

/// Advanced, **unstable** per-stage tuning knobs for the chessboard detector.
///
/// Behind [`DetectorParams::advanced`](super::DetectorParams::advanced). The
/// knobs are named after the live topological pipeline stages. The defaults are
/// chosen to hold the detector's precision-by-construction contract — a
/// calibration consumer has no basis to set any of them and should leave the
/// whole struct at [`Default`]. Tune a knob only when a specific input fails and
/// you have evidence for the change.
///
/// **NOT covered by semver.** These knobs are named after, and coupled to,
/// internal pipeline stages; they may be **renamed, retyped, or removed
/// between minor versions** without a major-version bump. Do not depend on the
/// field set being stable. The stable configuration contract lives entirely on
/// [`DetectorParams`](super::DetectorParams)'s four top-level fields.
///
/// When set on [`DetectorParams`](super::DetectorParams) via
/// [`with_advanced`](super::DetectorParams::with_advanced), the whole struct
/// serializes under a nested `"advanced"` JSON object — the knobs are not
/// flattened. When left unset, the serialized config carries no `"advanced"`
/// key and detection behaves exactly as if every knob held its [`Default`]
/// value (see [`DetectorParams::effective_tuning`](super::DetectorParams::effective_tuning)).
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AdvancedTuning {
    /// Tuning knobs for the
    /// [`GraphBuildAlgorithm::Topological`](super::GraphBuildAlgorithm::Topological)
    /// grid builder — the only graph builder the detector ships.
    #[serde(default = "default_topological_params")]
    pub topological: TopologicalParams,

    /// Tuning knobs for the shared local-geometry component merger that
    /// reunites the topological grid's connected components in label space.
    #[serde(default = "default_component_merge_params")]
    pub component_merge: LocalMergeParams,

    // --- `prefilter` stage ---------------------------------------------------
    /// Corners are dropped when `c.fit_rms > max_fit_rms_ratio * c.contrast`
    /// (and `c.contrast > 0`). `f32::INFINITY` disables the filter.
    pub max_fit_rms_ratio: f32,

    // --- `cluster_axes` stage -----------------------------------------------
    /// Number of histogram bins on `[0, π)` for axis-direction clustering.
    pub num_bins: usize,
    /// Max 2-means refinement iterations over axis votes.
    pub max_iters_2means: usize,
    /// Per-axis absolute tolerance (degrees) for a corner's axis to count as
    /// matching a cluster center. The effective per-corner gate is
    /// `cluster_tol_deg + cluster_sigma_k * max(σ_a0, σ_a1)`, so noisier
    /// axis estimates get proportional slack — see [`cluster_sigma_k`].
    ///
    /// [`cluster_sigma_k`]: AdvancedTuning::cluster_sigma_k
    pub cluster_tol_deg: f32,
    /// Multiplier on the per-corner axis sigma added to [`cluster_tol_deg`]
    /// when admitting a corner. Default `0.0`: sigma-aware tolerance is
    /// plumbed through but disabled. Set to e.g. `2.0` (a ≈95% one-sided
    /// confidence band) to pass corners whose true axis is within tolerance
    /// but whose ChESS estimate fell outside under noise.
    ///
    /// [`cluster_tol_deg`]: AdvancedTuning::cluster_tol_deg
    #[serde(default = "default_cluster_sigma_k")]
    pub cluster_sigma_k: f32,
    /// Minimal angular separation (degrees) between the two peaks. Guards
    /// against seed-peak collisions; true grid axes are `~90°` apart.
    pub peak_min_separation_deg: f32,
    /// Minimal fraction of total axis-vote weight required for a peak to be
    /// considered.
    pub min_peak_weight_fraction: f32,

    // --- recall boosters (interior fill + line extrapolation) ----------------
    /// Candidate-search radius (fraction of `s`) around a predicted `(i, j)`
    /// when the booster attaches a corner to an empty cell.
    pub attach_search_rel: f32,
    /// Axis alignment tolerance at attachment time (degrees).
    pub attach_axis_tol_deg: f32,
    /// Ambiguity factor: if the second-nearest candidate is within
    /// `factor × nearest_distance`, the attachment is skipped.
    pub attach_ambiguity_factor: f32,
    /// Edge-length window (fraction of `s`) enforced when admitting edges
    /// from a newly-attached corner to its labelled neighbours.
    pub step_tol: f32,
    /// Edge axis-direction tolerance (degrees) enforced at admission time.
    pub edge_axis_tol_deg: f32,
    /// Enable the weak-cluster rescue booster: re-admit corners that
    /// clustered only within the looser `weak_cluster_tol_deg`.
    pub enable_weak_cluster_rescue: bool,
    /// Cluster tolerance for "weakly clustered" corners eligible as recall-
    /// booster candidates. Must be ≥ `cluster_tol_deg`.
    pub weak_cluster_tol_deg: f32,
    /// Cap on the outer booster loop.
    pub max_booster_iters: u32,

    // --- mandatory final geometry check -------------------------------------
    /// Line-collinearity tolerance (fraction of cell_size) for the MANDATORY
    /// final geometry check that runs before any detection is emitted. Loose
    /// by design — the geometry check's role is to catch gross mislabels
    /// (diagonal / full-cell shifts), not the borderline perspective drift the
    /// grid builder already accepted.
    ///
    /// Default `0.45` of cell_size.
    #[serde(default = "default_geometry_check_line_tol_rel")]
    pub geometry_check_line_tol_rel: f32,
    /// Local-H residual tolerance (fraction of cell_size) for the MANDATORY
    /// final geometry check. A diagonal mislabel shifts a corner by ~1.4 cell
    /// from its predicted position; `0.6 × cell_size` is well below that gap
    /// while leaving the legitimate perspective-distorted corners alone.
    ///
    /// Default `0.6` of cell_size.
    #[serde(default = "default_geometry_check_local_h_tol_rel")]
    pub geometry_check_local_h_tol_rel: f32,
    /// Minimum members required to fit a line / column for the geometry
    /// check's collinearity test.
    pub line_min_members: usize,
    /// When `true`, the geometry check's tolerances are multiplied by a
    /// per-corner local step (computed from labelled grid neighbours) instead
    /// of the global cell size. Anisotropic thresholds catch outliers in
    /// dense (perspective-foreshortened) regions that a global threshold would
    /// miss, and stay loose in distorted regions where the local cell pitch
    /// grows.
    ///
    /// Default `false` (see `default_validate_step_aware` for the rationale —
    /// enabling it currently drops a labelled corner on one blessed bench
    /// image). Set to `true` to opt in per dataset.
    #[serde(default = "default_validate_step_aware")]
    pub validate_step_aware: bool,
    /// Enable the final local edge-shape gate (the direct topological
    /// wrong-label check: interior skipped-corner edges and duplicate-pixel
    /// labels).
    ///
    /// Default `true` for direct chessboard detection. Downstream
    /// target-specific detectors with their own geometry/ID alignment gates
    /// (e.g. ChArUco) may disable it to preserve recall.
    #[serde(default = "default_enable_final_edge_shape_check")]
    pub enable_final_edge_shape_check: bool,
}

impl Default for AdvancedTuning {
    fn default() -> Self {
        Self {
            topological: default_topological_params(),
            component_merge: LocalMergeParams::default(),

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
            // ChArUco snaps in testdata/). 0.05 was tuned for cleaner
            // capture conditions where corners are sharper and mass
            // concentrates tightly; 0.02 is still comfortably above
            // pure-noise bins.
            min_peak_weight_fraction: 0.02,

            attach_search_rel: 0.35,
            attach_axis_tol_deg: 15.0,
            attach_ambiguity_factor: 1.5,
            step_tol: 0.25,
            edge_axis_tol_deg: 15.0,
            enable_weak_cluster_rescue: true,
            weak_cluster_tol_deg: 18.0,
            max_booster_iters: 5,

            geometry_check_line_tol_rel: default_geometry_check_line_tol_rel(),
            geometry_check_local_h_tol_rel: default_geometry_check_local_h_tol_rel(),
            line_min_members: 3,
            validate_step_aware: default_validate_step_aware(),
            enable_final_edge_shape_check: default_enable_final_edge_shape_check(),
        }
    }
}
