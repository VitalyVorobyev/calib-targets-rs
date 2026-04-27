//! Local-homography Stage 6 extension.
//!
//! [`extend_via_local_homography`] fits a per-candidate `H` from the K
//! nearest labelled corners (by grid Manhattan distance) and applies the
//! same per-cell filter ladder as the global-H pass. See the crate-level
//! module doc in [`super`] for the motivation and precision contract.

use std::collections::{HashMap, HashSet};

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::Point2;

use crate::homography::estimate_homography_with_quality;
use crate::square::extension::common::{try_attach_at_cell, TryCellResult};
use crate::square::extension::{ExtensionStats, LocalExtensionParams};
use crate::square::grow::{GrowResult, GrowValidator};

/// Extend the labelled grid outward (and into interior holes) using a
/// **per-candidate local homography** fit from the K nearest labelled
/// corners (by grid Manhattan distance).
///
/// Each candidate cell gets its own H, fit from the labels closest to
/// it in `(i, j)`-space. The per-candidate trust gate is the worst-case
/// residual on the K supports relative to `cell_size`; a poor local fit
/// aborts that candidate alone, not the whole pass.
///
/// `ExtensionStats::h_residual_median_px` and `h_residual_max_px`
/// aggregate residuals across **all** per-candidate fits in this pass
/// (median / worst across all supports). `h_trusted` is `true` if at
/// least one candidate's local fit passed its trust gate.
pub fn extend_via_local_homography<V: GrowValidator>(
    positions: &[Point2<f32>],
    grow: &mut GrowResult,
    cell_size: f32,
    params: &LocalExtensionParams,
    validator: &V,
) -> ExtensionStats {
    let mut stats = ExtensionStats::default();

    if grow.labelled.len() < params.min_k {
        return stats;
    }

    // KD-tree of un-labelled, eligible corners.
    let mut tree: KdTree<f32, 2> = KdTree::new();
    let mut tree_slot_to_corner: Vec<usize> = Vec::new();
    for (idx, pos) in positions.iter().enumerate() {
        if !grow.by_corner.contains_key(&idx) && validator.is_eligible(idx) {
            tree.add(&[pos.x, pos.y], tree_slot_to_corner.len() as u64);
            tree_slot_to_corner.push(idx);
        }
    }

    let search_r = params.common.search_rel * cell_size;
    let r2 = search_r * search_r;
    let max_residual_px = params.common.max_residual_rel * cell_size;

    let mut all_residuals: Vec<f32> = Vec::new();

    for iter in 0..params.common.max_iters {
        let cells =
            enumerate_extension_cells_deep(&grow.labelled, params.extend_depth.max(1) as i32);
        let mut attached_this_iter = 0usize;

        for cell in cells {
            if grow.labelled.contains_key(&cell) {
                continue;
            }

            // K nearest labelled corners by grid Manhattan distance.
            let nearest = nearest_labelled_by_grid(&grow.labelled, cell, params.k_nearest);
            if nearest.len() < params.min_k {
                stats.rejected_no_candidate += 1;
                continue;
            }

            // Fit local H from these labels.
            let grid_pts: Vec<Point2<f32>> = nearest
                .iter()
                .map(|&(i, j, _)| Point2::new(i as f32, j as f32))
                .collect();
            let img_pts: Vec<Point2<f32>> =
                nearest.iter().map(|&(_, _, idx)| positions[idx]).collect();
            let Some((h, _)) = estimate_homography_with_quality(&grid_pts, &img_pts) else {
                continue;
            };

            // Per-candidate trust gate: worst residual on the K supports.
            let mut max_resid: f32 = 0.0;
            for k in 0..grid_pts.len() {
                let pred = h.apply(grid_pts[k]);
                let dx = pred.x - img_pts[k].x;
                let dy = pred.y - img_pts[k].y;
                let r = (dx * dx + dy * dy).sqrt();
                if r > max_resid {
                    max_resid = r;
                }
                all_residuals.push(r);
            }
            if max_resid > max_residual_px {
                continue;
            }

            // Predict the candidate cell position.
            let pred = h.apply(Point2::new(cell.0 as f32, cell.1 as f32));

            // Parity gate + candidate collection.
            let required_label = validator.required_label_at(cell.0, cell.1);
            let mut hits: Vec<(usize, f32)> = Vec::new();
            let mut rejected_label_count = 0usize;
            for nn in tree
                .within_unsorted::<SquaredEuclidean>(&[pred.x, pred.y], r2)
                .into_iter()
            {
                let idx = tree_slot_to_corner[nn.item as usize];
                if grow.by_corner.contains_key(&idx) {
                    continue;
                }
                if let Some(req) = required_label {
                    let Some(got) = validator.label_of(idx) else {
                        rejected_label_count += 1;
                        continue;
                    };
                    if got != req {
                        rejected_label_count += 1;
                        continue;
                    }
                }
                hits.push((idx, nn.distance.sqrt()));
            }
            stats.rejected_label += rejected_label_count;
            hits.sort_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.0.cmp(&b.0)));

            match try_attach_at_cell(
                cell,
                pred,
                &hits,
                params.common.ambiguity_factor,
                grow,
                positions,
                validator,
            ) {
                TryCellResult::NoCandidates => {
                    stats.rejected_no_candidate += 1;
                }
                TryCellResult::Ambiguous => {
                    stats.rejected_ambiguous += 1;
                }
                TryCellResult::ValidatorRejected => {
                    stats.rejected_validator += 1;
                }
                TryCellResult::EdgeRejected => {
                    stats.rejected_edge += 1;
                }
                TryCellResult::Attached(c_idx) => {
                    grow.labelled.insert(cell, c_idx);
                    grow.by_corner.insert(c_idx, cell);
                    grow.holes.remove(&cell);
                    grow.ambiguous.remove(&cell);
                    stats.attached += 1;
                    stats.attached_indices.push(c_idx);
                    stats.attached_cells.push(cell);
                    attached_this_iter += 1;
                    stats.h_trusted = true;
                }
            }
        }

        stats.iterations = iter as usize + 1;
        if attached_this_iter == 0 {
            break;
        }
    }

    if !all_residuals.is_empty() {
        all_residuals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        stats.h_residual_median_px = Some(all_residuals[all_residuals.len() / 2]);
        stats.h_residual_max_px = Some(*all_residuals.last().unwrap());
    }

    stats
}

/// Cells worth trying for the deeper local-H pass: every interior hole,
/// plus all cells within `depth` Manhattan distance past the labelled
/// bbox edge. The local-H per-cell trust gate is responsible for
/// rejecting cells whose K-nearest support gives a poor fit.
pub(super) fn enumerate_extension_cells_deep(
    labelled: &HashMap<(i32, i32), usize>,
    depth: i32,
) -> Vec<(i32, i32)> {
    if labelled.is_empty() || depth < 1 {
        return Vec::new();
    }
    let (mut min_i, mut max_i, mut min_j, mut max_j) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    let mut rows: HashSet<i32> = HashSet::new();
    let mut cols: HashSet<i32> = HashSet::new();
    for &(i, j) in labelled.keys() {
        min_i = min_i.min(i);
        max_i = max_i.max(i);
        min_j = min_j.min(j);
        max_j = max_j.max(j);
        cols.insert(i);
        rows.insert(j);
    }

    let mut out: HashSet<(i32, i32)> = HashSet::new();
    // Interior holes (cells in bbox not labelled).
    for j in min_j..=max_j {
        for i in min_i..=max_i {
            if !labelled.contains_key(&(i, j)) {
                out.insert((i, j));
            }
        }
    }
    // Side strips of width `depth`, aligned with labelled rows / columns.
    for d in 1..=depth {
        for &j in &rows {
            out.insert((min_i - d, j));
            out.insert((max_i + d, j));
        }
        for &i in &cols {
            out.insert((i, min_j - d));
            out.insert((i, max_j + d));
        }
        for d2 in 1..=depth {
            out.insert((min_i - d, min_j - d2));
            out.insert((min_i - d, max_j + d2));
            out.insert((max_i + d, min_j - d2));
            out.insert((max_i + d, max_j + d2));
        }
    }
    let mut v: Vec<(i32, i32)> = out.into_iter().collect();
    v.sort_unstable();
    v
}

/// Find the K labelled corners closest to `target` by Manhattan distance
/// in `(i, j)`-space. Ties broken deterministically by `(i, j, idx)`.
/// Returns `(i, j, idx)` triples.
pub(super) fn nearest_labelled_by_grid(
    labelled: &HashMap<(i32, i32), usize>,
    target: (i32, i32),
    k: usize,
) -> Vec<(i32, i32, usize)> {
    let mut sorted: Vec<((i32, i32), usize, i32)> = labelled
        .iter()
        .map(|(&(i, j), &idx)| {
            let d = (i - target.0).abs() + (j - target.1).abs();
            ((i, j), idx, d)
        })
        .collect();
    sorted.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then_with(|| a.0 .0.cmp(&b.0 .0))
            .then_with(|| a.0 .1.cmp(&b.0 .1))
            .then_with(|| a.1.cmp(&b.1))
    });
    sorted
        .into_iter()
        .take(k)
        .map(|((i, j), idx, _)| (i, j, idx))
        .collect()
}
