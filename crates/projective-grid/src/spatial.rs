//! Static 2-D point index behind every neighbour query in the crate.
//!
//! The grid stages build an index once over a fixed set of corner positions and
//! then ask two questions of it: *which points lie within `r` of here* and
//! *which `k` points are nearest to here*. [`PointIndex`] answers both with a
//! balanced kd-tree stored implicitly in one array: the median of each range
//! (along `x` and `y` in turn) sits at the range's midpoint, its lower half to
//! the left and its upper half to the right. There are no node allocations and
//! no external dependency; building is `O(n log n)` via `select_nth_unstable`.
//!
//! ## Determinism contract
//!
//! Callers make order-sensitive decisions from these hits (first acceptable
//! candidate, ambiguity checks, the `k`-th neighbour), so the order must not
//! depend on the tree's internal layout:
//!
//! * Every query returns hits sorted by `(distance², item)` — exact distance
//!   ties fall back to the caller-supplied `item`.
//! * [`PointIndex::nearest_n`] resolves a tie *at* the `k`-th place the same
//!   way: of several points equally distant at the cut, the smaller `item`s
//!   are kept. The result equals a brute-force sort truncated to `k`.
//!
//! The squared distance is `dx² + dy²` in `f32`, and the radius test is
//! inclusive (`d² <= r²`).

use nalgebra::Point2;

/// Ranges this short are scanned linearly instead of split further.
const LEAF_LEN: usize = 8;

/// One query hit: the `item` the point was indexed with, and its squared
/// distance to the query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Neighbour {
    pub(crate) item: usize,
    pub(crate) distance_sq: f32,
}

impl Neighbour {
    /// The `(distance², item)` order of the determinism contract.
    fn precedes(&self, other: &Self) -> bool {
        self.distance_sq < other.distance_sq
            || (self.distance_sq == other.distance_sq && self.item < other.item)
    }
}

/// Immutable 2-D point index; see the [module docs](self) for the layout and
/// the ordering contract.
pub(crate) struct PointIndex {
    /// `(position, item)` entries in kd order; see [`build`].
    entries: Vec<([f32; 2], usize)>,
}

impl PointIndex {
    /// Index `(position, item)` pairs. `item` is returned by queries and is the
    /// tie-break key, so it should identify the point (a slot or corner index).
    pub(crate) fn new(points: impl IntoIterator<Item = (Point2<f32>, usize)>) -> Self {
        let mut entries: Vec<([f32; 2], usize)> = points
            .into_iter()
            .map(|(p, item)| ([p.x, p.y], item))
            .collect();
        build(&mut entries, 0);
        Self { entries }
    }

    /// Index `positions`, each tagged with its own slice index.
    pub(crate) fn from_positions(positions: &[Point2<f32>]) -> Self {
        Self::new(positions.iter().copied().zip(0..))
    }

    /// Every indexed point with `distance² <= radius_sq` from `center`,
    /// sorted by `(distance², item)`.
    pub(crate) fn within(&self, center: Point2<f32>, radius_sq: f32) -> Vec<Neighbour> {
        let mut hits = Vec::new();
        self.within_range(
            0,
            self.entries.len(),
            0,
            [center.x, center.y],
            radius_sq,
            &mut hits,
        );
        hits.sort_unstable_by(|a, b| {
            a.distance_sq
                .total_cmp(&b.distance_sq)
                .then_with(|| a.item.cmp(&b.item))
        });
        hits
    }

    /// The `k` points nearest to `center` (fewer when the index holds fewer),
    /// sorted by `(distance², item)`. A query point that is itself indexed is
    /// returned as its own nearest neighbour.
    pub(crate) fn nearest_n(&self, center: Point2<f32>, k: usize) -> Vec<Neighbour> {
        let mut best = Vec::with_capacity(k + 1);
        if k > 0 {
            self.nearest_range(0, self.entries.len(), 0, [center.x, center.y], k, &mut best);
        }
        best
    }

    fn hit(&self, i: usize, q: [f32; 2]) -> Neighbour {
        let (p, item) = self.entries[i];
        let dx = p[0] - q[0];
        let dy = p[1] - q[1];
        Neighbour {
            item,
            distance_sq: dx * dx + dy * dy,
        }
    }

    /// Visit `lo..hi`, whose median along `axis` sits at its midpoint.
    fn within_range(
        &self,
        lo: usize,
        hi: usize,
        axis: usize,
        q: [f32; 2],
        radius_sq: f32,
        out: &mut Vec<Neighbour>,
    ) {
        if hi - lo <= LEAF_LEN {
            out.extend(
                (lo..hi)
                    .map(|i| self.hit(i, q))
                    .filter(|h| h.distance_sq <= radius_sq),
            );
            return;
        }
        let mid = lo + (hi - lo) / 2;
        let h = self.hit(mid, q);
        if h.distance_sq <= radius_sq {
            out.push(h);
        }
        let diff = q[axis] - self.entries[mid].0[axis];
        let (near, far) = split(lo, mid, hi, diff);
        self.within_range(near.0, near.1, axis ^ 1, q, radius_sq, out);
        if diff * diff <= radius_sq {
            self.within_range(far.0, far.1, axis ^ 1, q, radius_sq, out);
        }
    }

    /// Like [`Self::within_range`], keeping the `k` best hits in `best`
    /// (sorted, at most `k` long).
    fn nearest_range(
        &self,
        lo: usize,
        hi: usize,
        axis: usize,
        q: [f32; 2],
        k: usize,
        best: &mut Vec<Neighbour>,
    ) {
        if hi - lo <= LEAF_LEN {
            for i in lo..hi {
                offer(best, k, self.hit(i, q));
            }
            return;
        }
        let mid = lo + (hi - lo) / 2;
        offer(best, k, self.hit(mid, q));
        let diff = q[axis] - self.entries[mid].0[axis];
        let (near, far) = split(lo, mid, hi, diff);
        self.nearest_range(near.0, near.1, axis ^ 1, q, k, best);
        // `<=`, not `<`: the far side may still hold an exact tie with the
        // current k-th hit and a smaller item, which the contract must keep.
        if best.len() < k || diff * diff <= best[k - 1].distance_sq {
            self.nearest_range(far.0, far.1, axis ^ 1, q, k, best);
        }
    }
}

/// Lay `entries` out in kd order: the median along `axis` (ties by item, so the
/// layout is a function of the input alone) moves to the midpoint, then each
/// half is laid out along the other axis.
fn build(entries: &mut [([f32; 2], usize)], axis: usize) {
    if entries.len() <= LEAF_LEN {
        return;
    }
    let mid = entries.len() / 2;
    entries.select_nth_unstable_by(mid, |a, b| {
        a.0[axis].total_cmp(&b.0[axis]).then_with(|| a.1.cmp(&b.1))
    });
    let (lower, upper) = entries.split_at_mut(mid);
    build(lower, axis ^ 1);
    build(&mut upper[1..], axis ^ 1);
}

/// The `(near, far)` sub-ranges around `mid` for a query `diff` along the split
/// axis (`diff = query − median`).
fn split(lo: usize, mid: usize, hi: usize, diff: f32) -> ((usize, usize), (usize, usize)) {
    if diff < 0.0 {
        ((lo, mid), (mid + 1, hi))
    } else {
        ((mid + 1, hi), (lo, mid))
    }
}

/// Insert `hit` into the sorted `best` list, keeping at most `k` entries.
fn offer(best: &mut Vec<Neighbour>, k: usize, hit: Neighbour) {
    if best.len() == k && !hit.precedes(&best[k - 1]) {
        return;
    }
    let at = best.partition_point(|b| b.precedes(&hit));
    best.insert(at, hit);
    best.truncate(k);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Brute-force reference: all points sorted by `(d², item)`.
    fn brute(points: &[Point2<f32>], c: Point2<f32>) -> Vec<Neighbour> {
        let mut all: Vec<Neighbour> = points
            .iter()
            .enumerate()
            .map(|(item, p)| {
                let dx = p.x - c.x;
                let dy = p.y - c.y;
                Neighbour {
                    item,
                    distance_sq: dx * dx + dy * dy,
                }
            })
            .collect();
        all.sort_by(|a, b| {
            a.distance_sq
                .total_cmp(&b.distance_sq)
                .then_with(|| a.item.cmp(&b.item))
        });
        all
    }

    /// A unit lattice: every interior point has four neighbours at exactly
    /// distance 1 and four at exactly √2, so ties are everywhere.
    fn lattice(n: i32) -> Vec<Point2<f32>> {
        (0..n)
            .flat_map(|j| (0..n).map(move |i| Point2::new(i as f32, j as f32)))
            .collect()
    }

    /// A 25 px-pitch board with sub-pixel jitter from a seeded xorshift, plus
    /// a few exact duplicates.
    fn jittered_board() -> Vec<Point2<f32>> {
        let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut jitter = move || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s % 1000) as f32 / 1000.0 - 0.5
        };
        let mut pts: Vec<Point2<f32>> = (0..40)
            .flat_map(|j| (0..37).map(move |i| (i, j)))
            .map(|(i, j)| Point2::new(i as f32 * 25.0 + jitter(), j as f32 * 25.0 + jitter()))
            .collect();
        pts.extend([pts[17], pts[500], pts[500]]);
        pts
    }

    fn assert_matches_brute(pts: &[Point2<f32>], queries: &[Point2<f32>], radii_sq: &[f32]) {
        let idx = PointIndex::from_positions(pts);
        for &c in queries {
            let all = brute(pts, c);
            for &r_sq in radii_sq {
                let want: Vec<Neighbour> = all
                    .iter()
                    .copied()
                    .filter(|n| n.distance_sq <= r_sq)
                    .collect();
                assert_eq!(idx.within(c, r_sq), want, "within c={c:?} r²={r_sq}");
            }
            for k in [1, 2, 3, 5, 7, 9, 30, 200, pts.len() + 3] {
                let want: Vec<Neighbour> = all.iter().copied().take(k).collect();
                assert_eq!(idx.nearest_n(c, k), want, "nearest c={c:?} k={k}");
            }
        }
    }

    #[test]
    fn empty_index_answers_nothing() {
        let idx = PointIndex::from_positions(&[]);
        assert!(idx.within(Point2::new(0.0, 0.0), 100.0).is_empty());
        assert!(idx.nearest_n(Point2::new(0.0, 0.0), 3).is_empty());
    }

    #[test]
    fn exact_lattice_matches_brute_force_including_ties() {
        let pts = lattice(9);
        let queries = [
            Point2::new(4.0, 4.0),
            Point2::new(0.0, 0.0),
            Point2::new(0.3, 7.6),
            Point2::new(2.5, 6.5),
            Point2::new(-1.0, -1.0),
        ];
        assert_matches_brute(&pts, &queries, &[0.0, 1.0, 2.0, 4.0, 12.5]);
        // Exactly on the radius: the four axis neighbours at d² = 1 are in.
        let idx = PointIndex::from_positions(&pts);
        assert_eq!(idx.within(Point2::new(4.0, 4.0), 1.0).len(), 5);
    }

    #[test]
    fn jittered_board_matches_brute_force() {
        let pts = jittered_board();
        let mut queries: Vec<Point2<f32>> = pts.iter().step_by(37).copied().collect();
        queries.extend([
            Point2::new(-60.0, 400.0),
            Point2::new(462.5, 487.5),
            Point2::new(2000.0, -5.0),
        ]);
        assert_matches_brute(&pts, &queries, &[0.0, 56.25, 625.0, 2500.0]);
    }

    #[test]
    fn item_tags_are_the_tie_break_key() {
        // Two points at the same position: the smaller tag wins the single slot.
        let idx = PointIndex::new([(Point2::new(1.0, 1.0), 7), (Point2::new(1.0, 1.0), 3)]);
        let hit = idx.nearest_n(Point2::new(0.0, 0.0), 1);
        assert_eq!(hit[0].item, 3);
        assert_eq!(hit[0].distance_sq, 2.0);
    }
}
