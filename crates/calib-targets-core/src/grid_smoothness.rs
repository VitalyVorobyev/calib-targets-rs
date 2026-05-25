//! Geometry-only smoothness helpers for square labelled grids.

use crate::GridCoords;
use nalgebra::{Point2, RealField};
use std::collections::HashMap;

fn lit<F: RealField + Copy>(val: f64) -> F {
    F::from_subset(&val)
}

/// Predict a square-grid corner position from complete cardinal neighbor
/// pairs.
///
/// Uses midpoint averaging:
/// horizontal `0.5 * (P(i-1,j) + P(i+1,j))` and vertical
/// `0.5 * (P(i,j-1) + P(i,j+1))`. Returns `None` when neither pair is
/// present.
pub fn square_predict_grid_position<F: RealField + Copy>(
    grid: &HashMap<GridCoords, Point2<F>>,
    idx: GridCoords,
) -> Option<Point2<F>> {
    let half: F = lit(0.5);
    let mut pred_sum = Point2::new(F::zero(), F::zero());
    let mut pred_count = 0u32;

    let left = GridCoords {
        i: idx.i - 1,
        j: idx.j,
    };
    let right = GridCoords {
        i: idx.i + 1,
        j: idx.j,
    };
    if let (Some(pl), Some(pr)) = (grid.get(&left), grid.get(&right)) {
        let mid = Point2::new(half * (pl.x + pr.x), half * (pl.y + pr.y));
        pred_sum.x += mid.x;
        pred_sum.y += mid.y;
        pred_count += 1;
    }

    let up = GridCoords {
        i: idx.i,
        j: idx.j - 1,
    };
    let down = GridCoords {
        i: idx.i,
        j: idx.j + 1,
    };
    if let (Some(pu), Some(pd)) = (grid.get(&up), grid.get(&down)) {
        let mid = Point2::new(half * (pu.x + pd.x), half * (pu.y + pd.y));
        pred_sum.x += mid.x;
        pred_sum.y += mid.y;
        pred_count += 1;
    }

    if pred_count == 0 {
        return None;
    }

    let n: F = lit(pred_count as f64);
    Some(Point2::new(pred_sum.x / n, pred_sum.y / n))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_grid(rows: i32, cols: i32, spacing: f32) -> HashMap<GridCoords, Point2<f32>> {
        let mut map = HashMap::new();
        for j in 0..rows {
            for i in 0..cols {
                map.insert(
                    GridCoords { i, j },
                    Point2::new(i as f32 * spacing, j as f32 * spacing),
                );
            }
        }
        map
    }

    #[test]
    fn predicts_from_horizontal_and_vertical_midpoints() {
        let grid = make_grid(3, 3, 60.0);
        let pred = square_predict_grid_position(&grid, GridCoords { i: 1, j: 1 }).unwrap();
        assert!((pred.x - 60.0).abs() < 1e-6);
        assert!((pred.y - 60.0).abs() < 1e-6);
    }

    #[test]
    fn predicts_from_one_available_pair() {
        let mut grid = HashMap::new();
        grid.insert(GridCoords { i: 0, j: 1 }, Point2::new(0.0, 60.0));
        grid.insert(GridCoords { i: 2, j: 1 }, Point2::new(120.0, 60.0));
        let pred = square_predict_grid_position(&grid, GridCoords { i: 1, j: 1 }).unwrap();
        assert_eq!(pred, Point2::new(60.0, 60.0));
    }

    #[test]
    fn isolated_points_are_skipped() {
        let mut grid = HashMap::new();
        grid.insert(GridCoords { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        grid.insert(GridCoords { i: 5, j: 5 }, Point2::new(300.0, 300.0));
        assert!(square_predict_grid_position(&grid, GridCoords { i: 0, j: 0 }).is_none());
    }
}
