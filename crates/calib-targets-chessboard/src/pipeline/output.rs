//! Final output: convert a labelled grid into a [`Detection`].
//!
//! Stage 13 — `(i, j) → corner_idx` map to a
//! [`TargetKind::Chessboard`] [`TargetDetection`].
//!
//! # NOTE — bespoke cleanup vs. `projective_grid::square::cleanup`
//!
//! Phase 1 added generic output-cleanup helpers (`rebase_to_origin`,
//! `canonicalize_top_left`, `sorted_grid_points`). This module keeps
//! its own `build_detection` / `canonicalize_orientation` pair rather
//! than swapping to those helpers, because the chessboard variant
//! differs in two behaviour-relevant ways:
//!
//! 1. It operates on an in-place `Vec<((i32, i32), usize)>` and
//!    interleaves the rebase, swap, and sign-flip steps so the result
//!    of `canonicalize_orientation` feeds directly into the stable
//!    `(j, i)` sort below — the generic helpers take/return a
//!    `HashMap` and would force two extra round-trips.
//! 2. `canonicalize_orientation` returns a `bool` (`swap`) so the
//!    caller can swap the parallel `grid_directions` angle pair. The
//!    generic [`canonicalize_top_left`](projective_grid::square::cleanup::canonicalize_top_left)
//!    returns a `GridTransform`; deriving the axis-swap flag from it
//!    would be an extra inference step with its own edge cases.
//!
//! The rebase here is the same `min (i, j) → (0, 0)` shift as
//! [`rebase_to_origin`](projective_grid::square::cleanup::rebase_to_origin);
//! swapping just that one step would not reduce the surface, so the
//! whole pair is kept local to preserve byte-for-byte behaviour and the
//! non-negative-label invariant from `grow::grow_from_seed`.

use crate::cluster::ClusterCenters;
use crate::corner::CornerAug;
use crate::grow::GrowResult;
use calib_targets_core::{GridCoords, LabeledCorner, TargetDetection, TargetKind};

use super::types::Detection;

/// Public re-export so the topological dispatch path can reuse the same
/// canonicalisation + non-negative-rebase logic as the seed-and-grow
/// pipeline. The two pipelines emit identical [`Detection`] shapes.
pub fn build_detection_from_grow(
    corners: &[CornerAug],
    grow: &GrowResult,
    centers: ClusterCenters,
    cell_size: f32,
) -> Detection {
    build_detection(corners, grow, centers, cell_size)
}

pub(crate) fn build_detection(
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
