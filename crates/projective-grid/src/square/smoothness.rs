//! Grid smoothness analysis: predict corner positions from neighbors.
//!
//! For each grid corner, the expected position can be predicted from its
//! immediate cardinal neighbors via midpoint averaging. Corners that deviate
//! significantly from the prediction are likely false detections.

use crate::float_helpers::lit;
use crate::local_step::LocalStep;
use crate::Float;
use crate::GridCoords;
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
///
/// Use this function for post-grow outlier detection via
/// [`square_find_inconsistent_corners`]. For in-the-loop BFS attachment with
/// arbitrary neighbour lists and a global-step fallback, see
/// [`crate::square::grow::predict_from_neighbours`].
pub fn square_predict_grid_position<F: Float>(
    grid: &HashMap<GridCoords, Point2<F>>,
    idx: GridCoords,
) -> Option<Point2<F>> {
    let half: F = lit(0.5);
    let mut pred_sum = Point2::new(F::zero(), F::zero());
    let mut pred_count = 0u32;

    // Horizontal pair
    let left = GridCoords {
        i: idx.i - 1,
        j: idx.j,
    };
    let right = GridCoords {
        i: idx.i + 1,
        j: idx.j,
    };
    if let (Some(&pl), Some(&pr)) = (grid.get(&left), grid.get(&right)) {
        let mid = Point2::new(half * (pl.x + pr.x), half * (pl.y + pr.y));
        pred_sum.x += mid.x;
        pred_sum.y += mid.y;
        pred_count += 1;
    }

    // Vertical pair
    let up = GridCoords {
        i: idx.i,
        j: idx.j - 1,
    };
    let down = GridCoords {
        i: idx.i,
        j: idx.j + 1,
    };
    if let (Some(&pu), Some(&pd)) = (grid.get(&up), grid.get(&down)) {
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

/// Find grid corners whose position deviates from the neighbor-predicted
/// position by more than `threshold` pixels.
///
/// Returns `(grid_index, predicted_position)` for each inconsistent corner.
pub fn square_find_inconsistent_corners<F: Float>(
    grid: &HashMap<GridCoords, Point2<F>>,
    threshold: F,
) -> Vec<(GridCoords, Point2<F>)> {
    let threshold_sq = threshold * threshold;
    let mut flagged = Vec::new();

    for (&idx, &pos) in grid {
        if let Some(predicted) = square_predict_grid_position(grid, idx) {
            let dx = pos.x - predicted.x;
            let dy = pos.y - predicted.y;
            if dx * dx + dy * dy > threshold_sq {
                flagged.push((idx, predicted));
            }
        }
    }

    flagged
}

/// Step-aware variant of [`square_find_inconsistent_corners`].
///
/// Flags a corner when `|pos - predicted| > threshold_rel * local_step`, where
/// the local step is the average of the corner's `(step_u, step_v)` as
/// estimated by [`crate::estimate_local_steps`]. This catches the case where a
/// false corner sits in the right direction but at a fraction of the expected
/// spacing — a subtle failure mode that absolute-pixel thresholds can miss
/// when the grid is viewed at an unexpectedly large or small scale.
///
/// Corners without a local-step entry, or whose local step has zero
/// confidence, fall back to the absolute pixel threshold in
/// [`square_find_inconsistent_corners`]. Corners without enough neighbors for a
/// position prediction are skipped (same behaviour as the non-step variant).
pub fn square_find_inconsistent_corners_step_aware<F: Float>(
    grid: &HashMap<GridCoords, Point2<F>>,
    local_steps: &HashMap<GridCoords, LocalStep<F>>,
    threshold_rel: F,
    threshold_px_floor: F,
) -> Vec<(GridCoords, Point2<F>)> {
    let half: F = lit(0.5);
    let mut flagged = Vec::new();
    let floor_sq = threshold_px_floor * threshold_px_floor;

    for (&idx, &pos) in grid {
        let Some(predicted) = square_predict_grid_position(grid, idx) else {
            continue;
        };
        let dx = pos.x - predicted.x;
        let dy = pos.y - predicted.y;
        let err_sq = dx * dx + dy * dy;

        let step_threshold_sq = match local_steps.get(&idx) {
            Some(ls) if ls.confidence > F::zero() => {
                let step_mean = (ls.step_u + ls.step_v) * half;
                if step_mean <= F::zero() {
                    floor_sq
                } else {
                    let t = threshold_rel * step_mean;
                    t * t
                }
            }
            _ => floor_sq,
        };

        if err_sq > step_threshold_sq {
            flagged.push((idx, predicted));
        }
    }

    flagged
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
    fn clean_grid_has_no_inconsistencies() {
        let grid = make_grid(5, 5, 60.0);
        let flagged = square_find_inconsistent_corners(&grid, 3.0);
        assert!(flagged.is_empty());
    }

    #[test]
    fn displaced_corner_is_flagged() {
        let mut grid = make_grid(3, 3, 60.0);
        let center = GridCoords { i: 1, j: 1 };
        grid.insert(center, Point2::new(69.0, 69.0)); // displaced by 9px each axis

        let flagged = square_find_inconsistent_corners(&grid, 3.0);
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
                    GridCoords { i, j },
                    Point2::new(i as f32 * spacing * scale, j as f32 * spacing * scale),
                );
            }
        }

        // Mild perspective should not flag anything at a 3px threshold
        let flagged = square_find_inconsistent_corners(&grid, 3.0);
        assert!(flagged.is_empty());
    }

    #[test]
    fn isolated_corners_are_skipped() {
        let mut grid = HashMap::new();
        grid.insert(GridCoords { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        grid.insert(GridCoords { i: 5, j: 5 }, Point2::new(300.0, 300.0));

        let flagged = square_find_inconsistent_corners(&grid, 3.0);
        assert!(flagged.is_empty());
    }

    fn local_step_map(
        grid: &HashMap<GridCoords, Point2<f32>>,
        step: f32,
    ) -> HashMap<GridCoords, LocalStep<f32>> {
        grid.keys()
            .map(|&idx| {
                (
                    idx,
                    LocalStep {
                        step_u: step,
                        step_v: step,
                        confidence: 1.0,
                        supporters_u: 4,
                        supporters_v: 4,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn step_aware_flags_wrong_relative_distance() {
        let spacing = 60.0;
        let mut grid = make_grid(3, 3, spacing);
        // Keep neighbors intact; displace the center by 0.4 × spacing.
        let center = GridCoords { i: 1, j: 1 };
        let displacement = 0.4 * spacing;
        grid.insert(
            center,
            Point2::new(spacing + displacement, spacing + displacement),
        );
        let steps = local_step_map(&grid, spacing);

        // 20% of step = 12 px floor would miss a 24 px displacement is 40% of step.
        // But we want to catch 40% displacements → threshold_rel 0.2.
        let flagged = square_find_inconsistent_corners_step_aware(&grid, &steps, 0.2, 2.0);
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].0, center);
    }

    #[test]
    fn step_aware_preserves_floor_when_step_missing() {
        let spacing = 60.0;
        let mut grid = make_grid(3, 3, spacing);
        grid.insert(
            GridCoords { i: 1, j: 1 },
            Point2::new(spacing + 5.0, spacing),
        );
        let steps: HashMap<GridCoords, LocalStep<f32>> = HashMap::new();

        // Floor of 3.0 catches the 5 px displacement.
        let tight = square_find_inconsistent_corners_step_aware(&grid, &steps, 0.2, 3.0);
        assert_eq!(tight.len(), 1);

        // Floor of 10.0 does not.
        let loose = square_find_inconsistent_corners_step_aware(&grid, &steps, 0.2, 10.0);
        assert!(loose.is_empty());
    }

    #[test]
    fn predict_from_single_pair() {
        let mut grid = HashMap::new();
        grid.insert(GridCoords { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        grid.insert(GridCoords { i: 1, j: 0 }, Point2::new(60.0, 0.0));
        grid.insert(GridCoords { i: 2, j: 0 }, Point2::new(120.0, 0.0));

        let pred = square_predict_grid_position(&grid, GridCoords { i: 1, j: 0 }).unwrap();
        assert!((pred.x - 60.0f32).abs() < 0.01);
        assert!((pred.y - 0.0f32).abs() < 0.01);
    }
}
