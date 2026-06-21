//! Boundary extension + NoCluster rescue stages.
//!
//! Both stages extrapolate the labelled set outward via a fitted
//! homography, reusing the pattern-agnostic extension primitives from
//! `projective-grid`. The chess-specific logic lives in the policies
//! they pass through (`ChessboardSquareAttachPolicy` /
//! `ChessboardRescueValidator`): parity, axis-cluster match, and the
//! axis-slot-swap edge invariant.

use std::collections::HashSet;

use crate::cluster::ClusterCenters;
use crate::corner::CornerAug;
use crate::grow::{ChessboardRescueValidator, ChessboardSquareAttachPolicy, GrowResult};
use crate::params::DetectorParams;

use nalgebra::Point2;
use projective_grid::seed_and_grow::extension::{
    extend_via_global_homography, extend_via_local_homography, ExtensionParams, ExtensionStats,
    LocalExtensionParams,
};

/// Boundary extension stage: extrapolation via locally-fit homography.
///
/// Builds a `Point2<f32>` view of the corner positions and a fresh
/// chessboard validator, then delegates to
/// [`projective_grid::seed_and_grow::extension::extend_via_global_homography`]
/// (when `boundary_extension_local_h` is `false`) or
/// [`projective_grid::seed_and_grow::extension::extend_via_local_homography`]
/// (default). The extension's blacklist tracking is approach (b): rejected
/// attachments fall through to the regular Stage-7 mechanism on the
/// next iteration. Stats include `attached_indices` for future
/// approach-(a) comparison work.
pub(crate) fn run_boundary_extension(
    corners: &[CornerAug],
    grow_res: &mut GrowResult,
    centers: ClusterCenters,
    cell_size: f32,
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
) -> ExtensionStats {
    let positions: Vec<Point2<f32>> = corners.iter().map(|c| c.position).collect();

    // Boundary extension runs in post-rebase coords, so the validator's
    // `required_label_at(i, j)` must add the rebase parity shift back
    // to query the chessboard parity that BFS used in pre-rebase
    // coords. See `GrowResult::rebase_i_mod2` for the full discussion.
    let parity_shift = (grow_res.rebase_i_mod2 + grow_res.rebase_j_mod2).rem_euclid(2);
    let validator =
        ChessboardSquareAttachPolicy::new(corners, blacklist, centers, cell_size, params)
            .with_parity_shift(parity_shift);
    let tuning = params.effective_tuning();
    if tuning.boundary_extension_local_h {
        let mut local_params = LocalExtensionParams::default();
        local_params.k_nearest = tuning.boundary_extension_k_nearest;
        extend_via_local_homography(&positions, grow_res, cell_size, &local_params, &validator)
    } else {
        extend_via_global_homography(
            &positions,
            grow_res,
            cell_size,
            &ExtensionParams::default(),
            &validator,
        )
    }
}

/// NoCluster rescue stage. Reuses
/// [`projective_grid::seed_and_grow::extension::extend_via_local_homography`]
/// with [`ChessboardRescueValidator`] (admits `Strong` / `NoCluster`
/// corners within `rescue_axis_tol_deg` and infers parity from axes).
/// Same per-cell local-H prediction + position match + ambiguity
/// gate + edge invariant as boundary extension — only the eligibility / label
/// gates are relaxed.
pub(crate) fn run_no_cluster_rescue(
    corners: &[CornerAug],
    grow_res: &mut GrowResult,
    centers: ClusterCenters,
    cell_size: f32,
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
) -> ExtensionStats {
    let positions: Vec<Point2<f32>> = corners.iter().map(|c| c.position).collect();

    // No-cluster rescue runs in post-rebase coords; the rescue validator's
    // `required_label_at(i, j)` adds the rebase parity shift back to
    // recover the BFS pre-rebase chessboard parity at the post-rebase
    // cell. See `GrowResult::rebase_i_mod2`.
    let parity_shift = (grow_res.rebase_i_mod2 + grow_res.rebase_j_mod2).rem_euclid(2);
    let validator = ChessboardRescueValidator::new(corners, blacklist, centers, cell_size, params)
        .with_parity_shift(parity_shift);
    let tuning = params.effective_tuning();
    let mut local_params = LocalExtensionParams::default();
    local_params.k_nearest = tuning.no_cluster_rescue_k_nearest;
    local_params.common.search_rel = tuning.rescue_search_rel;
    extend_via_local_homography(&positions, grow_res, cell_size, &local_params, &validator)
}
