//! Advanced, unstable per-stage tuning knobs for the chessboard detector.
//!
//! [`ChessboardAdvancedTuning`] is the opt-in sub-struct behind
//! [`ChessboardParams::advanced`](super::ChessboardParams::advanced). It holds the
//! tuning knobs for the live topological pipeline stages: the grid builder, the
//! shared component merge, the strength/fit pre-filter, axis clustering, the
//! recall boosters, and the mandatory final geometry check.
//!
//! **Stability.** Unlike the stable core on [`ChessboardParams`], the fields of
//! `ChessboardAdvancedTuning` are **NOT covered by semver**. They are named after, and
//! coupled to, internal pipeline stages, so they may be **renamed, retyped, or
//! removed between minor versions** as the detector evolves. Treat them as an
//! escape hatch for a specific failing input backed by evidence — not as part
//! of the public configuration contract. A calibration consumer has no basis
//! to set any of them and should leave the struct at [`Default`].
//!
//! When [`ChessboardParams::advanced`](super::ChessboardParams::advanced) is set,
//! the whole struct is serialized under a nested `"advanced"` JSON object — the
//! knobs are **not** flattened into the top-level config. When it is `None`,
//! no `"advanced"` key appears and the detector behaves exactly as if every
//! knob held its [`Default`] value.
//!
//! All spatial tolerances are **multiplicative with respect to `s`** (the
//! global cell size) — the pipeline is scale-invariant once `s` is known. All
//! angular tolerances are absolute degrees.

use projective_grid::expert::component::LocalMergeParams;
use projective_grid::expert::TopologicalParams;
use serde::{Deserialize, Serialize};

/// Advanced, **unstable** per-stage tuning knobs for the chessboard detector.
///
/// Behind [`ChessboardParams::advanced`](super::ChessboardParams::advanced). The
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
/// [`ChessboardParams`](super::ChessboardParams)'s three top-level fields.
///
/// When set on [`ChessboardParams`](super::ChessboardParams) via
/// [`with_advanced`](super::ChessboardParams::with_advanced), the whole struct
/// serializes under a nested `"advanced"` JSON object — the knobs are not
/// flattened. When left unset, the serialized config carries no `"advanced"`
/// key and detection behaves exactly as if every knob held its [`Default`]
/// value (see [`ChessboardParams::effective_tuning`](super::ChessboardParams::effective_tuning)).
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChessboardAdvancedTuning {
    /// Tuning knobs for the topological (Delaunay + axis-driven cell test)
    /// grid builder — the only graph builder the detector ships.
    pub topological: TopologicalParams,

    /// Tuning knobs for the shared local-geometry component merger that
    /// reunites the topological grid's connected components in label space.
    pub component_merge: LocalMergeParams,

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
    /// [`cluster_sigma_k`]: ChessboardAdvancedTuning::cluster_sigma_k
    pub cluster_tol_deg: f32,
    /// Multiplier on the per-corner axis sigma added to [`cluster_tol_deg`]
    /// when admitting a corner. Default `0.0`: sigma-aware tolerance is
    /// plumbed through but disabled. Set to e.g. `2.0` (a ≈95% one-sided
    /// confidence band) to pass corners whose true axis is within tolerance
    /// but whose ChESS estimate fell outside under noise.
    ///
    /// [`cluster_tol_deg`]: ChessboardAdvancedTuning::cluster_tol_deg
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

    /// Enable a final, geometry-only rescue for strong corners whose measured
    /// axes failed clustering.
    ///
    /// This pass is deliberately narrower than the axis-aware booster: it
    /// requires an L-shaped set of three already-labelled neighbours and does
    /// not let recovered corners support further recovery. It exists for the
    /// case where the image-space corner position is reliable but the local
    /// orientation fit is not.
    #[serde(default = "default_enable_geometry_only_recovery")]
    pub enable_geometry_only_recovery: bool,
    /// Maximum geometry-only prediction residual and candidate-search radius,
    /// as a fraction of the local cell size.
    #[serde(default = "default_geometry_recovery_tol_rel")]
    pub geometry_recovery_tol_rel: f32,

    // --- mandatory final geometry check -------------------------------------
    /// Line-collinearity tolerance (fraction of cell_size) for the MANDATORY
    /// final geometry check that runs before any detection is emitted. Loose
    /// by design — the geometry check's role is to catch gross mislabels
    /// (diagonal / full-cell shifts), not the borderline perspective drift the
    /// grid builder already accepted.
    ///
    /// Default `0.45` of cell_size. A tight tolerance here produces
    /// catastrophic recall regressions on heavy-radial-distortion boards.
    pub geometry_check_line_tol_rel: f32,
    /// Local-H residual tolerance (fraction of cell_size) for the MANDATORY
    /// final geometry check. A diagonal mislabel shifts a corner by ~1.4 cell
    /// from its predicted position; `0.6 × cell_size` is well below that gap
    /// while leaving the legitimate perspective-distorted corners alone.
    ///
    /// Default `0.6` of cell_size.
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
    /// Default `false`: the capability is shipped without changing behaviour.
    /// Enabling it currently drops one labelled corner on
    /// `testdata/puzzleboard_reference/example1.png` (the tighter back-edge
    /// tolerance over-flags), so treat it as a focused per-dataset experiment
    /// until a tuned tolerance pair holds the precision contract on every
    /// blessed image. Set to `true` to opt in.
    pub validate_step_aware: bool,
    /// Enable the final local edge-shape gate (the direct topological
    /// wrong-label check: interior skipped-corner edges and duplicate-pixel
    /// labels).
    ///
    /// Default `true` for direct chessboard detection. Downstream
    /// target-specific detectors with their own geometry/ID alignment gates
    /// (e.g. ChArUco) may disable it to preserve recall.
    pub enable_final_edge_shape_check: bool,
}

impl Default for ChessboardAdvancedTuning {
    fn default() -> Self {
        Self {
            topological: TopologicalParams::default()
                .with_opposing_edge_ratio_max(10.0)
                .with_edge_length_band(0.0, 1.8),
            component_merge: LocalMergeParams::default(),

            num_bins: 90,
            max_iters_2means: 10,
            cluster_tol_deg: 12.0,
            cluster_sigma_k: 0.0,
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
            enable_geometry_only_recovery: default_enable_geometry_only_recovery(),
            geometry_recovery_tol_rel: default_geometry_recovery_tol_rel(),

            geometry_check_line_tol_rel: 0.45,
            geometry_check_local_h_tol_rel: 0.6,
            line_min_members: 3,
            validate_step_aware: false,
            enable_final_edge_shape_check: true,
        }
    }
}

const fn default_enable_geometry_only_recovery() -> bool {
    true
}

const fn default_geometry_recovery_tol_rel() -> f32 {
    0.15
}
