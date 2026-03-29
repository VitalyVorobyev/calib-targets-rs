//! Grid smoothness analysis: predict corner positions from neighbors.
//!
//! For each grid corner, the expected position can be predicted from its
//! immediate cardinal neighbors via midpoint averaging. Corners that deviate
//! significantly from the prediction are likely false detections.

use crate::grid_index::GridIndex;
use nalgebra::Point2;
use std::collections::HashMap;

/// Predict a grid corner's position from its cardinal neighbors.
///
/// Uses midpoint averaging:
/// - Horizontal: `0.5 * (P(i-1,j) + P(i+1,j))`
/// - Vertical: `0.5 * (P(i,j-1) + P(i,j+1))`
///
/// Returns the average of available predictions, or `None` if no complete
/// neighbor pair exists (need at least one horizontal or vertical pair).
pub fn predict_grid_position(
    grid: &HashMap<GridIndex, Point2<f32>>,
    idx: GridIndex,
) -> Option<Point2<f32>> {
    let mut pred_sum = Point2::new(0.0f32, 0.0f32);
    let mut pred_count = 0u32;

    // Horizontal pair
    let left = GridIndex {
        i: idx.i - 1,
        j: idx.j,
    };
    let right = GridIndex {
        i: idx.i + 1,
        j: idx.j,
    };
    if let (Some(&pl), Some(&pr)) = (grid.get(&left), grid.get(&right)) {
        let mid = Point2::new(0.5 * (pl.x + pr.x), 0.5 * (pl.y + pr.y));
        pred_sum.x += mid.x;
        pred_sum.y += mid.y;
        pred_count += 1;
    }

    // Vertical pair
    let up = GridIndex {
        i: idx.i,
        j: idx.j - 1,
    };
    let down = GridIndex {
        i: idx.i,
        j: idx.j + 1,
    };
    if let (Some(&pu), Some(&pd)) = (grid.get(&up), grid.get(&down)) {
        let mid = Point2::new(0.5 * (pu.x + pd.x), 0.5 * (pu.y + pd.y));
        pred_sum.x += mid.x;
        pred_sum.y += mid.y;
        pred_count += 1;
    }

    if pred_count == 0 {
        return None;
    }

    Some(Point2::new(
        pred_sum.x / pred_count as f32,
        pred_sum.y / pred_count as f32,
    ))
}

/// Find grid corners whose position deviates from the neighbor-predicted
/// position by more than `threshold` pixels.
///
/// Returns `(grid_index, predicted_position)` for each inconsistent corner.
pub fn find_inconsistent_corners(
    grid: &HashMap<GridIndex, Point2<f32>>,
    threshold: f32,
) -> Vec<(GridIndex, Point2<f32>)> {
    let threshold_sq = threshold * threshold;
    let mut flagged = Vec::new();

    for (&idx, &pos) in grid {
        if let Some(predicted) = predict_grid_position(grid, idx) {
            let dx = pos.x - predicted.x;
            let dy = pos.y - predicted.y;
            if dx * dx + dy * dy > threshold_sq {
                flagged.push((idx, predicted));
            }
        }
    }

    flagged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_grid(rows: i32, cols: i32, spacing: f32) -> HashMap<GridIndex, Point2<f32>> {
        let mut map = HashMap::new();
        for j in 0..rows {
            for i in 0..cols {
                map.insert(
                    GridIndex { i, j },
                    Point2::new(i as f32 * spacing, j as f32 * spacing),
                );
            }
        }
        map
    }

    #[test]
    fn clean_grid_has_no_inconsistencies() {
        let grid = make_grid(5, 5, 60.0);
        let flagged = find_inconsistent_corners(&grid, 3.0);
        assert!(flagged.is_empty());
    }

    #[test]
    fn displaced_corner_is_flagged() {
        let mut grid = make_grid(3, 3, 60.0);
        let center = GridIndex { i: 1, j: 1 };
        grid.insert(center, Point2::new(69.0, 69.0)); // displaced by 9px each axis

        let flagged = find_inconsistent_corners(&grid, 3.0);
        assert_eq!(1, flagged.len());
        assert_eq!(center, flagged[0].0);

        // Predicted position should be the midpoint of neighbors = (60, 60)
        let pred = flagged[0].1;
        assert!((pred.x - 60.0).abs() < 0.01);
        assert!((pred.y - 60.0).abs() < 0.01);
    }

    #[test]
    fn perspective_distorted_grid_passes() {
        let spacing = 60.0;
        let mut grid = HashMap::new();
        for j in 0..5 {
            let scale = 1.0 + 0.02 * j as f32;
            for i in 0..5 {
                grid.insert(
                    GridIndex { i, j },
                    Point2::new(i as f32 * spacing * scale, j as f32 * spacing * scale),
                );
            }
        }

        // Mild perspective should not flag anything at a 3px threshold
        let flagged = find_inconsistent_corners(&grid, 3.0);
        assert!(flagged.is_empty());
    }

    #[test]
    fn isolated_corners_are_skipped() {
        let mut grid = HashMap::new();
        grid.insert(GridIndex { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        grid.insert(GridIndex { i: 5, j: 5 }, Point2::new(300.0, 300.0));

        let flagged = find_inconsistent_corners(&grid, 3.0);
        assert!(flagged.is_empty());
    }

    #[test]
    fn predict_from_single_pair() {
        let mut grid = HashMap::new();
        grid.insert(GridIndex { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        grid.insert(GridIndex { i: 1, j: 0 }, Point2::new(60.0, 0.0));
        grid.insert(GridIndex { i: 2, j: 0 }, Point2::new(120.0, 0.0));

        let pred = predict_grid_position(&grid, GridIndex { i: 1, j: 0 }).unwrap();
        assert!((pred.x - 60.0).abs() < 0.01);
        assert!((pred.y - 0.0).abs() < 0.01);
    }
}
