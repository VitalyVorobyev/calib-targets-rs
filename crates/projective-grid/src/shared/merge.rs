//! Local-geometry-only component merge.
//!
//! The topological pipeline can leave multiple disconnected grid
//! components when a board is partially occluded, when a line of
//! corners drops below the strength threshold, or when topological
//! filtering removes a noisy quad in the middle of the board. This
//! module attempts to reunite components in label space.
//!
//! The merge is lattice-parameterized: [`merge_components_local`] uses the
//! square symmetry group (D4) for byte-compatibility with the square facades;
//! [`merge_components_local_for`] takes a [`LatticeKind`] and uses its symmetry
//! group (D6 for hex — a hex relabelling has 12 automorphisms).
//!
//! # Acceptance criterion
//!
//! Local geometry only — never a global homography fit. Strong radial
//! distortion can break a single global homography across the whole
//! board, so we score component pairs purely from agreement between
//! corners that should coincide after a candidate alignment:
//!
//! - **Per-component cell size** (median nearest-neighbour distance
//!   along the component's `i` and `j` axes) must agree within
//!   `cell_size_ratio_tol`.
//! - **Per-corner positions** of overlapping labels must agree within
//!   `position_tol_rel * mean_cell_size` pixels.
//! - **Overlap count** must reach `min_overlap`.
//!
//! Component reorientation uses the symmetry group of the lattice (the eight
//! elements of D4 for square, the twelve of D6 for hex). The translation is
//! fixed by an anchor-pair correspondence; we try every anchor pair from each
//! component to find the best alignment.
//!
//! # Out-of-scope (v1)
//!
//! Disjoint label sets with no overlap. Such pairs are common when an
//! entire row of corners is missing. The current implementation rejects
//! them; extend by adding a "predict-next-corner" check that compares
//! one component's predicted boundary position to the other's actual
//! boundary corner.

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

use crate::lattice::{Coord, GridTransform, LatticeKind, D4_TRANSFORMS};

// Preserve the historical square-facade tie-break order while deriving every
// transform from the canonical lattice table.
const GRID_TRANSFORMS_D4: [GridTransform; 8] = [
    D4_TRANSFORMS[0],
    D4_TRANSFORMS[3],
    D4_TRANSFORMS[2],
    D4_TRANSFORMS[1],
    D4_TRANSFORMS[4],
    D4_TRANSFORMS[5],
    D4_TRANSFORMS[6],
    D4_TRANSFORMS[7],
];

/// Tuning knobs for [`merge_components_local`].
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LocalMergeParams {
    /// Position tolerance for accepting two corners as the same physical
    /// point, expressed as a fraction of the mean per-component cell
    /// size in pixels. Default: `0.20`.
    pub position_tol_rel: f32,
    /// Cell-size agreement tolerance: `|s_p - s_q| / max(s_p, s_q)` must
    /// be ≤ this value to even attempt a merge. Default: `0.20`.
    pub cell_size_ratio_tol: f32,
    /// Minimum number of overlapping labels (after candidate alignment)
    /// for a merge to be accepted. Default: `2`.
    pub min_overlap: usize,
    /// Upper bound on returned components after merging. Default: `4`.
    pub max_components: usize,
}

impl Default for LocalMergeParams {
    fn default() -> Self {
        Self {
            position_tol_rel: 0.20,
            cell_size_ratio_tol: 0.20,
            min_overlap: 2,
            max_components: 4,
        }
    }
}

/// Output of [`merge_components_local`].
#[derive(Clone, Debug, Default)]
pub struct ComponentMergeResult {
    /// One labelling per surviving component. Each is rebased to start
    /// at `(0, 0)`. Corners in the input may appear in multiple
    /// components if alignment was ambiguous.
    pub components: Vec<HashMap<(i32, i32), usize>>,
    /// Counters describing how many components were merged.
    pub diagnostics: ComponentMergeStats,
}

/// Diagnostics for a single merge call.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct ComponentMergeStats {
    /// Number of components supplied to the merge.
    pub components_in: usize,
    /// Number of components remaining after merging.
    pub components_out: usize,
    /// Number of pairwise merges that passed the geometry gate.
    pub merges_accepted: usize,
}

fn euclidean(p: Point2<f32>, q: Point2<f32>) -> f32 {
    ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt()
}

/// Median nearest-neighbour cell size along grid axes (i and j directions).
/// Falls back to 0.0 if the component has fewer than two corners.
fn estimate_cell_size(labelled: &HashMap<(i32, i32), usize>, positions: &[Point2<f32>]) -> f32 {
    let mut dists: Vec<f32> = Vec::new();
    for (&(i, j), &idx) in labelled.iter() {
        let p = positions[idx];
        if let Some(&right) = labelled.get(&(i + 1, j)) {
            dists.push(euclidean(p, positions[right]));
        }
        if let Some(&down) = labelled.get(&(i, j + 1)) {
            dists.push(euclidean(p, positions[down]));
        }
    }
    if dists.is_empty() {
        return 0.0;
    }
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    dists[dists.len() / 2]
}

/// Apply D4 transform to label coordinates.
#[inline]
fn apply_transform(t: GridTransform, ij: (i32, i32)) -> (i32, i32) {
    let v = t.apply(Coord::new(ij.0, ij.1));
    (v.u, v.v)
}

/// For a candidate `(transform, delta)`, score the alignment by full
/// label-space overlap.
///
/// Counts every `p` label whose `transform · ij_p + delta` exists as a
/// key in `labelled_q` (regardless of pixel distance), and tracks the
/// worst pixel-position disagreement among those overlapping label
/// pairs. The histogram-based candidate enumeration in
/// [`find_best_alignment`] only sees pairs already within `pos_tol`, so
/// without this re-scoring an alignment whose label-space overlap
/// includes one or more pairs *outside* `pos_tol` would silently merge.
/// That would corrupt downstream calibration. Use this re-scoring as
/// the precision gate before accepting a candidate.
fn score_alignment(
    labelled_p: &HashMap<(i32, i32), usize>,
    labelled_q: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
    t: GridTransform,
    delta: (i32, i32),
) -> (usize, f32) {
    let mut overlap = 0usize;
    let mut max_err = 0.0f32;
    for (&ij_p, &idx_p) in labelled_p.iter() {
        let ij_t = apply_transform(t, ij_p);
        let ij_q = (ij_t.0 + delta.0, ij_t.1 + delta.1);
        if let Some(&idx_q) = labelled_q.get(&ij_q) {
            let err = euclidean(positions[idx_p], positions[idx_q]);
            overlap += 1;
            if err > max_err {
                max_err = err;
            }
        }
    }
    (overlap, max_err)
}

/// Find the best (transform, offset) for merging `p` into `q`'s frame.
///
/// Two-pass strategy:
///
/// 1. **Hough enumeration.** Index `q`'s positions in a KD-tree, then
///    for each label in `p` find every `q` label whose pixel
///    position is within `pos_tol` and vote each match into a histogram
///    bin keyed by the candidate `(transform, label-delta)`. This
///    surfaces a small set of candidate alignments in `O(P log Q)`,
///    replacing the previous `O(P² Q)` anchor enumeration.
/// 2. **Full-overlap re-scoring.** Each surviving candidate is
///    re-scored by [`score_alignment`] over the *full* label-space
///    overlap (every `p` label whose `transform · ij_p + delta` is a
///    key in `labelled_q`, regardless of pixel distance). The
///    candidate is accepted only when the re-scored overlap meets
///    `min_overlap` AND the re-scored `max_err` is within `pos_tol`.
///    This is the precision gate: a histogram bin can pass with
///    `min_overlap` position-close inliers even when other label-space
///    overlaps under the same alignment sit far above tolerance, and
///    accepting such an alignment would corrupt downstream calibration.
///    Re-scoring catches that case.
///
/// The accepted candidate set is then ranked by
/// `(overlap_full desc, max_err_full asc, transform_index asc,
/// delta asc)` — a strict total order that matches the original
/// algorithm's tiebreaker (which preferred identity by D4 iteration
/// order).
fn find_best_alignment(
    labelled_p: &HashMap<(i32, i32), usize>,
    labelled_q: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
    cell_size: f32,
    params: &LocalMergeParams,
    transforms: &[GridTransform],
) -> Option<(GridTransform, (i32, i32), usize)> {
    let pos_tol = params.position_tol_rel * cell_size.max(1.0);
    let pos_tol_sq = pos_tol * pos_tol;

    // KD-tree over c_q label positions. The slot index maps back to
    // q_entries[slot] = (ij_q, idx_q).
    let q_entries: Vec<((i32, i32), usize)> = labelled_q.iter().map(|(k, v)| (*k, *v)).collect();
    if q_entries.is_empty() {
        return None;
    }
    let mut tree: KdTree<f32, 2> = KdTree::new();
    for (slot, (_, idx)) in q_entries.iter().enumerate() {
        let pos = positions[*idx];
        tree.add(&[pos.x, pos.y], slot as u64);
    }

    // Pass 1: Hough enumeration. The bin counts position-close votes
    // only — that's a *lower bound* on the full label-space overlap.
    let mut hist: HashMap<(u8, i32, i32), usize> = HashMap::new();
    for (&ij_p, &idx_p) in labelled_p.iter() {
        let pos_p = positions[idx_p];
        for nn in tree
            .within_unsorted::<SquaredEuclidean>(&[pos_p.x, pos_p.y], pos_tol_sq)
            .into_iter()
        {
            let slot = nn.item as usize;
            let (ij_q, _idx_q) = q_entries[slot];
            for (t_idx, t) in transforms.iter().enumerate() {
                let tij_p = apply_transform(*t, ij_p);
                let key = (t_idx as u8, ij_q.0 - tij_p.0, ij_q.1 - tij_p.1);
                *hist.entry(key).or_insert(0usize) += 1;
            }
        }
    }

    // Pass 2: re-score each candidate over the full label-space
    // overlap. A bin survives only when every `c_p` label that maps
    // (under this t/δ) to a key in `c_q.labelled` is within `pos_tol`
    // — see `score_alignment` for the precision contract.
    //
    // Tiebreaker: prefer higher overlap, then lower max_err, then
    // smaller transform index (identity = 0, so identity wins ties),
    // then lexicographic delta — matching the original algorithm's
    // iteration order on highly symmetric synthetic test grids.
    let mut best: Option<(u8, (i32, i32), usize, f32)> = None;
    for (&(t_idx, di, dj), &kdtree_overlap) in &hist {
        if kdtree_overlap < params.min_overlap {
            // Histogram is a lower bound on the full overlap, but only
            // for pairs already within `pos_tol`. A bin that fails the
            // KD-tree-overlap floor cannot reach `min_overlap`
            // position-close pairs and is rejected outright; we don't
            // even bother re-scoring.
            continue;
        }
        let t = transforms[t_idx as usize];
        let delta = (di, dj);
        let (overlap_full, max_err_full) =
            score_alignment(labelled_p, labelled_q, positions, t, delta);
        if overlap_full < params.min_overlap || max_err_full > pos_tol {
            continue;
        }
        let take = match &best {
            None => true,
            Some((best_t_idx, best_delta, best_overlap, best_err)) => {
                if overlap_full != *best_overlap {
                    overlap_full > *best_overlap
                } else if (max_err_full - *best_err).abs() > f32::EPSILON {
                    max_err_full < *best_err
                } else if t_idx != *best_t_idx {
                    t_idx < *best_t_idx
                } else {
                    (di, dj) < *best_delta
                }
            }
        };
        if take {
            best = Some((t_idx, (di, dj), overlap_full, max_err_full));
        }
    }
    best.map(|(t_idx, d, n, _)| (transforms[t_idx as usize], d, n))
}

fn rebase(labelled: &mut HashMap<(i32, i32), usize>) {
    if labelled.is_empty() {
        return;
    }
    let min_i = labelled.keys().map(|(i, _)| *i).min().unwrap();
    let min_j = labelled.keys().map(|(_, j)| *j).min().unwrap();
    if min_i == 0 && min_j == 0 {
        return;
    }
    let rebased: HashMap<(i32, i32), usize> = labelled
        .drain()
        .map(|((i, j), v)| ((i - min_i, j - min_j), v))
        .collect();
    *labelled = rebased;
}

/// Greedy local merge.
///
/// Strategy: estimate each component's cell size, then for every pair
/// `(p, q)` (largest-first by labelled count), search for an
/// alignment that satisfies the cell-size, overlap, and position
/// tolerances. On success, rewrite `p`'s labels into `q`'s frame and
/// merge into `q`. Repeat until no further merges are possible or the
/// `max_components` cap is reached.
///
/// Every component labels corners out of the **same** `positions` slice: a
/// component maps `(i, j) → index`, and that index means the same corner in
/// every component. That shared index space is what lets the merge keep the
/// labelling injective in both directions — one coordinate holds one corner,
/// and one corner sits at one coordinate. Components that each carried their
/// own index space could not be checked that way, so the two are not separable
/// and the signature does not offer them separately.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_components = components.len()),
    )
)]
pub fn merge_components_local(
    components: &[HashMap<(i32, i32), usize>],
    positions: &[Point2<f32>],
    params: &LocalMergeParams,
) -> ComponentMergeResult {
    // Default to the square symmetry group, preserving the historical
    // byte-identical behaviour for the square callers (topological + seed-and-
    // grow facades). The lattice-parameterized variant is
    // [`merge_components_local_for`].
    merge_components_local_with_transforms(components, positions, params, &GRID_TRANSFORMS_D4)
}

/// Lattice-parameterized [`merge_components_local`]: reunite components under
/// the symmetry group of `lattice` (D4 for square, D6 for hex). The hex path
/// uses this so the 12 D6 relabellings of a hex component are all candidate
/// alignments.
pub fn merge_components_local_for(
    components: &[HashMap<(i32, i32), usize>],
    positions: &[Point2<f32>],
    params: &LocalMergeParams,
    lattice: LatticeKind,
) -> ComponentMergeResult {
    merge_components_local_with_transforms(
        components,
        positions,
        params,
        lattice.symmetry_transforms(),
    )
}

fn merge_components_local_with_transforms(
    components: &[HashMap<(i32, i32), usize>],
    positions: &[Point2<f32>],
    params: &LocalMergeParams,
    transforms: &[GridTransform],
) -> ComponentMergeResult {
    let mut stats = ComponentMergeStats {
        components_in: components.len(),
        ..Default::default()
    };
    if components.is_empty() {
        return ComponentMergeResult {
            components: Vec::new(),
            diagnostics: stats,
        };
    }

    // Working copies.
    let mut working: Vec<HashMap<(i32, i32), usize>> = components.to_vec();
    let mut cell_sizes: Vec<f32> = components
        .iter()
        .map(|labelled| estimate_cell_size(labelled, positions))
        .collect();

    let mut alive: Vec<bool> = vec![true; components.len()];
    let mut changed = true;
    while changed {
        changed = false;
        // Order alive components by size descending; bigger anchors are
        // more reliable.
        let mut order: Vec<usize> = (0..components.len()).filter(|i| alive[*i]).collect();
        order.sort_by(|a, b| working[*b].len().cmp(&working[*a].len()));

        'outer: for &i in &order {
            for &j in &order {
                if i == j || !alive[i] || !alive[j] {
                    continue;
                }
                // Cell-size sanity gate.
                let s_i = cell_sizes[i].max(1e-3);
                let s_j = cell_sizes[j].max(1e-3);
                let ratio = (s_i - s_j).abs() / s_i.max(s_j);
                if ratio > params.cell_size_ratio_tol {
                    continue;
                }
                let cell_size = 0.5 * (s_i + s_j);
                let Some((t, delta, _overlap)) = find_best_alignment(
                    &working[i],
                    &working[j],
                    positions,
                    cell_size,
                    params,
                    transforms,
                ) else {
                    continue;
                };
                // Merge i into j (the larger component is j by ordering).
                // For each label in i, transform to j's frame and insert it —
                // subject to *both* directions of the labelling's injectivity:
                //
                // - the destination coordinate must be free (`or_insert` keeps
                //   j's value on an i↔j coordinate collision), and
                // - the source corner must not already be labelled elsewhere in
                //   j. Components split by the walk can share *vertices* (the
                //   split is on shared quad edges), so without this an overlap
                //   as thin as `min_overlap` could re-label one physical corner
                //   at a run of lattice coordinates. That is a labelled set no
                //   homography admits, and it is unrecoverable for the consumer
                //   — a gap is honest, a duplicate is actively harmful.
                //
                // `i` is killed immediately below (`alive[i] = false`) and its
                // map is never read again — the final collection filters dead
                // components — so move it out with `mem::take` instead of
                // cloning. The result is independent of the order i's pairs are
                // drained: i's keys are unique within its own map, `or_insert`
                // resolves coordinate collisions in j's favour regardless of
                // order, and `claimed` is seeded from j before the drain begins
                // so a source corner already in j is rejected no matter when it
                // is visited.
                let mut claimed: HashSet<usize> = working[j].values().copied().collect();
                for (ij, idx_i) in std::mem::take(&mut working[i]) {
                    if claimed.contains(&idx_i) {
                        continue;
                    }
                    let tij = apply_transform(t, ij);
                    let key = (tij.0 + delta.0, tij.1 + delta.1);
                    if let Entry::Vacant(slot) = working[j].entry(key) {
                        slot.insert(idx_i);
                        claimed.insert(idx_i);
                    }
                }
                alive[i] = false;
                cell_sizes[j] = 0.5 * (cell_sizes[i] + cell_sizes[j]);
                stats.merges_accepted += 1;
                changed = true;
                continue 'outer;
            }
        }
    }

    let mut out: Vec<HashMap<(i32, i32), usize>> = working
        .into_iter()
        .zip(alive.iter().copied())
        .filter_map(|(m, a)| if a { Some(m) } else { None })
        .collect();
    // Sort by size desc, cap, rebase.
    out.sort_by_key(|m| std::cmp::Reverse(m.len()));
    out.truncate(params.max_components);
    for m in &mut out {
        rebase(m);
    }
    stats.components_out = out.len();
    ComponentMergeResult {
        components: out,
        diagnostics: stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Labels = HashMap<(i32, i32), usize>;
    type Positions = Vec<Point2<f32>>;

    fn component_5x5() -> (Labels, Positions) {
        let mut labelled = HashMap::new();
        let mut positions = Vec::new();
        for j in 0..5 {
            for i in 0..5 {
                let idx = positions.len();
                labelled.insert((i, j), idx);
                positions.push(Point2::new(i as f32 * 10.0, j as f32 * 10.0));
            }
        }
        (labelled, positions)
    }

    #[test]
    fn identical_components_merge_into_one() {
        // Both components label the *same* corners — same indices, as the walk
        // produces when one mesh is reachable two ways.
        let (labels, positions) = component_5x5();
        let res = merge_components_local(
            &[labels.clone(), labels],
            &positions,
            &LocalMergeParams::default(),
        );
        assert_eq!(res.components.len(), 1);
        assert_eq!(res.components[0].len(), 25);
        assert_eq!(res.diagnostics.merges_accepted, 1);
    }

    #[test]
    fn shifted_components_with_overlap_merge() {
        // C1: labels (0..3, 0..5) at world (0..2, 0..4) * step
        // C2: labels (0..3, 0..5) at world (3..5, 0..4) * step
        // Overlap if we offset C2 by (2, 0): C1 cell (2, j) coincides with C2 cell (0, j) world-wise.
        let step = 10.0;
        // One 5x5 world grid; both components index into it, so the shared
        // column carries the same corner index in each — exactly what the walk
        // hands the merge.
        let mut positions = Vec::new();
        let mut world = HashMap::new();
        for j in 0..5 {
            for i in 0..5 {
                world.insert((i, j), positions.len());
                positions.push(Point2::new(i as f32 * step, j as f32 * step));
            }
        }
        let mut l1 = HashMap::new();
        let mut l2 = HashMap::new();
        for j in 0..5 {
            for i in 0..3 {
                l1.insert((i, j), world[&(i, j)]);
                l2.insert((i, j), world[&(i + 2, j)]);
            }
        }
        let res = merge_components_local(&[l1, l2], &positions, &LocalMergeParams::default());
        assert_eq!(res.components.len(), 1);
        // Combined unique labels: (0..5, 0..5) = 25.
        assert_eq!(res.components[0].len(), 25);
    }

    #[test]
    fn cell_size_mismatch_blocks_merge() {
        let (l1, mut positions) = component_5x5();
        // A second, disjoint set of corners at twice the pitch — cell size
        // differs by 2x, so the sanity gate must refuse the merge.
        let mut l2 = HashMap::new();
        for j in 0..5 {
            for i in 0..5 {
                l2.insert((i, j), positions.len());
                positions.push(Point2::new(i as f32 * 20.0, j as f32 * 20.0));
            }
        }
        let res = merge_components_local(&[l1, l2], &positions, &LocalMergeParams::default());
        assert_eq!(res.components.len(), 2);
        assert_eq!(res.diagnostics.merges_accepted, 0);
    }

    /// Regression for the precision contract: a histogram bin can pass
    /// `min_overlap` on position-close votes alone while another
    /// label-aligned pair under the same `(transform, delta)` sits far
    /// outside `pos_tol`. Without the full-overlap re-score, the merge
    /// would proceed and corrupt the grid labelling.
    ///
    /// Setup: two 2×2 components share three corners exactly, but one
    /// corner has drifted ~5× the cell size in `c_q`. The histogram
    /// counts three position-close votes for `(identity, (0, 0))` —
    /// enough to clear `min_overlap = 2`. The full label-space
    /// overlap is four with `max_err ≈ 56 px`, which the precision
    /// gate must reject.
    #[test]
    fn drifted_overlapping_corner_blocks_merge() {
        let cell = 10.0_f32;
        let mut positions: Positions = Vec::new();
        // C1: 4 labels on the unit cell, exact positions.
        let mut l1: Labels = HashMap::new();
        for j in 0..2 {
            for i in 0..2 {
                l1.insert((i, j), positions.len());
                positions.push(Point2::new(i as f32 * cell, j as f32 * cell));
            }
        }
        // C2: a distinct set of corners at the same labels, but its (1, 1)
        // corner sits at (50, 50) — far outside `pos_tol = 0.20 x cell = 2.0`
        // from C1's (10, 10).
        let mut l2: Labels = HashMap::new();
        for j in 0..2 {
            for i in 0..2 {
                l2.insert((i, j), positions.len());
                positions.push(if (i, j) == (1, 1) {
                    Point2::new(50.0, 50.0)
                } else {
                    Point2::new(i as f32 * cell, j as f32 * cell)
                });
            }
        }
        let res = merge_components_local(&[l1, l2], &positions, &LocalMergeParams::default());
        assert_eq!(
            res.components.len(),
            2,
            "drifted corner should block the merge entirely"
        );
        assert_eq!(res.diagnostics.merges_accepted, 0);
    }

    // --- Hex (D6) merge -------------------------------------------------

    fn hex_model(q: i32, r: i32) -> Point2<f32> {
        let sqrt3_2 = 3.0_f32.sqrt() * 0.5;
        Point2::new(q as f32 + 0.5 * r as f32, sqrt3_2 * r as f32)
    }

    /// Build a hex axial patch (radius `radius`) at pixel `scale`, with an axial
    /// relabelling applied by `relabel` (a D6 element index 0..12) so the merge
    /// must undo the automorphism. Positions are in model pixels regardless of
    /// the relabelling (the physical points are the same).
    fn hex_component(radius: i32, scale: f32, relabel: usize) -> (Labels, Positions) {
        let t = crate::lattice::D6_TRANSFORMS[relabel];
        let mut labelled = HashMap::new();
        let mut positions = Vec::new();
        for q in -radius..=radius {
            for r in (-radius).max(-q - radius)..=radius.min(-q + radius) {
                let idx = positions.len();
                let m = hex_model(q, r);
                positions.push(Point2::new(m.x * scale, m.y * scale));
                let c = t.apply(Coord::new(q, r));
                labelled.insert((c.u, c.v), idx);
            }
        }
        (labelled, positions)
    }

    #[test]
    fn hex_identical_components_merge_under_d6() {
        // Two copies of the same hex patch, the second relabelled by a
        // non-identity D6 element. The D6-aware merge must reunite them into
        // one component (the D4-only merge would not find the alignment).
        let (l1, positions) = hex_component(2, 14.0, 0);
        let (l2, _) = hex_component(2, 14.0, 4); // 120 deg rotation, same corners
        let res = merge_components_local_for(
            &[l1.clone(), l2],
            &positions,
            &LocalMergeParams::default(),
            LatticeKind::Hex,
        );
        assert_eq!(
            res.components.len(),
            1,
            "D6 merge should reunite the relabelled hex copies"
        );
        assert_eq!(res.components[0].len(), l1.len());
        assert_eq!(res.diagnostics.merges_accepted, 1);
    }

    #[test]
    fn hex_relabelled_copy_merges_for_every_d6_element() {
        // For every D6 automorphism, a relabelled copy must still merge — the
        // 12-element symmetry group is fully exercised.
        for relabel in 0..crate::lattice::D6_TRANSFORMS.len() {
            let (l1, positions) = hex_component(2, 16.0, 0);
            let (l2, _) = hex_component(2, 16.0, relabel);
            let res = merge_components_local_for(
                &[l1, l2],
                &positions,
                &LocalMergeParams::default(),
                LatticeKind::Hex,
            );
            assert_eq!(
                res.components.len(),
                1,
                "D6 element {relabel} failed to merge"
            );
        }
    }
}
