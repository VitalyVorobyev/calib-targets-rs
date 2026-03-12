use calib_targets_aruco::{GridCell, MarkerCell};
use calib_targets_core::{GridCoords, LabeledCorner};
use nalgebra::Point2;
use std::collections::HashMap;

#[cfg(feature = "tracing")]
use tracing::instrument;

pub(crate) type CornerMap = HashMap<GridCoords, Point2<f32>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MarkerCellSource {
    CompleteQuad,
    InferredThreeCorners { missing_corner: usize },
}

#[derive(Clone, Debug)]
pub(crate) struct SampledMarkerCell {
    pub cell: MarkerCell,
    pub source: MarkerCellSource,
}

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
#[cfg(test)]
#[cfg_attr(feature = "tracing", instrument(level = "info", skip(map), fields(corners=map.len())))]
pub(crate) fn build_marker_cells(map: &CornerMap) -> Vec<MarkerCell> {
    build_marker_cell_candidates(map)
        .into_iter()
        .filter_map(|candidate| match candidate.source {
            MarkerCellSource::CompleteQuad => Some(candidate.cell),
            MarkerCellSource::InferredThreeCorners { .. } => None,
        })
        .collect()
}

/// Enumerate square cells from a sparse corner map.
///
/// - complete 2x2 quads are returned directly;
/// - cells with exactly one missing corner are recovered with a local affine
///   completion and kept as auxiliary hypotheses.
#[cfg_attr(feature = "tracing", instrument(level = "info", skip(map), fields(corners=map.len())))]
pub(crate) fn build_marker_cell_candidates(map: &CornerMap) -> Vec<SampledMarkerCell> {
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

            let corners = [
                map.get(&g00).copied(),
                map.get(&g10).copied(),
                map.get(&g11).copied(),
                map.get(&g01).copied(),
            ];

            if let Some(candidate) = build_marker_cell_candidate(i, j, corners) {
                out.push(candidate);
            }
        }
    }

    out
}

fn build_marker_cell_candidate(
    gx: i32,
    gy: i32,
    corners: [Option<Point2<f32>>; 4],
) -> Option<SampledMarkerCell> {
    let present = corners.iter().flatten().count();
    match present {
        4 => {
            let corners_img = [corners[0]?, corners[1]?, corners[2]?, corners[3]?];
            quad_is_valid(&corners_img).then_some(SampledMarkerCell {
                cell: MarkerCell {
                    gc: GridCell { gx, gy },
                    corners_img,
                },
                source: MarkerCellSource::CompleteQuad,
            })
        }
        3 => {
            let missing_corner = corners.iter().position(|corner| corner.is_none())?;
            let corners_img = infer_three_corner_quad(corners, missing_corner)?;
            quad_is_valid(&corners_img).then_some(SampledMarkerCell {
                cell: MarkerCell {
                    gc: GridCell { gx, gy },
                    corners_img,
                },
                source: MarkerCellSource::InferredThreeCorners { missing_corner },
            })
        }
        _ => None,
    }
}

fn infer_three_corner_quad(
    corners: [Option<Point2<f32>>; 4],
    missing_corner: usize,
) -> Option<[Point2<f32>; 4]> {
    let tl = corners[0];
    let tr = corners[1];
    let br = corners[2];
    let bl = corners[3];

    let inferred = match missing_corner {
        0 => point_sum_diff(bl?, tr?, br?),
        1 => point_sum_diff(tl?, br?, bl?),
        2 => point_sum_diff(tr?, bl?, tl?),
        3 => point_sum_diff(tl?, br?, tr?),
        _ => return None,
    };

    Some([
        tl.unwrap_or(inferred),
        tr.unwrap_or(inferred),
        br.unwrap_or(inferred),
        bl.unwrap_or(inferred),
    ])
}

fn point_sum_diff(a: Point2<f32>, b: Point2<f32>, c: Point2<f32>) -> Point2<f32> {
    Point2::from(a.coords + b.coords - c.coords)
}

fn quad_is_valid(quad: &[Point2<f32>; 4]) -> bool {
    let area = polygon_area(quad).abs();
    if !area.is_finite() || area <= 1e-3 {
        return false;
    }

    let mut sign = 0.0f32;
    for idx in 0..4 {
        let p0 = quad[idx];
        let p1 = quad[(idx + 1) % 4];
        let p2 = quad[(idx + 2) % 4];
        let v1 = p1 - p0;
        let v2 = p2 - p1;
        let cross = v1.x * v2.y - v1.y * v2.x;
        if !cross.is_finite() || cross.abs() <= 1e-4 {
            return false;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if cross.signum() != sign {
            return false;
        }
    }

    true
}

fn polygon_area(quad: &[Point2<f32>; 4]) -> f32 {
    let mut area = 0.0f32;
    for idx in 0..4 {
        let p0 = quad[idx];
        let p1 = quad[(idx + 1) % 4];
        area += p0.x * p1.y - p1.x * p0.y;
    }
    0.5 * area
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

    #[test]
    fn build_marker_cell_candidates_infers_single_missing_corner() {
        let mut map = CornerMap::new();
        map.insert(GridCoords { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        map.insert(GridCoords { i: 1, j: 0 }, Point2::new(2.0, 0.0));
        map.insert(GridCoords { i: 1, j: 1 }, Point2::new(2.0, 2.0));

        let candidates = build_marker_cell_candidates(&map);
        assert_eq!(candidates.len(), 1);
        assert_eq!(
            candidates[0].source,
            MarkerCellSource::InferredThreeCorners { missing_corner: 3 }
        );
        assert_eq!(
            candidates[0].cell.corners_img,
            [
                Point2::new(0.0, 0.0),
                Point2::new(2.0, 0.0),
                Point2::new(2.0, 2.0),
                Point2::new(0.0, 2.0),
            ]
        );
    }

    #[test]
    fn build_marker_cell_candidates_rejects_degenerate_quad() {
        let corners = [
            Some(Point2::new(0.0, 0.0)),
            Some(Point2::new(1.0, 0.0)),
            Some(Point2::new(2.0, 0.0)),
            None,
        ];
        assert!(build_marker_cell_candidate(0, 0, corners).is_none());
    }
}
