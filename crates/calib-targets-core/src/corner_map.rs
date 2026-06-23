//! Grid → image corner maps and complete-cell enumeration.
//!
//! A [`CornerMap`] associates integer grid intersections ([`GridCoords`]) with
//! their sub-pixel image positions. The marker and ChArUco detectors both warp
//! the unit square cells of such a map to read out circles / ArUco bits, so the
//! per-cell corner lookup — and the canonical **TL, TR, BR, BL** quad order it
//! encodes — lives here once instead of being re-derived in each detector.

use std::collections::HashMap;

use nalgebra::Point2;

use crate::corner::GridCoords;

/// A grid → image corner map: integer grid intersections to their sub-pixel
/// image positions. The shared currency of the marker / ChArUco square-cell
/// warp paths.
pub type CornerMap = HashMap<GridCoords, Point2<f32>>;

/// Inclusive grid-coordinate bounds `(min_i, min_j, max_i, max_j)` over a
/// corner map's keys, or `None` when the map is empty.
///
/// Callers derive their own cell-index scan range from these corner bounds: the
/// complete cells of the map span lower-left corners `min_i ..= max_i - 1` by
/// `min_j ..= max_j - 1`.
pub fn corner_map_bounds(map: &CornerMap) -> Option<(i32, i32, i32, i32)> {
    let mut keys = map.keys();
    let first = keys.next()?;
    let (mut min_i, mut min_j, mut max_i, mut max_j) = (first.i, first.j, first.i, first.j);
    for g in keys {
        min_i = min_i.min(g.i);
        min_j = min_j.min(g.j);
        max_i = max_i.max(g.i);
        max_j = max_j.max(g.j);
    }
    Some((min_i, min_j, max_i, max_j))
}

/// The four image corners of the unit cell whose top-left intersection is grid
/// `(i, j)`, in **TL, TR, BR, BL** order (clockwise — the canonical quad order
/// used for every homography fit in the workspace). Returns `None` if any of
/// the cell's four grid intersections is absent from the map.
pub fn complete_cell_corners(map: &CornerMap, i: i32, j: i32) -> Option<[Point2<f32>; 4]> {
    let tl = *map.get(&GridCoords { i, j })?;
    let tr = *map.get(&GridCoords { i: i + 1, j })?;
    let br = *map.get(&GridCoords { i: i + 1, j: j + 1 })?;
    let bl = *map.get(&GridCoords { i, j: j + 1 })?;
    Some([tl, tr, br, bl])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2×2 intersections forming a single complete unit cell at lower-left
    /// grid `(0, 0)`.
    fn unit_grid() -> CornerMap {
        let mut map = CornerMap::new();
        map.insert(GridCoords { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        map.insert(GridCoords { i: 1, j: 0 }, Point2::new(10.0, 0.0));
        map.insert(GridCoords { i: 1, j: 1 }, Point2::new(10.0, 10.0));
        map.insert(GridCoords { i: 0, j: 1 }, Point2::new(0.0, 10.0));
        map
    }

    #[test]
    fn bounds_none_when_empty() {
        assert_eq!(corner_map_bounds(&CornerMap::new()), None);
    }

    #[test]
    fn bounds_span_all_keys() {
        let mut map = unit_grid();
        map.insert(GridCoords { i: 3, j: -2 }, Point2::new(1.0, 1.0));
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
        map.remove(&GridCoords { i: 1, j: 1 }); // drop BR
        assert_eq!(complete_cell_corners(&map, 0, 0), None);
    }
}
