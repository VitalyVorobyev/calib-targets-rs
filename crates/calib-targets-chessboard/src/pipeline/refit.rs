//! Stage 9: post-grow cluster-centre refit + second extension pass.
//!
//! Recomputes the cluster centres from the labelled axes alone (no
//! marker contribution). If the resulting shift is large enough to
//! move a borderline corner across the cluster gate, the labelled set
//! is re-classified and a second `extend_boundary` / `rescue_no_cluster`
//! pass runs under the refined centres. See `CLAUDE.md` "Evidence-
//! driven detector debugging" for the case study that motivated this.

use std::collections::HashSet;

use crate::cluster::{
    angular_dist_pi, assign_corner, effective_tol_rad, fix_partial_slot_flips_post_stage6,
    refit_centers_from_labelled, AxisCluster, ClusterCenters,
};
use crate::corner::{CornerAug, CornerStage};
use crate::grow::{grow_from_seed, ChessboardSquareAttachPolicy, GrowResult};
use crate::params::DetectorParams;
use crate::seed::Seed;

use nalgebra::Point2;

use super::extension::{run_stage6, run_stage6_5_rescue};
use super::types::{BfsExtendTrace, ExtensionTrace, RefitTrace};

/// Per-pass output of [`run_refit`].
///
/// Bundles the four diagnostic traces the refit pass can produce plus
/// the (possibly updated) active cluster centres, so the orchestrator
/// can fold them into the iteration trace without a wide tuple.
pub(crate) struct RefitOutput {
    /// Centre-refit trace. `None` when the refit was disabled or too
    /// few labels were available to recompute the centres.
    pub refit: Option<RefitTrace>,
    /// Cardinal-neighbour BFS-extension trace from the second pass.
    pub bfs_extend: Option<BfsExtendTrace>,
    /// Second-pass `extend_boundary` trace.
    pub extension2: Option<ExtensionTrace>,
    /// Second-pass `rescue_no_cluster` trace.
    pub rescue2: Option<ExtensionTrace>,
    /// The cluster centres in effect after the refit. Equal to the
    /// input `active_centers` when the refit did not run a second pass.
    pub active_centers: ClusterCenters,
}

/// Stage 9: post-grow centre refit. Recompute the cluster centres from
/// the labelled axes alone, and — when the shift exceeds
/// `refit_min_shift_deg` — re-classify the `Strong` / `NoCluster`
/// corners and run a second extension + rescue pass.
///
/// Mutates `augs` and `grow_res` in place (the optional destructive
/// re-grow replaces `grow_res` wholesale). Returns the diagnostic
/// traces plus the cluster centres in effect afterwards.
pub(crate) fn run_refit(
    augs: &mut [CornerAug],
    grow_res: &mut GrowResult,
    active_centers: ClusterCenters,
    seed: Seed,
    cell_size: f32,
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
) -> RefitOutput {
    let mut out = RefitOutput {
        refit: None,
        bfs_extend: None,
        extension2: None,
        rescue2: None,
        active_centers,
    };
    let tuning = params.effective_tuning();
    if !tuning.enable_post_grow_refit {
        return out;
    }

    let labelled_indices: Vec<usize> = grow_res.labelled.values().copied().collect();
    let Some(new_centers) = refit_centers_from_labelled(
        augs,
        &labelled_indices,
        active_centers,
        tuning.refit_min_labelled,
    ) else {
        return out;
    };

    let shift_rad = angular_dist_pi(new_centers.theta0, active_centers.theta0)
        .max(angular_dist_pi(new_centers.theta1, active_centers.theta1));
    let trigger_rad = tuning.refit_min_shift_deg.to_radians();
    let mut promoted = 0u32;
    let second_pass_ran = shift_rad > trigger_rad;
    if second_pass_ran {
        // Re-classify Strong + NoCluster under the refined centres.
        // Already-Labeled corners keep their `(i, j)`; their `label`
        // slot is preserved from the original assignment (the slot
        // doesn't flip under a small centre shift).
        let base_tol = tuning.cluster_tol_deg.to_radians();
        let sigma_k = tuning.cluster_sigma_k;
        for aug in augs.iter_mut() {
            if !matches!(
                aug.stage,
                CornerStage::Strong | CornerStage::NoCluster { .. }
            ) {
                continue;
            }
            let tol = effective_tol_rad(aug, base_tol, sigma_k);
            match assign_corner(aug, new_centers, tol) {
                AxisCluster::Labeled { label, .. } => {
                    aug.label = Some(label);
                    aug.stage = CornerStage::Clustered { label };
                    promoted += 1;
                }
                AxisCluster::Unclustered { max_d_rad } => {
                    aug.stage = CornerStage::NoCluster {
                        max_d_deg: max_d_rad.to_degrees(),
                    };
                }
            }
        }
        // Optionally re-grow BFS from scratch first. The destructive
        // regrow lifts recall on cases where the existing labelled
        // set's bbox doesn't reach the orphans (small3.png left
        // strip, 1+ cells past the bbox edge — extend alone cannot
        // reach those without widening the search radius into the
        // SHIFT-INCONSISTENT regime). The trade-off is that
        // grow_from_seed under a small (~3°) centre shift can flip
        // borderline parity slots and lose some existing corners —
        // the cardinal extend below recovers them.
        if tuning.enable_post_grow_bfs_regrow {
            for aug in augs.iter_mut() {
                if blacklist.contains(&aug.input_index) {
                    continue;
                }
                if let CornerStage::Labeled { .. } = aug.stage {
                    if let Some(label) = aug.label {
                        aug.stage = CornerStage::Clustered { label };
                    }
                }
            }
            *grow_res = grow_from_seed(augs, seed, new_centers, cell_size, blacklist, params);
        }

        // Non-destructive cardinal-neighbour BFS extension: walks the
        // labelled bbox boundary one cell at a time using
        // cardinal-only prediction (K=1). Default ON. When the regrow
        // above dropped a few previously-labelled corners under the
        // new centres (small1.png / small4.png case), extend recovers
        // them — the dropped corners are typically one cell past the
        // regrown bbox boundary. Same tolerances as initial BFS
        // (wider radii produce SHIFT-INCONSISTENT labelling per the
        // small3.png precision verification).
        if tuning.enable_post_grow_bfs_extend {
            let positions: Vec<Point2<f32>> = augs.iter().map(|c| c.position).collect();
            // Stage 6.75 BFS extend runs in post-rebase coords; same
            // parity-shift rationale as `run_stage6`.
            let bfs_parity_shift = (grow_res.rebase_i_mod2 + grow_res.rebase_j_mod2).rem_euclid(2);
            let bfs_validator =
                ChessboardSquareAttachPolicy::new(augs, blacklist, new_centers, cell_size, params)
                    .with_parity_shift(bfs_parity_shift);
            let bfs_params = projective_grid::seed_and_grow::grow::GrowParams::new(
                tuning.attach_search_rel,
                tuning.attach_ambiguity_factor,
            );
            let bfs_stats = projective_grid::seed_and_grow::grow_extend::extend_from_labelled(
                &positions,
                grow_res,
                cell_size,
                &bfs_params,
                &bfs_validator,
            );
            for (k, &idx) in bfs_stats.attached_indices.iter().enumerate() {
                let at = bfs_stats.attached_cells[k];
                augs[idx].stage = CornerStage::Labeled {
                    at,
                    local_h_residual_px: None,
                };
            }
            out.bfs_extend = Some(BfsExtendTrace {
                attached: bfs_stats.attached,
                rejected_no_candidate: bfs_stats.rejected_no_candidate,
                rejected_ambiguous: bfs_stats.rejected_ambiguous,
                rejected_edge: bfs_stats.rejected_edge,
                attached_indices: bfs_stats.attached_indices.clone(),
            });
        }
        // Second-pass Stage 6 / 6.5 with the new centres so any cells
        // the BFS still missed get a second look at the wider local-H
        // prediction radius.
        let ext2 = run_stage6(augs, grow_res, new_centers, cell_size, blacklist, params);
        for (k, &idx) in ext2.attached_indices.iter().enumerate() {
            let at = ext2.attached_cells[k];
            augs[idx].stage = CornerStage::Labeled {
                at,
                local_h_residual_px: None,
            };
        }
        if ext2.h_quality.is_some() || ext2.h_residual_median_px.is_some() {
            out.extension2 = Some(ExtensionTrace::from(&ext2));
        }
        // Second-pass slot-flip fix mirrors the first pass: detect any
        // orphans whose slot ordering disagrees with the labelled
        // set's parity (using the refined centres + extended labelled
        // set), and flip them so the second-pass rescue can attach.
        if tuning.enable_partial_slot_flip_fix {
            let _flipped = fix_partial_slot_flips_post_stage6(
                augs,
                &grow_res.labelled,
                cell_size,
                tuning.partial_slot_flip_k_nearest,
            );
        }
        if tuning.enable_stage6_5_rescue {
            let rescue2 =
                run_stage6_5_rescue(augs, grow_res, new_centers, cell_size, blacklist, params);
            for (k, &idx) in rescue2.attached_indices.iter().enumerate() {
                let at = rescue2.attached_cells[k];
                augs[idx].stage = CornerStage::Labeled {
                    at,
                    local_h_residual_px: None,
                };
            }
            out.rescue2 = Some(ExtensionTrace::from(&rescue2));
        }
        out.active_centers = new_centers;
    }
    out.refit = Some(RefitTrace {
        shift_deg: shift_rad.to_degrees(),
        new_centers_deg: [
            new_centers.theta0.to_degrees(),
            new_centers.theta1.to_degrees(),
        ],
        labelled_used: labelled_indices.len(),
        promoted,
        second_pass_ran,
    });
    out
}
