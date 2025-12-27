use calib_targets_aruco::{GridCell, MarkerCell};
use calib_targets_core::{GridCoords, LabeledCorner};
use nalgebra::Point2;
use std::collections::HashMap;

#[cfg(feature = "tracing")]
use tracing::instrument;

pub(crate) type CornerMap = HashMap<GridCoords, Point2<f32>>;

/// Build a grid -> image map from inlier chessboard corners.
#[cfg_attr(feature = "tracing", instrument(level = "info", skip(corners, inliers), fields(corners=inliers.len())))]
pub(crate) fn build_corner_map(corners: &[LabeledCorner], inliers: &[usize]) -> CornerMap {
    let mut map = HashMap::new();
    for &idx in inliers {
        if let Some(c) = corners.get(idx) {
            if let Some(g) = c.grid {
                map.insert(g, c.position);
            }
        }
    }
    map
}

/// Enumerate complete square cells (TL, TR, BR, BL) from a corner map.
#[cfg_attr(feature = "tracing", instrument(level = "info", skip(map), fields(corners=map.len())))]
pub(crate) fn build_marker_cells(map: &CornerMap) -> Vec<MarkerCell> {
    let mut min_i = i32::MAX;
    let mut min_j = i32::MAX;
    let mut max_i = i32::MIN;
    let mut max_j = i32::MIN;

    for g in map.keys() {
        min_i = min_i.min(g.i);
        min_j = min_j.min(g.j);
        max_i = max_i.max(g.i);
        max_j = max_j.max(g.j);
    }

    if min_i == i32::MAX || min_j == i32::MAX {
        return Vec::new();
    }

    let cells_x = (max_i - min_i).max(0) as usize;
    let cells_y = (max_j - min_j).max(0) as usize;
    let mut out = Vec::with_capacity(cells_x * cells_y);
    for j in min_j..max_j {
        for i in min_i..max_i {
            let g00 = GridCoords { i, j };
            let g10 = GridCoords { i: i + 1, j };
            let g11 = GridCoords { i: i + 1, j: j + 1 };
            let g01 = GridCoords { i, j: j + 1 };

            let (Some(&p00), Some(&p10), Some(&p11), Some(&p01)) =
                (map.get(&g00), map.get(&g10), map.get(&g11), map.get(&g01))
            else {
                continue;
            };

            out.push(MarkerCell {
                gc: GridCell { gx: i, gy: j },
                corners_img: [p00, p10, p11, p01],
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_corner_map_filters_inliers() {
        let corners = vec![
            LabeledCorner {
                position: Point2::new(1.0, 2.0),
                grid: Some(GridCoords { i: 0, j: 0 }),
                id: None,
                target_position: None,
                score: 0.5,
            },
            LabeledCorner {
                position: Point2::new(3.0, 4.0),
                grid: None,
                id: None,
                target_position: None,
                score: 0.5,
            },
            LabeledCorner {
                position: Point2::new(5.0, 6.0),
                grid: Some(GridCoords { i: 1, j: 0 }),
                id: None,
                target_position: None,
                score: 0.5,
            },
        ];

        let map = build_corner_map(&corners, &[0, 2]);
        assert_eq!(map.len(), 2);
        assert_eq!(
            map.get(&GridCoords { i: 0, j: 0 }),
            Some(&Point2::new(1.0, 2.0))
        );
        assert_eq!(
            map.get(&GridCoords { i: 1, j: 0 }),
            Some(&Point2::new(5.0, 6.0))
        );
    }

    #[test]
    fn build_marker_cells_skips_incomplete_cells() {
        let mut map = CornerMap::new();
        map.insert(GridCoords { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        map.insert(GridCoords { i: 1, j: 0 }, Point2::new(1.0, 0.0));
        map.insert(GridCoords { i: 1, j: 1 }, Point2::new(1.0, 1.0));

        let cells = build_marker_cells(&map);
        assert!(cells.is_empty());
    }

    #[test]
    fn build_marker_cells_orders_corners_clockwise() {
        let mut map = CornerMap::new();
        let p00 = Point2::new(0.0, 0.0);
        let p10 = Point2::new(1.0, 0.0);
        let p11 = Point2::new(1.0, 1.0);
        let p01 = Point2::new(0.0, 1.0);
        map.insert(GridCoords { i: 0, j: 0 }, p00);
        map.insert(GridCoords { i: 1, j: 0 }, p10);
        map.insert(GridCoords { i: 1, j: 1 }, p11);
        map.insert(GridCoords { i: 0, j: 1 }, p01);

        let cells = build_marker_cells(&map);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].corners_img, [p00, p10, p11, p01]);
    }
}
