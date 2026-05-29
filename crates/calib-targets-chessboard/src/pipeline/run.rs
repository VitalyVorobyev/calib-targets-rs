//! The detector orchestrator.
//!
//! [`run_pipeline`] is the thin driver over the named stage modules in
//! [`super`]: it owns the `find_seed → grow → validate` loop and, on
//! the converged iteration, calls each post-grow stage in order
//! (`extend_boundary` → `fix_partial_slot_flip` → `rescue_no_cluster`
//! → `refit` → `apply_boosters` → `final_geometry_check`). It carries
//! no stage logic itself — every stage body lives in its own sibling
//! module.

use std::collections::HashSet;

use crate::boosters::{apply_boosters, BoosterResult};
use crate::cluster::{cluster_axes_debug, fix_partial_slot_flips_post_stage6};
use crate::corner::{CornerAug, CornerStage};
use crate::grow::{grow_from_seed, GrowResult};
use crate::params::DetectorParams;
use crate::seed::{find_seed, SeedOutput};
use crate::validate::{validate, ValidationResult};

use super::extension::{run_stage6, run_stage6_5_rescue};
use super::geometry_check::run_geometry_check;
use super::output::build_detection;
use super::prefilter::{passes_fit_quality, passes_strength};
use super::refit::run_refit;
use super::types::{DebugFrame, ExtensionTrace, IterationTrace, DEBUG_FRAME_SCHEMA};

/// Run the full chessboard pipeline for a single component.
///
/// `consumed` carries the corner indices already claimed by an earlier
/// `detect_all` component; those corners are left at `Raw` so they are
/// invisible to every stage. Always returns a [`DebugFrame`] — the
/// `detection` field is `None` when no component was recovered.
///
/// Most callers use [`crate::Detector`] rather than this directly;
/// it is exposed for tooling that needs to drive a single pipeline
/// pass with an explicit `consumed` set.
pub fn run_pipeline(
    params: &DetectorParams,
    corners: &[crate::corner::ChessCorner],
    consumed: &HashSet<usize>,
) -> DebugFrame {
    let input_count = corners.len();
    let mut augs: Vec<CornerAug> = corners
        .iter()
        .enumerate()
        .map(|(i, c)| CornerAug::from_chess_corner(i, c))
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
        cluster_debug: None,
    };

    // Stage 1: pre-filter.
    // Corners already consumed by a previous `detect_all` iteration are
    // left at `Raw` stage, which makes them invisible to clustering,
    // seed search, grow, and validation.
    for aug in augs.iter_mut() {
        if consumed.contains(&aug.input_index) {
            continue;
        }
        if passes_strength(aug, params) && passes_fit_quality(aug, params) {
            aug.stage = CornerStage::Strong;
        }
    }
    if augs
        .iter()
        .filter(|a| matches!(a.stage, CornerStage::Strong))
        .count()
        < params.min_labeled_corners
    {
        frame.corners = augs;
        return frame;
    }

    // Stage 2 + 3: clustering.
    let (centers_opt, cluster_debug) = cluster_axes_debug(&mut augs, params);
    frame.cluster_debug = Some(cluster_debug);
    let centers = match centers_opt {
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
    let max_iters = params.effective_tuning().max_validation_iters.max(1);

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

        let seed_out: SeedOutput = match find_seed(&augs, centers, params) {
            Some(s) => s,
            None => break,
        };
        let seed = seed_out.seed;
        let cell_size = seed_out.cell_size;
        frame.cell_size = Some(cell_size);
        frame.seed = Some([seed.a, seed.b, seed.c, seed.d]);

        let mut grow_res: GrowResult =
            grow_from_seed(&mut augs, seed, centers, cell_size, &blacklist, params);

        let labelled_count = grow_res.labelled.len();

        let v: ValidationResult = validate(&augs, &grow_res.labelled, cell_size, params);
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
            && labelled_count >= params.min_labeled_corners;

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

            let iteration = run_converged_iteration(ConvergedCtx {
                params,
                it,
                centers,
                seed,
                cell_size,
                labelled_count,
                new_blacklist,
                augs: &mut augs,
                grow_res,
                blacklist: &mut blacklist,
                frame_boosters: &mut frame.boosters,
                local_h_residuals: &v.local_h_residuals,
            });
            frame.iterations.push(iteration.trace);
            if let Some(detection) = iteration.detection {
                frame.detection = Some(detection);
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
            rescue: None,
            refit: None,
            bfs_extend: None,
            extension2: None,
            rescue2: None,
            geometry_check: None,
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

/// Inputs to [`run_converged_iteration`]. Bundled into a struct so the
/// orchestrator does not exceed the workspace `too_many_arguments`
/// limit and so the post-grow stage sequence reads as one call.
struct ConvergedCtx<'a> {
    params: &'a DetectorParams,
    it: u32,
    centers: crate::cluster::ClusterCenters,
    seed: crate::seed::Seed,
    cell_size: f32,
    labelled_count: usize,
    new_blacklist: Vec<usize>,
    augs: &'a mut [CornerAug],
    grow_res: GrowResult,
    blacklist: &'a mut HashSet<usize>,
    frame_boosters: &'a mut Option<BoosterResult>,
    local_h_residuals: &'a std::collections::HashMap<usize, f32>,
}

/// Output of [`run_converged_iteration`]: the iteration trace plus the
/// detection (when one was emitted).
struct ConvergedOutput {
    trace: IterationTrace,
    detection: Option<super::types::ChessboardDetection>,
}

/// Run every post-grow stage on the converged + validated labelled set.
///
/// Stage order (matches the crate-level pipeline enumeration):
/// `extend_boundary` → `fix_partial_slot_flip` → `rescue_no_cluster`
/// → `refit` → `apply_boosters` → `final_geometry_check` →
/// post-geometry rescue. Every stage body lives in its own sibling
/// module; this function only sequences them and folds their
/// diagnostics into one [`IterationTrace`].
fn run_converged_iteration(ctx: ConvergedCtx<'_>) -> ConvergedOutput {
    let ConvergedCtx {
        params,
        it,
        centers,
        seed,
        cell_size,
        labelled_count,
        new_blacklist,
        augs,
        mut grow_res,
        blacklist,
        frame_boosters,
        local_h_residuals,
    } = ctx;

    // `active_centers` is the cluster pair currently in effect for
    // downstream stages. It starts at the Stage-3 estimate; the
    // Stage-6.75 refit may replace it with a labelled-set-only estimate
    // that's unbiased by marker corners.
    let mut active_centers = centers;

    // Bind the advanced tuning once; `None` yields the defaults.
    let tuning = params.effective_tuning();

    // Stage 6: boundary extrapolation via globally-fit homography.
    // Runs on the **converged + validated** labelled set so the H fit
    // isn't pulled by mid-loop candidates that the validator would
    // later reject. Same parity / axis-cluster / edge invariants as
    // BFS, plus a tighter ambiguity gate. Blacklist scope: we
    // re-validate immediately after Stage 6 and drop any extension
    // attachments the validator rejects, but DON'T re-run the BFS /
    // re-fit H.
    let extension_stats = run_stage6(
        augs,
        &mut grow_res,
        active_centers,
        cell_size,
        blacklist,
        params,
    );
    for (k, &idx) in extension_stats.attached_indices.iter().enumerate() {
        let at = extension_stats.attached_cells[k];
        augs[idx].stage = CornerStage::Labeled {
            at,
            local_h_residual_px: None,
        };
    }
    if extension_stats.attached > 0 {
        // Re-validate on the extended set. Any rejection that targets
        // an extension attachment is dropped via approach (b);
        // rejections that target BFS labels are also applied (the H
        // fit may have surfaced a borderline corner the inner loop
        // missed).
        let v_post = validate(augs, &grow_res.labelled, cell_size, params);
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

    // Stage 6.25: post-grow partial slot-flip fix.
    // chess-corners 0.9 DiskFit can pick the opposite antipodal dark
    // sector for ~1–8% of clean clustered corners on real photos,
    // leaving them with a (axes[0], axes[1]) ordering that disagrees
    // with the rest of the chessboard. Stage 6 BFS rejects these
    // because every cardinal edge fails the alternating-parity rule.
    // Detect them now (using the labelled set as parity ground-truth)
    // and swap their slots so Stage 6.5 can attach them via the
    // standard rescue path. RingFit is unaffected — its slot orderings
    // are consistent by construction.
    if tuning.enable_partial_slot_flip_fix {
        let _flipped = fix_partial_slot_flips_post_stage6(
            augs,
            &grow_res.labelled,
            cell_size,
            tuning.partial_slot_flip_k_nearest,
        );
    }

    // Stage 6.5: NoCluster rescue. Re-considers `Strong` / `NoCluster`
    // corners as candidates using the same local-H prediction
    // machinery as Stage 6, with a wider axis-tolerance
    // (`rescue_axis_tol_deg`) and inferred parity. Position match +
    // parity match + axis-slot-swap edge invariant keep precision.
    let mut iteration_rescue: Option<ExtensionTrace> = None;
    if tuning.enable_stage6_5_rescue {
        let rescue_stats = run_stage6_5_rescue(
            augs,
            &mut grow_res,
            active_centers,
            cell_size,
            blacklist,
            params,
        );
        for (k, &idx) in rescue_stats.attached_indices.iter().enumerate() {
            let at = rescue_stats.attached_cells[k];
            augs[idx].stage = CornerStage::Labeled {
                at,
                local_h_residual_px: None,
            };
        }
        iteration_rescue = Some(ExtensionTrace::from(&rescue_stats));
        // No post-rescue revalidation: the rescue's per-candidate
        // gates (position match against local-H, parity match against
        // the cluster centers, axis-slot-swap edge invariant,
        // ambiguity gate) already enforce precision on every addition.
        // Re-running line-collinearity / local-H residual after rescue
        // empirically over-flags borderline corners that the booster
        // would have admitted, costing more recall than the rescue
        // gains.
    }
    // Stage 6 ran if it produced residual stats (either global-H,
    // which sets `h_quality`, or local-H, which sets
    // `h_residual_median_px` per-candidate aggregate).
    let iteration_extension =
        if extension_stats.h_quality.is_some() || extension_stats.h_residual_median_px.is_some() {
            Some(ExtensionTrace::from(&extension_stats))
        } else {
            None
        };

    // Stage 6.75: post-grow centre refit. Recompute the cluster
    // centres from the labelled axes alone (no marker contribution),
    // and if the shift is large enough to move a borderline corner
    // across the gate, re-classify Strong/NoCluster corners and run
    // Stage 6 / 6.5 again. See CLAUDE.md "Evidence-driven detector
    // debugging" for the small3.png case study.
    let refit_out = run_refit(
        augs,
        &mut grow_res,
        active_centers,
        seed,
        cell_size,
        blacklist,
        params,
    );
    active_centers = refit_out.active_centers;

    let mut grow_mut = grow_res;

    // Recall boosters: interior gap fill + line extrapolation. Runs
    // after Stage 6 so it can fill any holes the global-H pass left
    // behind. Same attachment invariants as growth.
    let booster: BoosterResult = apply_boosters(
        augs,
        &mut grow_mut,
        active_centers,
        cell_size,
        blacklist,
        params,
    );
    *frame_boosters = Some(booster);

    // Write local-H residuals onto labelled corners.
    for (&c_idx, &resid) in local_h_residuals {
        if let CornerStage::Labeled { at, .. } = augs[c_idx].stage {
            augs[c_idx].stage = CornerStage::Labeled {
                at,
                local_h_residual_px: Some(resid),
            };
        }
    }

    // Mandatory final geometry check. Drops any labelled corner that
    // fails the shared square-grid validation, including the final
    // local edge-shape gate. Refuses the detection entirely if the
    // surviving labelled count drops below `min_labeled_corners`.
    let mut geometry_check_trace = run_geometry_check(
        augs,
        &mut grow_mut,
        active_centers,
        cell_size,
        blacklist,
        params,
    );

    // Stage 6.5b: post-geometry-check rescue. Re-run the rescue once
    // on the surviving labelled set so cells freed by the geometry
    // check (where mis-attached corners were dropped) can be re-filled
    // by orphans the rescue couldn't reach before because those cells
    // were occupied. The rescue's per-candidate gates (position match,
    // parity match, edge invariant) are unchanged. The geometry check
    // is re-run once after rescue; otherwise a post-geometry attachment
    // can bypass the mandatory final precision gate.
    //
    // Targets the chess-corners 0.9 DiskFit case where BFS
    // mis-attaches a partial-slot-flip orphan to the wrong cell,
    // blocking the right orphan; only after geometry check drops the
    // wrong attachment does the right orphan have a chance.
    if tuning.enable_post_geometry_rescue && !geometry_check_trace.detection_refused {
        let rescue_post = run_stage6_5_rescue(
            augs,
            &mut grow_mut,
            active_centers,
            cell_size,
            blacklist,
            params,
        );
        for (k, &idx) in rescue_post.attached_indices.iter().enumerate() {
            let at = rescue_post.attached_cells[k];
            augs[idx].stage = CornerStage::Labeled {
                at,
                local_h_residual_px: None,
            };
        }
        let rescue_geometry_trace = run_geometry_check(
            augs,
            &mut grow_mut,
            active_centers,
            cell_size,
            blacklist,
            params,
        );
        geometry_check_trace.dropped += rescue_geometry_trace.dropped;
        geometry_check_trace.dropped_line_collinearity +=
            rescue_geometry_trace.dropped_line_collinearity;
        geometry_check_trace.dropped_local_h_residual +=
            rescue_geometry_trace.dropped_local_h_residual;
        geometry_check_trace.dropped_edge_invariant += rescue_geometry_trace.dropped_edge_invariant;
        geometry_check_trace.dropped_disconnected += rescue_geometry_trace.dropped_disconnected;
        geometry_check_trace.components_seen = rescue_geometry_trace.components_seen;
        geometry_check_trace.detection_refused = rescue_geometry_trace.detection_refused;
    }

    let trace = IterationTrace {
        iter: it,
        labelled_count,
        new_blacklist,
        converged: true,
        extension: iteration_extension,
        rescue: iteration_rescue,
        refit: refit_out.refit,
        bfs_extend: refit_out.bfs_extend,
        extension2: refit_out.extension2,
        rescue2: refit_out.rescue2,
        geometry_check: Some(geometry_check_trace.clone()),
    };
    let final_count = grow_mut.labelled.len();
    let detection =
        if !geometry_check_trace.detection_refused && final_count >= params.min_labeled_corners {
            Some(build_detection(augs, &grow_mut))
        } else {
            None
        };

    ConvergedOutput { trace, detection }
}
