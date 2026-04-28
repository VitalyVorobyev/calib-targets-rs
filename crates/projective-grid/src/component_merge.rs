//! Local-geometry-only component merge for square grids.
//!
//! Both the topological pipeline ([`crate::topological`]) and the
//! chessboard-v2 seed-and-grow pipeline can leave multiple disconnected
//! grid components when a board is partially occluded, when a line of
//! ChESS corners drops below the strength threshold, or when topological
//! filtering removes a noisy quad in the middle of the board. This
//! module attempts to reunite components in label space.
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
//! Component reorientation uses the eight elements of D4
//! ([`crate::GRID_TRANSFORMS_D4`]). The translation is fixed by an
//! anchor-pair correspondence; we try every anchor pair from each
//! component to find the best alignment.
//!
//! # Out-of-scope (v1)
//!
//! Disjoint label sets with no overlap. Such pairs are common when an
//! entire row of corners is missing. The current implementation rejects
//! them; extend by adding a "predict-next-corner" check that compares
//! one component's predicted boundary position to the other's actual
//! boundary corner.

use std::collections::HashMap;

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

use crate::square::alignment::GridTransform;

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

/// Slim view over one component's data for merging.
#[derive(Clone, Copy, Debug)]
pub struct ComponentInput<'a> {
    /// `(i, j) → corner_idx` (indices into `positions`).
    pub labelled: &'a HashMap<(i32, i32), usize>,
    pub positions: &'a [Point2<f32>],
}

/// Output of [`merge_components_local`].
#[derive(Clone, Debug, Default)]
pub struct ComponentMergeResult {
    /// One labelling per surviving component. Each is rebased to start
    /// at `(0, 0)`. Corners in the input may appear in multiple
    /// components if alignment was ambiguous.
    pub components: Vec<HashMap<(i32, i32), usize>>,
    pub diagnostics: ComponentMergeStats,
}

/// Diagnostics for a single merge call.
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct ComponentMergeStats {
    pub components_in: usize,
    pub components_out: usize,
    pub merges_accepted: usize,
}

fn euclidean(p: Point2<f32>, q: Point2<f32>) -> f32 {
    ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt()
}

/// Median nearest-neighbour cell size along grid axes (i and j directions).
/// Falls back to 0.0 if the component has fewer than two corners.
fn estimate_cell_size(c: &ComponentInput<'_>) -> f32 {
    let mut dists: Vec<f32> = Vec::new();
    for (&(i, j), &idx) in c.labelled.iter() {
        let p = c.positions[idx];
        if let Some(&right) = c.labelled.get(&(i + 1, j)) {
            dists.push(euclidean(p, c.positions[right]));
        }
        if let Some(&down) = c.labelled.get(&(i, j + 1)) {
            dists.push(euclidean(p, c.positions[down]));
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
    let v = t.apply(ij.0, ij.1);
    (v.i, v.j)
}

/// Find the best (transform, offset) for merging `c_p` into `c_q`'s frame.
///
/// Strategy: position-based Hough transform on (transform, label-delta).
///
/// For two components to merge, every overlapping label pair must have
/// near-identical pixel positions (since the labels refer to the same
/// physical corner in different label frames). We exploit this by
/// indexing `c_q`'s positions in a KD-tree, then for each label in
/// `c_p` finding all `c_q` labels whose pixel position is within
/// `pos_tol`. Each such pair is a vote for whatever (transform, delta)
/// would map them onto each other. The (t, δ) bin with the most votes
/// is the right alignment.
///
/// This replaces the previous O(P²Q) anchor enumeration with
/// O(P log Q) KD-tree queries, scaling many orders of magnitude better
/// on realistic component sizes (200+ labels per component).
fn find_best_alignment(
    c_p: &ComponentInput<'_>,
    c_q: &ComponentInput<'_>,
    cell_size: f32,
    params: &LocalMergeParams,
) -> Option<(GridTransform, (i32, i32), usize)> {
    let pos_tol = params.position_tol_rel * cell_size.max(1.0);
    let pos_tol_sq = pos_tol * pos_tol;

    // KD-tree over c_q label positions. The slot index maps back to
    // q_entries[slot] = (ij_q, idx_q).
    let q_entries: Vec<((i32, i32), usize)> = c_q.labelled.iter().map(|(k, v)| (*k, *v)).collect();
    if q_entries.is_empty() {
        return None;
    }
    let mut tree: KdTree<f32, 2> = KdTree::new();
    for (slot, (_, idx)) in q_entries.iter().enumerate() {
        let pos = c_q.positions[*idx];
        tree.add(&[pos.x, pos.y], slot as u64);
    }

    // Histogram bin: (transform_index, delta_i, delta_j) →
    // (overlap_count, max_position_error).
    let mut hist: HashMap<(u8, i32, i32), (usize, f32)> = HashMap::new();

    for (&ij_p, &idx_p) in c_p.labelled.iter() {
        let pos_p = c_p.positions[idx_p];
        for nn in tree
            .within_unsorted::<SquaredEuclidean>(&[pos_p.x, pos_p.y], pos_tol_sq)
            .into_iter()
        {
            let slot = nn.item as usize;
            let (ij_q, idx_q) = q_entries[slot];
            let err = euclidean(pos_p, c_q.positions[idx_q]);
            for (t_idx, t) in crate::GRID_TRANSFORMS_D4.iter().enumerate() {
                let tij_p = apply_transform(*t, ij_p);
                let key = (t_idx as u8, ij_q.0 - tij_p.0, ij_q.1 - tij_p.1);
                let entry = hist.entry(key).or_insert((0usize, 0.0f32));
                entry.0 += 1;
                if err > entry.1 {
                    entry.1 = err;
                }
            }
        }
    }

    // Tiebreaker: prefer higher overlap, then lower max_err, then
    // smaller transform index (identity = 0, so identity wins ties),
    // then lexicographic delta. The transform-index tiebreaker
    // matches the original algorithm's iteration order, which
    // implicitly preferred identity when multiple D4 transforms
    // produced valid alignments at the same overlap (e.g. on highly
    // symmetric synthetic test grids).
    let mut best: Option<(u8, (i32, i32), usize, f32)> = None;
    for ((t_idx, di, dj), (overlap, max_err)) in hist.into_iter() {
        if overlap < params.min_overlap {
            continue;
        }
        // The KD-tree query already enforced max_err ≤ pos_tol per
        // contribution, so re-checking max_err is defensive only.
        if max_err > pos_tol {
            continue;
        }
        let take = match &best {
            None => true,
            Some((best_t_idx, best_delta, best_overlap, best_err)) => {
                if overlap != *best_overlap {
                    overlap > *best_overlap
                } else if (max_err - *best_err).abs() > f32::EPSILON {
                    max_err < *best_err
                } else if t_idx != *best_t_idx {
                    t_idx < *best_t_idx
                } else {
                    (di, dj) < *best_delta
                }
            }
        };
        if take {
            best = Some((t_idx, (di, dj), overlap, max_err));
        }
    }
    best.map(|(t_idx, d, n, _)| (crate::GRID_TRANSFORMS_D4[t_idx as usize], d, n))
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
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_components = inputs.len()),
    )
)]
pub fn merge_components_local(
    inputs: &[ComponentInput<'_>],
    params: &LocalMergeParams,
) -> ComponentMergeResult {
    let mut stats = ComponentMergeStats {
        components_in: inputs.len(),
        ..Default::default()
    };
    if inputs.is_empty() {
        return ComponentMergeResult {
            components: Vec::new(),
            diagnostics: stats,
        };
    }

    // Working copies.
    let mut working: Vec<HashMap<(i32, i32), usize>> =
        inputs.iter().map(|c| c.labelled.clone()).collect();
    let positions_per: Vec<&[Point2<f32>]> = inputs.iter().map(|c| c.positions).collect();
    let mut cell_sizes: Vec<f32> = inputs.iter().map(estimate_cell_size).collect();

    let mut alive: Vec<bool> = vec![true; inputs.len()];
    let mut changed = true;
    while changed {
        changed = false;
        // Order alive components by size descending; bigger anchors are
        // more reliable.
        let mut order: Vec<usize> = (0..inputs.len()).filter(|i| alive[*i]).collect();
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
                let c_p = ComponentInput {
                    labelled: &working[i],
                    positions: positions_per[i],
                };
                let c_q = ComponentInput {
                    labelled: &working[j],
                    positions: positions_per[j],
                };
                let Some((t, delta, _overlap)) = find_best_alignment(&c_p, &c_q, cell_size, params)
                else {
                    continue;
                };
                // Merge i into j (the larger component is j by ordering).
                // For each label in i, transform to j's frame, insert if
                // not already present (keeping j's value on conflict).
                for (&ij, &idx_i) in working[i].clone().iter() {
                    let tij = apply_transform(t, ij);
                    let key = (tij.0 + delta.0, tij.1 + delta.1);
                    working[j].entry(key).or_insert(idx_i);
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
        let (l1, p1) = component_5x5();
        let (l2, p2) = component_5x5();
        let inputs = vec![
            ComponentInput {
                labelled: &l1,
                positions: &p1,
            },
            ComponentInput {
                labelled: &l2,
                positions: &p2,
            },
        ];
        let res = merge_components_local(&inputs, &LocalMergeParams::default());
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
        let mut l1 = HashMap::new();
        let mut p1 = Vec::new();
        for j in 0..5 {
            for i in 0..3 {
                let idx = p1.len();
                l1.insert((i, j), idx);
                p1.push(Point2::new(i as f32 * step, j as f32 * step));
            }
        }
        let mut l2 = HashMap::new();
        let mut p2 = Vec::new();
        for j in 0..5 {
            for i in 0..3 {
                let idx = p2.len();
                l2.insert((i, j), idx);
                p2.push(Point2::new((i as f32 + 2.0) * step, j as f32 * step));
            }
        }
        let inputs = vec![
            ComponentInput {
                labelled: &l1,
                positions: &p1,
            },
            ComponentInput {
                labelled: &l2,
                positions: &p2,
            },
        ];
        let res = merge_components_local(&inputs, &LocalMergeParams::default());
        assert_eq!(res.components.len(), 1);
        // Combined unique labels: (0..5, 0..5) = 25.
        assert_eq!(res.components[0].len(), 25);
    }

    #[test]
    fn cell_size_mismatch_blocks_merge() {
        let (l1, p1) = component_5x5();
        // Same labels but positions stretched 2x — cell size differs by 2x.
        let mut l2 = HashMap::new();
        let mut p2 = Vec::new();
        for j in 0..5 {
            for i in 0..5 {
                let idx = p2.len();
                l2.insert((i, j), idx);
                p2.push(Point2::new(i as f32 * 20.0, j as f32 * 20.0));
            }
        }
        let inputs = vec![
            ComponentInput {
                labelled: &l1,
                positions: &p1,
            },
            ComponentInput {
                labelled: &l2,
                positions: &p2,
            },
        ];
        let res = merge_components_local(&inputs, &LocalMergeParams::default());
        assert_eq!(res.components.len(), 2);
        assert_eq!(res.diagnostics.merges_accepted, 0);
    }
}
