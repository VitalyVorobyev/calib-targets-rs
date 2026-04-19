use calib_targets_core::{ChessConfig, OrientationClusteringParams};
use serde::{Deserialize, Serialize};

/// How [`crate::build_chessboard_grid_graph`] validates neighbor edges.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChessboardGraphMode {
    /// Legacy path: absolute `min/max_spacing_pix` window + single-orientation
    /// orthogonality check (Simple / Cluster validators). Kept as the default
    /// so existing callers keep their exact behavior.
    #[default]
    Legacy,
    /// Step-consistent two-axis validator from the plan's Phase 3. Uses the
    /// 0.6-era `axes` descriptor plus per-corner local-step estimation to
    /// reject lattice edges whose magnitude disagrees with the local step —
    /// the primary defense against ChArUco marker-internal corners.
    TwoAxis,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GridGraphParams {
    /// Validator family used when building the neighbor graph.
    #[serde(default)]
    pub mode: ChessboardGraphMode,

    // Legacy (ChessboardGraphMode::Legacy) knobs: absolute spacing window +
    // single-orientation tolerance.
    pub min_spacing_pix: f32,
    pub max_spacing_pix: f32,
    pub k_neighbors: usize,
    pub orientation_tolerance_deg: f32,

    // Two-axis (ChessboardGraphMode::TwoAxis) knobs.
    /// Lower bound on `|offset| / local_step` accepted by the two-axis
    /// validator. Defaults to 0.7.
    #[serde(default = "default_min_step_rel")]
    pub min_step_rel: f32,
    /// Upper bound on `|offset| / local_step` accepted by the two-axis
    /// validator. Defaults to 1.3.
    #[serde(default = "default_max_step_rel")]
    pub max_step_rel: f32,
    /// Angular tolerance (degrees) on the "edge lies along an axis" test used
    /// by the two-axis validator. Scales up to 2× by each endpoint's axis
    /// sigma. Defaults to 10°.
    #[serde(default = "default_angular_tol_deg")]
    pub angular_tol_deg: f32,
    /// Fallback absolute step (pixels) used by the two-axis validator when a
    /// corner's local-step confidence is zero. Also sets the KD-tree pre-filter
    /// distance. Defaults to 50 px.
    #[serde(default = "default_step_fallback_pix")]
    pub step_fallback_pix: f32,
}

fn default_min_step_rel() -> f32 {
    0.7
}

fn default_max_step_rel() -> f32 {
    1.3
}

fn default_angular_tol_deg() -> f32 {
    10.0
}

fn default_step_fallback_pix() -> f32 {
    50.0
}

impl Default for GridGraphParams {
    fn default() -> Self {
        Self {
            mode: ChessboardGraphMode::default(),
            min_spacing_pix: 5.0,
            max_spacing_pix: 50.0,
            k_neighbors: 8,
            orientation_tolerance_deg: 22.5,
            min_step_rel: default_min_step_rel(),
            max_step_rel: default_max_step_rel(),
            angular_tol_deg: default_angular_tol_deg(),
            step_fallback_pix: default_step_fallback_pix(),
        }
    }
}

/// Parameters specific to the chessboard detector.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChessboardParams {
    /// ChESS corner detector configuration.
    #[serde(default)]
    pub chess: ChessConfig,

    /// Minimal corner strength to consider.
    pub min_corner_strength: f32,

    /// Minimal number of corners in a detection to be considered valid.
    pub min_corners: usize,

    /// Expected number of *inner* corners in vertical direction (rows).
    pub expected_rows: Option<u32>,

    /// Expected number of *inner* corners in horizontal direction (cols).
    pub expected_cols: Option<u32>,

    /// Minimal completeness ratio (#detected corners / full grid size)
    /// when expected_rows/cols are provided.
    pub completeness_threshold: f32,

    pub use_orientation_clustering: bool,
    pub orientation_clustering_params: OrientationClusteringParams,

    /// Grid graph construction parameters.
    #[serde(default)]
    pub graph: GridGraphParams,

    /// Maximum ratio `fit_rms / contrast` of the upstream two-axis corner fit
    /// accepted as a candidate (using the 0.6 `CornerDescriptor.fit_rms` and
    /// `CornerDescriptor.contrast` fields surfaced on `calib_targets_core::Corner`).
    ///
    /// This is the insurance filter from the plan's P1.3: false responses on
    /// smooth ArUco marker interiors tend to have large `fit_rms` relative to
    /// `contrast`. The real defense against marker-internal corners lands in
    /// Phase 3's step-consistency validator, so this knob defaults to
    /// `f32::INFINITY` (disabled) to preserve pre-filter behavior.
    ///
    /// A corner is kept iff `fit_rms <= max_fit_rms_ratio * contrast`. Corners
    /// whose descriptor was never populated (`contrast == 0`) are accepted
    /// regardless to stay compatible with adapters that ignore the new fields.
    #[serde(default = "default_max_fit_rms_ratio")]
    pub max_fit_rms_ratio: f32,

    /// Whether the post-BFS **global**-homography residual prune runs.
    ///
    /// Default `true` preserves the pre-existing behavior: after BFS
    /// assigns `(i, j)` labels, a two-tier homography refinement drops
    /// corners whose pixel position disagrees with the best-fit ideal-
    /// grid homography (see `prune_by_homography_residual` in
    /// `detector.rs`). The global fit breaks under non-trivial lens
    /// distortion — flip this off for high-distortion / wide-FoV
    /// captures.
    #[serde(default = "default_true")]
    pub enable_global_homography_prune: bool,

    /// Local-homography residual prune knobs (Phase B).
    #[serde(default)]
    pub local_homography: LocalHomographyPruneParams,

    /// If set, the detector rejects post-prune detections whose
    /// **local-homography** residual p95 (in pixels) exceeds this value.
    ///
    /// This is the post-prune quality gate: a correctly-labelled grid
    /// always has sub-pixel local residuals, regardless of lens
    /// distortion. A high post-prune p95 signals that pruning bottomed
    /// out at `min_corners` before clearing the label errors — the
    /// remaining corners form a self-consistent-looking-but-wrong
    /// lattice.
    ///
    /// `None` (default) disables the gate for back-compat.
    #[serde(default)]
    pub max_local_homography_p95_px: Option<f32>,

    /// Post-graph geometric-sanity cleanup parameters.
    ///
    /// These run **after** the neighbor graph is built and **before**
    /// connected-component extraction. They drop edges that pass every
    /// per-edge validator in isolation but violate graph-global planar-
    /// grid invariants (asymmetric directed edges, bent Right/Left or
    /// Up/Down chains, crossing edges).
    #[serde(default)]
    pub graph_cleanup: GraphCleanupParams,

    /// Minimum size of a connected graph component to be promoted to a
    /// `TargetDetection`. Components smaller than this are dropped as
    /// chaff. When `None`, falls back to [`Self::min_corners`] for
    /// back-compat.
    #[serde(default)]
    pub min_component_size: Option<usize>,

    /// Phase 5 gap-fill parameters. After BFS + local-H prune, predict
    /// missing `(i, j)` positions from labelled neighbors and attach
    /// nearby strong corners.
    #[serde(default)]
    pub gap_fill: GapFillParams,
}

/// Phase 5 gap-fill: recover missing `(i, j)` positions whose neighbors
/// are labelled.
///
/// After BFS + local-H prune, for each integer `(i, j)` in the
/// bounding box of the labelled set, if enough labelled neighbors
/// exist inside a `window_half`-cell window, fit a local affine
/// `(i, j) → (x, y)` map, predict the missing pixel position, and
/// search the strong-corner pool for the nearest unlabelled corner
/// within `search_rel × local_step`. Attach if found.
///
/// Targets the "missing corner with all 4 neighbors detected" failure
/// mode from `docs/120issues.txt`.
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GapFillParams {
    /// Whether the gap-fill pass runs.
    #[serde(default = "default_true")]
    pub enable: bool,
    /// Window half-width in grid cells. `2` means labelled neighbors
    /// are collected from a 5×5 window around the target `(i, j)`.
    #[serde(default = "default_gap_window_half")]
    pub window_half: i32,
    /// Minimum labelled neighbors required for the affine fit. With
    /// 2 parameters per axis (`a, b`) an affine needs 3 points
    /// minimum; we require 4 by default for noise robustness.
    #[serde(default = "default_gap_min_neighbors")]
    pub min_neighbors: usize,
    /// Search radius as a fraction of the local cell size. A candidate
    /// strong corner must lie within `search_rel × local_step` of the
    /// predicted position to be attached.
    #[serde(default = "default_gap_search_rel")]
    pub search_rel: f32,
    /// Maximum refit iterations. Newly-attached corners enable further
    /// gap-fill for their neighbors, so the pass iterates until no
    /// further corners are attached (or the cap is reached).
    #[serde(default = "default_gap_max_iters")]
    pub max_iters: u32,
}

impl Default for GapFillParams {
    fn default() -> Self {
        Self {
            enable: true,
            window_half: default_gap_window_half(),
            min_neighbors: default_gap_min_neighbors(),
            search_rel: default_gap_search_rel(),
            max_iters: default_gap_max_iters(),
        }
    }
}

fn default_gap_window_half() -> i32 {
    2
}
fn default_gap_min_neighbors() -> usize {
    4
}
fn default_gap_search_rel() -> f32 {
    0.4
}
fn default_gap_max_iters() -> u32 {
    3
}

/// Knobs for the post-graph geometric-sanity cleanup passes.
///
/// All three passes default on because the cost is tiny and the wins
/// on the 120-snap dataset are large: planarity kills cross-cell
/// spurious edges, straightness kills bent L/R or U/D pairs that let
/// BFS walk across visually-disconnected regions of the image, and
/// symmetry kills one-sided "best candidate" edges where the reverse
/// vote went elsewhere.
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GraphCleanupParams {
    /// Drop directed edges whose reverse is missing.
    #[serde(default = "default_true")]
    pub enforce_symmetry: bool,
    /// Drop the worse of each Right/Left or Up/Down pair whose chord
    /// angle deviates from 180° by more than `max_straightness_deg`.
    #[serde(default = "default_true")]
    pub enforce_straightness: bool,
    /// Drop the worse of each crossing edge pair.
    #[serde(default = "default_true")]
    pub enforce_planarity: bool,
    /// Max allowed deviation (degrees) of a Right/Left or Up/Down chord
    /// pair from 180°. Above this the pair is considered bent and the
    /// worse edge is dropped. Default 15°.
    #[serde(default = "default_max_straightness_deg")]
    pub max_straightness_deg: f32,
}

impl Default for GraphCleanupParams {
    fn default() -> Self {
        Self {
            enforce_symmetry: true,
            enforce_straightness: true,
            enforce_planarity: true,
            max_straightness_deg: default_max_straightness_deg(),
        }
    }
}

fn default_max_straightness_deg() -> f32 {
    15.0
}

/// Parameters for the local-homography residual prune (Phase B).
///
/// For each labelled corner, a local homography is fit from the other
/// labelled corners within a `window_half`-cell window, the current
/// corner is predicted via that homography, and corners whose observed
/// pixel position disagrees by more than
/// `max(threshold_rel × local_step, threshold_px_floor)` are dropped.
///
/// The prune iterates (refit + redrop) up to `max_iters` times or until
/// no further corners are dropped.
#[non_exhaustive]
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalHomographyPruneParams {
    /// Whether the prune runs. Default `false` — Phase A behavior.
    #[serde(default)]
    pub enable: bool,
    /// Window half-width in grid cells. `2` means a 5×5 neighborhood.
    #[serde(default = "default_local_window_half")]
    pub window_half: i32,
    /// Minimum labelled neighbors required inside the window for a fit
    /// to be considered (below this the corner is kept — no prediction).
    #[serde(default = "default_local_min_neighbors")]
    pub min_neighbors: usize,
    /// Residual threshold as a fraction of the local step (per-corner
    /// average of `step_u`, `step_v`). When the per-corner step is
    /// unavailable, falls back to a global step estimate.
    #[serde(default = "default_local_threshold_rel")]
    pub threshold_rel: f32,
    /// Absolute pixel floor on the residual threshold — always applied
    /// whether or not a local step is available.
    #[serde(default = "default_local_threshold_px_floor")]
    pub threshold_px_floor: f32,
    /// Maximum refit/redrop iterations.
    #[serde(default = "default_local_max_iters")]
    pub max_iters: u32,
}

impl Default for LocalHomographyPruneParams {
    fn default() -> Self {
        Self {
            enable: false,
            window_half: default_local_window_half(),
            min_neighbors: default_local_min_neighbors(),
            threshold_rel: default_local_threshold_rel(),
            threshold_px_floor: default_local_threshold_px_floor(),
            max_iters: default_local_max_iters(),
        }
    }
}

fn default_local_window_half() -> i32 {
    2
}
fn default_local_min_neighbors() -> usize {
    5
}
fn default_local_threshold_rel() -> f32 {
    0.15
}
fn default_local_threshold_px_floor() -> f32 {
    2.0
}
fn default_local_max_iters() -> u32 {
    16
}

fn default_max_fit_rms_ratio() -> f32 {
    f32::INFINITY
}

fn default_true() -> bool {
    true
}

impl ChessboardParams {
    /// Three-config sweep preset: default + high-threshold + low-threshold.
    ///
    /// Useful for challenging images where a single threshold may miss corners.
    pub fn sweep_default() -> Vec<Self> {
        let base = Self::default();
        let mut high = base.clone();
        high.chess.threshold_value = 0.15;
        let mut low = base.clone();
        low.chess.threshold_value = 0.08;
        vec![base, high, low]
    }
}

impl Default for ChessboardParams {
    fn default() -> Self {
        Self {
            chess: ChessConfig::default(),
            min_corner_strength: 0.0,
            min_corners: 16,
            expected_rows: None,
            expected_cols: None,
            completeness_threshold: 0.7,
            use_orientation_clustering: true,
            orientation_clustering_params: OrientationClusteringParams::default(),
            graph: GridGraphParams::default(),
            max_fit_rms_ratio: default_max_fit_rms_ratio(),
            enable_global_homography_prune: true,
            local_homography: LocalHomographyPruneParams::default(),
            max_local_homography_p95_px: None,
            graph_cleanup: GraphCleanupParams::default(),
            min_component_size: None,
            gap_fill: GapFillParams::default(),
        }
    }
}
