//! Static 2-D point index behind every neighbour query in the crate.
//!
//! The grid stages build an index once over a fixed set of corner positions and
//! then ask two questions of it: *which points lie within `r` of here* and
//! *which `k` points are nearest to here*. [`PointIndex`] answers both on top of
//! a bulk-loaded [`rstar::RTree`].
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
//!   are kept.
//!
//! The squared distance is `dx² + dy²` in `f32`, and the radius test is
//! inclusive (`d² <= r²`).

use nalgebra::Point2;
use rstar::primitives::GeomWithData;
use rstar::RTree;

type Entry = GeomWithData<[f32; 2], usize>;

/// One query hit: the `item` the point was indexed with, and its squared
/// distance to the query.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Neighbour {
    pub(crate) item: usize,
    pub(crate) distance_sq: f32,
}

/// Immutable 2-D point index; see the [module docs](self) for the ordering
/// contract.
pub(crate) struct PointIndex {
    tree: RTree<Entry>,
}

impl PointIndex {
    /// Index `(position, item)` pairs. `item` is returned by queries and is the
    /// tie-break key, so it should identify the point (a slot or corner index).
    pub(crate) fn new(points: impl IntoIterator<Item = (Point2<f32>, usize)>) -> Self {
        let entries = points
            .into_iter()
            .map(|(p, item)| GeomWithData::new([p.x, p.y], item))
            .collect();
        Self {
            tree: RTree::bulk_load(entries),
        }
    }

    /// Index `positions`, each tagged with its own slice index.
    pub(crate) fn from_positions(positions: &[Point2<f32>]) -> Self {
        Self::new(positions.iter().copied().zip(0..))
    }

    /// Every indexed point with `distance² <= radius_sq` from `center`,
    /// sorted by `(distance², item)`.
    pub(crate) fn within(&self, center: Point2<f32>, radius_sq: f32) -> Vec<Neighbour> {
        let mut hits: Vec<Neighbour> = self
            .tree
            .locate_within_distance([center.x, center.y], radius_sq)
            .map(|e| Neighbour {
                item: e.data,
                distance_sq: squared_distance(e.geom(), center),
            })
            .collect();
        sort_hits(&mut hits);
        hits
    }

    /// The `k` points nearest to `center` (fewer when the index holds fewer),
    /// sorted by `(distance², item)`. A query point that is itself indexed is
    /// returned as its own nearest neighbour.
    pub(crate) fn nearest_n(&self, center: Point2<f32>, k: usize) -> Vec<Neighbour> {
        if k == 0 {
            return Vec::new();
        }
        let mut hits: Vec<Neighbour> = Vec::with_capacity(k + 1);
        // The iterator yields points in non-decreasing distance. Keep going past
        // the k-th hit while the distance still equals the k-th distance, so the
        // whole tie group at the cut is seen before the item tie-break picks.
        for (e, distance_sq) in self
            .tree
            .nearest_neighbor_iter_with_distance_2([center.x, center.y])
        {
            if hits.len() >= k && distance_sq > hits[k - 1].distance_sq {
                break;
            }
            hits.push(Neighbour {
                item: e.data,
                distance_sq,
            });
        }
        sort_hits(&mut hits);
        hits.truncate(k);
        hits
    }
}

fn squared_distance(p: &[f32; 2], q: Point2<f32>) -> f32 {
    let dx = p[0] - q.x;
    let dy = p[1] - q.y;
    dx * dx + dy * dy
}

fn sort_hits(hits: &mut [Neighbour]) {
    hits.sort_unstable_by(|a, b| {
        a.distance_sq
            .total_cmp(&b.distance_sq)
            .then_with(|| a.item.cmp(&b.item))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Brute-force reference: all points sorted by `(d², item)`.
    fn brute(points: &[Point2<f32>], c: Point2<f32>) -> Vec<Neighbour> {
        let mut all: Vec<Neighbour> = points
            .iter()
            .enumerate()
            .map(|(item, p)| Neighbour {
                item,
                distance_sq: squared_distance(&[p.x, p.y], c),
            })
            .collect();
        sort_hits(&mut all);
        all
    }

    /// A unit lattice: every interior point has four neighbours at exactly
    /// distance 1 and four at exactly √2, so ties are everywhere.
    fn lattice(n: i32) -> Vec<Point2<f32>> {
        (0..n)
            .flat_map(|j| (0..n).map(move |i| Point2::new(i as f32, j as f32)))
            .collect()
    }

    #[test]
    fn empty_index_answers_nothing() {
        let idx = PointIndex::from_positions(&[]);
        assert!(idx.within(Point2::new(0.0, 0.0), 100.0).is_empty());
        assert!(idx.nearest_n(Point2::new(0.0, 0.0), 3).is_empty());
    }

    #[test]
    fn within_matches_brute_force_and_includes_the_boundary() {
        let pts = lattice(9);
        let idx = PointIndex::from_positions(&pts);
        for c in [
            Point2::new(4.0, 4.0),
            Point2::new(0.3, 7.6),
            Point2::new(-1.0, -1.0),
        ] {
            for r_sq in [0.0_f32, 1.0, 2.0, 4.0, 12.5] {
                let want: Vec<Neighbour> = brute(&pts, c)
                    .into_iter()
                    .filter(|n| n.distance_sq <= r_sq)
                    .collect();
                assert_eq!(idx.within(c, r_sq), want, "c={c:?} r²={r_sq}");
            }
        }
        // Exactly on the radius: the four axis neighbours at d² = 1.
        assert_eq!(idx.within(Point2::new(4.0, 4.0), 1.0).len(), 5);
    }

    #[test]
    fn nearest_n_breaks_ties_at_the_cut_by_item() {
        let pts = lattice(9);
        let idx = PointIndex::from_positions(&pts);
        for c in [
            Point2::new(4.0, 4.0),
            Point2::new(0.0, 0.0),
            Point2::new(2.5, 6.5),
        ] {
            for k in [1, 2, 3, 5, 7, 9, 30, 200] {
                let mut want = brute(&pts, c);
                want.truncate(k);
                assert_eq!(idx.nearest_n(c, k), want, "c={c:?} k={k}");
            }
        }
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
