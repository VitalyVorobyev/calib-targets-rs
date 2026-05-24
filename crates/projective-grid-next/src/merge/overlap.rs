//! Overlapping-label-set component merge (current legacy behaviour, preserved).
//!
//! For two components `A` and `B` and a candidate symmetry transform `t`,
//! the overlap merger searches for an integer offset `delta` such that
//! `t · ij_A + delta` maps onto `ij_B` for at least `min_overlap` of `A`'s
//! labels. The Hough-style histogram phase from the legacy crate is preserved
//! verbatim; the full-overlap re-score is the precision gate that catches
//! drifted-corner positives the histogram would silently admit (see the
//! legacy `drifted_overlapping_corner_blocks_merge` regression test).

use std::collections::HashMap;

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::{ComplexField, Point2};

use crate::float::Float;
use crate::lattice::{Coord, GridTransform};

use super::{ComponentInput, MergeCandidate, MergeParams};

/// Find a `(transform, delta)` such that mapping `a`'s labels onto `b`'s
/// frame yields at least `params.min_overlap` matching labels with all
/// position residuals within `params.position_residual_max_rel * cell_size`.
///
/// Returns `None` when no such alignment exists.
pub(super) fn find_overlap_merge<F>(
    a: &ComponentInput<'_, F>,
    b: &ComponentInput<'_, F>,
    params: &MergeParams<F>,
) -> Option<MergeCandidate<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let cell_size = (a.cell_size + b.cell_size) * <F as From<f32>>::from(0.5_f32);
    let pos_tol = params.position_residual_max_rel * cell_size;
    let pos_tol_sq = pos_tol * pos_tol;

    // KD-tree over b's positions.
    let b_entries: Vec<(Coord, usize)> = b.labels.iter().map(|(k, v)| (*k, *v)).collect();
    if b_entries.is_empty() {
        return None;
    }
    let mut tree: KdTree<F, 2> = KdTree::new();
    for (slot, (_, idx)) in b_entries.iter().enumerate() {
        let pos = b.positions[*idx];
        tree.add(&[pos.x, pos.y], slot as u64);
    }

    // Hough enumeration. Position-close votes only; the histogram is a lower
    // bound on the full label-space overlap.
    let mut hist: HashMap<(usize, i32, i32), usize> = HashMap::new();
    for (&ij_a, &idx_a) in a.labels.iter() {
        let pos_a = a.positions[idx_a];
        for nn in tree
            .within_unsorted::<SquaredEuclidean>(&[pos_a.x, pos_a.y], pos_tol_sq)
            .into_iter()
        {
            let slot = nn.item as usize;
            let (ij_b, _) = b_entries[slot];
            for (t_idx, t) in params.symmetry.iter().enumerate() {
                let tij = t.apply(ij_a);
                let key = (t_idx, ij_b.0 - tij.0, ij_b.1 - tij.1);
                *hist.entry(key).or_insert(0) += 1;
            }
        }
    }

    // Re-score each bin over the full label-space overlap.
    let mut best: Option<BestAlignment<F>> = None;
    for (&(t_idx, di, dj), &kd_overlap) in &hist {
        if kd_overlap < params.min_overlap {
            continue;
        }
        let t = params.symmetry[t_idx];
        let delta = (di, dj);
        let (overlap_full, max_err_full) = score_alignment(a, b, t, delta);
        if overlap_full < params.min_overlap || max_err_full > pos_tol {
            continue;
        }
        let take = match &best {
            None => true,
            Some(prev) => {
                if overlap_full != prev.overlap {
                    overlap_full > prev.overlap
                } else if ComplexField::abs(max_err_full - prev.max_err) > F::default_epsilon() {
                    max_err_full < prev.max_err
                } else if t_idx != prev.t_idx {
                    t_idx < prev.t_idx
                } else {
                    (di, dj) < prev.delta
                }
            }
        };
        if take {
            best = Some(BestAlignment {
                t_idx,
                delta,
                overlap: overlap_full,
                max_err: max_err_full,
            });
        }
    }
    best.map(|b_align| {
        let t = params.symmetry[b_align.t_idx];
        let merged = apply_merge(a, b, t, b_align.delta);
        MergeCandidate {
            transform: t,
            delta: b_align.delta,
            overlap: b_align.overlap,
            max_residual: b_align.max_err,
            merged_labels: merged,
        }
    })
}

struct BestAlignment<F: Float> {
    t_idx: usize,
    delta: Coord,
    overlap: usize,
    max_err: F,
}

/// Re-score a candidate `(transform, delta)` over the full label-space
/// overlap. Counts every `a` label whose `transform · ij + delta` exists in
/// `b.labels`, regardless of pixel distance, and tracks the worst position
/// disagreement among those pairs.
fn score_alignment<F>(
    a: &ComponentInput<'_, F>,
    b: &ComponentInput<'_, F>,
    t: GridTransform,
    delta: (i32, i32),
) -> (usize, F)
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let mut overlap = 0usize;
    let mut max_err = F::zero();
    for (&ij_a, &idx_a) in a.labels.iter() {
        let tij = t.apply(ij_a);
        let key = (tij.0 + delta.0, tij.1 + delta.1);
        if let Some(&idx_b) = b.labels.get(&key) {
            let pa = a.positions[idx_a];
            let pb = b.positions[idx_b];
            let dx = pa.x - pb.x;
            let dy = pa.y - pb.y;
            let err = (dx * dx + dy * dy).sqrt();
            overlap += 1;
            if err > max_err {
                max_err = err;
            }
        }
    }
    (overlap, max_err)
}

/// Apply a merge: take all `a` labels into `b`'s frame and combine with `b`'s.
fn apply_merge<F: Float>(
    a: &ComponentInput<'_, F>,
    b: &ComponentInput<'_, F>,
    t: GridTransform,
    delta: (i32, i32),
) -> HashMap<Coord, Point2<F>> {
    let mut merged: HashMap<Coord, Point2<F>> = HashMap::new();
    // b's labels first; b owns the merged frame.
    for (&coord, &idx) in b.labels.iter() {
        merged.insert(coord, b.positions[idx]);
    }
    // a's labels mapped into b's frame; on conflict keep b's.
    for (&ij, &idx) in a.labels.iter() {
        let tij = t.apply(ij);
        let key = (tij.0 + delta.0, tij.1 + delta.1);
        merged.entry(key).or_insert(a.positions[idx]);
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lattice::{LatticeKind, D4_TRANSFORMS};
    use crate::merge::MergeMode;

    fn make_component<F: Float>(
        i_range: std::ops::Range<i32>,
        j_range: std::ops::Range<i32>,
        cell: F,
        ox: F,
        oy: F,
    ) -> (HashMap<Coord, usize>, Vec<Point2<F>>) {
        let mut labels = HashMap::new();
        let mut positions = Vec::new();
        for j in j_range {
            for i in i_range.clone() {
                let idx = positions.len();
                labels.insert((i, j), idx);
                positions.push(Point2::new(
                    <F as From<f32>>::from(i as f32) * cell + ox,
                    <F as From<f32>>::from(j as f32) * cell + oy,
                ));
            }
        }
        (labels, positions)
    }

    #[test]
    fn overlap_merges_shifted_identical_grids_f32() {
        type F = f32;
        let s: F = 10.0;
        let (la, pa) = make_component::<F>(0..3, 0..5, s, 0.0, 0.0);
        let (lb, pb) = make_component::<F>(0..3, 0..5, s, 2.0 * s, 0.0);
        let a = ComponentInput {
            positions: &pa,
            labels: la,
            cell_size: s,
        };
        let b = ComponentInput {
            positions: &pb,
            labels: lb,
            cell_size: s,
        };
        let params = MergeParams::<F> {
            symmetry: &D4_TRANSFORMS,
            expected_lattice: LatticeKind::Square,
            mode: MergeMode::OverlapOnly,
            min_overlap: 2,
            position_residual_max_rel: 0.20,
            cell_size_disagreement_max: 0.20,
            max_components: 8,
        };
        let res = find_overlap_merge(&a, &b, &params).expect("expected overlap merge");
        assert!(res.overlap >= 2);
        // Merged labels in b's frame should span columns 0..5 with the
        // shift; we don't validate the exact count here (caller orchestrator
        // rebases), only that the merge produced a labelling.
        assert!(!res.merged_labels.is_empty());
    }
}
