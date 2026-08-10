//! Geometry-only smoothness helpers for square labelled grids.

use crate::Coord;
use nalgebra::{Point2, RealField};
use projective_grid::{expert::lattice::predict_grid_position, LatticeKind};
use std::collections::HashMap;

/// Predict a square-grid corner position from complete cardinal neighbor
/// pairs.
///
/// Uses midpoint averaging:
/// horizontal `0.5 * (P(u-1,v) + P(u+1,v))` and vertical
/// `0.5 * (P(u,v-1) + P(u,v+1))`. Returns `None` when neither pair is
/// present.
pub fn square_predict_grid_position<F: RealField + Copy>(
    grid: &HashMap<Coord, Point2<F>>,
    idx: Coord,
) -> Option<Point2<F>> {
    predict_grid_position(grid, idx, LatticeKind::Square).map(|prediction| prediction.position)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_grid(rows: i32, cols: i32, spacing: f32) -> HashMap<Coord, Point2<f32>> {
        let mut map = HashMap::new();
        for v in 0..rows {
            for u in 0..cols {
                map.insert(
                    Coord::new(u, v),
                    Point2::new(u as f32 * spacing, v as f32 * spacing),
                );
            }
        }
        map
    }

    #[test]
    fn predicts_from_horizontal_and_vertical_midpoints() {
        let grid = make_grid(3, 3, 60.0);
        let pred = square_predict_grid_position(&grid, Coord::new(1, 1)).unwrap();
        assert!((pred.x - 60.0).abs() < 1e-6);
        assert!((pred.y - 60.0).abs() < 1e-6);
    }

    #[test]
    fn predicts_from_one_available_pair() {
        let mut grid = HashMap::new();
        grid.insert(Coord::new(0, 1), Point2::new(0.0, 60.0));
        grid.insert(Coord::new(2, 1), Point2::new(120.0, 60.0));
        let pred = square_predict_grid_position(&grid, Coord::new(1, 1)).unwrap();
        assert_eq!(pred, Point2::new(60.0, 60.0));
    }

    #[test]
    fn isolated_points_are_skipped() {
        let mut grid = HashMap::new();
        grid.insert(Coord::new(0, 0), Point2::new(0.0, 0.0));
        grid.insert(Coord::new(5, 5), Point2::new(300.0, 300.0));
        assert!(square_predict_grid_position(&grid, Coord::new(0, 0)).is_none());
    }
}
