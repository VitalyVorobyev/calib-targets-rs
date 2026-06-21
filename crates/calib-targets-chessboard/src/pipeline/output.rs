//! Final output: convert a labelled grid into a [`ChessboardDetection`].
//!
//! Stage 13 — `(i, j) → corner_idx` map to a typed
//! [`ChessboardDetection`] of [`ChessboardCorner`] entries.
//!
//! # NOTE — bespoke cleanup
//!
//! This module keeps its own `build_detection` /
//! `canonicalize_orientation` pair because the chessboard variant
//! operates on an in-place
//! `Vec<((i32, i32), usize)>` and interleaves the rebase, swap, and
//! sign-flip steps so the result of `canonicalize_orientation` feeds
//! directly into the stable `(j, i)` sort below.
//!
//! The rebase here is the same `min (i, j) → (0, 0)` shift as
//! the square-grid grow path. Keeping the whole pair local preserves
//! byte-for-byte behaviour and the non-negative-label invariant from
//! `grow::grow_from_seed`.

use crate::corner::CornerAug;
use calib_targets_core::GridCoords;
use projective_grid::shared::grow::GrowResult;

use super::types::{ChessboardCorner, ChessboardDetection};

/// Public re-export so the topological dispatch path can reuse the same
/// canonicalisation + non-negative-rebase logic as the seed-and-grow
/// pipeline. The two pipelines emit identical [`ChessboardDetection`]
/// shapes.
///
/// `cell_size` is the grid pitch in pixels to record on the result (see
/// [`ChessboardDetection::cell_size`]).
pub fn build_detection_from_grow(
    corners: &[CornerAug],
    grow: &GrowResult,
    cell_size: f32,
) -> ChessboardDetection {
    build_detection(corners, grow, cell_size)
}

pub(crate) fn build_detection(
    corners: &[CornerAug],
    grow: &GrowResult,
    cell_size: f32,
) -> ChessboardDetection {
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
    canonicalize_orientation(&mut labelled_pairs, corners);

    // Sort by (j, i) so the output order is stable.
    labelled_pairs.sort_by_key(|&((i, j), _)| (j, i));

    let mut chessboard_corners: Vec<ChessboardCorner> = Vec::with_capacity(labelled_pairs.len());
    for ((i, j), c_idx) in labelled_pairs {
        let c = &corners[c_idx];
        chessboard_corners.push(ChessboardCorner {
            position: c.position,
            grid: GridCoords { i, j },
            input_index: c.input_index,
            score: c.strength,
        });
    }

    ChessboardDetection {
        corners: chessboard_corners,
        cell_size: Some(cell_size),
    }
}

/// Canonicalize grid orientation so +i points roughly +x (right) and +j
/// points roughly +y (down) in image pixel coordinates. Preserves the
/// labelling up to axis permutation / sign flips and keeps `(i, j)`
/// non-negative with the bounding-box minimum at `(0, 0)`.
fn canonicalize_orientation(labelled_pairs: &mut [((i32, i32), usize)], corners: &[CornerAug]) {
    if labelled_pairs.len() < 2 {
        return;
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
        return;
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
        return;
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
}
