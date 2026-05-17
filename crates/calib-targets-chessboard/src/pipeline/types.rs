//! Output and per-stage diagnostic types for the detector pipeline.
//!
//! These are pure data carriers: the [`Detection`] result, the
//! serde-friendly [`DebugFrame`] introspection payload, and the
//! per-stage trace structs that the `bench diagnose` tooling renders.
//! No pipeline logic lives here — see [`super::run`] for the
//! orchestrator and the sibling stage modules for the stage bodies.

use crate::boosters::BoosterResult;
use crate::cluster::ClusterDebug;
use crate::corner::{CornerAug, CornerStage};
use calib_targets_core::TargetDetection;

use projective_grid::square::grow_extension::ExtensionStats;
use serde::Serialize;

/// Final detection output.
#[derive(Clone, Debug, Serialize)]
pub struct Detection {
    pub grid_directions: [f32; 2],
    pub cell_size: f32,
    pub target: TargetDetection,
    /// Indices into the input `corners` slice for the labelled corners,
    /// in the same order as `target.corners`.
    ///
    /// Consumers that need to map labelled grid points back to raw ChESS
    /// inputs (e.g., ChArUco marker alignment) should read this vector
    /// rather than reconstructing the mapping from positions.
    pub strong_indices: Vec<usize>,
}

/// Current [`DebugFrame`] schema version.
///
/// Bumped when fields are removed, renamed, or their semantics change.
/// Adding new optional fields does NOT bump the schema. Downstream
/// tooling (Python overlay script, etc.) should warn on mismatch.
pub const DEBUG_FRAME_SCHEMA: u32 = 1;

/// Compact debug payload — one per detection call.
///
/// Flat and serde-friendly so the Python overlay script can render
/// every decision stage.
#[derive(Clone, Debug, Serialize)]
pub struct DebugFrame {
    /// Schema version — see [`DEBUG_FRAME_SCHEMA`].
    pub schema: u32,
    pub input_count: usize,
    pub grid_directions: Option<[f32; 2]>,
    pub cell_size: Option<f32>,
    pub seed: Option<[usize; 4]>,
    pub iterations: Vec<IterationTrace>,
    /// Summary from the `apply_boosters` stage (`None` when boosters
    /// didn't run — e.g., empty or seed failure).
    pub boosters: Option<BoosterResult>,
    pub detection: Option<Detection>,
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

#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct IterationTrace {
    pub iter: u32,
    pub labelled_count: usize,
    pub new_blacklist: Vec<usize>,
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
    /// `projective_grid::square::grow_extend::extend_from_labelled`.
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
    pub attached: usize,
    pub rejected_no_candidate: usize,
    pub rejected_ambiguous: usize,
    pub rejected_edge: usize,
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
    /// Reason summary: count of drops attributed to each predicate.
    pub dropped_line_collinearity: u32,
    pub dropped_local_h_residual: u32,
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

#[derive(Clone, Debug, Serialize)]
pub struct ExtensionTrace {
    pub h_trusted: bool,
    pub h_residual_median_px: Option<f32>,
    pub h_residual_max_px: Option<f32>,
    pub iterations: usize,
    pub attached: usize,
    pub rejected_no_candidate: usize,
    pub rejected_ambiguous: usize,
    pub rejected_label: usize,
    pub rejected_validator: usize,
    pub rejected_edge: usize,
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
            rejected_validator: s.rejected_validator,
            rejected_edge: s.rejected_edge,
            attached_indices: s.attached_indices.clone(),
        }
    }
}

/// Compact per-stage counters derived from a [`DebugFrame`].
///
/// Cheaper to log / dashboard than the full [`DebugFrame`]: each field is a
/// single integer (or boolean). Use
/// [`Detector::detect_instrumented`](crate::Detector::detect_instrumented)
/// to get these alongside the detection.
#[derive(Clone, Copy, Debug, Default, Serialize)]
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
            counts.labelled_final = d.target.corners.len();
        }
        counts
    }
}

/// A [`Detection`] (when produced) paired with derived [`StageCounts`].
///
/// `detection` may be `None` even when counts are populated — the pipeline
/// always runs to whichever stage failed first.
#[derive(Clone, Debug, Serialize)]
pub struct InstrumentedResult {
    pub detection: Option<Detection>,
    pub counts: StageCounts,
}
