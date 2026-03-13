use calib_targets_aruco::{GridCell, MarkerCell};
use calib_targets_core::{estimate_homography_rect_to_img, GridCoords, LabeledCorner};
use nalgebra::{Point2, Vector2};
use std::collections::HashMap;

#[cfg(feature = "tracing")]
use tracing::instrument;

pub(crate) type CornerMap = HashMap<GridCoords, Point2<f32>>;

const LOCAL_LATTICE_RADII: [i32; 2] = [1, 2];
const MIN_LOCAL_SUPPORT_POINTS: usize = 4;
const MAX_LOCAL_SUPPORT_POINTS: usize = 8;
const MIN_LOCAL_STEP_PX: f32 = 1e-3;
const MIN_LOCAL_REPROJECTION_TOLERANCE_PX: f32 = 1.5;
const MAX_LOCAL_REPROJECTION_TOLERANCE_RATIO: f32 = 0.20;
const MAX_AXIS_PREDICTION_DISAGREEMENT_RATIO: f32 = 0.25;

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
/// - cells with exactly one missing corner are recovered with a local
///   lattice fit when enough nearby support exists, otherwise they fall back
///   to a simple parallelogram completion and stay auxiliary hypotheses.
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

            if let Some(candidate) = build_marker_cell_candidate(map, i, j, corners) {
                out.push(candidate);
            }
        }
    }

    out
}

fn build_marker_cell_candidate(
    map: &CornerMap,
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
            let corners_img = infer_three_corner_quad(map, gx, gy, corners, missing_corner)?;
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
    map: &CornerMap,
    gx: i32,
    gy: i32,
    corners: [Option<Point2<f32>>; 4],
    missing_corner: usize,
) -> Option<[Point2<f32>; 4]> {
    let cell_grids = cell_corner_grids(gx, gy);
    let inferred = infer_missing_corner_from_local_lattice(map, &cell_grids, corners)
        .filter(|&point| quad_is_valid(&build_quad_with_inferred(corners, point)))
        .or_else(|| {
            infer_missing_corner_from_axis_steps(map, &cell_grids, corners)
                .filter(|&point| quad_is_valid(&build_quad_with_inferred(corners, point)))
        })
        .or_else(|| infer_missing_corner_parallelogram(corners, missing_corner))?;

    Some(build_quad_with_inferred(corners, inferred))
}

fn infer_missing_corner_from_local_lattice(
    map: &CornerMap,
    cell_grids: &[GridCoords; 4],
    corners: [Option<Point2<f32>>; 4],
) -> Option<Point2<f32>> {
    let missing_corner = corners.iter().position(|corner| corner.is_none())?;

    for radius in LOCAL_LATTICE_RADII {
        if !missing_corner_has_axis_support(map, cell_grids, missing_corner, radius) {
            continue;
        }

        let (grid_pts, image_pts) = local_lattice_support(map, cell_grids, missing_corner, radius);
        if grid_pts.len() < MIN_LOCAL_SUPPORT_POINTS {
            continue;
        }

        let homography = estimate_homography_rect_to_img(&grid_pts, &image_pts)?;
        if !local_fit_reprojects_known_corners(
            &homography,
            map,
            cell_grids,
            corners,
            local_reprojection_tolerance_px(map, cell_grids, radius),
        ) {
            continue;
        }

        let inferred = homography.apply(grid_point(cell_grids[missing_corner]));
        if inferred.x.is_finite() && inferred.y.is_finite() {
            return Some(inferred);
        }
    }

    None
}

fn infer_missing_corner_from_axis_steps(
    map: &CornerMap,
    cell_grids: &[GridCoords; 4],
    corners: [Option<Point2<f32>>; 4],
) -> Option<Point2<f32>> {
    let missing_corner = corners.iter().position(|corner| corner.is_none())?;
    let missing_grid = cell_grids[missing_corner];

    for radius in LOCAL_LATTICE_RADII {
        let horizontal = estimate_horizontal_step(map, cell_grids, missing_grid, radius);
        let vertical = estimate_vertical_step(map, cell_grids, missing_grid, radius);
        let predictions =
            axis_missing_corner_predictions(corners, missing_corner, horizontal, vertical);
        if predictions.is_empty() {
            continue;
        }

        let tolerance_px = axis_prediction_tolerance_px(horizontal, vertical);
        if let Some(inferred) = combine_axis_predictions(&predictions, tolerance_px) {
            return Some(inferred);
        }
    }

    None
}

fn missing_corner_has_axis_support(
    map: &CornerMap,
    cell_grids: &[GridCoords; 4],
    missing_corner: usize,
    radius: i32,
) -> bool {
    let missing = cell_grids[missing_corner];
    let min_i = cell_grids.iter().map(|grid| grid.i).min().unwrap_or(0) - radius;
    let max_i = cell_grids.iter().map(|grid| grid.i).max().unwrap_or(0) + radius;
    let min_j = cell_grids.iter().map(|grid| grid.j).min().unwrap_or(0) - radius;
    let max_j = cell_grids.iter().map(|grid| grid.j).max().unwrap_or(0) + radius;

    let horizontal = (min_i..max_i).any(|i| {
        map.contains_key(&GridCoords { i, j: missing.j })
            && map.contains_key(&GridCoords {
                i: i + 1,
                j: missing.j,
            })
    });
    let vertical = (min_j..max_j).any(|j| {
        map.contains_key(&GridCoords { i: missing.i, j })
            && map.contains_key(&GridCoords {
                i: missing.i,
                j: j + 1,
            })
    });

    horizontal && vertical
}

fn estimate_horizontal_step(
    map: &CornerMap,
    cell_grids: &[GridCoords; 4],
    missing_grid: GridCoords,
    radius: i32,
) -> Option<Vector2<f32>> {
    let min_i = cell_grids.iter().map(|grid| grid.i).min().unwrap_or(0) - radius;
    let max_i = cell_grids.iter().map(|grid| grid.i).max().unwrap_or(0) + radius;
    let target_mid_i = missing_grid.i as f32 + 0.5;

    let mut samples = Vec::new();
    for i in min_i..max_i {
        let start = GridCoords {
            i,
            j: missing_grid.j,
        };
        let end = GridCoords {
            i: i + 1,
            j: missing_grid.j,
        };
        let (Some(&p0), Some(&p1)) = (map.get(&start), map.get(&end)) else {
            continue;
        };
        samples.push((((i as f32 + 0.5) - target_mid_i).abs(), p1 - p0));
    }

    combine_step_vectors(samples)
}

fn estimate_vertical_step(
    map: &CornerMap,
    cell_grids: &[GridCoords; 4],
    missing_grid: GridCoords,
    radius: i32,
) -> Option<Vector2<f32>> {
    let min_j = cell_grids.iter().map(|grid| grid.j).min().unwrap_or(0) - radius;
    let max_j = cell_grids.iter().map(|grid| grid.j).max().unwrap_or(0) + radius;
    let target_mid_j = missing_grid.j as f32 + 0.5;

    let mut samples = Vec::new();
    for j in min_j..max_j {
        let start = GridCoords {
            i: missing_grid.i,
            j,
        };
        let end = GridCoords {
            i: missing_grid.i,
            j: j + 1,
        };
        let (Some(&p0), Some(&p1)) = (map.get(&start), map.get(&end)) else {
            continue;
        };
        samples.push((((j as f32 + 0.5) - target_mid_j).abs(), p1 - p0));
    }

    combine_step_vectors(samples)
}

fn combine_step_vectors(samples: Vec<(f32, Vector2<f32>)>) -> Option<Vector2<f32>> {
    let mut samples: Vec<(f32, Vector2<f32>)> = samples
        .into_iter()
        .filter(|(_, step)| {
            step.x.is_finite() && step.y.is_finite() && step.norm() > MIN_LOCAL_STEP_PX
        })
        .collect();
    if samples.is_empty() {
        return None;
    }

    samples.sort_by(|(a_dist, a_step), (b_dist, b_step)| {
        a_dist
            .total_cmp(b_dist)
            .then_with(|| a_step.norm().total_cmp(&b_step.norm()))
    });

    let base = samples[0].1;
    let base_norm = base.norm();
    let mut weighted = Vector2::new(0.0, 0.0);
    let mut weight_sum = 0.0f32;

    for (distance, step) in samples.into_iter().take(3) {
        let step_norm = step.norm();
        let dot = base.dot(&step);
        if dot <= 0.0 {
            continue;
        }
        let ratio = step_norm / base_norm;
        if !(0.5..=2.0).contains(&ratio) {
            continue;
        }
        let weight = 1.0 / (1.0 + distance);
        weighted += step * weight;
        weight_sum += weight;
    }

    (weight_sum > 0.0).then_some(weighted / weight_sum)
}

fn axis_missing_corner_predictions(
    corners: [Option<Point2<f32>>; 4],
    missing_corner: usize,
    horizontal: Option<Vector2<f32>>,
    vertical: Option<Vector2<f32>>,
) -> Vec<Point2<f32>> {
    let tl = corners[0];
    let tr = corners[1];
    let br = corners[2];
    let bl = corners[3];
    let mut predictions = Vec::with_capacity(2);

    match missing_corner {
        0 => {
            if let (Some(tr), Some(step)) = (tr, horizontal) {
                predictions.push(tr - step);
            }
            if let (Some(bl), Some(step)) = (bl, vertical) {
                predictions.push(bl - step);
            }
        }
        1 => {
            if let (Some(tl), Some(step)) = (tl, horizontal) {
                predictions.push(tl + step);
            }
            if let (Some(br), Some(step)) = (br, vertical) {
                predictions.push(br - step);
            }
        }
        2 => {
            if let (Some(bl), Some(step)) = (bl, horizontal) {
                predictions.push(bl + step);
            }
            if let (Some(tr), Some(step)) = (tr, vertical) {
                predictions.push(tr + step);
            }
        }
        3 => {
            if let (Some(br), Some(step)) = (br, horizontal) {
                predictions.push(br - step);
            }
            if let (Some(tl), Some(step)) = (tl, vertical) {
                predictions.push(tl + step);
            }
        }
        _ => {}
    }

    predictions
}

fn axis_prediction_tolerance_px(
    horizontal: Option<Vector2<f32>>,
    vertical: Option<Vector2<f32>>,
) -> f32 {
    let mut scales = Vec::new();
    if let Some(step) = horizontal {
        scales.push(step.norm());
    }
    if let Some(step) = vertical {
        scales.push(step.norm());
    }
    let scale = if scales.is_empty() {
        0.0
    } else {
        scales.iter().sum::<f32>() / scales.len() as f32
    };
    (scale * MAX_AXIS_PREDICTION_DISAGREEMENT_RATIO).max(MIN_LOCAL_REPROJECTION_TOLERANCE_PX)
}

fn combine_axis_predictions(predictions: &[Point2<f32>], tolerance_px: f32) -> Option<Point2<f32>> {
    match predictions {
        [] => None,
        [only] => Some(*only),
        [a, b] => {
            if point_distance(*a, *b) > tolerance_px {
                return None;
            }
            Some(Point2::from((a.coords + b.coords) * 0.5))
        }
        _ => None,
    }
}

fn infer_missing_corner_parallelogram(
    corners: [Option<Point2<f32>>; 4],
    missing_corner: usize,
) -> Option<Point2<f32>> {
    let tl = corners[0];
    let tr = corners[1];
    let br = corners[2];
    let bl = corners[3];

    match missing_corner {
        0 => Some(point_sum_diff(bl?, tr?, br?)),
        1 => Some(point_sum_diff(tl?, br?, bl?)),
        2 => Some(point_sum_diff(tr?, bl?, tl?)),
        3 => Some(point_sum_diff(tl?, br?, tr?)),
        _ => None,
    }
}

fn cell_corner_grids(gx: i32, gy: i32) -> [GridCoords; 4] {
    [
        GridCoords { i: gx, j: gy },
        GridCoords { i: gx + 1, j: gy },
        GridCoords {
            i: gx + 1,
            j: gy + 1,
        },
        GridCoords { i: gx, j: gy + 1 },
    ]
}

fn grid_point(grid: GridCoords) -> Point2<f32> {
    Point2::new(grid.i as f32, grid.j as f32)
}

fn local_lattice_support(
    map: &CornerMap,
    cell_grids: &[GridCoords; 4],
    missing_corner: usize,
    radius: i32,
) -> (Vec<Point2<f32>>, Vec<Point2<f32>>) {
    let min_i = cell_grids.iter().map(|grid| grid.i).min().unwrap_or(0) - radius;
    let max_i = cell_grids.iter().map(|grid| grid.i).max().unwrap_or(0) + radius;
    let min_j = cell_grids.iter().map(|grid| grid.j).min().unwrap_or(0) - radius;
    let max_j = cell_grids.iter().map(|grid| grid.j).max().unwrap_or(0) + radius;
    let missing_grid = cell_grids[missing_corner];
    let cell_center = Point2::new(
        0.5 * (cell_grids[0].i + cell_grids[2].i) as f32,
        0.5 * (cell_grids[0].j + cell_grids[2].j) as f32,
    );

    let mut support: Vec<(i32, i32, i32, Point2<f32>)> = map
        .iter()
        .filter(|(grid, _)| {
            grid.i >= min_i
                && grid.i <= max_i
                && grid.j >= min_j
                && grid.j <= max_j
                && (grid.i == missing_grid.i
                    || grid.j == missing_grid.j
                    || cell_grids.contains(grid))
        })
        .map(|(grid, &point)| {
            let chebyshev = ((grid.i as f32 - cell_center.x).abs() * 2.0)
                .max((grid.j as f32 - cell_center.y).abs() * 2.0)
                .round() as i32;
            (chebyshev, grid.j, grid.i, point)
        })
        .collect();
    support.sort_by_key(|(chebyshev, j, i, _)| (*chebyshev, *j, *i));

    let mut grid_pts = Vec::with_capacity(support.len());
    let mut image_pts = Vec::with_capacity(support.len());
    for (_, j, i, point) in support.into_iter().take(MAX_LOCAL_SUPPORT_POINTS) {
        grid_pts.push(Point2::new(i as f32, j as f32));
        image_pts.push(point);
    }

    (grid_pts, image_pts)
}

fn local_fit_reprojects_known_corners(
    homography: &calib_targets_core::Homography,
    map: &CornerMap,
    cell_grids: &[GridCoords; 4],
    corners: [Option<Point2<f32>>; 4],
    tolerance_px: f32,
) -> bool {
    corners.iter().enumerate().all(|(idx, observed)| {
        let Some(observed) = observed else {
            return true;
        };
        let predicted = homography.apply(grid_point(cell_grids[idx]));
        if !predicted.x.is_finite() || !predicted.y.is_finite() {
            return false;
        }
        let dx = predicted.x - observed.x;
        let dy = predicted.y - observed.y;
        let error = (dx * dx + dy * dy).sqrt();
        error <= tolerance_px
            && map
                .get(&cell_grids[idx])
                .is_some_and(|point| (*point - *observed).norm() <= 1e-6)
    })
}

fn local_reprojection_tolerance_px(
    map: &CornerMap,
    cell_grids: &[GridCoords; 4],
    radius: i32,
) -> f32 {
    let step = local_support_step_px(map, cell_grids, radius).unwrap_or(0.0);
    (step * MAX_LOCAL_REPROJECTION_TOLERANCE_RATIO).max(MIN_LOCAL_REPROJECTION_TOLERANCE_PX)
}

fn local_support_step_px(
    map: &CornerMap,
    cell_grids: &[GridCoords; 4],
    radius: i32,
) -> Option<f32> {
    let min_i = cell_grids.iter().map(|grid| grid.i).min().unwrap_or(0) - radius;
    let max_i = cell_grids.iter().map(|grid| grid.i).max().unwrap_or(0) + radius;
    let min_j = cell_grids.iter().map(|grid| grid.j).min().unwrap_or(0) - radius;
    let max_j = cell_grids.iter().map(|grid| grid.j).max().unwrap_or(0) + radius;

    let mut steps = Vec::new();
    for (&grid, &point) in map {
        if grid.i < min_i || grid.i > max_i || grid.j < min_j || grid.j > max_j {
            continue;
        }

        if let Some(next) = map.get(&GridCoords {
            i: grid.i + 1,
            j: grid.j,
        }) {
            steps.push((*next - point).norm());
        }
        if let Some(next) = map.get(&GridCoords {
            i: grid.i,
            j: grid.j + 1,
        }) {
            steps.push((*next - point).norm());
        }
    }

    steps.retain(|step| step.is_finite() && *step > MIN_LOCAL_STEP_PX);
    if steps.is_empty() {
        return None;
    }

    steps.sort_by(|a, b| a.total_cmp(b));
    Some(steps[steps.len() / 2])
}

fn point_sum_diff(a: Point2<f32>, b: Point2<f32>, c: Point2<f32>) -> Point2<f32> {
    Point2::from(a.coords + b.coords - c.coords)
}

fn build_quad_with_inferred(
    corners: [Option<Point2<f32>>; 4],
    inferred: Point2<f32>,
) -> [Point2<f32>; 4] {
    [
        corners[0].unwrap_or(inferred),
        corners[1].unwrap_or(inferred),
        corners[2].unwrap_or(inferred),
        corners[3].unwrap_or(inferred),
    ]
}

fn point_distance(a: Point2<f32>, b: Point2<f32>) -> f32 {
    let delta = a - b;
    (delta.x * delta.x + delta.y * delta.y).sqrt()
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
    use calib_targets_core::Homography;

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
        let map = CornerMap::new();
        let corners = [
            Some(Point2::new(0.0, 0.0)),
            Some(Point2::new(1.0, 0.0)),
            Some(Point2::new(2.0, 0.0)),
            None,
        ];
        assert!(build_marker_cell_candidate(&map, 0, 0, corners).is_none());
    }

    #[test]
    fn build_marker_cell_candidates_use_local_lattice_support_for_all_missing_corners() {
        for missing_corner in 0..4 {
            assert_projective_local_lattice_case(missing_corner);
        }
    }

    fn assert_projective_local_lattice_case(missing_corner: usize) {
        let mut map = projective_corner_map();
        let grids = cell_corner_grids(1, 1);
        let expected = map
            .remove(&grids[missing_corner])
            .expect("removed support corner should exist");
        let sparse_corners = grids.map(|grid| map.get(&grid).copied());

        let candidate = build_marker_cell_candidates(&map)
            .into_iter()
            .find(|candidate| candidate.cell.gc.gx == 1 && candidate.cell.gc.gy == 1)
            .expect("expected inferred candidate for center cell");

        assert_eq!(
            candidate.source,
            MarkerCellSource::InferredThreeCorners { missing_corner }
        );

        let inferred = candidate.cell.corners_img[missing_corner];
        let local_error = point_distance(inferred, expected);
        assert!(
            local_error <= 0.05,
            "expected local lattice fit to recover missing corner within 0.05 px, got {local_error}"
        );

        let fallback = infer_missing_corner_parallelogram(sparse_corners, missing_corner)
            .expect("parallelogram fallback should still exist");
        let fallback_error = point_distance(fallback, expected);
        assert!(
            local_error + 0.1 <= fallback_error,
            "expected local lattice fit to beat parallelogram fallback by at least 0.1 px, got local={local_error} fallback={fallback_error}"
        );
    }

    fn projective_corner_map() -> CornerMap {
        let homography =
            Homography::from_array([[2.0, 0.55, 12.0], [0.30, 1.5, 8.0], [0.28, 0.18, 1.0]]);
        let mut map = CornerMap::new();
        for j in 0..=3 {
            for i in 0..=3 {
                map.insert(
                    GridCoords { i, j },
                    homography.apply(Point2::new(i as f32, j as f32)),
                );
            }
        }
        map
    }
}
