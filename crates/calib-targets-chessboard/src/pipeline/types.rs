//! Output and per-stage diagnostic types for the detector pipeline.
//!
//! These are pure data carriers: the [`ChessboardDetection`] result and
//! its [`ChessboardCorner`] entries, the serde-friendly [`DebugFrame`]
//! introspection payload, and the per-stage trace structs that the
//! `bench diagnose` tooling renders. No pipeline logic lives here — see
//! [`super::run`] for the orchestrator and the sibling stage modules for
//! the stage bodies.

#[cfg(feature = "diagnostics")]
use crate::boosters::BoosterResult;
#[cfg(feature = "diagnostics")]
use crate::cluster::ClusterDebug;
#[cfg(feature = "diagnostics")]
use crate::corner::{CornerAug, CornerStage};
use calib_targets_core::GridCoords;

use nalgebra::Point2;
use projective_grid::seed_and_grow::extension::ExtensionStats;
use serde::Serialize;

/// Lean pipeline outcome for the hot `detect()` / `detect_all()` path.
///
/// Carries only the detection, *without* assembling a [`DebugFrame`]. The
/// pipeline returns this on the non-diagnostic path so the heavy
/// per-corner / per-iteration trace accumulation is never paid for. The
/// seed-derived grid cell size travels on the detection itself
/// ([`ChessboardDetection::cell_size`]); [`DebugFrame`] (built only by
/// `detect_with_diagnostics`, behind the `diagnostics` feature) carries
/// the same `detection` plus the full introspection payload.
pub(crate) struct PipelineOutcome {
    /// The final detection; `None` when the pipeline emitted no detection.
    pub detection: Option<ChessboardDetection>,
}

/// A single labelled chessboard corner.
///
/// `#[non_exhaustive]`: construct with [`ChessboardCorner::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct ChessboardCorner {
    /// Sub-pixel image position.
    pub position: Point2<f32>,
    /// Grid label (i, j). A chessboard corner is always labelled — non-optional.
    pub grid: GridCoords,
    /// Index into the detector's input `&[ChessCorner]` slice that produced this corner.
    pub input_index: usize,
    /// Corner score.
    pub score: f32,
}

impl ChessboardCorner {
    /// Create a corner from its position, grid label, input provenance, and score.
    pub fn new(position: Point2<f32>, grid: GridCoords, input_index: usize, score: f32) -> Self {
        Self {
            position,
            grid,
            input_index,
            score,
        }
    }
}

/// Result of chessboard detection: the labelled corner set.
///
/// `#[non_exhaustive]`: construct with [`ChessboardDetection::new`].
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct ChessboardDetection {
    /// The labelled corners.
    pub corners: Vec<ChessboardCorner>,
    /// Grid cell size in pixels, derived from the self-consistent 2×2
    /// seed quad's mean edge length and refined through grow. `None`
    /// when no seed was found (no detection is emitted in that case, so
    /// this is `Some` for every returned detection). Exposed on the
    /// stable result so consumers can scale geometry checks and overlays
    /// without opting into the [`crate::diagnostics`] surface.
    pub cell_size: Option<f32>,
}

impl ChessboardDetection {
    /// Create a detection from its labelled corner set.
    ///
    /// `cell_size` defaults to `None`; populate it with
    /// [`ChessboardDetection::with_cell_size`].
    pub fn new(corners: Vec<ChessboardCorner>) -> Self {
        Self {
            corners,
            cell_size: None,
        }
    }

    /// Set the grid [`cell_size`](Self::cell_size) (builder style).
    #[must_use]
    pub fn with_cell_size(mut self, cell_size: f32) -> Self {
        self.cell_size = Some(cell_size);
        self
    }
}

/// Current [`DebugFrame`] schema version.
///
/// Bumped when fields are removed, renamed, or their semantics change.
/// Adding new optional fields does NOT bump the schema. Downstream
/// tooling (Python overlay script, etc.) should warn on mismatch.
#[cfg(feature = "diagnostics")]
pub const DEBUG_FRAME_SCHEMA: u32 = 1;

/// Compact debug payload — one per detection call.
///
/// Flat and serde-friendly so the Python overlay script can render
/// every decision stage.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct DebugFrame {
    /// Schema version — see [`DEBUG_FRAME_SCHEMA`].
    pub schema: u32,
    /// Number of corners passed into the detector.
    pub input_count: usize,
    /// The two recovered grid-direction angles `[θ₀, θ₁]` in radians;
    /// `None` when clustering did not run.
    pub grid_directions: Option<[f32; 2]>,
    /// Seed-derived cell size in pixels; `None` when no seed was found.
    pub cell_size: Option<f32>,
    /// The four input-corner indices of the chosen 2×2 seed quad;
    /// `None` when `find_seed` failed.
    pub seed: Option<[usize; 4]>,
    /// One trace per `find_seed → grow → validate` iteration.
    pub iterations: Vec<IterationTrace>,
    /// Summary from the `apply_boosters` stage (`None` when boosters
    /// didn't run — e.g., empty or seed failure).
    pub boosters: Option<BoosterResult>,
    /// The final detection; `None` when the pipeline emitted no detection.
    pub detection: Option<ChessboardDetection>,
    /// All corners carried through the pipeline (same indexing as
    /// the input slice). `stage` captures where each corner ended
    /// up.
    pub corners: Vec<CornerAug>,
    /// `cluster_axes` introspection — smoothed histogram, peak
    /// seeds, refined centers. Surfaced for offline triage. `None`
    /// only when `prefilter` produced no `Strong` corners (clustering
    /// wasn't run).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_debug: Option<ClusterDebug>,
}

/// Per-iteration trace of the `find_seed → grow → validate` loop.
#[cfg(feature = "diagnostics")]
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct IterationTrace {
    /// Zero-based index of this iteration.
    pub iter: u32,
    /// Number of labelled corners at the end of this iteration.
    pub labelled_count: usize,
    /// Input-corner indices newly blacklisted by this iteration's validation.
    pub new_blacklist: Vec<usize>,
    /// `true` when validation produced no new blacklist (the loop stops).
    pub converged: bool,
    /// `extend_boundary` summary for this iteration. `None` when too
    /// few BFS labels were available, or when the fitted-H residual
    /// gate refused to extrapolate. Records what happened so we can
    /// compare blacklist scope strategies on real data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<ExtensionTrace>,
    /// `rescue_no_cluster` summary for this iteration. `None` when the
    /// rescue pass didn't run (disabled, or no converged iteration
    /// produced a labelled set to rescue around). Records the same
    /// `attached / rejected_*` breakdown as `extension`, so a diagnose
    /// dump can tell whether the rescue gate is firing or whether the
    /// relevant cells are not even being enumerated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rescue: Option<ExtensionTrace>,
    /// `refit_cluster_centers` summary for this iteration. `None`
    /// when the refit was disabled, when too few labels were
    /// available, or when the refit shift was below the trigger
    /// threshold (no second `extend_boundary` / `rescue_no_cluster`
    /// pass needed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refit: Option<RefitTrace>,
    /// Cardinal-neighbour BFS extension after refit, if
    /// `enable_post_grow_bfs_extend` is set. Records `attached /
    /// rejected_*` from
    /// `projective_grid::seed_and_grow::grow_extend::extend_from_labelled`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bfs_extend: Option<BfsExtendTrace>,
    /// Post-refit second-pass `extend_boundary` summary, if it ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension2: Option<ExtensionTrace>,
    /// Post-refit second-pass `rescue_no_cluster` summary, if it ran.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rescue2: Option<ExtensionTrace>,
    /// Final geometry-check summary. The geometry check is a
    /// MANDATORY precision gate — it always runs on every emitted
    /// detection. `None` only if no detection was emitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry_check: Option<GeometryCheckTrace>,
}

/// Diagnose payload for the post-grow cardinal-neighbour BFS
/// extension (`enable_post_grow_bfs_extend`).
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct BfsExtendTrace {
    /// Number of corners attached by the BFS extension.
    pub attached: usize,
    /// Candidate cells skipped because no corner sat near the prediction.
    pub rejected_no_candidate: usize,
    /// Candidate cells skipped because two corners were equally plausible.
    pub rejected_ambiguous: usize,
    /// Candidate corners rejected by the induced-edge geometry check.
    pub rejected_edge: usize,
    /// Input-corner indices of the corners attached in this pass.
    pub attached_indices: Vec<usize>,
}

/// Diagnose payload for the post-grow centre refit
/// (`refit_cluster_centers`).
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct RefitTrace {
    /// Magnitude of the centre shift (degrees, max over both axes).
    pub shift_deg: f32,
    /// New `(θ₀, θ₁)` after refit, in degrees.
    pub new_centers_deg: [f32; 2],
    /// Number of labelled corners used in the refit.
    pub labelled_used: usize,
    /// Number of `Strong` / `NoCluster` corners promoted to
    /// `Clustered` under the new centres.
    pub promoted: u32,
    /// Whether the second `extend_boundary` / `rescue_no_cluster`
    /// pass actually ran (i.e. the shift was above
    /// `refit_min_shift_deg`).
    pub second_pass_ran: bool,
}

/// Diagnose payload for the mandatory final geometry check.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize)]
pub struct GeometryCheckTrace {
    /// Number of labelled corners that failed the geometry check
    /// and were dropped from the final detection.
    pub dropped: u32,
    /// Drops attributed to the line-collinearity predicate.
    pub dropped_line_collinearity: u32,
    /// Drops attributed to the local-homography residual predicate.
    pub dropped_local_h_residual: u32,
    /// Drops attributed to the final local edge-shape predicate
    /// (cardinal support, adjacent-edge continuation, and complete-cell
    /// opposite-side consistency).
    pub dropped_edge_invariant: u32,
    /// Number of labelled corners dropped because they were not in
    /// the largest cardinally-connected component. Catches isolated
    /// false-positive labels (a marker corner that happened to pass
    /// the cluster + edge gates but sits below or to the side of the
    /// main grid with no cardinal labelled neighbours).
    pub dropped_disconnected: u32,
    /// Number of cardinally-connected components found before the
    /// drop pass. `1` is the chessboard contract; `> 1` always
    /// triggers `dropped_disconnected > 0`.
    pub components_seen: u32,
    /// Whether the detection was refused entirely because the
    /// surviving labelled count fell below `min_labeled_corners`.
    pub detection_refused: bool,
}

/// Diagnose payload for one homography-based boundary-extension pass
/// (`extend_boundary` / `rescue_no_cluster`). Mirrors
/// [`ExtensionStats`](projective_grid::seed_and_grow::extension::ExtensionStats).
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ExtensionTrace {
    /// `false` when the residual gate refused to extrapolate (no-op pass).
    pub h_trusted: bool,
    /// Median reprojection residual on the labelled set in pixels;
    /// `None` when the H wasn't fit.
    pub h_residual_median_px: Option<f32>,
    /// Maximum reprojection residual on the labelled set in pixels;
    /// `None` when the H wasn't fit.
    pub h_residual_max_px: Option<f32>,
    /// Number of extension iterations actually run.
    pub iterations: usize,
    /// Number of corners attached across all iterations.
    pub attached: usize,
    /// Candidate cells skipped because no corner sat near the prediction.
    pub rejected_no_candidate: usize,
    /// Candidate cells skipped because two corners were equally plausible.
    pub rejected_ambiguous: usize,
    /// Candidate cells skipped because the target `(i, j)` was already labelled.
    pub rejected_label: usize,
    /// Candidate corners rejected by the square-grid attach policy.
    pub rejected_policy: usize,
    /// Candidate corners rejected by the induced-edge geometry check.
    pub rejected_edge: usize,
    /// Input-corner indices of the corners attached in this pass.
    pub attached_indices: Vec<usize>,
}

impl From<&ExtensionStats> for ExtensionTrace {
    fn from(s: &ExtensionStats) -> Self {
        Self {
            h_trusted: s.h_trusted,
            h_residual_median_px: s.h_residual_median_px,
            h_residual_max_px: s.h_residual_max_px,
            iterations: s.iterations,
            attached: s.attached,
            rejected_no_candidate: s.rejected_no_candidate,
            rejected_ambiguous: s.rejected_ambiguous,
            rejected_label: s.rejected_label,
            rejected_policy: s.rejected_policy,
            rejected_edge: s.rejected_edge,
            attached_indices: s.attached_indices.clone(),
        }
    }
}

/// Compact per-stage counters derived from a [`DebugFrame`].
///
/// Cheaper to log / dashboard than the full [`DebugFrame`]: each field is a
/// single integer (or boolean). Obtain a [`DebugFrame`] from
/// [`Detector::detect_with_diagnostics`](crate::Detector::detect_with_diagnostics)
/// and pass it to [`StageCounts::from_frame`].
#[cfg(feature = "diagnostics")]
#[derive(Clone, Copy, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct StageCounts {
    /// Corners passed into the detector.
    pub input_corners: usize,
    /// Corners reaching `CornerStage::Strong` (`prefilter` survivors).
    pub after_strength_filter: usize,
    /// Corners reaching `CornerStage::Clustered` (`cluster_axes`
    /// survivors).
    pub after_clustering: usize,
    /// `true` if a seed was found (`find_seed` succeeded).
    pub seed_found: bool,
    /// Number of validation iterations executed
    /// (`find_seed → grow → validate` loop).
    pub validation_iterations: u32,
    /// Total corners blacklisted across all validation iterations.
    pub blacklisted_total: usize,
    /// Final labelled corner count after `apply_boosters` (`0` if no
    /// detection was emitted).
    pub labelled_final: usize,
}

#[cfg(feature = "diagnostics")]
impl StageCounts {
    /// Derive counts from a [`DebugFrame`].
    pub fn from_frame(frame: &DebugFrame) -> Self {
        let mut counts = StageCounts {
            input_corners: frame.input_count,
            ..Default::default()
        };
        for aug in &frame.corners {
            match aug.stage {
                CornerStage::Strong
                | CornerStage::Clustered { .. }
                | CornerStage::AttachmentAmbiguous { .. }
                | CornerStage::AttachmentFailedInvariants { .. }
                | CornerStage::Labeled { .. }
                | CornerStage::LabeledThenBlacklisted { .. } => {
                    counts.after_strength_filter += 1;
                }
                CornerStage::Raw | CornerStage::NoCluster { .. } => {}
            }
            match aug.stage {
                CornerStage::Clustered { .. }
                | CornerStage::AttachmentAmbiguous { .. }
                | CornerStage::AttachmentFailedInvariants { .. }
                | CornerStage::Labeled { .. }
                | CornerStage::LabeledThenBlacklisted { .. } => {
                    counts.after_clustering += 1;
                }
                _ => {}
            }
            if matches!(aug.stage, CornerStage::LabeledThenBlacklisted { .. }) {
                counts.blacklisted_total += 1;
            }
        }
        counts.seed_found = frame.seed.is_some();
        counts.validation_iterations = frame.iterations.len() as u32;
        if let Some(d) = &frame.detection {
            counts.labelled_final = d.corners.len();
        }
        counts
    }
}
