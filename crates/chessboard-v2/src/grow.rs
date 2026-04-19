//! Stage 6 — BFS-style growth over the labelled `(i, j)` set.
//!
//! Growth starts from the 4-corner seed and expands the boundary
//! queue one position at a time. This is the **practical** grow
//! algorithm:
//!
//! 1. For each boundary `(i, j)`, find any labelled neighbors in a
//!    3×3 window — **one is enough**.
//! 2. Predict the target's pixel position **per-neighbor** by
//!    adding a grid-aligned offset in the neighbor's local axes:
//!    `pred_k = pos(N_k) + di·(s·u_k) + dj·(s·v_k)` where
//!    `(u_k, v_k)` are the unit vectors of `(+i, +j)` grid
//!    directions at that neighbor. These are canonicalised once
//!    from the seed (so `u` points from `seed.a` toward `seed.b`,
//!    `v` from `seed.a` toward `seed.c`).
//! 3. Average all per-neighbor predictions to get `pred`.
//! 4. Attach the nearest strong-cluster corner within
//!    `attach_search_rel × s` of `pred` if:
//!    - its cluster matches `(i + j) mod 2` parity (hard),
//!    - both axes fall within `attach_axis_tol_deg` of the two
//!      cluster centers,
//!    - no other candidate lies within `ambiguity_factor ×`
//!      the nearest distance,
//!    - at least one induced edge to a labelled cardinal neighbor
//!      passes length + axis-slot-swap checks (soft — we trust
//!      post-validation to catch subtler breakages).
//!
//! Key differences from the pre-rewrite algorithm:
//! * No affine-from-3-neighbors: prediction works with **one**
//!   labelled neighbor. The old algorithm's affine fit became
//!   singular when 3 neighbors were collinear (common near the
//!   seed's edges), producing "single-line" detections.
//! * Only **one** induced edge must pass invariants at attach
//!   time, not all. The other edges are checked by post-
//!   validation (Stage 7), which can prune false attachments
//!   without blocking every marginal legitimate one.
//! * Grid vectors `(u, v)` are taken from the seed in pixel space,
//!   so they automatically carry the correct sign convention and
//!   are robust to local axis noise.

use crate::cluster::{angular_dist_pi, wrap_pi, ClusterCenters};
use crate::corner::{ClusterLabel, CornerAug, CornerStage};
use crate::params::DetectorParams;
use crate::seed::Seed;
use calib_targets_core::AxisEstimate;
use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};
use std::collections::{HashMap, HashSet, VecDeque};

/// Outcome of a grow pass.
pub struct GrowResult {
    /// `(i, j) → corners_index` map of accepted labels.
    pub labelled: HashMap<(i32, i32), usize>,
    /// `corners_index → (i, j)` inverse map.
    pub by_corner: HashMap<usize, (i32, i32)>,
    /// Positions with ≥2 candidates inside the ambiguity window.
    pub ambiguous: HashSet<(i32, i32)>,
    /// Positions with no accepted candidate.
    pub holes: HashSet<(i32, i32)>,
    /// Grid vectors carried forward — overlays / boosters use them.
    pub grid_u: Vector2<f32>,
    pub grid_v: Vector2<f32>,
}

/// Grow from the seed. Returns accepted `(i, j) → index` labels.
///
/// `blacklist` — corner indices to exclude from candidate searches.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_corners = corners.len(), cell_size = cell_size, blacklist_size = blacklist.len())
    )
)]
pub fn grow_from_seed(
    corners: &mut [CornerAug],
    seed: Seed,
    centers: ClusterCenters,
    cell_size: f32,
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
) -> GrowResult {
    // Grid vectors from the seed: u = (B − A) / s, v = (C − A) / s.
    // These carry the sign convention (which axis is +i vs +j) so
    // growth doesn't ambiguate axis directions.
    let grid_u = {
        let raw = corners[seed.b].position - corners[seed.a].position;
        let n = raw.norm().max(1e-6);
        raw / n
    };
    let grid_v = {
        let raw = corners[seed.c].position - corners[seed.a].position;
        let n = raw.norm().max(1e-6);
        raw / n
    };

    // KD-tree over every Clustered corner not in blacklist.
    let mut tree: KdTree<f32, 2> = KdTree::new();
    let mut eligible_indices: Vec<usize> = Vec::new();
    for (slot, corner) in corners.iter().enumerate() {
        if blacklist.contains(&slot) {
            continue;
        }
        if matches!(corner.stage, CornerStage::Clustered { .. }) {
            tree.add(
                &[corner.position.x, corner.position.y],
                eligible_indices.len() as u64,
            );
            eligible_indices.push(slot);
        }
    }

    let mut labelled: HashMap<(i32, i32), usize> = HashMap::new();
    let mut by_corner: HashMap<usize, (i32, i32)> = HashMap::new();
    let mut ambiguous: HashSet<(i32, i32)> = HashSet::new();
    let mut holes: HashSet<(i32, i32)> = HashSet::new();

    // Install seed corners.
    for (ij, idx) in [
        ((0, 0), seed.a),
        ((1, 0), seed.b),
        ((0, 1), seed.c),
        ((1, 1), seed.d),
    ] {
        labelled.insert(ij, idx);
        by_corner.insert(idx, ij);
    }

    // Boundary queue: unlabelled cardinal neighbors of seed corners.
    let mut boundary: VecDeque<(i32, i32)> = VecDeque::new();
    let mut seen_boundary: HashSet<(i32, i32)> = HashSet::new();
    for ij in labelled.keys().copied().collect::<Vec<_>>() {
        enqueue_cardinal_neighbors(ij, &labelled, &mut boundary, &mut seen_boundary);
    }

    let attach_tol = params.attach_axis_tol_deg.to_radians();
    let edge_tol = params.edge_axis_tol_deg.to_radians();
    let step_tol = params.step_tol;
    let search_r = params.attach_search_rel * cell_size;

    while let Some(pos) = boundary.pop_front() {
        if labelled.contains_key(&pos) {
            continue;
        }
        let required_label = required_label_at(pos.0, pos.1);

        // Any labelled neighbor in a 3×3 window is enough.
        let neighbors = collect_labelled_neighbors(pos, 1, &labelled, corners);
        if neighbors.is_empty() {
            holes.insert(pos);
            continue;
        }

        // Predict via axis-vector from every neighbor; average.
        let pred = predict_from_neighbors(pos, &neighbors, grid_u, grid_v, cell_size);

        // Find all candidates within search_r; filter by parity + axes
        // match; pick the unique nearest.
        let candidates = collect_candidates(
            &tree,
            &eligible_indices,
            pred,
            search_r,
            corners,
            blacklist,
            &by_corner,
            required_label,
            centers,
            attach_tol,
        );

        match choose_unambiguous(&candidates, params.attach_ambiguity_factor) {
            CandidateChoice::None => {
                holes.insert(pos);
            }
            CandidateChoice::Ambiguous => {
                ambiguous.insert(pos);
            }
            CandidateChoice::Unique(c_idx, _dist) => {
                // Require at least ONE induced edge to a labelled cardinal
                // neighbor to pass invariants. This is the minimal local
                // sanity check; post-validation catches wider errors.
                if !any_cardinal_edge_ok(
                    c_idx, pos, &labelled, corners, cell_size, step_tol, edge_tol,
                ) {
                    holes.insert(pos);
                    continue;
                }

                labelled.insert(pos, c_idx);
                by_corner.insert(c_idx, pos);
                corners[c_idx].stage = CornerStage::Labeled {
                    at: pos,
                    local_h_residual_px: None,
                };
                enqueue_cardinal_neighbors(pos, &labelled, &mut boundary, &mut seen_boundary);
            }
        }
    }

    // Rebase labels so (min_i, min_j) = (0, 0). Overlays are easier to read.
    let (min_i, min_j) = labelled
        .keys()
        .fold((i32::MAX, i32::MAX), |(a, b), &(i, j)| (a.min(i), b.min(j)));
    if min_i != 0 || min_j != 0 {
        let rebased: HashMap<(i32, i32), usize> = labelled
            .into_iter()
            .map(|((i, j), idx)| ((i - min_i, j - min_j), idx))
            .collect();
        let rebased_by_corner: HashMap<usize, (i32, i32)> =
            rebased.iter().map(|(&ij, &idx)| (idx, ij)).collect();
        // Update each labelled corner's stage.at to the rebased coord.
        for (&ij, &idx) in &rebased {
            corners[idx].stage = CornerStage::Labeled {
                at: ij,
                local_h_residual_px: None,
            };
        }
        labelled = rebased;
        by_corner = rebased_by_corner;
    }
    // Rebase holes + ambiguous too so overlays / debug are consistent.
    let rebase_pos = |(i, j)| (i - min_i, j - min_j);
    let ambiguous: HashSet<(i32, i32)> = ambiguous.into_iter().map(rebase_pos).collect();
    let holes: HashSet<(i32, i32)> = holes.into_iter().map(rebase_pos).collect();

    GrowResult {
        labelled,
        by_corner,
        ambiguous,
        holes,
        grid_u,
        grid_v,
    }
}

/// Parity of a labelled cell under the seed convention (seed `A` at
/// `(0, 0)` is `Canonical`).
fn required_label_at(i: i32, j: i32) -> ClusterLabel {
    if (i + j).rem_euclid(2) == 0 {
        ClusterLabel::Canonical
    } else {
        ClusterLabel::Swapped
    }
}

fn enqueue_cardinal_neighbors(
    pos: (i32, i32),
    labelled: &HashMap<(i32, i32), usize>,
    boundary: &mut VecDeque<(i32, i32)>,
    seen: &mut HashSet<(i32, i32)>,
) {
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (pos.0 + di, pos.1 + dj);
        if !labelled.contains_key(&neigh) && seen.insert(neigh) {
            boundary.push_back(neigh);
        }
    }
}

fn collect_labelled_neighbors(
    pos: (i32, i32),
    window_half: i32,
    labelled: &HashMap<(i32, i32), usize>,
    corners: &[CornerAug],
) -> Vec<((i32, i32), Point2<f32>)> {
    let mut out = Vec::new();
    for dj in -window_half..=window_half {
        for di in -window_half..=window_half {
            if di == 0 && dj == 0 {
                continue;
            }
            let neigh = (pos.0 + di, pos.1 + dj);
            if let Some(&idx) = labelled.get(&neigh) {
                out.push((neigh, corners[idx].position));
            }
        }
    }
    out
}

/// Average of per-neighbor axis-vector predictions.
///
/// Each neighbor contributes `pos(neigh) + di·s·u + dj·s·v`. For
/// robustness we weight all contributions equally — the grid
/// vectors are global so the predictions should agree modulo local
/// distortion, and a simple mean is a decent estimator.
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

/// Collect attachment candidates inside the search radius with
/// parity + axes-match filters applied.
#[allow(clippy::too_many_arguments)]
fn collect_candidates(
    tree: &KdTree<f32, 2>,
    eligible_indices: &[usize],
    pred: Point2<f32>,
    search_r: f32,
    corners: &[CornerAug],
    blacklist: &HashSet<usize>,
    by_corner: &HashMap<usize, (i32, i32)>,
    required_label: ClusterLabel,
    centers: ClusterCenters,
    attach_tol: f32,
) -> Vec<(usize, f32)> {
    let r2 = search_r * search_r;
    let mut out: Vec<(usize, f32)> = Vec::new();
    for nn in tree
        .within_unsorted::<SquaredEuclidean>(&[pred.x, pred.y], r2)
        .into_iter()
    {
        let slot = nn.item as usize;
        let idx = eligible_indices[slot];
        if blacklist.contains(&idx) || by_corner.contains_key(&idx) {
            continue;
        }
        let c = &corners[idx];
        let label = match c.stage {
            CornerStage::Clustered { label } => label,
            _ => continue,
        };
        if label != required_label {
            continue;
        }
        if !axes_match_centers(&c.axes, centers, attach_tol) {
            continue;
        }
        let d = nn.distance.sqrt();
        out.push((idx, d));
    }
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}

fn axes_match_centers(axes: &[AxisEstimate; 2], centers: ClusterCenters, tol: f32) -> bool {
    let a0 = wrap_pi(axes[0].angle);
    let a1 = wrap_pi(axes[1].angle);
    let canon_max = angular_dist_pi(a0, centers.theta0).max(angular_dist_pi(a1, centers.theta1));
    let swap_max = angular_dist_pi(a0, centers.theta1).max(angular_dist_pi(a1, centers.theta0));
    canon_max.min(swap_max) <= tol
}

enum CandidateChoice {
    None,
    Ambiguous,
    Unique(usize, f32),
}

fn choose_unambiguous(candidates: &[(usize, f32)], ambiguity_factor: f32) -> CandidateChoice {
    if candidates.is_empty() {
        return CandidateChoice::None;
    }
    if candidates.len() == 1 {
        let (idx, d) = candidates[0];
        return CandidateChoice::Unique(idx, d);
    }
    let (idx, d) = candidates[0];
    let (_, d2) = candidates[1];
    if d <= f32::EPSILON {
        return CandidateChoice::Ambiguous;
    }
    if d2 / d < ambiguity_factor {
        CandidateChoice::Ambiguous
    } else {
        CandidateChoice::Unique(idx, d)
    }
}

/// Accept `c_idx` at `pos` if AT LEAST ONE induced edge to a
/// labelled cardinal neighbor passes the attachment-time invariants
/// (length window + axis-match + slot swap).
///
/// Post-validation (Stage 7) will revisit the other edges with
/// line-collinearity and local-H residual checks. This keeps
/// growth liberal in the common case where one bad edge (due to a
/// noisy neighbor at a distant position) shouldn't block
/// attachment of an otherwise-valid corner.
fn any_cardinal_edge_ok(
    c_idx: usize,
    pos: (i32, i32),
    labelled: &HashMap<(i32, i32), usize>,
    corners: &[CornerAug],
    cell_size: f32,
    step_tol: f32,
    edge_tol: f32,
) -> bool {
    let c = &corners[c_idx];
    let min_len = (1.0 - step_tol) * cell_size;
    let max_len = (1.0 + step_tol) * cell_size;
    let mut found_any = false;
    for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        let neigh = (pos.0 + di, pos.1 + dj);
        let Some(&n_idx) = labelled.get(&neigh) else {
            continue;
        };
        found_any = true;
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
    // No labelled cardinal neighbors → defer to post-validation. The
    // position is only reached via BFS from a labelled neighbor, so
    // `found_any` should be true in practice; treat false-fall-through
    // as a safety net.
    !found_any
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::cluster_axes;
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
        strength: f32,
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
            strength,
        };
        let mut aug = CornerAug::from_corner(idx, &c);
        aug.stage = CornerStage::Strong;
        aug
    }

    fn build_clean_grid(rows: i32, cols: i32, s: f32) -> Vec<CornerAug> {
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
                out.push(make_corner(idx, x, y, axis_u, axis_v, label, 1.0));
                idx += 1;
            }
        }
        out
    }

    #[test]
    fn labels_every_corner_on_clean_5x5() {
        let mut corners = build_clean_grid(5, 5, 20.0);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let seed = find_seed(&corners, centers, &params).expect("seed").seed;
        let blacklist = HashSet::new();
        let res = grow_from_seed(&mut corners, seed, centers, 20.0, &blacklist, &params);
        assert_eq!(res.labelled.len(), 25, "should label all 25");
        // Coordinates are rebased to non-negative.
        assert!(res.labelled.keys().all(|(i, j)| *i >= 0 && *j >= 0));
    }

    #[test]
    fn rejects_parity_wrong_false_corner() {
        let mut corners = build_clean_grid(5, 5, 20.0);
        // Flip the center corner's axes so its Stage-3 label flips.
        let center_idx = 2 * 5 + 2;
        corners[center_idx].axes = [
            AxisEstimate {
                angle: std::f32::consts::FRAC_PI_2,
                sigma: 0.01,
            },
            AxisEstimate {
                angle: 0.0,
                sigma: 0.01,
            },
        ];
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let seed = find_seed(&corners, centers, &params).expect("seed").seed;
        let blacklist = HashSet::new();
        let res = grow_from_seed(&mut corners, seed, centers, 20.0, &blacklist, &params);
        assert!(
            !res.by_corner.contains_key(&center_idx),
            "parity-wrong corner was labelled"
        );
        assert!(res.labelled.len() >= 20);
    }

    #[test]
    fn grows_along_single_column_when_neighbors_are_collinear() {
        // 7-row by 1-column-wide strip — at every position along j,
        // the only labelled neighbors when predicting (0, j+1) sit on
        // the same column. Old affine-fit algorithm would fail here.
        let s = 25.0;
        let mut corners = build_clean_grid(7, 2, s);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let seed = find_seed(&corners, centers, &params).expect("seed").seed;
        let blacklist = HashSet::new();
        let res = grow_from_seed(&mut corners, seed, centers, s, &blacklist, &params);
        // Expect all 14 corners labelled (2 cols × 7 rows).
        assert_eq!(res.labelled.len(), 14, "got {}", res.labelled.len());
    }
}
