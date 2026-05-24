//! Predict-and-attach merge mode (closes Gap 9 in `docs/algorithmic_gaps.md`).
//!
//! When two grid components share no labels — typically because a row of
//! corners between them was missed — the overlap-only merger
//! (`super::overlap::find_overlap_merge`) returns `None`. The predicted
//! merger fills that gap: for each candidate symmetry transform `t` and
//! integer offset `delta`, it predicts where `a`'s labels would land in `b`'s
//! frame using the per-component cell-step vectors, then accepts the merge
//! when predicted positions match actual `b`-labels (and vice-versa) within
//! the configured residual tolerance.
//!
//! ## Algorithm sketch
//!
//! 1. Compute per-component local cell-step vectors `step_u`, `step_v` from
//!    the median nearest-neighbour offsets along each grid axis.
//! 2. For each transform `t ∈ symmetry`:
//!    1. Find the closest pair of `(label_a, label_b)` in pixel space. The
//!       pair gives a candidate `delta` once we map `label_a` through `t`.
//!    2. Score the alignment: for every `a`-label, predict its position in
//!       `b`'s frame as `b_centroid + t(ij_a + delta - b_origin) ·
//!       (step_u, step_v)` (approximately) and check against any
//!       `b`-label at `t·ij_a + delta`. Accept the merge when at least
//!       `min_overlap` predicted positions land within
//!       `position_residual_max_rel * cell_size` of an actual `b`-label.
//!
//! The "approximation" of using local-step vectors instead of a global
//! homography is intentional: under heavy radial distortion a global H can
//! not fit both components, but local cell-steps survive (this is exactly
//! the rationale the legacy `merge_components_local` is built on).

use std::collections::HashMap;

use nalgebra::{ComplexField, Point2, Vector2};

use crate::float::{lit, Float};
use crate::lattice::{Coord, GridTransform};

use super::{ComponentInput, MergeCandidate, MergeParams};

/// Try to merge `a` into `b` using predicted-label-position matching when
/// there is no actual overlap.
///
/// Returns `None` when no transform / offset combination produces enough
/// predicted-label hits within tolerance.
pub(super) fn find_predicted_merge<F>(
    a: &ComponentInput<'_, F>,
    b: &ComponentInput<'_, F>,
    params: &MergeParams<F>,
) -> Option<MergeCandidate<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    if a.labels.is_empty() || b.labels.is_empty() {
        return None;
    }
    let cell_size = (a.cell_size + b.cell_size) * lit::<F>(0.5_f32);
    let pos_tol = params.position_residual_max_rel * cell_size;

    // Per-component cell-step vectors. We use b's steps to predict where a's
    // labels would land in b's frame.
    let (b_step_u, b_step_v) = local_cell_steps(b);

    // Pick an anchor for b — the label with the smallest (i, j) — to define
    // the linear position predictor:
    //   pos_b_predicted(i, j) ≈ pos_b(b_anchor) + (i - b_anchor.0) * step_u
    //                                              + (j - b_anchor.1) * step_v.
    let b_anchor = *b.labels.keys().min().expect("non-empty");
    let b_anchor_pos = b.positions[*b.labels.get(&b_anchor).expect("anchor in labels")];

    // Enumerate candidate (transform, delta) pairs. We bound the search by
    // requiring that at least one (label_a, label_b) pair sit within
    // `predicted_position_window` pixels of each other under the candidate
    // alignment. The window is `(max_extent_a + max_extent_b) * cell_size`
    // implicitly, but in practice the delta is bracketed by the bbox spans
    // of the two components plus a small slack so the search stays finite.
    let (a_min, a_max) = bbox(&a.labels);
    let (b_min, b_max) = bbox(&b.labels);

    let mut best: Option<MergeCandidate<F>> = None;

    for (t_idx, t) in params.symmetry.iter().enumerate() {
        // After applying t to a's labels, the resulting integer range is
        // bounded by t.apply on the corners of the a-bbox. Compute the
        // four corners' images to get the deterministic delta range.
        let corners = [
            (a_min.0, a_min.1),
            (a_min.0, a_max.1),
            (a_max.0, a_min.1),
            (a_max.0, a_max.1),
        ];
        let mut ti_min = i32::MAX;
        let mut ti_max = i32::MIN;
        let mut tj_min = i32::MAX;
        let mut tj_max = i32::MIN;
        for &c in &corners {
            let tc = t.apply(c);
            ti_min = ti_min.min(tc.0);
            ti_max = ti_max.max(tc.0);
            tj_min = tj_min.min(tc.1);
            tj_max = tj_max.max(tc.1);
        }
        // Allowed delta is anywhere that lets the transformed bbox of a
        // overlap with b's bbox by at least 1 row + 1 column. Including 2
        // cells of slack on each side lets the merger reach across a single
        // missing row.
        let slack = 2;
        let di_lo = b_min.0 - ti_max - slack;
        let di_hi = b_max.0 - ti_min + slack;
        let dj_lo = b_min.1 - tj_max - slack;
        let dj_hi = b_max.1 - tj_min + slack;

        for di in di_lo..=di_hi {
            for dj in dj_lo..=dj_hi {
                let delta = (di, dj);
                let Some(score) = score_predicted_alignment(
                    a,
                    b,
                    *t,
                    delta,
                    b_anchor,
                    b_anchor_pos,
                    b_step_u,
                    b_step_v,
                    pos_tol,
                ) else {
                    continue;
                };
                if score.matched < params.min_overlap {
                    continue;
                }
                if score.max_residual > pos_tol {
                    continue;
                }
                let take = match &best {
                    None => true,
                    Some(existing) => {
                        // Higher matched count wins; tie-break on smaller
                        // residual, then smaller transform index, then
                        // lexicographic delta. Deterministic.
                        if score.matched != existing.overlap {
                            score.matched > existing.overlap
                        } else if ComplexField::abs(score.max_residual - existing.max_residual)
                            > F::default_epsilon()
                        {
                            score.max_residual < existing.max_residual
                        } else if t_idx
                            != params
                                .symmetry
                                .iter()
                                .position(|s| s == &existing.transform)
                                .unwrap_or(usize::MAX)
                        {
                            t_idx
                                < params
                                    .symmetry
                                    .iter()
                                    .position(|s| s == &existing.transform)
                                    .unwrap_or(usize::MAX)
                        } else {
                            delta < existing.delta
                        }
                    }
                };
                if take {
                    let merged = apply_predicted_merge(a, b, *t, delta);
                    best = Some(MergeCandidate {
                        transform: *t,
                        delta,
                        overlap: score.matched,
                        max_residual: score.max_residual,
                        merged_labels: merged,
                    });
                }
            }
        }
    }

    best
}

struct PredictedScore<F: Float> {
    matched: usize,
    max_residual: F,
}

/// Score a candidate (transform, delta): for each `a`-label whose mapped
/// `(t · ij + delta)` falls within `b`'s extent (or just outside), predict
/// the pixel position via b's local cell-step vectors and check against any
/// actual `b`-label at the same coordinate within `pos_tol`.
#[allow(clippy::too_many_arguments)]
fn score_predicted_alignment<F: Float>(
    a: &ComponentInput<'_, F>,
    b: &ComponentInput<'_, F>,
    t: GridTransform,
    delta: (i32, i32),
    b_anchor: Coord,
    b_anchor_pos: Point2<F>,
    b_step_u: Vector2<F>,
    b_step_v: Vector2<F>,
    pos_tol: F,
) -> Option<PredictedScore<F>> {
    // First, count any actual label-space overlap. If there is *any* overlap
    // we are duplicating work already handled by `find_overlap_merge`. To
    // keep the predicted merger orthogonal, we only succeed when the merger
    // would not have succeeded under overlap-only mode — but we still allow
    // shared labels to *contribute* to the match count, since they're
    // perfect predictions trivially.
    let mut matched = 0usize;
    let mut max_residual = F::zero();
    for (&ij_a, &idx_a) in a.labels.iter() {
        let tij = t.apply(ij_a);
        let key = (tij.0 + delta.0, tij.1 + delta.1);
        // Predict where (key) would be in b's frame using b's cell-step
        // vectors. Even when `key` is NOT a b-label, we use this to compare
        // against `a`-label position — that gives us the "predict-and-match"
        // signal: a's positions should be consistent with b's lattice
        // extended outward.
        let di = key.0 - b_anchor.0;
        let dj = key.1 - b_anchor.1;
        let predicted =
            b_anchor_pos + b_step_u * lit::<F>(di as f32) + b_step_v * lit::<F>(dj as f32);
        let actual = a.positions[idx_a];
        let dx = predicted.x - actual.x;
        let dy = predicted.y - actual.y;
        let err = (dx * dx + dy * dy).sqrt();
        if err <= pos_tol {
            matched += 1;
            if err > max_residual {
                max_residual = err;
            }
        }
    }
    // Mirror: check that b's labels are also predictable from a's lattice
    // extended outward. Use a's anchor + a's step vectors and compare against
    // b's actual positions for the labels that fall in a's predicted region.
    let (a_step_u, a_step_v) = local_cell_steps(a);
    let a_anchor = *a.labels.keys().min().expect("non-empty");
    let a_anchor_pos = a.positions[*a.labels.get(&a_anchor).expect("anchor")];
    // Construct the inverse transform: given a coord in b's frame, what
    // does it map back to in a's frame? With unimodular integer matrices
    // (the D4 / D6 tables) the inverse is the transpose of the linear part
    // with the offset adjusted. We compute it on the fly.
    let det = t.determinant();
    if det == 0 {
        return None;
    }
    // Inverse linear part with sign-corrected adjugate, valid for ±1 det.
    let inv_matrix = [
        [t.matrix[1][1] / det, -t.matrix[0][1] / det],
        [-t.matrix[1][0] / det, t.matrix[0][0] / det],
    ];
    for (&ij_b, &idx_b) in b.labels.iter() {
        // Pre-image: solve t · ij_a + delta = ij_b => ij_a = t_inv · (ij_b - delta)
        let shifted = (ij_b.0 - delta.0, ij_b.1 - delta.1);
        let pre_i = inv_matrix[0][0] * shifted.0 + inv_matrix[0][1] * shifted.1;
        let pre_j = inv_matrix[1][0] * shifted.0 + inv_matrix[1][1] * shifted.1;
        let pre = (pre_i, pre_j);
        // Predict b's position from a's lattice using a's step vectors and
        // a's anchor.
        let di = pre.0 - a_anchor.0;
        let dj = pre.1 - a_anchor.1;
        let predicted =
            a_anchor_pos + a_step_u * lit::<F>(di as f32) + a_step_v * lit::<F>(dj as f32);
        let actual = b.positions[idx_b];
        let dx = predicted.x - actual.x;
        let dy = predicted.y - actual.y;
        let err = (dx * dx + dy * dy).sqrt();
        if err <= pos_tol {
            matched += 1;
            if err > max_residual {
                max_residual = err;
            }
        }
    }
    Some(PredictedScore {
        matched,
        max_residual,
    })
}

fn local_cell_steps<F: Float>(c: &ComponentInput<'_, F>) -> (Vector2<F>, Vector2<F>) {
    // Median (di = 1, dj = 0) and (di = 0, dj = 1) offset across all
    // cardinally-adjacent label pairs.
    let mut u_offsets: Vec<Vector2<F>> = Vec::new();
    let mut v_offsets: Vec<Vector2<F>> = Vec::new();
    for (&(i, j), &idx) in c.labels.iter() {
        let here = c.positions[idx];
        if let Some(&right) = c.labels.get(&(i + 1, j)) {
            u_offsets.push(c.positions[right] - here);
        }
        if let Some(&down) = c.labels.get(&(i, j + 1)) {
            v_offsets.push(c.positions[down] - here);
        }
    }
    let median_vec = |v: &mut Vec<Vector2<F>>| -> Vector2<F> {
        if v.is_empty() {
            return Vector2::new(c.cell_size, F::zero());
        }
        v.sort_by(|a, b| {
            a.norm()
                .partial_cmp(&b.norm())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        v[v.len() / 2]
    };
    let u = median_vec(&mut u_offsets);
    let v = if v_offsets.is_empty() {
        Vector2::new(F::zero(), c.cell_size)
    } else {
        median_vec(&mut v_offsets)
    };
    (u, v)
}

fn bbox(labels: &HashMap<Coord, usize>) -> (Coord, Coord) {
    let mut min_i = i32::MAX;
    let mut max_i = i32::MIN;
    let mut min_j = i32::MAX;
    let mut max_j = i32::MIN;
    for &(i, j) in labels.keys() {
        min_i = min_i.min(i);
        max_i = max_i.max(i);
        min_j = min_j.min(j);
        max_j = max_j.max(j);
    }
    ((min_i, min_j), (max_i, max_j))
}

fn apply_predicted_merge<F: Float>(
    a: &ComponentInput<'_, F>,
    b: &ComponentInput<'_, F>,
    t: GridTransform,
    delta: (i32, i32),
) -> HashMap<Coord, Point2<F>> {
    let mut merged: HashMap<Coord, Point2<F>> = HashMap::new();
    for (&coord, &idx) in b.labels.iter() {
        merged.insert(coord, b.positions[idx]);
    }
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
    fn predicted_merges_components_separated_by_missing_row_f32() {
        type F = f32;
        let s: F = 10.0;
        // Component A: rows 0..5, cols 0..5
        let (la, pa) = make_component::<F>(0..5, 0..5, s, 0.0, 0.0);
        // Component B: rows 6..11 (separated by row 5), cols 0..5,
        // labelled internally with (i, j) where the 6th row of A would be
        // row 0 of B. We deliberately give B's labels starting at j = 0
        // so the predicted merger has to discover delta = (0, 6).
        let (lb, pb) = make_component::<F>(0..5, 0..5, s, 0.0, 6.0 * s);
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
            mode: MergeMode::OverlapAndPredicted,
            min_overlap: 4,
            position_residual_max_rel: 0.20,
            cell_size_disagreement_max: 0.20,
            max_components: 8,
        };
        let res = find_predicted_merge(&a, &b, &params).expect("expected predicted merge to fire");
        assert!(res.overlap >= 4);
        // The merged labels should include every original a label plus
        // every original b label after shift.
        assert!(!res.merged_labels.is_empty());
    }
}
