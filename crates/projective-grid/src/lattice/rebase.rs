//! Rebase lattice labels so the labelled bounding-box minimum is `(0, 0)`.
//!
//! Every labelled set this crate hands out is non-negative with its bounding
//! box anchored at the origin. This module is the single implementation of
//! that shift, for both key representations the pipeline uses (`Coord` and
//! the `(i, j)` tuples of the shared back-half).

use std::collections::HashMap;
use std::hash::Hash;

use super::Coord;

/// An integer lattice key with `(u, v)` components.
pub(crate) trait LatticeKey: Copy + Eq + Hash {
    /// The `(u, v)` components.
    fn uv(self) -> (i32, i32);
    /// Build a key from `(u, v)` components.
    fn from_uv(u: i32, v: i32) -> Self;
}

impl LatticeKey for Coord {
    fn uv(self) -> (i32, i32) {
        (self.u, self.v)
    }

    fn from_uv(u: i32, v: i32) -> Self {
        Coord::new(u, v)
    }
}

impl LatticeKey for (i32, i32) {
    fn uv(self) -> (i32, i32) {
        self
    }

    fn from_uv(u: i32, v: i32) -> Self {
        (u, v)
    }
}

/// Component-wise minimum of `keys`, or `None` when there are none.
fn bbox_min<K: LatticeKey>(keys: impl IntoIterator<Item = K>) -> Option<(i32, i32)> {
    keys.into_iter()
        .map(LatticeKey::uv)
        .reduce(|(a, b), (u, v)| (a.min(u), b.min(v)))
}

/// Shift every key of `labelled` so the bounding-box minimum is `(0, 0)`.
///
/// No-op when the map is empty or already anchored at the origin.
pub(crate) fn rebase_to_origin<K: LatticeKey, V>(labelled: &mut HashMap<K, V>) {
    let Some((min_u, min_v)) = bbox_min(labelled.keys().copied()) else {
        return;
    };
    if min_u == 0 && min_v == 0 {
        return;
    }
    *labelled = labelled
        .drain()
        .map(|(k, val)| {
            let (u, v) = k.uv();
            (K::from_uv(u - min_u, v - min_v), val)
        })
        .collect();
}

/// Shift the coordinate of every item (reached through `coord`) so the
/// bounding-box minimum is `(0, 0)`.
///
/// No-op when `items` is empty or already anchored at the origin.
pub(crate) fn rebase_slice_to_origin<T>(items: &mut [T], coord: impl Fn(&mut T) -> &mut Coord) {
    let Some((min_u, min_v)) = bbox_min(items.iter_mut().map(|item| *coord(item))) else {
        return;
    };
    if min_u == 0 && min_v == 0 {
        return;
    }
    for item in items.iter_mut() {
        let c = coord(item);
        c.u -= min_u;
        c.v -= min_v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_rebase_shifts_bbox_min_to_origin() {
        let mut m: HashMap<(i32, i32), usize> = HashMap::from([((-2, 3), 0), ((1, 5), 1)]);
        rebase_to_origin(&mut m);
        assert_eq!(m, HashMap::from([((0, 0), 0), ((3, 2), 1)]));

        let mut c: HashMap<Coord, usize> =
            HashMap::from([(Coord::new(4, 7), 0), (Coord::new(5, 9), 1)]);
        rebase_to_origin(&mut c);
        assert_eq!(
            c,
            HashMap::from([(Coord::new(0, 0), 0), (Coord::new(1, 2), 1)])
        );

        let mut empty: HashMap<Coord, usize> = HashMap::new();
        rebase_to_origin(&mut empty);
        assert!(empty.is_empty());
    }

    #[test]
    fn slice_rebase_shifts_bbox_min_to_origin() {
        let mut v = vec![Coord::new(-1, 2), Coord::new(3, -4)];
        rebase_slice_to_origin(&mut v, |c| c);
        assert_eq!(v, vec![Coord::new(0, 6), Coord::new(4, 0)]);
    }
}
