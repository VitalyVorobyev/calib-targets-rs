//! Candidate acceptance: KD-tree radius search + nearest/2nd-nearest
//! ambiguity gate.
//!
//! Extracted from the legacy `square::grow::collect_candidates` +
//! `choose_unambiguous` so both the BFS engine
//! ([`crate::grow::engine::bfs_grow`]) and the post-grow fill
//! ([`crate::refine::fill`], Phase 4) share the same acceptance pipeline.

use std::collections::HashSet;

use kiddo::{KdTree, SquaredEuclidean};
use nalgebra::Point2;

use crate::float::Float;

/// A candidate observation discovered by [`collect_candidates`].
///
/// Carries the index into the caller's observation slice, the squared
/// distance from the prediction (cheap to compute, monotonic in the
/// real distance), and the pixel position so the caller can run further
/// per-candidate diagnostics without re-indexing.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Candidate<F: Float> {
    /// Observation index in the caller's slice.
    pub idx: usize,
    /// Euclidean distance from the predicted position, in pixels.
    pub distance: F,
    /// The candidate's position.
    pub position: Point2<F>,
}

/// Outcome of [`choose_unambiguous`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct UnambiguousChoice<F: Float> {
    /// The accepted observation index.
    pub idx: usize,
    /// `nearest / second` distance ratio when ≥ 2 candidates were
    /// available; `+∞` when only one candidate exists. Useful for
    /// diagnostic events even on the accept path.
    pub ratio: F,
}

/// Collect observation indices within `search_radius` (pixels) of
/// `target_position`, excluding any indices already labelled.
///
/// Sorted in ascending order of distance. Returns an empty `Vec` when no
/// observations are within range.
pub fn collect_candidates<F>(
    target_position: Point2<F>,
    search_radius: F,
    kd_tree: &KdTree<F, 2>,
    slot_to_idx: &[usize],
    excluded_indices: &HashSet<usize>,
    positions: &[Point2<F>],
) -> Vec<Candidate<F>>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    let r2 = search_radius * search_radius;
    let mut out: Vec<Candidate<F>> = kd_tree
        .within_unsorted::<SquaredEuclidean>(&[target_position.x, target_position.y], r2)
        .into_iter()
        .filter_map(|nn| {
            let slot = nn.item as usize;
            let idx = slot_to_idx[slot];
            if excluded_indices.contains(&idx) {
                return None;
            }
            Some(Candidate {
                idx,
                distance: nn.distance.sqrt(),
                position: positions[idx],
            })
        })
        .collect();
    out.sort_by(|a, b| {
        a.distance
            .partial_cmp(&b.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Why [`choose_unambiguous`] declined to commit to a candidate.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum AmbiguityReason<F: Float> {
    /// `candidates` was empty.
    Empty,
    /// Two candidates were within `ambiguity_ratio` of each other.
    TooClose {
        /// Distance to the nearest candidate (pixels).
        nearest: F,
        /// Distance to the runner-up (pixels).
        second: F,
        /// `second / nearest`. Values below `ambiguity_ratio` reject.
        ratio: F,
    },
}

/// Pick a single candidate when the second-nearest is meaningfully farther.
///
/// Accept iff `candidates.is_empty() == false` AND one of:
///
/// * exactly one candidate exists; or
/// * `second.distance / first.distance >= ambiguity_ratio` (the
///   "nearest is meaningfully closer than runner-up" gate). `ambiguity_ratio`
///   is typically `1.3` to `1.5` in production.
///
/// A nearest distance of exactly zero (the prediction landing on top of an
/// observation, common on clean synthetic grids) is **not** ambiguous on its
/// own — only the runner-up ratio matters. To keep the divide stable, the
/// ratio comparison uses `second >= ambiguity_ratio * nearest` rather than
/// `second / nearest`.
///
/// Returns `Err` with a typed reason on rejection so the BFS engine can emit
/// the appropriate `GrowRejected` event.
pub fn choose_unambiguous<F: Float>(
    candidates: &[Candidate<F>],
    ambiguity_ratio: F,
) -> Result<UnambiguousChoice<F>, AmbiguityReason<F>> {
    if candidates.is_empty() {
        return Err(AmbiguityReason::Empty);
    }
    let first = candidates[0];
    if candidates.len() == 1 {
        // Infinity ratio carries the "no runner-up" signal cleanly.
        let inf = F::one() / F::default_epsilon();
        return Ok(UnambiguousChoice {
            idx: first.idx,
            ratio: inf,
        });
    }
    let second = candidates[1];
    // Use multiplicative form so a nearest distance of exactly zero (clean
    // synthetic grid: prediction lands on the observation) does not cause a
    // divide-by-zero in the ratio gate. The condition is
    // `second / nearest >= ambiguity_ratio`, rewritten as
    // `second >= ambiguity_ratio * nearest`.
    if second.distance < ambiguity_ratio * first.distance {
        let ratio = if first.distance > F::zero() {
            second.distance / first.distance
        } else {
            F::one()
        };
        return Err(AmbiguityReason::TooClose {
            nearest: first.distance,
            second: second.distance,
            ratio,
        });
    }
    let ratio = if first.distance > F::zero() {
        second.distance / first.distance
    } else {
        F::one() / F::default_epsilon()
    };
    Ok(UnambiguousChoice {
        idx: first.idx,
        ratio,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::{abs, lit};

    fn assert_collect_empty_when_no_hits<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let positions: Vec<Point2<F>> = vec![Point2::new(lit::<F>(100.0_f32), F::zero())];
        let mut tree: KdTree<F, 2> = KdTree::new();
        tree.add(&[positions[0].x, positions[0].y], 0u64);
        let slot_to_idx = vec![0usize];
        let excluded = HashSet::new();
        let target = Point2::new(F::zero(), F::zero());
        let radius = lit::<F>(10.0_f32);
        let cands = collect_candidates(target, radius, &tree, &slot_to_idx, &excluded, &positions);
        assert!(cands.is_empty());
    }

    fn assert_collect_excludes_set<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let positions: Vec<Point2<F>> = vec![
            Point2::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(1.0_f32), F::zero()),
        ];
        let mut tree: KdTree<F, 2> = KdTree::new();
        for (slot, p) in positions.iter().enumerate() {
            tree.add(&[p.x, p.y], slot as u64);
        }
        let slot_to_idx = vec![0usize, 1];
        let mut excluded = HashSet::new();
        excluded.insert(0);
        let target = Point2::new(F::zero(), F::zero());
        let radius = lit::<F>(10.0_f32);
        let cands = collect_candidates(target, radius, &tree, &slot_to_idx, &excluded, &positions);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].idx, 1);
    }

    fn assert_choose_empty_returns_err<F: Float>() {
        let res: Result<UnambiguousChoice<F>, _> = choose_unambiguous::<F>(&[], lit::<F>(1.3_f32));
        assert!(matches!(res, Err(AmbiguityReason::Empty)));
    }

    fn assert_choose_single_returns_ok<F: Float>() {
        let cand = Candidate {
            idx: 7,
            distance: lit::<F>(2.0_f32),
            position: Point2::new(F::zero(), F::zero()),
        };
        let res = choose_unambiguous::<F>(&[cand], lit::<F>(1.3_f32)).unwrap();
        assert_eq!(res.idx, 7);
    }

    fn assert_choose_too_close_returns_err<F: Float>() {
        let c0 = Candidate {
            idx: 0,
            distance: lit::<F>(1.0_f32),
            position: Point2::new(F::zero(), F::zero()),
        };
        let c1 = Candidate {
            idx: 1,
            distance: lit::<F>(1.1_f32),
            position: Point2::new(F::zero(), F::zero()),
        };
        let res = choose_unambiguous::<F>(&[c0, c1], lit::<F>(1.3_f32));
        let Err(AmbiguityReason::TooClose {
            ratio,
            nearest,
            second,
        }) = res
        else {
            panic!("expected TooClose, got {res:?}");
        };
        assert!(abs::<F>(ratio - lit::<F>(1.1_f32)) < lit::<F>(1e-4_f32));
        assert!(abs::<F>(nearest - lit::<F>(1.0_f32)) < lit::<F>(1e-4_f32));
        assert!(abs::<F>(second - lit::<F>(1.1_f32)) < lit::<F>(1e-4_f32));
    }

    fn assert_choose_zero_nearest_accepted_when_runner_up_far<F: Float>() {
        // On a clean synthetic grid the prediction can land exactly on top of
        // the candidate observation, giving `nearest = 0`. The acceptance gate
        // must still admit when the runner-up is meaningfully away.
        let c0 = Candidate {
            idx: 0,
            distance: F::zero(),
            position: Point2::new(F::zero(), F::zero()),
        };
        let c1 = Candidate {
            idx: 1,
            distance: lit::<F>(5.0_f32),
            position: Point2::new(F::zero(), F::zero()),
        };
        let res = choose_unambiguous::<F>(&[c0, c1], lit::<F>(1.3_f32)).unwrap();
        assert_eq!(res.idx, 0);
    }

    fn assert_choose_runner_up_far_enough<F: Float>() {
        let c0 = Candidate {
            idx: 0,
            distance: lit::<F>(1.0_f32),
            position: Point2::new(F::zero(), F::zero()),
        };
        let c1 = Candidate {
            idx: 1,
            distance: lit::<F>(2.0_f32),
            position: Point2::new(F::zero(), F::zero()),
        };
        let res = choose_unambiguous::<F>(&[c0, c1], lit::<F>(1.3_f32)).unwrap();
        assert_eq!(res.idx, 0);
        assert!(abs::<F>(res.ratio - lit::<F>(2.0_f32)) < lit::<F>(1e-4_f32));
    }

    #[test]
    fn collect_empty_f32() {
        assert_collect_empty_when_no_hits::<f32>();
    }
    #[test]
    fn collect_empty_f64() {
        assert_collect_empty_when_no_hits::<f64>();
    }
    #[test]
    fn collect_excludes_f32() {
        assert_collect_excludes_set::<f32>();
    }
    #[test]
    fn collect_excludes_f64() {
        assert_collect_excludes_set::<f64>();
    }
    #[test]
    fn choose_empty_f32() {
        assert_choose_empty_returns_err::<f32>();
    }
    #[test]
    fn choose_empty_f64() {
        assert_choose_empty_returns_err::<f64>();
    }
    #[test]
    fn choose_single_f32() {
        assert_choose_single_returns_ok::<f32>();
    }
    #[test]
    fn choose_single_f64() {
        assert_choose_single_returns_ok::<f64>();
    }
    #[test]
    fn choose_too_close_f32() {
        assert_choose_too_close_returns_err::<f32>();
    }
    #[test]
    fn choose_too_close_f64() {
        assert_choose_too_close_returns_err::<f64>();
    }
    #[test]
    fn choose_runner_up_far_f32() {
        assert_choose_runner_up_far_enough::<f32>();
    }
    #[test]
    fn choose_runner_up_far_f64() {
        assert_choose_runner_up_far_enough::<f64>();
    }
    #[test]
    fn choose_zero_nearest_runner_up_far_f32() {
        assert_choose_zero_nearest_accepted_when_runner_up_far::<f32>();
    }
    #[test]
    fn choose_zero_nearest_runner_up_far_f64() {
        assert_choose_zero_nearest_accepted_when_runner_up_far::<f64>();
    }
}
