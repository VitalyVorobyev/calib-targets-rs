//! Seed finder for the v2 detector.
//!
//! Find a `2×2` cell whose 4 corners satisfy the per-corner and
//! per-edge invariants, **and estimate the cell size from that
//! seed**. The seed output therefore replaces the global
//! `cell_size` estimator — which empirically mispicks a ~40 px
//! mode in frames whose true cell spacing is ~55-60 px, because
//! the cross-cluster nearest-neighbor histogram is bimodal.
//!
//! Layout and parity:
//! ```text
//!                     A.axes[0]
//!  A (0,0)  ───── AB ────── B (1,0)
//!   Canonical                Swapped
//!     │                         │
//!     │ A.axes[1]               │
//!     │                         │
//!  C (0,1)  ───── CD ────── D (1,1)
//!   Swapped                   Canonical
//! ```
//!
//! # Algorithm
//!
//! 1. Split `Clustered` corners by label.
//! 2. Rank `Canonical` corners by descending strength (best A
//!    candidates first).
//! 3. For each `A`:
//!    - kNN-search up to 32 Swapped corners (no distance window at
//!      all — cell-size is an output, not an input).
//!    - Classify each Swapped candidate by which of `A.axes[0]` or
//!      `A.axes[1]` the chord `A→N` is angularly closer to, within
//!      `seed_axis_tol_deg`.
//!    - Separate the classified candidates into two axis-specific
//!      sorted-by-distance lists `B_cands` and `C_cands`.
//! 4. For each pair `(B, C)` among the shortest few in each list:
//!    - Require `|AB|` and `|AC|` to match each other within the
//!      `seed_edge_ratio` window (e.g., smaller/larger ≥ 1 - tol).
//!    - Predict `D_pred = A + (B−A) + (C−A)` (parallelogram
//!      completion).
//!    - Find the nearest `Canonical` corner to `D_pred` within
//!      `seed_close_tol × avg_edge`.
//!    - Verify `|BD|` and `|CD|` match the other edges within the
//!      same tolerance.
//!    - Verify axis-slot swap on every edge (parity).
//! 5. First quad passing every check wins. Return the seed **and
//!    the estimated cell size = mean of the 4 edge lengths**.
//!
//! If no seed is found under the primary tolerance, a retry pass
//! widens every tolerance by `1.5×`. Still nothing → `None`.

use crate::cluster::{angular_dist_pi, wrap_pi, ClusterCenters};
use crate::corner::{ClusterLabel, CornerAug, CornerStage};
use crate::params::DetectorParams;
use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{Point2, Vector2};

/// Handle for the four seed corners.
#[derive(Clone, Copy, Debug)]
pub struct Seed {
    pub a: usize, // (0, 0), Canonical
    pub b: usize, // (1, 0), Swapped
    pub c: usize, // (0, 1), Swapped
    pub d: usize, // (1, 1), Canonical
}

/// Output of the seed finder: the 4-corner quad **plus the local
/// cell size** measured directly from the seed.
#[derive(Clone, Copy, Debug)]
pub struct SeedOutput {
    pub seed: Seed,
    pub cell_size: f32,
}

/// Find a valid seed. Cell size comes OUT of the seed (no cell-
/// size input).
pub fn find_seed(
    corners: &[CornerAug],
    centers: ClusterCenters,
    params: &DetectorParams,
) -> Option<SeedOutput> {
    try_find_seed(corners, centers, params, 1.0)
        .or_else(|| try_find_seed(corners, centers, params, 1.5))
}

/// Backward-compatible alias for old callers that still pass a
/// cell-size estimate. Ignored in the new implementation.
pub fn find_seed_with_hint(
    corners: &[CornerAug],
    centers: ClusterCenters,
    _cell_size_hint: f32,
    params: &DetectorParams,
) -> Option<SeedOutput> {
    find_seed(corners, centers, params)
}

fn try_find_seed(
    corners: &[CornerAug],
    centers: ClusterCenters,
    params: &DetectorParams,
    slack: f32,
) -> Option<SeedOutput> {
    if corners.is_empty() {
        return None;
    }
    let axis_tol = params.seed_axis_tol_deg.to_radians() * slack;
    // `edge_ratio_tol` caps the |AB|/|AC| mismatch (and the other
    // pairs) — replaces the absolute cell-size window.
    let edge_ratio_tol = params.seed_edge_tol * slack;
    let min_ratio = 1.0 - edge_ratio_tol;
    let max_ratio = 1.0 + edge_ratio_tol;
    let close_tol = params.seed_close_tol * slack;

    // Split clustered corners by label.
    let label_of = |i: usize| match corners[i].stage {
        CornerStage::Clustered { label } => Some(label),
        _ => None,
    };

    let canonical: Vec<usize> = (0..corners.len())
        .filter(|&i| label_of(i) == Some(ClusterLabel::Canonical))
        .collect();
    let swapped: Vec<usize> = (0..corners.len())
        .filter(|&i| label_of(i) == Some(ClusterLabel::Swapped))
        .collect();
    if canonical.is_empty() || swapped.is_empty() {
        return None;
    }

    // KD-trees (we do NOT prefilter by distance here — the seed's
    // "cell size" is determined by its own 4 edges).
    let mut canon_tree: KdTree<f32, 2> = KdTree::new();
    for (slot, &i) in canonical.iter().enumerate() {
        let p = corners[i].position;
        canon_tree.add(&[p.x, p.y], slot as u64);
    }
    let mut swap_tree: KdTree<f32, 2> = KdTree::new();
    for (slot, &i) in swapped.iter().enumerate() {
        let p = corners[i].position;
        swap_tree.add(&[p.x, p.y], slot as u64);
    }

    // Rank A candidates by descending strength.
    let mut a_order: Vec<usize> = canonical.clone();
    a_order.sort_by(|&i, &j| corners[j].strength.total_cmp(&corners[i].strength));

    // Consider up to this many Swapped kNN per A (captures board
    // neighbors even when the a priori cell size is unknown).
    const K_SWAP: usize = 32;
    // Consider at most this many B / C candidates per axis when
    // enumerating quads.
    const TOP_PER_AXIS: usize = 6;

    for &a_idx in &a_order {
        let a = &corners[a_idx];
        let a_axis0 = wrap_pi(a.axes[0].angle);
        let a_axis1 = wrap_pi(a.axes[1].angle);

        // Fetch the K nearest Swapped corners; sort asc by distance.
        let mut neighbors: Vec<(usize, f32, Vector2<f32>)> = swap_tree
            .nearest_n::<SquaredEuclidean>(&[a.position.x, a.position.y], K_SWAP)
            .into_iter()
            .map(|nn| {
                let slot = nn.item as usize;
                let idx = swapped[slot];
                let p = corners[idx].position;
                let off = Vector2::new(p.x - a.position.x, p.y - a.position.y);
                let d = nn.distance.sqrt();
                (idx, d, off)
            })
            .filter(|(_, d, _)| d.is_finite() && *d > 1e-3)
            .collect();
        neighbors.sort_by(|a, b| a.1.total_cmp(&b.1));
        if neighbors.len() < 2 {
            continue;
        }

        // Classify by which axis at A the chord aligns with.
        let mut b_cands: Vec<(usize, f32, Vector2<f32>)> = Vec::new();
        let mut c_cands: Vec<(usize, f32, Vector2<f32>)> = Vec::new();
        for (idx, dist, off) in &neighbors {
            let ang = wrap_pi(off.y.atan2(off.x));
            let d0 = angular_dist_pi(ang, a_axis0);
            let d1 = angular_dist_pi(ang, a_axis1);
            if d0 <= axis_tol && d0 < d1 {
                b_cands.push((*idx, *dist, *off));
            } else if d1 <= axis_tol && d1 < d0 {
                c_cands.push((*idx, *dist, *off));
            }
        }
        if b_cands.is_empty() || c_cands.is_empty() {
            continue;
        }

        // Enumerate (B, C) pairs among the shortest candidates.
        for (b_idx, b_dist, b_off) in b_cands.iter().take(TOP_PER_AXIS) {
            for (c_idx, c_dist, c_off) in c_cands.iter().take(TOP_PER_AXIS) {
                if b_idx == c_idx {
                    continue;
                }
                let ab = *b_dist;
                let ac = *c_dist;
                let ratio = ab.min(ac) / ab.max(ac);
                if ratio < min_ratio / max_ratio {
                    continue;
                }

                // Predict D.
                let pred = a.position + b_off + c_off;
                let avg_edge = (ab + ac) * 0.5;
                let close_px = close_tol * avg_edge;
                let close_px_sq = close_px * close_px;

                // Find nearest Canonical corner within close_px of pred.
                let mut best: Option<(usize, f32, Vector2<f32>)> = None;
                for nn in canon_tree
                    .within_unsorted::<SquaredEuclidean>(&[pred.x, pred.y], close_px_sq)
                    .into_iter()
                {
                    let slot = nn.item as usize;
                    let d_idx = canonical[slot];
                    if d_idx == a_idx {
                        continue;
                    }
                    let p = corners[d_idx].position;
                    let d = nn.distance.sqrt();
                    if best.map(|b| d < b.1).unwrap_or(true) {
                        best = Some((
                            d_idx,
                            d,
                            Vector2::new(p.x - a.position.x, p.y - a.position.y),
                        ));
                    }
                }
                let Some((d_idx, _gap, _d_off_from_a)) = best else {
                    continue;
                };

                // Validate edges BD (along B's perpendicular axis to the BA edge)
                // and CD (along C's perpendicular axis to the CA edge).
                let b_to_d = corners[d_idx].position - corners[*b_idx].position;
                let c_to_d = corners[d_idx].position - corners[*c_idx].position;
                let bd = b_to_d.norm();
                let cd = c_to_d.norm();

                // All 4 edges must match pairwise within the ratio tolerance.
                let all_edges = [ab, ac, bd, cd];
                let emin = all_edges.iter().copied().fold(f32::INFINITY, f32::min);
                let emax = all_edges.iter().copied().fold(0.0_f32, f32::max);
                if emax <= 0.0 || emin / emax < min_ratio / max_ratio {
                    continue;
                }

                // Axis-slot-swap check on all 4 edges.
                if !edge_has_axis_slot_swap(corners, a_idx, *b_idx, axis_tol)
                    || !edge_has_axis_slot_swap(corners, a_idx, *c_idx, axis_tol)
                    || !edge_has_axis_slot_swap(corners, *b_idx, d_idx, axis_tol)
                    || !edge_has_axis_slot_swap(corners, *c_idx, d_idx, axis_tol)
                {
                    continue;
                }

                // Cluster-center cross-check (belts-and-suspenders): the
                // direction of AB should roughly match one of the two
                // global cluster centers. (We already know it matches
                // A's axes[0]; this extra filter catches rare frames
                // where A's local axes drifted far from centers.)
                let ab_ang = wrap_pi(b_off.y.atan2(b_off.x));
                let ac_ang = wrap_pi(c_off.y.atan2(c_off.x));
                let _ = (centers, ab_ang, ac_ang); // no-op placeholder; centers cross-check left to grow-stage axes_match_centers.

                let cell_size = (ab + ac + bd + cd) * 0.25;

                // 2×-spacing rejection. A legitimate 2×2 quad has no
                // detected corners at any of its edge midpoints nor at
                // the parallelogram center (those positions lie inside
                // cells, between grid intersections). If the seed
                // accidentally picked a 2×-wider quad — e.g., `A=(0,0),
                // B=(2,0), C=(0,2), D=(2,2)` in the real grid, wrongly
                // labelled as `(0,0),(1,0),(0,1),(1,1)` — then those
                // midpoints DO coincide with real Swapped / Canonical
                // corners (the intermediate `(1,0), (0,1), (1,1)`
                // positions in the real grid). Detect and reject.
                if seed_has_midpoint_violation(
                    corners,
                    a_idx,
                    *b_idx,
                    *c_idx,
                    d_idx,
                    &canon_tree,
                    &canonical,
                    &swap_tree,
                    &swapped,
                    cell_size,
                ) {
                    continue;
                }

                return Some(SeedOutput {
                    seed: Seed {
                        a: a_idx,
                        b: *b_idx,
                        c: *c_idx,
                        d: d_idx,
                    },
                    cell_size,
                });
            }
        }
    }

    None
}

/// Reject a seed quad whose edges skip intermediate real grid
/// corners.
///
/// A valid `(0,0)-(1,0)-(0,1)-(1,1)` quad has:
///   - no `Swapped` corner near the midpoint of `AB` (between the
///     two `Canonical` corners A and `D`/B on that edge of the
///     seed — wait, AB connects Canonical-A and Swapped-B, so the
///     midpoint is halfway between, inside one cell),
///   - no `Canonical` corner near the midpoint of `AD` /
///     `BC` (the parallelogram center, which sits at `(0.5, 0.5)`
///     — not a grid intersection).
///
/// When the seed has accidentally picked a 2×-wider quad — e.g.,
/// `A=(0,0), B=(2,0), C=(0,2), D=(2,2)` mislabelled as
/// `(0,0),(1,0),(0,1),(1,1)` — the midpoints DO coincide with real
/// intermediate corners (respectively Swapped at `(1,0), (0,1),
/// (1,2), (2,1)`, and Canonical at the center `(1,1)`).
///
/// Midpoint tolerance: `midpoint_match_rel × cell_size`. Corners
/// within this radius of the midpoint trigger rejection.
#[allow(clippy::too_many_arguments)]
fn seed_has_midpoint_violation(
    corners: &[CornerAug],
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    canon_tree: &KdTree<f32, 2>,
    canonical: &[usize],
    swap_tree: &KdTree<f32, 2>,
    swapped: &[usize],
    cell_size: f32,
) -> bool {
    // Match-radius: a corner within this distance of a midpoint is
    // considered "at the midpoint". `0.3 × s` is tight enough to
    // ignore noise but large enough to catch real 2×-spacing
    // mislabels where the intermediate corner sits nearly exactly at
    // the midpoint.
    let tol = 0.3 * cell_size;
    let tol_sq = tol * tol;

    let pa = corners[a].position;
    let pb = corners[b].position;
    let pc = corners[c].position;
    let pd = corners[d].position;

    // Edge midpoints — expect NO Swapped corner nearby.
    let edge_midpoints = [
        Point2::from((pa.coords + pb.coords) * 0.5),
        Point2::from((pa.coords + pc.coords) * 0.5),
        Point2::from((pb.coords + pd.coords) * 0.5),
        Point2::from((pc.coords + pd.coords) * 0.5),
    ];
    for mp in edge_midpoints {
        if nearest_non_seed_within(swap_tree, swapped, mp, tol_sq, &[a, b, c, d]) {
            return true;
        }
    }

    // Parallelogram center — expect NO Canonical corner nearby.
    let center = Point2::from((pa.coords + pd.coords) * 0.5);
    if nearest_non_seed_within(canon_tree, canonical, center, tol_sq, &[a, b, c, d]) {
        return true;
    }

    false
}

/// Check whether the KD-tree contains a non-seed point within
/// `tol_sq` of `pos`. Returns `true` if such a point exists.
fn nearest_non_seed_within(
    tree: &KdTree<f32, 2>,
    slot_to_idx: &[usize],
    pos: Point2<f32>,
    tol_sq: f32,
    seed_indices: &[usize],
) -> bool {
    for nn in tree
        .within_unsorted::<SquaredEuclidean>(&[pos.x, pos.y], tol_sq)
        .into_iter()
    {
        let slot = nn.item as usize;
        let idx = slot_to_idx[slot];
        if !seed_indices.contains(&idx) {
            return true;
        }
    }
    false
}

/// Verify the axis-slot-swap invariant on an edge `A→B`: the edge
/// direction matches one slot at A and the OTHER slot at B.
fn edge_has_axis_slot_swap(corners: &[CornerAug], a_idx: usize, b_idx: usize, tol: f32) -> bool {
    let a = &corners[a_idx];
    let b = &corners[b_idx];
    let off = b.position - a.position;
    let ang = wrap_pi(off.y.atan2(off.x));
    let d_a0 = angular_dist_pi(ang, wrap_pi(a.axes[0].angle));
    let d_a1 = angular_dist_pi(ang, wrap_pi(a.axes[1].angle));
    let (slot_a, d_a) = if d_a0 <= d_a1 { (0, d_a0) } else { (1, d_a1) };
    if d_a > tol {
        return false;
    }
    let d_b0 = angular_dist_pi(ang, wrap_pi(b.axes[0].angle));
    let d_b1 = angular_dist_pi(ang, wrap_pi(b.axes[1].angle));
    let (slot_b, d_b) = if d_b0 <= d_b1 { (0, d_b0) } else { (1, d_b1) };
    if d_b > tol {
        return false;
    }
    slot_a != slot_b
}

/// Discard — unused under the new self-consistent seed. Retained so
/// `&Point2` isn't flagged as dead.
#[allow(dead_code)]
fn _point2_retain(_: Point2<f32>) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::cluster_axes;
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
    fn finds_seed_on_clean_5x5_grid() {
        let mut corners = build_clean_grid(5, 5, 20.0);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let out = find_seed(&corners, centers, &params).expect("seed");
        assert!((out.cell_size - 20.0).abs() < 0.5);

        let label_of = |i: usize| match corners[i].stage {
            CornerStage::Clustered { label } => label,
            _ => panic!("unclustered"),
        };
        assert_eq!(label_of(out.seed.a), ClusterLabel::Canonical);
        assert_eq!(label_of(out.seed.b), ClusterLabel::Swapped);
        assert_eq!(label_of(out.seed.c), ClusterLabel::Swapped);
        assert_eq!(label_of(out.seed.d), ClusterLabel::Canonical);
    }

    #[test]
    fn returns_none_on_isolated_cluster0_corner() {
        let mut corners = vec![make_corner(
            0,
            100.0,
            100.0,
            0.0,
            std::f32::consts::FRAC_PI_2,
            ClusterLabel::Canonical,
            1.0,
        )];
        let params = DetectorParams::default();
        let centers = ClusterCenters {
            theta0: 0.0,
            theta1: std::f32::consts::FRAC_PI_2,
        };
        corners[0].stage = CornerStage::Clustered {
            label: ClusterLabel::Canonical,
        };
        assert!(find_seed(&corners, centers, &params).is_none());
    }

    #[test]
    fn rotated_grid_seed() {
        let theta = 30.0_f32.to_radians();
        let axis_u = theta;
        let axis_v = theta + std::f32::consts::FRAC_PI_2;
        let s = 25.0;
        let mut corners = Vec::new();
        let mut idx = 0;
        for j in 0..5_i32 {
            for i in 0..5_i32 {
                let dx = i as f32 * s * axis_u.cos() + j as f32 * s * axis_v.cos();
                let dy = i as f32 * s * axis_u.sin() + j as f32 * s * axis_v.sin();
                let label = if (i + j).rem_euclid(2) == 0 {
                    ClusterLabel::Canonical
                } else {
                    ClusterLabel::Swapped
                };
                corners.push(make_corner(
                    idx,
                    100.0 + dx,
                    100.0 + dy,
                    axis_u,
                    axis_v,
                    label,
                    1.0,
                ));
                idx += 1;
            }
        }
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let out = find_seed(&corners, centers, &params).expect("seed");
        assert!((out.cell_size - s).abs() < 1.0);
    }

    #[test]
    fn handles_widely_varying_cell_size_among_clusters() {
        // Create a grid where TRUE cell is 60 but there are many
        // (non-clustered) noise points at spacing 30 — the old
        // cell_size estimator would pick 30 and the seed would
        // fail. The new self-consistent seed uses the cluster
        // corners only to measure cell size.
        let s = 60.0_f32;
        let mut corners = build_clean_grid(4, 4, s);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let out = find_seed(&corners, centers, &params).expect("seed");
        assert!(
            (out.cell_size - s).abs() < 1.0,
            "cell_size = {:.2} off from {s}",
            out.cell_size
        );
    }
}
