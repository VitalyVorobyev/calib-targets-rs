//! Hex grid smoothness analysis: predict corner positions from neighbors.
//!
//! For each hex grid corner, the expected position can be predicted from its
//! immediate neighbors along three axial directions via midpoint averaging.
//! Corners that deviate significantly from the prediction are likely false detections.

use crate::float_helpers::lit;
use crate::Float;
use crate::GridIndex;
use nalgebra::Point2;
use std::collections::HashMap;

/// Opposite-pair deltas for the three hex axes (axial coordinates).
///
/// Each entry is `((dq_neg, dr_neg), (dq_pos, dr_pos))`.
const HEX_AXIS_PAIRS: [((i32, i32), (i32, i32)); 3] = [
    // E/W axis
    ((-1, 0), (1, 0)),
    // NE/SW axis
    ((1, -1), (-1, 1)),
    // NW/SE axis
    ((0, -1), (0, 1)),
];

/// Predict a hex grid corner's position from its neighbors along three axes.
///
/// Uses midpoint averaging on up to three opposite-direction pairs:
/// - E/W: midpoint of `(q-1, r)` and `(q+1, r)`
/// - NE/SW: midpoint of `(q+1, r-1)` and `(q-1, r+1)`
/// - NW/SE: midpoint of `(q, r-1)` and `(q, r+1)`
///
/// Returns the average of available predictions, or `None` if no complete
/// neighbor pair exists.
pub fn hex_predict_grid_position<F: Float>(
    grid: &HashMap<GridIndex, Point2<F>>,
    idx: GridIndex,
) -> Option<Point2<F>> {
    let half: F = lit(0.5);
    let mut pred_sum = Point2::new(F::zero(), F::zero());
    let mut pred_count = 0u32;

    for &((dq_a, dr_a), (dq_b, dr_b)) in &HEX_AXIS_PAIRS {
        let a = GridIndex {
            i: idx.i + dq_a,
            j: idx.j + dr_a,
        };
        let b = GridIndex {
            i: idx.i + dq_b,
            j: idx.j + dr_b,
        };
        if let (Some(&pa), Some(&pb)) = (grid.get(&a), grid.get(&b)) {
            pred_sum.x += half * (pa.x + pb.x);
            pred_sum.y += half * (pa.y + pb.y);
            pred_count += 1;
        }
    }

    if pred_count == 0 {
        return None;
    }

    let n: F = lit(pred_count as f64);
    Some(Point2::new(pred_sum.x / n, pred_sum.y / n))
}

/// Find hex grid corners whose position deviates from the neighbor-predicted
/// position by more than `threshold` pixels.
///
/// Returns `(grid_index, predicted_position)` for each inconsistent corner.
pub fn hex_find_inconsistent_corners<F: Float>(
    grid: &HashMap<GridIndex, Point2<F>>,
    threshold: F,
) -> Vec<(GridIndex, Point2<F>)> {
    let threshold_sq = threshold * threshold;
    let mut flagged = Vec::new();

    for (&idx, &pos) in grid {
        if let Some(predicted) = hex_predict_grid_position(grid, idx) {
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

    fn make_hex_grid(radius: i32, spacing: f32) -> HashMap<GridIndex, Point2<f32>> {
        let sqrt3 = 3.0f32.sqrt();
        let mut map = HashMap::new();
        for q in -radius..=radius {
            for r in -radius..=radius {
                if (q + r).abs() > radius {
                    continue;
                }
                let x = spacing * (q as f32 + r as f32 * 0.5);
                let y = spacing * (r as f32 * sqrt3 / 2.0);
                map.insert(GridIndex { i: q, j: r }, Point2::new(x, y));
            }
        }
        map
    }

    #[test]
    fn clean_hex_grid_has_no_inconsistencies() {
        let grid = make_hex_grid(3, 60.0);
        let flagged = hex_find_inconsistent_corners(&grid, 3.0);
        assert!(flagged.is_empty());
    }

    #[test]
    fn displaced_corner_is_flagged() {
        let mut grid = make_hex_grid(2, 60.0);
        let center = GridIndex { i: 0, j: 0 };
        // Displace center by (9, 9) pixels
        grid.insert(center, Point2::new(9.0, 9.0));

        let flagged = hex_find_inconsistent_corners(&grid, 3.0);
        assert_eq!(1, flagged.len());
        assert_eq!(center, flagged[0].0);

        // Predicted should be near (0, 0) — the ideal center
        let pred = flagged[0].1;
        assert!(pred.x.abs() < 0.01, "pred.x = {}", pred.x);
        assert!(pred.y.abs() < 0.01, "pred.y = {}", pred.y);
    }

    #[test]
    fn isolated_nodes_are_skipped() {
        let mut grid = HashMap::new();
        grid.insert(GridIndex { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        grid.insert(GridIndex { i: 10, j: 10 }, Point2::new(500.0, 500.0));

        let flagged = hex_find_inconsistent_corners(&grid, 3.0);
        assert!(flagged.is_empty());
    }

    #[test]
    fn prediction_from_single_axis_pair() {
        let spacing = 60.0;
        let sqrt3 = 3.0f32.sqrt();
        let mut grid = HashMap::new();
        // Just three points along the E/W axis: q = -1, 0, 1 at r = 0
        grid.insert(GridIndex { i: -1, j: 0 }, Point2::new(-spacing, 0.0));
        grid.insert(GridIndex { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        grid.insert(GridIndex { i: 1, j: 0 }, Point2::new(spacing, 0.0));

        let pred = hex_predict_grid_position(&grid, GridIndex { i: 0, j: 0 }).unwrap();
        assert!((pred.x - 0.0f32).abs() < 0.01);
        assert!((pred.y - 0.0f32).abs() < 0.01);

        // Three points along the NW/SE axis: (0,-1), (0,0), (0,1)
        let mut grid2 = HashMap::new();
        grid2.insert(
            GridIndex { i: 0, j: -1 },
            Point2::new(-0.5 * spacing, -sqrt3 / 2.0 * spacing),
        );
        grid2.insert(GridIndex { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        grid2.insert(
            GridIndex { i: 0, j: 1 },
            Point2::new(0.5 * spacing, sqrt3 / 2.0 * spacing),
        );

        let pred2 = hex_predict_grid_position(&grid2, GridIndex { i: 0, j: 0 }).unwrap();
        assert!((pred2.x - 0.0f32).abs() < 0.01);
        assert!((pred2.y - 0.0f32).abs() < 0.01);
    }

    #[test]
    fn perspective_distorted_hex_grid_passes() {
        let spacing = 60.0;
        let sqrt3 = 3.0f32.sqrt();
        let mut grid = HashMap::new();
        let radius: i32 = 3;
        for q in -radius..=radius {
            for r in -radius..=radius {
                if (q + r).abs() > radius {
                    continue;
                }
                let x = spacing * (q as f32 + r as f32 * 0.5);
                let y = spacing * (r as f32 * sqrt3 / 2.0);
                // Mild perspective: scale increases with y
                let scale = 1.0 + 0.01 * y / spacing;
                grid.insert(GridIndex { i: q, j: r }, Point2::new(x * scale, y * scale));
            }
        }

        let flagged = hex_find_inconsistent_corners(&grid, 3.0);
        assert!(flagged.is_empty());
    }
}
