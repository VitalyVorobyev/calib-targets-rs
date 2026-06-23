//! Grid → image corner maps and complete-cell enumeration.
//!
//! A [`CornerMap`] associates integer grid intersections ([`Coord`]) with
//! their sub-pixel image positions. The marker and ChArUco detectors both warp
//! the unit square cells of such a map to read out circles / ArUco bits, so the
//! per-cell corner lookup — and the canonical **TL, TR, BR, BL** quad order it
//! encodes — lives here once instead of being re-derived in each detector.

use std::collections::HashMap;

use nalgebra::Point2;

use crate::Coord;

/// A grid → image corner map: integer grid intersections to their sub-pixel
/// image positions. The shared currency of the marker / ChArUco square-cell
/// warp paths.
pub type CornerMap = HashMap<Coord, Point2<f32>>;

/// Inclusive grid-coordinate bounds `(min_u, min_v, max_u, max_v)` over a
/// corner map's keys, or `None` when the map is empty.
///
/// Callers derive their own cell-index scan range from these corner bounds: the
/// complete cells of the map span lower-left corners `min_u ..= max_u - 1` by
/// `min_v ..= max_v - 1`.
pub fn corner_map_bounds(map: &CornerMap) -> Option<(i32, i32, i32, i32)> {
    let mut keys = map.keys();
    let first = keys.next()?;
    let (mut min_u, mut min_v, mut max_u, mut max_v) = (first.u, first.v, first.u, first.v);
    for g in keys {
        min_u = min_u.min(g.u);
        min_v = min_v.min(g.v);
        max_u = max_u.max(g.u);
        max_v = max_v.max(g.v);
    }
    Some((min_u, min_v, max_u, max_v))
}

/// The four image corners of the unit cell whose top-left intersection is grid
/// `(u, v)`, in **TL, TR, BR, BL** order (clockwise — the canonical quad order
/// used for every homography fit in the workspace). Returns `None` if any of
/// the cell's four grid intersections is absent from the map.
pub fn complete_cell_corners(map: &CornerMap, u: i32, v: i32) -> Option<[Point2<f32>; 4]> {
    let tl = *map.get(&Coord::new(u, v))?;
    let tr = *map.get(&Coord::new(u + 1, v))?;
    let br = *map.get(&Coord::new(u + 1, v + 1))?;
    let bl = *map.get(&Coord::new(u, v + 1))?;
    Some([tl, tr, br, bl])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2×2 intersections forming a single complete unit cell at lower-left
    /// grid `(0, 0)`.
    fn unit_grid() -> CornerMap {
        let mut map = CornerMap::new();
        map.insert(Coord::new(0, 0), Point2::new(0.0, 0.0));
        map.insert(Coord::new(1, 0), Point2::new(10.0, 0.0));
        map.insert(Coord::new(1, 1), Point2::new(10.0, 10.0));
        map.insert(Coord::new(0, 1), Point2::new(0.0, 10.0));
        map
    }

    #[test]
    fn bounds_none_when_empty() {
        assert_eq!(corner_map_bounds(&CornerMap::new()), None);
    }

    #[test]
    fn bounds_span_all_keys() {
        let mut map = unit_grid();
        map.insert(Coord::new(3, -2), Point2::new(1.0, 1.0));
        assert_eq!(corner_map_bounds(&map), Some((0, -2, 3, 1)));
    }

    #[test]
    fn complete_cell_returns_tl_tr_br_bl() {
        let map = unit_grid();
        let c = complete_cell_corners(&map, 0, 0).expect("complete cell");
        assert_eq!(c[0], Point2::new(0.0, 0.0)); // TL = (i, j)
        assert_eq!(c[1], Point2::new(10.0, 0.0)); // TR = (i + 1, j)
        assert_eq!(c[2], Point2::new(10.0, 10.0)); // BR = (i + 1, j + 1)
        assert_eq!(c[3], Point2::new(0.0, 10.0)); // BL = (i, j + 1)
    }

    #[test]
    fn incomplete_cell_is_none() {
        let mut map = unit_grid();
        map.remove(&Coord::new(1, 1)); // drop BR
        assert_eq!(complete_cell_corners(&map, 0, 0), None);
    }
}
