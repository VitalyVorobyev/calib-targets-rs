//! Local grid smoothness pre-filter for ChArUco detection.
//!
//! Runs between `build_corner_map` and `build_marker_cells` to detect and
//! correct false corners (typically ArUco marker internal features picked up
//! by ChESS with a loose orientation tolerance).
//!
//! For each grid corner, the expected position is predicted from immediate
//! neighbors via midpoint averaging.  Corners that deviate significantly from
//! the prediction are re-detected locally or snapped to the predicted position.

use super::corner_validation::redetect_corner_in_roi;
use super::marker_sampling::CornerMap;
use calib_targets_core::{GrayImageView, GridCoords};
use chess_corners_core::ChessParams;
use log::debug;
use nalgebra::Point2;

/// Check grid corners for smoothness and fix or remove outliers.
///
/// For each corner `(i,j)`, predicts its position from neighbor pairs:
/// - Horizontal: `0.5 * (P(i-1,j) + P(i+1,j))`
/// - Vertical: `0.5 * (P(i,j-1) + P(i,j+1))`
///
/// If the deviation from the average prediction exceeds
/// `threshold_rel * px_per_square`, the corner is re-detected near the
/// predicted position using `redetect_corner_in_roi`.  If re-detection
/// fails, the corner is snapped to the predicted position (never removed).
pub(crate) fn smooth_grid_corners(
    corner_map: &mut CornerMap,
    image: &GrayImageView<'_>,
    px_per_square: f32,
    threshold_rel: f32,
    chess_params: &ChessParams,
) {
    if threshold_rel.is_infinite() || corner_map.len() < 3 {
        return;
    }

    let threshold_px = threshold_rel * px_per_square;
    let threshold_sq = threshold_px * threshold_px;
    let roi_half_px = (threshold_px * 3.0)
        .round()
        .max(8.0)
        .min(px_per_square * 0.5) as i32;

    // Collect flagged corners (don't mutate during iteration).
    let mut flagged: Vec<(GridCoords, Point2<f32>)> = Vec::new();

    let keys: Vec<GridCoords> = corner_map.keys().copied().collect();
    for gc in &keys {
        let pos = corner_map[gc];

        let mut pred_sum = Point2::new(0.0f32, 0.0f32);
        let mut pred_count = 0u32;

        // Horizontal pair
        let left = GridCoords {
            i: gc.i - 1,
            j: gc.j,
        };
        let right = GridCoords {
            i: gc.i + 1,
            j: gc.j,
        };
        if let (Some(&pl), Some(&pr)) = (corner_map.get(&left), corner_map.get(&right)) {
            let mid = Point2::new(0.5 * (pl.x + pr.x), 0.5 * (pl.y + pr.y));
            pred_sum.x += mid.x;
            pred_sum.y += mid.y;
            pred_count += 1;
        }

        // Vertical pair
        let up = GridCoords {
            i: gc.i,
            j: gc.j - 1,
        };
        let down = GridCoords {
            i: gc.i,
            j: gc.j + 1,
        };
        if let (Some(&pu), Some(&pd)) = (corner_map.get(&up), corner_map.get(&down)) {
            let mid = Point2::new(0.5 * (pu.x + pd.x), 0.5 * (pu.y + pd.y));
            pred_sum.x += mid.x;
            pred_sum.y += mid.y;
            pred_count += 1;
        }

        if pred_count == 0 {
            continue;
        }

        let predicted = Point2::new(
            pred_sum.x / pred_count as f32,
            pred_sum.y / pred_count as f32,
        );

        let dx = pos.x - predicted.x;
        let dy = pos.y - predicted.y;
        if dx * dx + dy * dy > threshold_sq {
            flagged.push((*gc, predicted));
        }
    }

    if !flagged.is_empty() {
        debug!(
            "grid smoothness: flagged {} corners for re-detection",
            flagged.len()
        );
    }

    for (gc, predicted) in flagged {
        match redetect_corner_in_roi(image, predicted, roi_half_px, chess_params) {
            Some(new_pos) => {
                debug!(
                    "grid smoothness: re-detected corner ({},{}) at ({:.1},{:.1}) -> ({:.1},{:.1})",
                    gc.i, gc.j, corner_map[&gc].x, corner_map[&gc].y, new_pos.x, new_pos.y
                );
                corner_map.insert(gc, new_pos);
            }
            None => {
                debug!(
                    "grid smoothness: snapped corner ({},{}) to predicted ({:.1},{:.1})",
                    gc.i, gc.j, predicted.x, predicted.y
                );
                corner_map.insert(gc, predicted);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a simple grid of corners with uniform spacing.
    fn make_grid(rows: i32, cols: i32, spacing: f32) -> CornerMap {
        let mut map = CornerMap::new();
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
    fn displaced_center_is_snapped_to_predicted() {
        let spacing = 60.0;
        let mut map = make_grid(3, 3, spacing);
        // Displace center corner by 15% of spacing
        let center = GridCoords { i: 1, j: 1 };
        map.insert(center, Point2::new(spacing + 9.0, spacing + 9.0));

        // With a dummy 1x1 image, redetect fails — corner should be
        // snapped to the predicted (midpoint) position, not removed.
        let dummy_image = GrayImageView {
            data: &[128],
            width: 1,
            height: 1,
        };
        let params = ChessParams::default();

        smooth_grid_corners(&mut map, &dummy_image, spacing, 0.05, &params);

        // All 9 corners should remain.
        assert_eq!(map.len(), 9);
        // Center should be snapped to predicted position (midpoint of neighbors = original).
        let pos = map[&center];
        assert!((pos.x - spacing).abs() < 0.01);
        assert!((pos.y - spacing).abs() < 0.01);
    }

    #[test]
    fn clean_grid_passes() {
        let spacing = 60.0;
        let map_orig = make_grid(3, 3, spacing);
        let mut map = map_orig.clone();

        let dummy_image = GrayImageView {
            data: &[128],
            width: 1,
            height: 1,
        };
        let params = ChessParams::default();

        smooth_grid_corners(&mut map, &dummy_image, spacing, 0.05, &params);

        // All corners should remain unchanged.
        assert_eq!(map.len(), 9);
        for (gc, pos) in &map_orig {
            assert_eq!(map.get(gc), Some(pos));
        }
    }

    #[test]
    fn perspective_distorted_grid_passes() {
        // Simulate mild perspective: spacing increases by ~2% per row.
        let spacing = 60.0;
        let mut map = CornerMap::new();
        for j in 0..5 {
            let scale = 1.0 + 0.02 * j as f32;
            for i in 0..5 {
                map.insert(
                    GridCoords { i, j },
                    Point2::new(i as f32 * spacing * scale, j as f32 * spacing * scale),
                );
            }
        }
        let orig_len = map.len();

        let dummy_image = GrayImageView {
            data: &[128],
            width: 1,
            height: 1,
        };
        let params = ChessParams::default();

        smooth_grid_corners(&mut map, &dummy_image, spacing, 0.05, &params);

        // No corners should be removed.
        assert_eq!(map.len(), orig_len);
    }

    #[test]
    fn edge_corner_with_one_pair() {
        let spacing = 60.0;
        let mut map = make_grid(1, 3, spacing); // single row: (0,0), (1,0), (2,0)
                                                // Displace middle corner
        let middle = GridCoords { i: 1, j: 0 };
        map.insert(middle, Point2::new(spacing + 12.0, 12.0));

        let dummy_image = GrayImageView {
            data: &[128],
            width: 1,
            height: 1,
        };
        let params = ChessParams::default();

        smooth_grid_corners(&mut map, &dummy_image, spacing, 0.05, &params);

        // Middle should be snapped to predicted position, not removed.
        assert_eq!(map.len(), 3);
        let pos = map[&middle];
        assert!((pos.x - spacing).abs() < 0.01);
        assert!((pos.y - 0.0).abs() < 0.01);
    }

    #[test]
    fn isolated_corner_is_skipped() {
        let spacing = 60.0;
        let mut map = CornerMap::new();
        // Only two corners, no complete pairs
        map.insert(GridCoords { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        map.insert(GridCoords { i: 2, j: 2 }, Point2::new(120.0, 120.0));

        let dummy_image = GrayImageView {
            data: &[128],
            width: 1,
            height: 1,
        };
        let params = ChessParams::default();

        smooth_grid_corners(&mut map, &dummy_image, spacing, 0.05, &params);

        // Both corners should remain (no pairs to predict from).
        assert_eq!(map.len(), 2);
    }
}
