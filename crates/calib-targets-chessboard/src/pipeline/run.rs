//! The detector orchestrator.
//!
//! Two entry points share one control-flow core:
//!
//! - [`run_pipeline_lean`] — the hot path. Runs the
//!   `find_seed → grow → validate` loop and every post-grow stage, then
//!   returns a lean [`PipelineOutcome`] (detection + cell size). It never
//!   assembles a [`DebugFrame`], never snapshots the per-corner state, and
//!   never builds the cluster histogram payload.
//! - [`run_pipeline`] — the diagnostics path (behind the `diagnostics`
//!   feature). Drives the *same* loop body via the shared helpers below
//!   and additionally accumulates a full [`DebugFrame`].
//!
//! The multi-iteration `find_seed → grow → validate → blacklist`
//! convergence loop itself lives in
//! [`projective_grid::seed_and_grow::pipeline`]; this module supplies the
//! chessboard-specific per-iteration geometry via [`ChessboardDriver`] and
//! replays the loop's per-iteration records onto the `CornerAug` stage
//! machine + [`DebugFrame`]. The converged-iteration post-grow stage
//! sequence lives in the shared [`run_converged_iteration`] helper so the
//! two entry points cannot diverge on which corners are admitted or
//! dropped — only on how much introspection they record.

use std::collections::HashSet;

use projective_grid::seed_and_grow::pipeline::{
    run_convergence_loop, IterationDriver, IterationOutcome, IterationProduct, LoopParams,
    LoopReport,
};

use crate::boosters::apply_boosters;
use crate::cluster::{cluster_axes, fix_partial_slot_flips, ClusterCenters};
use crate::corner::{CornerAug, CornerStage};
use crate::grow::{grow_from_seed, GrowResult};
use crate::params::DetectorParams;
use crate::seed::{find_seed, Seed, SeedOutput};
use crate::validate::{validate, ValidationResult};

use super::extension::{run_boundary_extension, run_no_cluster_rescue};
use super::geometry_check::run_geometry_check;
use super::output::build_detection;
use super::prefilter::{passes_fit_quality, passes_strength};
use super::refit::run_refit;
use super::types::{ChessboardDetection, PipelineOutcome};

#[cfg(feature = "diagnostics")]
use super::types::{
    BfsExtendTrace, DebugFrame, ExtensionTrace, GeometryCheckTrace, IterationTrace, RefitTrace,
    DEBUG_FRAME_SCHEMA,
};
#[cfg(feature = "diagnostics")]
use crate::boosters::BoosterResult;
#[cfg(feature = "diagnostics")]
use crate::cluster::cluster_axes_debug;

/// Stage 1: pre-filter. Advances every admissible corner `Raw → Strong`.
///
/// Corners already consumed by a previous `detect_all` iteration are left
/// at `Raw` (invisible to clustering, seed, grow, validation). Returns
/// `true` when at least `min_labeled_corners` corners reached `Strong`.
fn prefilter(augs: &mut [CornerAug], params: &DetectorParams, consumed: &HashSet<usize>) -> bool {
    for aug in augs.iter_mut() {
        if consumed.contains(&aug.input_index) {
            continue;
        }
        if passes_strength(aug, params) && passes_fit_quality(aug, params) {
            aug.stage = CornerStage::Strong;
        }
    }
    augs.iter()
        .filter(|a| matches!(a.stage, CornerStage::Strong))
        .count()
        >= params.min_labeled_corners
}

/// Chessboard driver for [`projective_grid`]'s convergence loop.
///
/// The generic loop in
/// [`projective_grid::seed_and_grow::pipeline`] owns the iteration counter,
/// the soft-convergence arithmetic, and the blacklist accumulation, and
/// reasons purely over feature **indices**. This driver supplies the
/// chessboard-specific per-iteration geometry — `find_seed → grow → validate`
/// composed with the chessboard's parity/cluster policy — and owns the
/// per-corner [`CornerAug`] stage machine the loop never touches.
///
/// # Stage mutation is load-bearing across iterations
///
/// `find_seed` selects seed candidates by reading
/// [`CornerStage::Clustered`], so the previous iteration's
/// `Labeled → LabeledThenBlacklisted` stage marks (for blacklisted corners)
/// and `Labeled → Clustered` resets (for surviving corners) must be applied
/// *before* the next `find_seed`. The driver therefore re-derives those
/// stage marks from the running blacklist at the start of every iteration
/// (idempotent), reproducing the historical end-of-iteration mark sequence
/// byte-for-byte. The terminal soft-convergence residual marks (which never
/// feed a subsequent `find_seed`) are applied by the post-loop replay.
///
/// The converged iteration's `seed` / `cell_size` / `grow_res` / validation
/// residuals are stashed so the post-loop handoff can drive
/// [`run_converged_iteration`] without re-running the geometry.
struct ChessboardDriver<'a> {
    augs: &'a mut [CornerAug],
    centers: ClusterCenters,
    params: &'a DetectorParams,
    /// Context of the most recent successful iteration, overwritten each
    /// pass. After the loop stops on a converged iteration this holds that
    /// iteration's context for the post-grow handoff.
    last: Option<IterationContext>,
}

/// The geometric context the driver stashes per iteration for the post-loop
/// converged handoff.
struct IterationContext {
    seed: Seed,
    cell_size: f32,
    grow_res: GrowResult,
    validation: ValidationResult,
}

impl<'a> ChessboardDriver<'a> {
    fn new(augs: &'a mut [CornerAug], centers: ClusterCenters, params: &'a DetectorParams) -> Self {
        Self {
            augs,
            centers,
            params,
            last: None,
        }
    }

    /// Re-derive the per-corner stage marks from the running blacklist:
    /// reset surviving `Labeled` corners to `Clustered` and mark blacklisted
    /// `Labeled` corners as `LabeledThenBlacklisted`. Reproduces the
    /// historical end-of-iteration mark sequence so `find_seed` sees an
    /// identical stage state.
    fn sync_stages_to_blacklist(&mut self, blacklist: &HashSet<usize>) {
        for aug in self.augs.iter_mut() {
            if blacklist.contains(&aug.input_index) {
                if let CornerStage::Labeled { at, .. } = aug.stage {
                    aug.stage = CornerStage::LabeledThenBlacklisted {
                        at,
                        reason: "post-validation outlier".into(),
                    };
                }
            } else if matches!(aug.stage, CornerStage::Labeled { .. }) {
                // Stage-3 → Stage-5 invariant: every Labeled corner carries
                // its cluster label. If it's somehow missing, leave the
                // stage as-is rather than panicking — the next iteration's
                // checks will re-handle this corner.
                if let Some(label) = aug.label {
                    aug.stage = CornerStage::Clustered { label };
                }
            }
        }
    }
}

impl IterationDriver for ChessboardDriver<'_> {
    fn run_iteration(&mut self, blacklist: &HashSet<usize>, _it: u32) -> IterationProduct {
        // Reset / mark stages from the running blacklist before seeding.
        self.sync_stages_to_blacklist(blacklist);

        let seed_out: SeedOutput = match find_seed(self.augs, self.centers, self.params) {
            Some(s) => s,
            None => return IterationProduct::seed_failed(),
        };
        let seed = seed_out.seed;
        let cell_size = seed_out.cell_size;

        let grow_res: GrowResult = grow_from_seed(
            self.augs,
            seed,
            self.centers,
            cell_size,
            blacklist,
            self.params,
        );

        let validation: ValidationResult =
            validate(self.augs, &grow_res.labelled, cell_size, self.params);

        let product = IterationProduct {
            seed_found: true,
            labelled: grow_res.labelled.clone(),
            validation_blacklist: validation.blacklist.iter().copied().collect(),
            cell_size: Some(cell_size),
            seed_indices: Some([seed.a, seed.b, seed.c, seed.d]),
        };

        self.last = Some(IterationContext {
            seed,
            cell_size,
            grow_res,
            validation,
        });

        product
    }
}

/// Hot-path entry point: run the full chessboard pipeline for a single
/// component and return a lean [`PipelineOutcome`] (detection + cell size).
///
/// Builds no [`DebugFrame`] and no per-stage traces — this is the path
/// [`crate::Detector::detect`] / [`crate::Detector::detect_all`] use.
///
/// `consumed` carries the corner indices already claimed by an earlier
/// `detect_all` component; those corners are left at `Raw` so they are
/// invisible to every stage.
pub(crate) fn run_pipeline_lean(
    params: &DetectorParams,
    corners: &[crate::corner::ChessCorner],
    consumed: &HashSet<usize>,
) -> PipelineOutcome {
    let mut augs: Vec<CornerAug> = corners
        .iter()
        .enumerate()
        .map(|(i, c)| CornerAug::from_chess_corner(i, c))
        .collect();

    if !prefilter(&mut augs, params, consumed) {
        return PipelineOutcome { detection: None };
    }

    // Stage 2 + 3: clustering (lean — no histogram payload).
    let Some(centers) = cluster_axes(&mut augs, params) else {
        return PipelineOutcome { detection: None };
    };

    let mut driver = ChessboardDriver::new(&mut augs, centers, params);
    let report = run_convergence_loop(&mut driver, loop_params(params));

    let detection = replay_converged(&mut driver, &report).and_then(|out| out.detection);

    PipelineOutcome { detection }
}

/// Build the generic loop parameters from the chessboard tuning. Mirrors the
/// historical soft-convergence constants (`it + 1 >= 2`, residual `≤ 2`,
/// labelled `>= min_labeled_corners`) so the loop decisions are byte-exact.
fn loop_params(params: &DetectorParams) -> LoopParams {
    LoopParams::new(
        params.effective_tuning().max_validation_iters,
        2,
        2,
        params.min_labeled_corners,
    )
}

/// Total blacklist accumulated across the loop — the union of every
/// iteration's `new_blacklist` delta, which equals the loop's internal
/// running blacklist at termination. Rebuilt here because the generic loop
/// owns the set internally and reports only the per-iteration deltas.
fn accumulated_blacklist(report: &LoopReport) -> HashSet<usize> {
    let mut bl = HashSet::new();
    for rec in &report.iterations {
        for &idx in &rec.new_blacklist {
            bl.insert(idx);
        }
    }
    bl
}

/// Replay the converged iteration's record onto the driver's stashed
/// geometry and run the full post-grow stage sequence.
///
/// Returns `None` when the loop did not converge (seed failure or
/// iteration-cap exhaustion). On a *soft* convergence, applies the record's
/// residual blacklist (stage marks + labelled-set strip) before the
/// post-grow stages, exactly as the historical inline loop did.
fn replay_converged(
    driver: &mut ChessboardDriver<'_>,
    report: &LoopReport,
) -> Option<ConvergedOutput> {
    let conv = report.converged_record()?;
    let soft = matches!(conv.outcome, IterationOutcome::Converged { soft: true });

    // The driver stashed the converged iteration's geometry on its last
    // `run_iteration` (the loop stops immediately after).
    let ctx = driver.last.take()?;
    let IterationContext {
        seed,
        cell_size,
        mut grow_res,
        validation,
    } = ctx;

    let mut blacklist = accumulated_blacklist(report);
    let augs = &mut *driver.augs;

    if soft {
        // Apply the residual blacklist before accepting so the emitted
        // detection excludes the flagged outliers. The generic loop already
        // stripped them from `conv.labelled`; mirror that onto `grow_res`
        // and the per-corner stage machine here.
        for &idx in &conv.new_blacklist {
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

    Some(run_converged_iteration(ConvergedCtx {
        params: driver.params,
        it: conv.iter,
        centers: driver.centers,
        seed,
        cell_size,
        labelled_count: conv.labelled_count,
        new_blacklist: conv.new_blacklist.clone(),
        augs,
        grow_res,
        blacklist: &mut blacklist,
        local_h_residuals: &validation.local_h_residuals,
    }))
}

/// Diagnostics entry point: run the full pipeline for a single component
/// and return a [`DebugFrame`] — the detection plus every per-stage trace.
///
/// Drives the identical loop body as [`run_pipeline_lean`] (so the emitted
/// detection is byte-identical) and additionally accumulates the
/// introspection payload. The `detection` field is `None` when no
/// component was recovered.
///
/// Most callers use [`crate::Detector`] rather than this directly; it is
/// exposed for tooling that needs to drive a single pipeline pass with an
/// explicit `consumed` set.
#[cfg(feature = "diagnostics")]
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

    if !prefilter(&mut augs, params, consumed) {
        frame.corners = augs;
        return frame;
    }

    // Stage 2 + 3: clustering (with histogram payload for triage).
    let (centers_opt, cluster_debug) = cluster_axes_debug(&mut augs, params);
    frame.cluster_debug = Some(cluster_debug);
    let Some(centers) = centers_opt else {
        frame.corners = augs;
        return frame;
    };
    frame.grid_directions = Some([centers.theta0, centers.theta1]);

    let mut driver = ChessboardDriver::new(&mut augs, centers, params);
    let report = run_convergence_loop(&mut driver, loop_params(params));

    // Most-recent seed cell-size / indices, mirroring the historical
    // per-iteration `LoopState` snapshot (the last record that carried a
    // seed wins).
    if let Some(rec) = report
        .iterations
        .iter()
        .rev()
        .find(|r| r.seed_indices.is_some())
    {
        frame.cell_size = rec.cell_size;
        frame.seed = rec.seed_indices;
    }

    // Replay the converged iteration's post-grow stages, if any. The driver
    // borrow ends here so `augs` is free again below.
    let mut converged_output = replay_converged(&mut driver, &report);

    // Emit one IterationTrace per recorded iteration. Non-converged and
    // seed-failed iterations carry no post-grow stage traces; the converged
    // iteration carries the full post-grow payload from the replay.
    for rec in &report.iterations {
        match rec.outcome {
            IterationOutcome::SeedFailed => break,
            IterationOutcome::NotConverged => {
                frame.iterations.push(IterationTrace {
                    iter: rec.iter,
                    labelled_count: rec.labelled_count,
                    new_blacklist: rec.new_blacklist.clone(),
                    converged: false,
                    extension: None,
                    rescue: None,
                    refit: None,
                    bfs_extend: None,
                    extension2: None,
                    rescue2: None,
                    geometry_check: None,
                });
            }
            IterationOutcome::Converged { .. } => {
                let Some(output) = converged_output.take() else {
                    break;
                };
                let ConvergedOutput {
                    detection,
                    boosters,
                    iter,
                    labelled_count,
                    new_blacklist,
                    extension,
                    rescue,
                    refit,
                    bfs_extend,
                    extension2,
                    rescue2,
                    geometry_check,
                } = output;
                frame.boosters = Some(boosters);
                frame.iterations.push(IterationTrace {
                    iter,
                    labelled_count,
                    new_blacklist,
                    converged: true,
                    extension,
                    rescue,
                    refit,
                    bfs_extend,
                    extension2,
                    rescue2,
                    geometry_check: Some(geometry_check),
                });
                frame.detection = detection;
                break;
            }
            _ => break,
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
    centers: ClusterCenters,
    seed: Seed,
    cell_size: f32,
    labelled_count: usize,
    new_blacklist: Vec<usize>,
    augs: &'a mut [CornerAug],
    grow_res: GrowResult,
    blacklist: &'a mut HashSet<usize>,
    local_h_residuals: &'a std::collections::HashMap<usize, f32>,
}

/// Output of [`run_converged_iteration`].
///
/// `detection` is the only field the hot path consumes. The per-stage
/// trace components (behind the `diagnostics` feature) are built only when
/// the diagnostics path drove the loop, so the lean path never pays for
/// them. Keeping a single converged-iteration body guarantees the two
/// paths cannot diverge on which corners survive — they differ only in
/// whether the traces are accumulated.
struct ConvergedOutput {
    detection: Option<ChessboardDetection>,
    #[cfg(feature = "diagnostics")]
    boosters: BoosterResult,
    #[cfg(feature = "diagnostics")]
    iter: u32,
    #[cfg(feature = "diagnostics")]
    labelled_count: usize,
    #[cfg(feature = "diagnostics")]
    new_blacklist: Vec<usize>,
    #[cfg(feature = "diagnostics")]
    extension: Option<ExtensionTrace>,
    #[cfg(feature = "diagnostics")]
    rescue: Option<ExtensionTrace>,
    #[cfg(feature = "diagnostics")]
    refit: Option<RefitTrace>,
    #[cfg(feature = "diagnostics")]
    bfs_extend: Option<BfsExtendTrace>,
    #[cfg(feature = "diagnostics")]
    extension2: Option<ExtensionTrace>,
    #[cfg(feature = "diagnostics")]
    rescue2: Option<ExtensionTrace>,
    #[cfg(feature = "diagnostics")]
    geometry_check: GeometryCheckTrace,
}

/// Run every post-grow stage on the converged + validated labelled set.
///
/// Stage order (matches the crate-level pipeline enumeration):
/// `extend_boundary` → `fix_partial_slot_flip` → `rescue_no_cluster`
/// → `refit` → `apply_boosters` → `final_geometry_check` →
/// post-geometry rescue. Every stage body lives in its own sibling
/// module; this function only sequences them and folds their
/// diagnostics into one [`ConvergedOutput`].
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
        local_h_residuals,
    } = ctx;

    // `it` / `labelled_count` / `new_blacklist` are recorded only in the
    // diagnostics trace; on the lean path they are carried purely so the
    // converged-iteration body has one signature. Mark them used.
    #[cfg(not(feature = "diagnostics"))]
    let _ = (it, labelled_count, &new_blacklist);

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
    let extension_stats = run_boundary_extension(
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
        let _flipped = fix_partial_slot_flips(
            augs,
            &grow_res.labelled,
            cell_size,
            tuning.partial_slot_flip_k_nearest,
        );
    }

    // NoCluster rescue: re-considers `Strong` / `NoCluster`
    // corners as candidates using the same local-H prediction
    // machinery as boundary extension, with a wider axis-tolerance
    // (`rescue_axis_tol_deg`) and inferred parity. Position match +
    // parity match + axis-slot-swap edge invariant keep precision.
    #[cfg(feature = "diagnostics")]
    let mut iteration_rescue: Option<ExtensionTrace> = None;
    if tuning.enable_no_cluster_rescue {
        let rescue_stats = run_no_cluster_rescue(
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
        #[cfg(feature = "diagnostics")]
        {
            iteration_rescue = Some(ExtensionTrace::from(&rescue_stats));
        }
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
    #[cfg(feature = "diagnostics")]
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
    // behind. Same attachment invariants as growth. The call's side
    // effects (labelled-set mutation) are load-bearing on both paths;
    // only the returned summary is diagnostic.
    #[cfg(feature = "diagnostics")]
    let booster: BoosterResult = apply_boosters(
        augs,
        &mut grow_mut,
        active_centers,
        cell_size,
        blacklist,
        params,
    );
    #[cfg(not(feature = "diagnostics"))]
    let _ = apply_boosters(
        augs,
        &mut grow_mut,
        active_centers,
        cell_size,
        blacklist,
        params,
    );

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
        let rescue_post = run_no_cluster_rescue(
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

    let final_count = grow_mut.labelled.len();
    let detection =
        if !geometry_check_trace.detection_refused && final_count >= params.min_labeled_corners {
            Some(build_detection(augs, &grow_mut, cell_size))
        } else {
            None
        };

    ConvergedOutput {
        detection,
        #[cfg(feature = "diagnostics")]
        boosters: booster,
        #[cfg(feature = "diagnostics")]
        iter: it,
        #[cfg(feature = "diagnostics")]
        labelled_count,
        #[cfg(feature = "diagnostics")]
        new_blacklist,
        #[cfg(feature = "diagnostics")]
        extension: iteration_extension,
        #[cfg(feature = "diagnostics")]
        rescue: iteration_rescue,
        #[cfg(feature = "diagnostics")]
        refit: refit_out.refit,
        #[cfg(feature = "diagnostics")]
        bfs_extend: refit_out.bfs_extend,
        #[cfg(feature = "diagnostics")]
        extension2: refit_out.extension2,
        #[cfg(feature = "diagnostics")]
        rescue2: refit_out.rescue2,
        #[cfg(feature = "diagnostics")]
        geometry_check: geometry_check_trace,
    }
}
