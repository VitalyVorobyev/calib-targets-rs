//! Detector orchestrator: run the precision core end-to-end.
//!
//! Stages 5–7 loop with a blacklist until validation converges or
//! `max_validation_iters` is reached. Stage 8 (recall boosters) extends
//! the labelled set without compromising invariants. `detect_all` peels
//! off disconnected components by re-entering the pipeline with already-
//! labelled inputs marked consumed.
//!
//! See `book/src/chessboard.md` for the full algorithm description.

use crate::boosters::{apply_boosters, BoosterResult};
use crate::cluster::{cluster_axes, ClusterCenters};
use crate::corner::{CornerAug, CornerStage};
use crate::grow::{grow_from_seed, ChessboardGrowValidator, GrowResult};
use crate::params::DetectorParams;
use crate::seed::{find_seed, SeedOutput};
use crate::validate::{validate, ValidationResult};
use calib_targets_core::{Corner, GridCoords, LabeledCorner, TargetDetection, TargetKind};
use nalgebra::Point2;
use projective_grid::square::grow_extension::{
    extend_via_global_homography, extend_via_local_homography, ExtensionParams, ExtensionStats,
    LocalExtensionParams,
};
use serde::Serialize;
use std::collections::HashSet;

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
    /// Summary from the Stage-8 recall boosters (`None` when
    /// boosters didn't run — e.g., empty or Stage-5 failure).
    pub boosters: Option<BoosterResult>,
    pub detection: Option<Detection>,
    /// All corners carried through the pipeline (same indexing as
    /// the input slice). `stage` captures where each corner ended
    /// up.
    pub corners: Vec<CornerAug>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IterationTrace {
    pub iter: u32,
    pub labelled_count: usize,
    pub new_blacklist: Vec<usize>,
    pub converged: bool,
    /// Stage-6 (boundary extrapolation) summary for this iteration.
    /// `None` when too few BFS labels were available, or when the
    /// fitted-H residual gate refused to extrapolate. Records what
    /// happened so we can compare blacklist scope strategies on real
    /// data (Q2 of the deep-dive roadmap).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<ExtensionTrace>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExtensionTrace {
    pub h_trusted: bool,
    pub h_residual_median_px: Option<f32>,
    pub h_residual_max_px: Option<f32>,
    pub iterations: u32,
    pub attached: u32,
    pub rejected_no_candidate: u32,
    pub rejected_ambiguous: u32,
    pub rejected_label: u32,
    pub rejected_validator: u32,
    pub rejected_edge: u32,
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
/// single integer (or boolean). Use [`Detector::detect_instrumented`] to get
/// these alongside the detection.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct StageCounts {
    /// Corners passed into the detector.
    pub input_corners: usize,
    /// Corners reaching `CornerStage::Strong` (Stage 1 survivors).
    pub after_strength_filter: usize,
    /// Corners reaching `CornerStage::Clustered` (Stage 3 survivors).
    pub after_clustering: usize,
    /// `true` if a seed was found (Stage 5 succeeded).
    pub seed_found: bool,
    /// Number of validation iterations executed (Stages 5–7 loop).
    pub validation_iterations: u32,
    /// Total corners blacklisted across all validation iterations.
    pub blacklisted_total: usize,
    /// Final labelled corner count after Stage 8 boosters
    /// (`0` if no detection was emitted).
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

/// Top-level detector.
pub struct Detector {
    pub params: DetectorParams,
}

impl Detector {
    pub fn new(params: DetectorParams) -> Self {
        Self { params }
    }

    /// Simple entry point: run the pipeline and return the best detection.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect(&self, corners: &[Corner]) -> Option<Detection> {
        self.detect_debug(corners).detection
    }

    /// Full-debug entry point for a single best detection.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_debug(&self, corners: &[Corner]) -> DebugFrame {
        self.detect_debug_excluding(corners, &HashSet::new())
    }

    /// Return every qualifying grid component from a single scene.
    ///
    /// Useful for ChArUco and similar setups where a single physical board
    /// can be split into multiple disconnected chessboard pieces by
    /// markers or occlusions. Each returned [`Detection`] carries its own
    /// locally-rebased `(i, j)` labels; alignment to a global frame is the
    /// caller's responsibility (ChArUco's marker decoding does this).
    ///
    /// Capped by [`DetectorParams::max_components`].
    ///
    /// Does NOT support scenes with multiple separate physical boards — one
    /// target per frame is the contract.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_all(&self, corners: &[Corner]) -> Vec<Detection> {
        self.detect_all_debug(corners)
            .into_iter()
            .filter_map(|f| f.detection)
            .collect()
    }

    /// Single-detection entry with derived per-stage counts.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_instrumented(&self, corners: &[Corner]) -> InstrumentedResult {
        let frame = self.detect_debug(corners);
        let counts = StageCounts::from_frame(&frame);
        InstrumentedResult {
            detection: frame.detection,
            counts,
        }
    }

    /// Multi-component entry with per-component derived counts.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_all_instrumented(&self, corners: &[Corner]) -> Vec<InstrumentedResult> {
        self.detect_all_debug(corners)
            .into_iter()
            .map(|frame| {
                let counts = StageCounts::from_frame(&frame);
                InstrumentedResult {
                    detection: frame.detection,
                    counts,
                }
            })
            .collect()
    }

    /// Full-debug multi-component entry point. See [`Self::detect_all`].
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_all_debug(&self, corners: &[Corner]) -> Vec<DebugFrame> {
        let cap = self.params.max_components.max(1) as usize;
        let mut consumed: HashSet<usize> = HashSet::new();
        let mut frames: Vec<DebugFrame> = Vec::with_capacity(cap);

        for _ in 0..cap {
            let frame = self.detect_debug_excluding(corners, &consumed);
            let Some(detection) = frame.detection.as_ref() else {
                // No further detectable component — include the empty frame
                // so caller can introspect the failure stage if desired.
                if frames.is_empty() {
                    frames.push(frame);
                }
                break;
            };
            for &idx in &detection.strong_indices {
                consumed.insert(idx);
            }
            frames.push(frame);
        }

        frames
    }

    fn detect_debug_excluding(&self, corners: &[Corner], consumed: &HashSet<usize>) -> DebugFrame {
        let input_count = corners.len();
        let mut augs: Vec<CornerAug> = corners
            .iter()
            .enumerate()
            .map(|(i, c)| CornerAug::from_corner(i, c))
            .collect();

        let mut frame = DebugFrame {
            schema: DEBUG_FRAME_SCHEMA,
            input_count,
            grid_directions: None,
            cell_size: None,
            seed: None,
            iterations: Vec::new(),
            boosters: None,
            detection: None,
            corners: Vec::new(),
        };

        // Stage 1: pre-filter.
        // Corners already consumed by a previous `detect_all` iteration are
        // left at `Raw` stage, which makes them invisible to clustering,
        // seed search, grow, and validation.
        for aug in augs.iter_mut() {
            if consumed.contains(&aug.input_index) {
                continue;
            }
            if passes_strength(aug, &self.params) && passes_fit_quality(aug, &self.params) {
                aug.stage = CornerStage::Strong;
            }
        }
        if augs
            .iter()
            .filter(|a| matches!(a.stage, CornerStage::Strong))
            .count()
            < self.params.min_labeled_corners
        {
            frame.corners = augs;
            return frame;
        }

        // Stage 2 + 3: clustering.
        let centers = match cluster_axes(&mut augs, &self.params) {
            Some(c) => c,
            None => {
                frame.corners = augs;
                return frame;
            }
        };
        frame.grid_directions = Some([centers.theta0, centers.theta1]);

        // Stages 4+5 (fused): the seed finder is now self-consistent
        // — it finds a 4-corner quad that matches itself in edge
        // lengths, and reports `cell_size` as the mean seed-edge
        // length. This avoids the bimodal-histogram failure where
        // the old global cell-size estimator picked a too-small
        // mode (typically marker-internal spacing rather than true
        // board spacing), leaving the downstream edge-window
        // `[0.75s, 1.25s]` excluding every legitimate neighbor.
        //
        // The detector loops with a blacklist; each iteration re-
        // runs the seed + growth pair.
        let mut blacklist: HashSet<usize> = HashSet::new();
        let max_iters = self.params.max_validation_iters.max(1);

        for it in 0..max_iters {
            // Reset any Labeled stage on corners not in blacklist —
            // re-run means re-label from scratch in this iteration.
            for aug in augs.iter_mut() {
                if matches!(aug.stage, CornerStage::Labeled { .. })
                    && !blacklist.contains(&aug.input_index)
                {
                    // Stage-3 → Stage-5 invariant: every Labeled corner
                    // carries its cluster label. If it's somehow missing,
                    // leave the stage as-is rather than panicking — the
                    // next iteration's checks will re-handle this corner.
                    if let Some(label) = aug.label {
                        aug.stage = CornerStage::Clustered { label };
                    }
                }
            }

            let seed_out: SeedOutput = match find_seed(&augs, centers, &self.params) {
                Some(s) => s,
                None => break,
            };
            let seed = seed_out.seed;
            let cell_size = seed_out.cell_size;
            frame.cell_size = Some(cell_size);
            frame.seed = Some([seed.a, seed.b, seed.c, seed.d]);

            let mut grow_res: GrowResult = grow_from_seed(
                &mut augs,
                seed,
                centers,
                cell_size,
                &blacklist,
                &self.params,
            );

            let labelled_count = grow_res.labelled.len();

            let v: ValidationResult = validate(&augs, &grow_res.labelled, cell_size, &self.params);
            let new_blacklist: Vec<usize> = v
                .blacklist
                .iter()
                .filter(|idx| !blacklist.contains(idx))
                .copied()
                .collect();

            let converged = new_blacklist.is_empty();
            // Soft convergence: when the validator keeps flagging a
            // small residual set (≤ 2 corners) over multiple rounds,
            // the labelled set has effectively stabilised — we're
            // oscillating on borderline-outlier corners. Apply the
            // current round's blacklist and accept. Bounded below
            // by `iter >= 2` so we never emit until we've seen
            // at least two validation passes confirm the bulk of
            // the labels.
            let soft_converged = !converged
                && it + 1 >= 2
                && new_blacklist.len() <= 2
                && labelled_count >= self.params.min_labeled_corners;
            // `iteration_extension` is filled below, after Stage 6 runs
            // on the converged labelled set.
            let iteration_extension: Option<ExtensionTrace>;

            if converged || soft_converged {
                if soft_converged {
                    // Apply the current round's blacklist before
                    // accepting so the emitted detection excludes
                    // the flagged outliers.
                    for &idx in &new_blacklist {
                        if let CornerStage::Labeled { at, .. } = augs[idx].stage {
                            augs[idx].stage = CornerStage::LabeledThenBlacklisted {
                                at,
                                reason: "soft-convergence outlier".into(),
                            };
                        }
                        grow_res.labelled.retain(|_, &mut v| v != idx);
                        grow_res.by_corner.remove(&idx);
                        blacklist.insert(idx);
                    }
                }

                // Stage 6: boundary extrapolation via globally-fit
                // homography. Runs on the **converged + validated**
                // labelled set so the H fit isn't pulled by mid-loop
                // candidates that the validator would later reject.
                // Same parity / axis-cluster / edge invariants as BFS,
                // plus a tighter ambiguity gate. Approach (b) blacklist
                // scope (Q2): we re-validate immediately after Stage 6
                // and drop any extension attachments the validator
                // rejects, but DON'T re-run the BFS / re-fit H.
                let extension_stats = run_stage6(
                    &augs,
                    &mut grow_res,
                    centers,
                    cell_size,
                    &blacklist,
                    &self.params,
                );
                for (k, &idx) in extension_stats.attached_indices.iter().enumerate() {
                    let at = extension_stats.attached_cells[k];
                    augs[idx].stage = CornerStage::Labeled {
                        at,
                        local_h_residual_px: None,
                    };
                }
                if extension_stats.attached > 0 {
                    // Re-validate on the extended set. Any rejection that
                    // targets an extension attachment is dropped via
                    // approach (b); rejections that target BFS labels
                    // are also applied (the H fit may have surfaced a
                    // borderline corner the inner loop missed).
                    let v_post = validate(&augs, &grow_res.labelled, cell_size, &self.params);
                    for &idx in v_post.blacklist.iter() {
                        if blacklist.contains(&idx) {
                            continue;
                        }
                        if let CornerStage::Labeled { at, .. } = augs[idx].stage {
                            augs[idx].stage = CornerStage::LabeledThenBlacklisted {
                                at,
                                reason: "post-extension outlier".into(),
                            };
                        }
                        grow_res.labelled.retain(|_, &mut v| v != idx);
                        grow_res.by_corner.remove(&idx);
                        blacklist.insert(idx);
                    }
                }
                // Stage 6 ran if it produced residual stats (either
                // global-H, which sets `h_quality`, or local-H, which
                // sets `h_residual_median_px` per-candidate aggregate).
                iteration_extension = if extension_stats.h_quality.is_some()
                    || extension_stats.h_residual_median_px.is_some()
                {
                    Some(ExtensionTrace::from(&extension_stats))
                } else {
                    None
                };

                let mut grow_mut = grow_res;

                // Phase E recall boosters: interior gap fill + line
                // extrapolation. Runs after Stage 6 so it can fill
                // any holes the global-H pass left behind. Same
                // attachment invariants as growth.
                let booster: BoosterResult = apply_boosters(
                    &mut augs,
                    &mut grow_mut,
                    centers,
                    cell_size,
                    &blacklist,
                    &self.params,
                );
                frame.boosters = Some(booster);

                // Write local-H residuals onto labelled corners.
                for (&c_idx, &resid) in &v.local_h_residuals {
                    if let CornerStage::Labeled { at, .. } = augs[c_idx].stage {
                        augs[c_idx].stage = CornerStage::Labeled {
                            at,
                            local_h_residual_px: Some(resid),
                        };
                    }
                }
                frame.iterations.push(IterationTrace {
                    iter: it,
                    labelled_count,
                    new_blacklist: new_blacklist.clone(),
                    converged: converged || soft_converged,
                    extension: iteration_extension,
                });
                let final_count = grow_mut.labelled.len();
                if final_count >= self.params.min_labeled_corners {
                    frame.detection = Some(build_detection(&augs, &grow_mut, centers, cell_size));
                }
                break;
            }

            // Non-converged iteration: record trace without extension.
            frame.iterations.push(IterationTrace {
                iter: it,
                labelled_count,
                new_blacklist: new_blacklist.clone(),
                converged: false,
                extension: None,
            });

            // Mark blacklisted corners and retry.
            for &idx in &new_blacklist {
                if let CornerStage::Labeled { at, .. } = augs[idx].stage {
                    augs[idx].stage = CornerStage::LabeledThenBlacklisted {
                        at,
                        reason: "post-validation outlier".into(),
                    };
                }
                blacklist.insert(idx);
            }
        }

        frame.corners = augs;
        frame
    }
}

/// Stage 6: boundary extrapolation via globally-fit homography.
///
/// Builds a `Point2<f32>` view of the corner positions and a fresh
/// chessboard validator, then delegates to
/// [`projective_grid::square::grow_extension::extend_via_global_homography`].
/// The extension's blacklist tracking is approach (b): rejected
/// attachments fall through to the regular Stage-7 mechanism on the
/// next iteration. Stats include `attached_indices` for future
/// approach-(a) comparison work.
fn run_stage6(
    corners: &[CornerAug],
    grow_res: &mut GrowResult,
    centers: ClusterCenters,
    cell_size: f32,
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
) -> ExtensionStats {
    let positions: Vec<Point2<f32>> = corners.iter().map(|c| c.position).collect();
    let mut pg_grow = projective_grid::square::grow::GrowResult {
        labelled: std::mem::take(&mut grow_res.labelled),
        by_corner: std::mem::take(&mut grow_res.by_corner),
        ambiguous: std::mem::take(&mut grow_res.ambiguous),
        holes: std::mem::take(&mut grow_res.holes),
        grid_u: grow_res.grid_u,
        grid_v: grow_res.grid_v,
    };

    let validator = ChessboardGrowValidator::new(corners, blacklist, centers, cell_size, params);
    let stats = if params.stage6_local_h {
        let mut local_params = LocalExtensionParams::default();
        local_params.k_nearest = params.stage6_local_k_nearest;
        extend_via_local_homography(
            &positions,
            &mut pg_grow,
            cell_size,
            &local_params,
            &validator,
        )
    } else {
        extend_via_global_homography(
            &positions,
            &mut pg_grow,
            cell_size,
            &ExtensionParams::default(),
            &validator,
        )
    };

    grow_res.labelled = pg_grow.labelled;
    grow_res.by_corner = pg_grow.by_corner;
    grow_res.ambiguous = pg_grow.ambiguous;
    grow_res.holes = pg_grow.holes;
    stats
}

fn passes_strength(aug: &CornerAug, params: &DetectorParams) -> bool {
    aug.strength >= params.min_corner_strength
}

fn passes_fit_quality(aug: &CornerAug, params: &DetectorParams) -> bool {
    if !params.max_fit_rms_ratio.is_finite() {
        return true;
    }
    if aug.contrast <= 0.0 {
        return true;
    }
    aug.fit_rms <= params.max_fit_rms_ratio * aug.contrast
}

fn build_detection(
    corners: &[CornerAug],
    grow: &GrowResult,
    centers: ClusterCenters,
    cell_size: f32,
) -> Detection {
    // Grow rebases (i, j) to non-negative already, but late-stage
    // mutations (soft-convergence outlier removal, booster additions
    // that extend the grid past the prior bbox) can leave the set
    // un-rebased. Re-rebase here so the non-negative invariant is a
    // `build_detection`-side guarantee.
    let mut labelled_pairs: Vec<((i32, i32), usize)> =
        grow.labelled.iter().map(|(&k, &v)| (k, v)).collect();
    if !labelled_pairs.is_empty() {
        let (min_i, min_j) = labelled_pairs
            .iter()
            .fold((i32::MAX, i32::MAX), |(a, b), &((i, j), _)| {
                (a.min(i), b.min(j))
            });
        if min_i != 0 || min_j != 0 {
            for ((i, j), _) in labelled_pairs.iter_mut() {
                *i -= min_i;
                *j -= min_j;
            }
        }
    }

    // Canonicalize orientation so +i points roughly +x (right) and +j
    // points roughly +y (down) in image coords. The grow stage assigns
    // (i, j) from the seed's internal axis-slot convention, which has
    // no relation to image orientation; without this step, (0, 0) can
    // land anywhere on the detected board.
    let swap_axes = canonicalize_orientation(&mut labelled_pairs, corners);

    // Sort by (j, i) so the output order is stable and we don't need a
    // post-hoc unwrap on `grid` downstream.
    labelled_pairs.sort_by_key(|&((i, j), _)| (j, i));

    let mut labeled_corners: Vec<LabeledCorner> = Vec::with_capacity(labelled_pairs.len());
    let mut strong_indices: Vec<usize> = Vec::with_capacity(labelled_pairs.len());
    for ((i, j), c_idx) in labelled_pairs {
        let c = &corners[c_idx];
        labeled_corners.push(LabeledCorner {
            position: c.position,
            grid: Some(GridCoords { i, j }),
            id: None,
            target_position: None,
            score: c.strength,
        });
        strong_indices.push(c.input_index);
    }

    // Swap the reported grid-direction angles when axes were swapped so
    // `grid_directions[0]` still describes the +i axis.
    let grid_directions = if swap_axes {
        [centers.theta1, centers.theta0]
    } else {
        [centers.theta0, centers.theta1]
    };

    Detection {
        grid_directions,
        cell_size,
        target: TargetDetection {
            kind: TargetKind::Chessboard,
            corners: labeled_corners,
        },
        strong_indices,
    }
}

/// Canonicalize grid orientation so +i points roughly +x (right) and +j
/// points roughly +y (down) in image pixel coordinates. Preserves the
/// labelling up to axis permutation / sign flips and keeps `(i, j)`
/// non-negative with the bounding-box minimum at `(0, 0)`. Returns
/// `true` when the i- and j-axes were swapped — the caller may need to
/// swap any parallel axis-indexed data (e.g. `grid_directions`).
fn canonicalize_orientation(
    labelled_pairs: &mut [((i32, i32), usize)],
    corners: &[CornerAug],
) -> bool {
    if labelled_pairs.len() < 2 {
        return false;
    }

    use std::collections::HashMap;
    let pos_by_ij: HashMap<(i32, i32), (f32, f32)> = labelled_pairs
        .iter()
        .map(|&((i, j), idx)| ((i, j), (corners[idx].position.x, corners[idx].position.y)))
        .collect();

    // Mean +i and +j step vectors (in image pixels) over all adjacent
    // labelled pairs. Averaging across the full grid makes the direction
    // robust to individual corner noise and perspective distortion.
    let mut vi_sum = (0.0_f32, 0.0_f32);
    let mut vj_sum = (0.0_f32, 0.0_f32);
    let mut vi_n = 0u32;
    let mut vj_n = 0u32;
    for (&(i, j), &(x, y)) in pos_by_ij.iter() {
        if let Some(&(xn, yn)) = pos_by_ij.get(&(i + 1, j)) {
            vi_sum.0 += xn - x;
            vi_sum.1 += yn - y;
            vi_n += 1;
        }
        if let Some(&(xn, yn)) = pos_by_ij.get(&(i, j + 1)) {
            vj_sum.0 += xn - x;
            vj_sum.1 += yn - y;
            vj_n += 1;
        }
    }
    if vi_n == 0 || vj_n == 0 {
        return false;
    }
    let vi = (vi_sum.0 / vi_n as f32, vi_sum.1 / vi_n as f32);
    let vj = (vj_sum.0 / vj_n as f32, vj_sum.1 / vj_n as f32);

    // Decide which original axis should become the "horizontal" (i)
    // axis. Swap when the +j axis has a larger |x| component than +i.
    let swap = vi.0.abs() < vj.0.abs();
    let new_vi = if swap { vj } else { vi };
    let new_vj = if swap { vi } else { vj };
    let flip_i = new_vi.0 < 0.0;
    let flip_j = new_vj.1 < 0.0;

    if !swap && !flip_i && !flip_j {
        return false;
    }

    // Compute extents of the post-swap labelling before rewriting, so
    // the sign flip stays within the non-negative domain.
    let mut imax = i32::MIN;
    let mut jmax = i32::MIN;
    for &((i, j), _) in labelled_pairs.iter() {
        let (ni, nj) = if swap { (j, i) } else { (i, j) };
        imax = imax.max(ni);
        jmax = jmax.max(nj);
    }

    for ((i, j), _) in labelled_pairs.iter_mut() {
        let (mut ni, mut nj) = if swap { (*j, *i) } else { (*i, *j) };
        if flip_i {
            ni = imax - ni;
        }
        if flip_j {
            nj = jmax - nj;
        }
        *i = ni;
        *j = nj;
    }
    swap
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::{AxisEstimate, Corner};
    use nalgebra::Point2;

    fn make_corner(idx: usize, x: f32, y: f32, swapped: bool) -> Corner {
        let (a0, a1) = if swapped {
            (std::f32::consts::FRAC_PI_2, 0.0)
        } else {
            (0.0, std::f32::consts::FRAC_PI_2)
        };
        let _ = idx;
        Corner {
            position: Point2::new(x, y),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: a0,
                    sigma: 0.01,
                },
                AxisEstimate {
                    angle: a1,
                    sigma: 0.01,
                },
            ],
            contrast: 10.0,
            fit_rms: 1.0,
            strength: 1.0,
        }
    }

    #[test]
    fn end_to_end_clean_grid() {
        let s = 20.0_f32;
        let mut corners = Vec::new();
        let mut k = 0;
        for j in 0..7_i32 {
            for i in 0..7_i32 {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let swapped = (i + j).rem_euclid(2) == 1;
                corners.push(make_corner(k, x, y, swapped));
                k += 1;
            }
        }
        let det = Detector::new(DetectorParams::default());
        let d = det.detect(&corners).expect("detection");
        assert_eq!(d.target.corners.len(), 49);
    }

    #[test]
    fn rejects_when_too_few_corners() {
        let det = Detector::new(DetectorParams::default());
        assert!(det.detect(&[]).is_none());
    }

    #[test]
    fn grid_origin_at_visual_top_left() {
        // Synthesize a 7×7 grid where the +x image axis corresponds to
        // (1, 0) and +y to (0, 1). Regardless of which axis-slot the
        // clusterer picks, `build_detection` must canonicalize so
        // (0, 0) lands at the smallest (x, y) corner.
        let s = 20.0_f32;
        let mut corners = Vec::new();
        let mut k = 0;
        for j in 0..7_i32 {
            for i in 0..7_i32 {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let swapped = (i + j).rem_euclid(2) == 1;
                corners.push(make_corner(k, x, y, swapped));
                k += 1;
            }
        }
        let det = Detector::new(DetectorParams::default());
        let d = det.detect(&corners).expect("detection");
        // Locate (0, 0) and the two neighbors.
        let by_ij: std::collections::HashMap<(i32, i32), (f32, f32)> = d
            .target
            .corners
            .iter()
            .filter_map(|c| {
                let g = c.grid?;
                Some(((g.i, g.j), (c.position.x, c.position.y)))
            })
            .collect();
        let p00 = by_ij.get(&(0, 0)).copied().expect("(0,0) labelled");
        let p10 = by_ij.get(&(1, 0)).copied().expect("(1,0) labelled");
        let p01 = by_ij.get(&(0, 1)).copied().expect("(0,1) labelled");
        // (0, 0) must be the top-left in pixel coords.
        assert!(
            p00.0 <= p10.0 && p00.1 <= p01.1,
            "(0,0) at {:?} not top-left vs (1,0)={:?} (0,1)={:?}",
            p00,
            p10,
            p01
        );
        // +i step must move right (+x).
        assert!(p10.0 > p00.0, "+i axis not right-pointing");
        // +j step must move down (+y).
        assert!(p01.1 > p00.1, "+j axis not down-pointing");
    }

    #[test]
    fn instrumented_counts_match_clean_grid() {
        let s = 20.0_f32;
        let mut corners = Vec::new();
        let mut k = 0;
        for j in 0..7_i32 {
            for i in 0..7_i32 {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let swapped = (i + j).rem_euclid(2) == 1;
                corners.push(make_corner(k, x, y, swapped));
                k += 1;
            }
        }
        let det = Detector::new(DetectorParams::default());
        let res = det.detect_instrumented(&corners);
        assert!(res.detection.is_some(), "expected detection on 7x7 grid");
        assert_eq!(res.counts.input_corners, 49);
        assert_eq!(res.counts.after_strength_filter, 49);
        assert_eq!(res.counts.after_clustering, 49);
        assert!(res.counts.seed_found);
        assert_eq!(res.counts.labelled_final, 49);
        assert_eq!(res.counts.blacklisted_total, 0);
        assert!(res.counts.validation_iterations >= 1);
    }
}
