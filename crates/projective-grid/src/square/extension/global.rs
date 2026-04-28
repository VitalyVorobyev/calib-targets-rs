//! Global-homography Stage 6 extension.
//!
//! [`extend_via_global_homography`] fits a single `H : (i,j) → pixel`
//! over the entire labelled set and uses it to predict cells outside
//! (or inside holes in) the labelled bounding box. See the crate-level
//! module doc in [`super`] for the precision contract and failure modes.

use std::collections::{HashMap, HashSet};

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::Point2;

use crate::homography::estimate_homography_with_quality;
use crate::square::extension::common::{try_attach_at_cell, TryCellResult};
use crate::square::extension::{ExtensionParams, ExtensionStats};
use crate::square::grow::{GrowResult, GrowValidator};

/// Try to extend the labelled grid outward (and into interior holes)
/// using a globally-fit homography. Mutates `grow.labelled` and
/// `grow.by_corner` in place.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_corners = positions.len(), num_labelled = grow.labelled.len(), cell_size = cell_size),
    )
)]
pub fn extend_via_global_homography<V: GrowValidator>(
    positions: &[Point2<f32>],
    grow: &mut GrowResult,
    cell_size: f32,
    params: &ExtensionParams,
    validator: &V,
) -> ExtensionStats {
    let mut stats = ExtensionStats::default();

    if grow.labelled.len() < params.min_labels_for_h {
        return stats;
    }

    // Fit global H from the labelled set.
    let mut grid_pts: Vec<Point2<f32>> = Vec::with_capacity(grow.labelled.len());
    let mut img_pts: Vec<Point2<f32>> = Vec::with_capacity(grow.labelled.len());
    for (&(i, j), &idx) in &grow.labelled {
        grid_pts.push(Point2::new(i as f32, j as f32));
        img_pts.push(positions[idx]);
    }
    let Some((h, quality)) = estimate_homography_with_quality(&grid_pts, &img_pts) else {
        return stats;
    };
    stats.h_quality = Some(quality);

    // Reprojection residuals on the labelled set.
    let mut residuals: Vec<f32> = Vec::with_capacity(grid_pts.len());
    for k in 0..grid_pts.len() {
        let pred = h.apply(grid_pts[k]);
        let dx = pred.x - img_pts[k].x;
        let dy = pred.y - img_pts[k].y;
        residuals.push((dx * dx + dy * dy).sqrt());
    }
    residuals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_res = residuals[residuals.len() / 2];
    let max_res = *residuals.last().unwrap();
    stats.h_residual_median_px = Some(median_res);
    stats.h_residual_max_px = Some(max_res);

    let median_thresh = params.max_median_residual_rel * cell_size;
    let max_thresh = params.common.max_residual_rel * cell_size;
    if median_res > median_thresh || max_res > max_thresh {
        return stats;
    }
    stats.h_trusted = true;

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

    for iter in 0..params.common.max_iters {
        let cells = enumerate_extension_cells(&grow.labelled);
        let mut attached_this_iter = 0usize;

        for cell in cells {
            if grow.labelled.contains_key(&cell) {
                continue;
            }

            let required_label = validator.required_label_at(cell.0, cell.1);
            let pred = h.apply(Point2::new(cell.0 as f32, cell.1 as f32));

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
                }
            }
        }

        stats.iterations = iter as usize + 1;
        if attached_this_iter == 0 {
            return stats;
        }
    }
    stats
}

/// Cells worth trying: every interior hole in the labelled bbox, plus
/// one step beyond the bbox in each direction (for rows / columns that
/// have at least one labelled member).
pub(super) fn enumerate_extension_cells(labelled: &HashMap<(i32, i32), usize>) -> Vec<(i32, i32)> {
    if labelled.is_empty() {
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
    for j in min_j..=max_j {
        for i in min_i..=max_i {
            if !labelled.contains_key(&(i, j)) {
                out.insert((i, j));
            }
        }
    }
    for &j in &rows {
        out.insert((min_i - 1, j));
        out.insert((max_i + 1, j));
    }
    for &i in &cols {
        out.insert((i, min_j - 1));
        out.insert((i, max_j + 1));
    }
    let mut v: Vec<(i32, i32)> = out.into_iter().collect();
    v.sort_unstable();
    v
}
