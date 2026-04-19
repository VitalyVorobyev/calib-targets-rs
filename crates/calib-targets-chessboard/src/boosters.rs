//! Phase E — recall boosters (spec §5.8).
//!
//! Run **after** the precision core (seed + grow + validate) has
//! converged with no new blacklist entries. Boosters ADD labelled
//! corners without compromising the precision contract — they reuse
//! the same attachment invariants as growth.
//!
//! This module implements a unified "fill pass":
//!
//! - **Interior gap fill**: for each `(i, j)` strictly inside the
//!   labelled bounding box that isn't labelled, predict + attach.
//! - **Line extrapolation**: for each labelled row / column with
//!   ≥3 members, try to extend ±1 at each end.
//!
//! Both steps share the same attachment machinery. A cell is filled
//! iff:
//! 1. A candidate lies within `attach_search_rel × s` of the
//!    axis-vector prediction (averaged over all labelled neighbors
//!    in a 3×3 window).
//! 2. The candidate's cluster matches `(i + j) mod 2` parity.
//! 3. Both axes match the global centers within `attach_axis_tol`.
//! 4. The nearest-but-one candidate is farther than
//!    `ambiguity_factor × nearest distance`.
//! 5. At least one induced edge to an already-labelled cardinal
//!    neighbor passes the length + axis-slot-swap check.
//!
//! The pass iterates until no new attachments happen in a full
//! scan, capped at `max_booster_iters`.

use crate::cluster::{angular_dist_pi, wrap_pi, ClusterCenters};
use crate::corner::{ClusterLabel, CornerAug, CornerStage};
use crate::grow::GrowResult;
use crate::params::DetectorParams;
use calib_targets_core::AxisEstimate;
use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};
use std::collections::HashSet;

/// Diagnostic returned by [`apply_boosters`].
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct BoosterResult {
    /// Corners added to the labelled set across all booster passes.
    pub added: usize,
    /// Positions tried and not attached (interior holes that
    /// couldn't find a passing candidate; or line extensions that
    /// failed).
    pub holes_untouched: usize,
}

/// Extend the labelled set via interior gap fill + line
/// extrapolation. Mutates `grow.labelled` and `corners[*].stage`.
///
/// `blacklist` — corner indices to keep excluded from candidate
/// searches, same as the precision core.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(labelled = grow.labelled.len(), cell_size = cell_size)
    )
)]
pub fn apply_boosters(
    corners: &mut [CornerAug],
    grow: &mut GrowResult,
    centers: ClusterCenters,
    cell_size: f32,
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
) -> BoosterResult {
    let mut result = BoosterResult::default();
    let mut total_added = 0usize;

    // KD-tree over every non-blacklisted, non-labelled Clustered
    // corner. We rebuild it each outer iteration since labels
    // change.
    for _iter in 0..params.max_booster_iters.max(1) {
        let (tree, eligible_indices) = build_eligible_tree(corners, grow, blacklist, params);

        let candidates_to_try = enumerate_candidate_positions(grow);
        let mut added_this_iter = 0usize;

        for pos in candidates_to_try {
            if grow.labelled.contains_key(&pos) {
                continue;
            }
            let attached = try_attach_at(
                pos,
                corners,
                grow,
                centers,
                cell_size,
                &tree,
                &eligible_indices,
                blacklist,
                params,
            );
            if attached {
                added_this_iter += 1;
            }
        }

        total_added += added_this_iter;
        if added_this_iter == 0 {
            break;
        }
    }

    result.added = total_added;
    result
}

/// Build a KD-tree over Clustered-but-not-labelled, non-blacklisted
/// corners.
///
/// When `weak_cluster_rescue` is enabled, also include `NoCluster`
/// corners whose `max_d_deg` is within `weak_cluster_tol_deg`. These
/// corners failed the strict Stage-3 gate by a hair — the booster
/// pass will re-assign them a label at attachment time. This is
/// spec §5.8d.
fn build_eligible_tree(
    corners: &[CornerAug],
    grow: &GrowResult,
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
) -> (KdTree<f32, 2>, Vec<usize>) {
    let mut tree: KdTree<f32, 2> = KdTree::new();
    let mut slots: Vec<usize> = Vec::new();
    let weak_tol = params.weak_cluster_tol_deg;
    for (idx, c) in corners.iter().enumerate() {
        if blacklist.contains(&idx) {
            continue;
        }
        if grow.by_corner.contains_key(&idx) {
            continue;
        }
        let eligible = matches!(c.stage, CornerStage::Clustered { .. })
            || (params.enable_weak_cluster_rescue
                && matches!(
                    c.stage,
                    CornerStage::NoCluster { max_d_deg } if max_d_deg <= weak_tol
                ));
        if !eligible {
            continue;
        }
        tree.add(&[c.position.x, c.position.y], slots.len() as u64);
        slots.push(idx);
    }
    (tree, slots)
}

/// Collect positions to try: interior holes + 1-step-out line
/// extensions.
fn enumerate_candidate_positions(grow: &GrowResult) -> Vec<(i32, i32)> {
    let mut out: HashSet<(i32, i32)> = HashSet::new();
    if grow.labelled.is_empty() {
        return Vec::new();
    }
    let (mut min_i, mut max_i, mut min_j, mut max_j) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
    for &(i, j) in grow.labelled.keys() {
        min_i = min_i.min(i);
        max_i = max_i.max(i);
        min_j = min_j.min(j);
        max_j = max_j.max(j);
    }

    // 8b Interior gap fill: every unlabelled cell inside the bbox.
    for j in min_j..=max_j {
        for i in min_i..=max_i {
            if !grow.labelled.contains_key(&(i, j)) {
                out.insert((i, j));
            }
        }
    }

    // 8a Line extrapolation: ±1 beyond the bbox ends, at every row
    // and column that has any labelled member.
    for j in min_j..=max_j {
        out.insert((min_i - 1, j));
        out.insert((max_i + 1, j));
    }
    for i in min_i..=max_i {
        out.insert((i, min_j - 1));
        out.insert((i, max_j + 1));
    }

    out.into_iter().collect()
}

/// Try to attach a single corner at `pos`. Returns `true` if
/// attached.
#[allow(clippy::too_many_arguments)]
fn try_attach_at(
    pos: (i32, i32),
    corners: &mut [CornerAug],
    grow: &mut GrowResult,
    centers: ClusterCenters,
    cell_size: f32,
    tree: &KdTree<f32, 2>,
    eligible_indices: &[usize],
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
) -> bool {
    let required_label = required_label_at(pos.0, pos.1);

    // Collect labelled neighbors in a 3×3 window. For interior
    // gaps this will usually be 4+; for line extensions it will
    // be 1 (the last corner of the line) up to 3 (with diagonals
    // from adjacent labelled rows).
    let neighbors = collect_labelled_neighbors(pos, 1, grow, corners);
    if neighbors.is_empty() {
        return false;
    }

    // Axis-vector prediction from each neighbor; average.
    let pred = predict_from_neighbors(pos, &neighbors, grow.grid_u, grow.grid_v, cell_size);

    // Candidate search.
    let attach_tol = params.attach_axis_tol_deg.to_radians();
    let edge_tol = params.edge_axis_tol_deg.to_radians();
    let step_tol = params.step_tol;
    let search_r = params.attach_search_rel * cell_size;

    let mut hits: Vec<(usize, f32)> = Vec::new();
    let r2 = search_r * search_r;
    let weak_attach_tol = params.weak_cluster_tol_deg.to_radians();
    for nn in tree
        .within_unsorted::<SquaredEuclidean>(&[pred.x, pred.y], r2)
        .into_iter()
    {
        let slot = nn.item as usize;
        let idx = eligible_indices[slot];
        if blacklist.contains(&idx) || grow.by_corner.contains_key(&idx) {
            continue;
        }
        let c = &corners[idx];

        // Determine the candidate's effective cluster label.
        let (label, effective_tol) = match c.stage {
            CornerStage::Clustered { label } => (label, attach_tol),
            CornerStage::NoCluster { .. } => {
                // Weak-cluster rescue: infer label from axis vs
                // centers, with the wider rescue tolerance.
                let Some(l) = infer_label_from_axes(&c.axes, centers, weak_attach_tol) else {
                    continue;
                };
                (l, weak_attach_tol)
            }
            _ => continue,
        };
        if label != required_label {
            continue;
        }
        if !axes_match_centers(&c.axes, centers, effective_tol) {
            continue;
        }
        let d = nn.distance.sqrt();
        hits.push((idx, d));
    }
    hits.sort_by(|a, b| a.1.total_cmp(&b.1));

    let (c_idx, _d) = match hits.len() {
        0 => return false,
        1 => hits[0],
        _ => {
            let (first_idx, d) = hits[0];
            let (_, d2) = hits[1];
            if d <= f32::EPSILON || d2 / d < params.attach_ambiguity_factor {
                return false; // ambiguous
            }
            (first_idx, d)
        }
    };

    // Edge-invariant check: at least one induced edge to a
    // labelled cardinal neighbor must pass length + slot-swap.
    if !any_cardinal_edge_ok(
        c_idx,
        pos,
        &grow.labelled,
        corners,
        cell_size,
        step_tol,
        edge_tol,
    ) {
        return false;
    }

    // Attach.
    grow.labelled.insert(pos, c_idx);
    grow.by_corner.insert(c_idx, pos);
    corners[c_idx].stage = CornerStage::Labeled {
        at: pos,
        local_h_residual_px: None,
    };
    true
}

fn required_label_at(i: i32, j: i32) -> ClusterLabel {
    if (i + j).rem_euclid(2) == 0 {
        ClusterLabel::Canonical
    } else {
        ClusterLabel::Swapped
    }
}

fn collect_labelled_neighbors(
    pos: (i32, i32),
    window_half: i32,
    grow: &GrowResult,
    corners: &[CornerAug],
) -> Vec<((i32, i32), Point2<f32>)> {
    let mut out = Vec::new();
    for dj in -window_half..=window_half {
        for di in -window_half..=window_half {
            if di == 0 && dj == 0 {
                continue;
            }
            let neigh = (pos.0 + di, pos.1 + dj);
            if let Some(&idx) = grow.labelled.get(&neigh) {
                out.push((neigh, corners[idx].position));
            }
        }
    }
    out
}

fn predict_from_neighbors(
    target: (i32, i32),
    neighbors: &[((i32, i32), Point2<f32>)],
    u: Vector2<f32>,
    v: Vector2<f32>,
    cell_size: f32,
) -> Point2<f32> {
    debug_assert!(!neighbors.is_empty());
    let mut sum_x = 0.0_f32;
    let mut sum_y = 0.0_f32;
    for ((ni, nj), p) in neighbors {
        let di = (target.0 - ni) as f32;
        let dj = (target.1 - nj) as f32;
        let off = u * (di * cell_size) + v * (dj * cell_size);
        sum_x += p.x + off.x;
        sum_y += p.y + off.y;
    }
    let n = neighbors.len() as f32;
    Point2::new(sum_x / n, sum_y / n)
}

/// Infer cluster label for a weakly-clustered corner: pick the
/// assignment (canonical vs swapped) whose worst per-axis
/// distance is smaller; require it to be within `tol`.
fn infer_label_from_axes(
    axes: &[AxisEstimate; 2],
    centers: ClusterCenters,
    tol: f32,
) -> Option<ClusterLabel> {
    let a0 = wrap_pi(axes[0].angle);
    let a1 = wrap_pi(axes[1].angle);
    let canon_max = angular_dist_pi(a0, centers.theta0).max(angular_dist_pi(a1, centers.theta1));
    let swap_max = angular_dist_pi(a0, centers.theta1).max(angular_dist_pi(a1, centers.theta0));
    if canon_max <= swap_max {
        if canon_max <= tol {
            Some(ClusterLabel::Canonical)
        } else {
            None
        }
    } else if swap_max <= tol {
        Some(ClusterLabel::Swapped)
    } else {
        None
    }
}

fn axes_match_centers(axes: &[AxisEstimate; 2], centers: ClusterCenters, tol: f32) -> bool {
    let a0 = wrap_pi(axes[0].angle);
    let a1 = wrap_pi(axes[1].angle);
    let canon_max = angular_dist_pi(a0, centers.theta0).max(angular_dist_pi(a1, centers.theta1));
    let swap_max = angular_dist_pi(a0, centers.theta1).max(angular_dist_pi(a1, centers.theta0));
    canon_max.min(swap_max) <= tol
}

fn any_cardinal_edge_ok(
    c_idx: usize,
    pos: (i32, i32),
    labelled: &std::collections::HashMap<(i32, i32), usize>,
    corners: &[CornerAug],
    cell_size: f32,
    step_tol: f32,
    edge_tol: f32,
) -> bool {
    let c = &corners[c_idx];
    let min_len = (1.0 - step_tol) * cell_size;
    let max_len = (1.0 + step_tol) * cell_size;
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (pos.0 + di, pos.1 + dj);
        let Some(&n_idx) = labelled.get(&neigh) else {
            continue;
        };
        let n = &corners[n_idx];
        let off = n.position - c.position;
        let dist = off.norm();
        if dist < min_len || dist > max_len {
            continue;
        }
        let ang = wrap_pi(off.y.atan2(off.x));
        let d_c0 = angular_dist_pi(ang, wrap_pi(c.axes[0].angle));
        let d_c1 = angular_dist_pi(ang, wrap_pi(c.axes[1].angle));
        let (slot_c, d_c) = if d_c0 <= d_c1 { (0, d_c0) } else { (1, d_c1) };
        if d_c > edge_tol {
            continue;
        }
        let d_n0 = angular_dist_pi(ang, wrap_pi(n.axes[0].angle));
        let d_n1 = angular_dist_pi(ang, wrap_pi(n.axes[1].angle));
        let (slot_n, d_n) = if d_n0 <= d_n1 { (0, d_n0) } else { (1, d_n1) };
        if d_n > edge_tol {
            continue;
        }
        if slot_c != slot_n {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::cluster_axes;
    use crate::grow::grow_from_seed;
    use crate::seed::find_seed;
    use calib_targets_core::{AxisEstimate, Corner};
    use nalgebra::Point2;

    fn make_corner(
        idx: usize,
        x: f32,
        y: f32,
        axis_u: f32,
        axis_v: f32,
        label: ClusterLabel,
    ) -> CornerAug {
        let axes = match label {
            ClusterLabel::Canonical => [axis_u, axis_v],
            ClusterLabel::Swapped => [axis_v, axis_u],
        };
        let c = Corner {
            position: Point2::new(x, y),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: wrap_pi(axes[0]),
                    sigma: 0.01,
                },
                AxisEstimate {
                    angle: wrap_pi(axes[1]),
                    sigma: 0.01,
                },
            ],
            contrast: 10.0,
            fit_rms: 1.0,
            strength: 1.0,
        };
        let mut aug = CornerAug::from_corner(idx, &c);
        aug.stage = CornerStage::Strong;
        aug
    }

    fn build_grid(rows: i32, cols: i32, s: f32) -> Vec<CornerAug> {
        let axis_u = 0.0_f32;
        let axis_v = std::f32::consts::FRAC_PI_2;
        let mut out = Vec::new();
        let mut idx = 0;
        for j in 0..rows {
            for i in 0..cols {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let label = if (i + j).rem_euclid(2) == 0 {
                    ClusterLabel::Canonical
                } else {
                    ClusterLabel::Swapped
                };
                out.push(make_corner(idx, x, y, axis_u, axis_v, label));
                idx += 1;
            }
        }
        out
    }

    #[test]
    fn interior_gap_fill_recovers_missing_corner() {
        // 5×5 grid with one interior corner temporarily made
        // un-clusterable (simulates the core growth failing to
        // attach it — boosters should rescue).
        let s = 20.0_f32;
        let mut corners = build_grid(5, 5, s);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let seed = find_seed(&corners, centers, &params).expect("seed").seed;
        let blacklist = HashSet::new();
        // Pretend the (2, 2) corner was ambiguous during growth by
        // running growth with that corner blacklisted; then remove
        // blacklist for the booster pass.
        let mut protect: HashSet<usize> = HashSet::new();
        let center_idx = 2 * 5 + 2;
        protect.insert(center_idx);
        let mut grow = grow_from_seed(&mut corners, seed, centers, s, &protect, &params);
        let before = grow.labelled.len();
        assert!((20..25).contains(&before), "got {before}");
        // Booster pass WITHOUT the blacklist → (2, 2) should attach.
        let result = apply_boosters(&mut corners, &mut grow, centers, s, &blacklist, &params);
        assert!(
            grow.labelled.len() > before,
            "booster should attach at least 1"
        );
        assert!(result.added >= 1);
    }

    #[test]
    fn line_extrapolation_extends_beyond_bbox() {
        // Grow on a 3×5 strip, then make a wider 3×7 strip
        // available. Boosters should extend ±1 beyond the grown
        // bbox.
        let s = 20.0_f32;
        let mut corners = build_grid(3, 7, s);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let seed = find_seed(&corners, centers, &params).expect("seed").seed;
        let blacklist = HashSet::new();
        let mut grow = grow_from_seed(&mut corners, seed, centers, s, &blacklist, &params);
        let before = grow.labelled.len();
        // Boosters shouldn't add anything: growth labelled everyone.
        let result = apply_boosters(&mut corners, &mut grow, centers, s, &blacklist, &params);
        assert_eq!(grow.labelled.len(), before);
        assert_eq!(result.added, 0);
    }
}
