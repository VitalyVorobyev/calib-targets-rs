//! Local grid smoothness pre-filter for ChArUco detection.
//!
//! Runs on the chessboard corner map before `build_marker_cells` to detect and
//! correct false corners (typically ArUco marker internal features picked up
//! by ChESS with a loose orientation tolerance).
//!
//! For each grid corner, the expected position is predicted from immediate
//! neighbors via midpoint averaging.  Corners that deviate significantly from
//! the prediction are re-detected locally or snapped to the predicted position.

use super::corner_refit::Redetector;
use calib_targets_core::{Coord, CornerMap, GrayImageView};
use log::debug;
use nalgebra::Point2;
use projective_grid::{expert::lattice::predict_grid_position, LatticeKind};

/// Check grid corners for smoothness and fix or remove outliers.
///
/// For each corner `(i,j)`, predicts its position from neighbor pairs:
/// - Horizontal: `0.5 * (P(i-1,j) + P(i+1,j))`
/// - Vertical: `0.5 * (P(i,j-1) + P(i,j+1))`
///
/// If the deviation from the average prediction exceeds
/// `threshold_rel * local_pitch`, the corner is re-detected near the
/// predicted position using [`Redetector::locate`].  If re-detection
/// fails, the corner is snapped to the predicted position (never removed).
pub(crate) fn smooth_grid_corners(
    corner_map: &mut CornerMap,
    image: &GrayImageView<'_>,
    px_per_square: f32,
    threshold_rel: f32,
    redetector: &mut Redetector<'_>,
) {
    if threshold_rel.is_infinite() || corner_map.len() < 3 {
        return;
    }

    // Collect flagged corners in deterministic coordinate order (don't mutate
    // during evaluation).
    let mut flagged: Vec<(Coord, Point2<f32>, i32)> = Vec::new();

    let mut keys: Vec<Coord> = corner_map.keys().copied().collect();
    keys.sort_unstable();
    for gc in &keys {
        let pos = corner_map[gc];

        let Some(predicted) = predict_grid_position(corner_map, *gc, LatticeKind::Square)
            .map(|prediction| prediction.position)
        else {
            continue;
        };

        let local_pitch = local_corner_pitch(corner_map, *gc, pos, px_per_square);
        let threshold_px = threshold_rel * local_pitch;
        let threshold_sq = threshold_px * threshold_px;

        let dx = pos.x - predicted.x;
        let dy = pos.y - predicted.y;
        if dx * dx + dy * dy > threshold_sq {
            let roi_half_px = (threshold_px * 3.0).round().max(8.0).min(local_pitch * 0.5) as i32;
            flagged.push((*gc, predicted, roi_half_px));
        }
    }

    if !flagged.is_empty() {
        debug!(
            "grid smoothness: flagged {} corners for re-detection",
            flagged.len()
        );
    }

    for (gc, predicted, roi_half_px) in flagged {
        // Only the position feeds the corner map (marker sampling); output
        // corners keep their measured position, so no covariance is needed.
        match redetector.locate(image, predicted, roi_half_px) {
            Some(new_pos) => {
                debug!(
                    "grid smoothness: re-detected corner ({},{}) at ({:.1},{:.1}) -> ({:.1},{:.1})",
                    gc.u, gc.v, corner_map[&gc].x, corner_map[&gc].y, new_pos.x, new_pos.y
                );
                corner_map.insert(gc, new_pos);
            }
            None => {
                debug!(
                    "grid smoothness: snapped corner ({},{}) to predicted ({:.1},{:.1})",
                    gc.u, gc.v, predicted.x, predicted.y
                );
                corner_map.insert(gc, predicted);
            }
        }
    }
}

/// Estimate local image pitch at `gc` from immediate grid neighbours.
///
/// Prefers opposing-neighbour spans `0.5 * ||P(+1) - P(-1)||` along each axis
/// so a displaced corner at `gc` cannot shrink or inflate its own pitch scale,
/// falling back to adjacent cardinal distances and finally `fallback_px`.
fn local_corner_pitch(
    corner_map: &CornerMap,
    gc: Coord,
    pos: Point2<f32>,
    fallback_px: f32,
) -> f32 {
    let mut sum = 0.0_f32;
    let mut count = 0usize;
    for (di, dj) in [(1, 0), (0, 1)] {
        let fwd = corner_map.get(&Coord::new(gc.u + di, gc.v + dj));
        let bwd = corner_map.get(&Coord::new(gc.u - di, gc.v - dj));
        match (fwd, bwd) {
            (Some(&pf), Some(&pb)) => {
                let d = 0.5 * (pf - pb).norm();
                if d.is_finite() && d > 0.0 {
                    sum += d;
                    count += 1;
                }
            }
            (Some(&p), None) | (None, Some(&p)) => {
                let d = (p - pos).norm();
                if d.is_finite() && d > 0.0 {
                    sum += d;
                    count += 1;
                }
            }
            (None, None) => {}
        }
    }
    if count > 0 {
        sum / count as f32
    } else {
        fallback_px
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::corner_refit::FrameNoise;

    /// Helper: build a simple grid of corners with uniform spacing.
    fn make_grid(rows: i32, cols: i32, spacing: f32) -> CornerMap {
        let mut map = CornerMap::new();
        for j in 0..rows {
            for i in 0..cols {
                map.insert(
                    Coord::new(i, j),
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
        let center = Coord::new(1, 1);
        map.insert(center, Point2::new(spacing + 9.0, spacing + 9.0));

        // With a dummy 1x1 image, redetect fails — corner should be
        // snapped to the predicted (midpoint) position, not removed.
        let dummy_image = GrayImageView {
            data: &[128],
            width: 1,
            height: 1,
        };
        let front_end = calib_targets_core::default_chess_config();
        let mut detector = Redetector::new(&front_end, FrameNoise::Unresolved);

        smooth_grid_corners(&mut map, &dummy_image, spacing, 0.05, &mut detector);

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
        let front_end = calib_targets_core::default_chess_config();
        let mut detector = Redetector::new(&front_end, FrameNoise::Unresolved);

        smooth_grid_corners(&mut map, &dummy_image, spacing, 0.05, &mut detector);

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
                    Coord::new(i, j),
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
        let front_end = calib_targets_core::default_chess_config();
        let mut detector = Redetector::new(&front_end, FrameNoise::Unresolved);

        smooth_grid_corners(&mut map, &dummy_image, spacing, 0.05, &mut detector);

        // No corners should be removed.
        assert_eq!(map.len(), orig_len);
    }

    #[test]
    fn edge_corner_with_one_pair() {
        let spacing = 60.0;
        let mut map = make_grid(1, 3, spacing); // single row: (0,0), (1,0), (2,0)
                                                // Displace middle corner
        let middle = Coord::new(1, 0);
        map.insert(middle, Point2::new(spacing + 12.0, 12.0));

        let dummy_image = GrayImageView {
            data: &[128],
            width: 1,
            height: 1,
        };
        let front_end = calib_targets_core::default_chess_config();
        let mut detector = Redetector::new(&front_end, FrameNoise::Unresolved);

        smooth_grid_corners(&mut map, &dummy_image, spacing, 0.05, &mut detector);

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
        map.insert(Coord::new(0, 0), Point2::new(0.0, 0.0));
        map.insert(Coord::new(2, 2), Point2::new(120.0, 120.0));

        let dummy_image = GrayImageView {
            data: &[128],
            width: 1,
            height: 1,
        };
        let front_end = calib_targets_core::default_chess_config();
        let mut detector = Redetector::new(&front_end, FrameNoise::Unresolved);

        smooth_grid_corners(&mut map, &dummy_image, spacing, 0.05, &mut detector);

        // Both corners should remain (no pairs to predict from).
        assert_eq!(map.len(), 2);
    }
}
