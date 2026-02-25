//! Homography-based corner validation for ChArUco detection.
//!
//! After the chessboard grid corners have been mapped to ChArUco IDs, this
//! module validates each corner's pixel position against a board-to-image
//! homography estimated from all inlier marker corners.  A corner that deviates
//! by more than a relative threshold from the homography-predicted position is
//! identified as a false positive (typically a marker-interior corner picked up
//! by ChESS), and is corrected by running a local ChESS re-detection centred on
//! the homography-predicted seed.
//!
//! ## Why a global homography instead of per-marker predictions
//!
//! The per-marker approach (using `corners_img` of adjacent decoded markers as
//! predictions) fails when ALL adjacent marker cells were decoded using the false
//! corner itself.  In that case every prediction equals the false position, the
//! self-contamination filter removes all of them, and the false corner is silently
//! kept.  This is the "dense self-contamination" failure mode.
//!
//! The global homography is estimated from ALL inlier marker corners (typically
//! 64–400+ correspondences on a large board).  A single false corner contributes
//! at most 2 wrong correspondences (from the ≤2 adjacent marker cells), which
//! are negligible outliers in the DLT fit.  The predicted seed is therefore
//! accurate even for corners near the false position.
//!
//! ## Pipeline stage
//!
//! ```text
//! map_charuco_corners() → [validate_and_fix_corners()] → CharucoDetectionResult
//! ```
//!
//! ## Inputs
//!
//! - `TargetDetection` with ChArUco IDs assigned.
//! - `Vec<MarkerDetection>` – alignment inliers, each with `corners_img` populated.
//! - `CharucoBoard` for board geometry.
//! - `CharucoAlignment` to convert marker grid coords to board cell coords.
//! - Full-resolution `GrayImageView` for local re-detection.
//!
//! ## Failure modes
//!
//! - Fewer than 4 inlier markers → homography cannot be estimated → skip validation.
//! - Homography estimation fails numerically → skip validation.
//! - Re-detection finds no corner near the seed → corner discarded.

use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{
    estimate_homography_rect_to_img, GrayImageView, LabeledCorner, TargetDetection, TargetKind,
};
use chess_corners_core::{
    detect::detect_corners_from_response_with_refiner,
    imageview::ImageView,
    response::{chess_response_u8_patch, Roi},
    ChessParams, Refiner,
};
use nalgebra::Point2;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration for the corner validation stage.
///
/// Groups the three tuning parameters so that `validate_and_fix_corners` stays
/// within the argument-count limit.
pub(crate) struct CornerValidationConfig<'a> {
    /// Side length of one board square in pixels (used to scale thresholds).
    pub px_per_square: f32,
    /// Maximum allowed deviation from the homography-predicted position,
    /// expressed as a fraction of `px_per_square`.  Set to `f32::INFINITY`
    /// to disable validation entirely.
    pub threshold_rel: f32,
    /// ChESS detector parameters used for local re-detection.
    pub chess_params: &'a ChessParams,
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Grid-corner offsets matching `corners_img` indices from `build_marker_cells`.
///
/// `corners_img = [p(gc0_x, gc0_y), p(gc0_x+1, gc0_y), p(gc0_x+1, gc0_y+1), p(gc0_x, gc0_y+1)]`
const GRID_CORNER_OFFSETS: [(i32, i32); 4] = [(0, 0), (1, 0), (1, 1), (0, 1)];

/// Recover the grid-frame cell TL coordinate (`gc0`) from `marker.gc` and
/// `marker.rotation`.
///
/// In `build_detection` (scan.rs) the marker's `gc` is offset from the cell TL
/// by the marker's rotation within the cell:
///
/// ```text
/// rotation 0 → gc = gc0           (TL)
/// rotation 1 → gc = gc0 + (1, 0)  (TR)
/// rotation 2 → gc = gc0 + (1, 1)  (BR)
/// rotation 3 → gc = gc0 + (0, 1)  (BL)
/// ```
///
/// Inverting: `gc0 = gc - rotation_offset`.
#[inline]
fn recover_gc0(marker: &MarkerDetection) -> (i32, i32) {
    match marker.rotation {
        1 => (marker.gc.gx - 1, marker.gc.gy),
        2 => (marker.gc.gx - 1, marker.gc.gy - 1),
        3 => (marker.gc.gx, marker.gc.gy - 1),
        _ => (marker.gc.gx, marker.gc.gy), // rotation 0 or unexpected
    }
}

/// Collect `(board_corner, image_corner)` correspondences from all inlier markers.
///
/// Returns two parallel vectors suitable for passing to
/// `estimate_homography_rect_to_img`:
/// - `board_pts[k]`: board-space position of corner k, in the same integer
///   coordinate system as `board.charuco_corner_id_from_board_corner`.
/// - `image_pts[k]`: image-space position of that same corner, from
///   `marker.corners_img`.
///
/// For each inlier marker:
/// 1. Recover `gc0` (cell TL in grid space) from `marker.gc` and `marker.rotation`.
/// 2. Map each of the 4 grid corners through `alignment.map()` to get the
///    board-frame corner position `(bi, bj)`.
/// 3. Only include the corner if it corresponds to a valid ChArUco ID (i.e.,
///    the board corner is an inner corner, not a board-border corner).
fn collect_board_to_image_correspondences(
    board: &CharucoBoard,
    markers: &[MarkerDetection],
    alignment: &CharucoAlignment,
) -> (Vec<Point2<f32>>, Vec<Point2<f32>>) {
    let mut board_pts: Vec<Point2<f32>> = Vec::with_capacity(markers.len() * 4);
    let mut image_pts: Vec<Point2<f32>> = Vec::with_capacity(markers.len() * 4);

    for marker in markers {
        let corners_img = match &marker.corners_img {
            Some(c) => *c,
            None => continue,
        };

        let (gc0_x, gc0_y) = recover_gc0(marker);

        for (img_idx, &(di, dj)) in GRID_CORNER_OFFSETS.iter().enumerate() {
            let [bi, bj] = alignment.map(gc0_x + di, gc0_y + dj);
            // Use charuco_object_xy for the board point so the coordinate
            // system matches corner.target_position used in the prediction.
            // Only inner corners (those with a ChArUco ID) are included.
            let Some(charuco_id) = board.charuco_corner_id_from_board_corner(bi, bj) else {
                continue;
            };
            let Some(board_pt) = board.charuco_object_xy(charuco_id) else {
                continue;
            };
            board_pts.push(board_pt);
            image_pts.push(corners_img[img_idx]);
        }
    }

    (board_pts, image_pts)
}

/// Run a local ChESS detection in a window centred on `seed`.
///
/// Uses `chess_response_u8_patch` on the full image restricted to a patch of
/// half-width `roi_half_px`, then runs NMS + subpixel refinement.
///
/// Returns the image-space position of the strongest corner found within
/// `roi_half_px` of the seed, or `None` if no corner is found.
fn redetect_corner_in_roi(
    image: &GrayImageView<'_>,
    seed: Point2<f32>,
    roi_half_px: i32,
    chess_params: &ChessParams,
) -> Option<Point2<f32>> {
    // Compute ROI in integer image coords, clamped to image bounds.
    let x0 = ((seed.x as i32) - roi_half_px).max(0) as usize;
    let y0 = ((seed.y as i32) - roi_half_px).max(0) as usize;
    let x1 = ((seed.x as i32) + roi_half_px + 1).min(image.width as i32) as usize;
    let y1 = ((seed.y as i32) + roi_half_px + 1).min(image.height as i32) as usize;

    if x1 <= x0 || y1 <= y0 {
        return None;
    }

    // Compute ChESS response only inside the ROI.
    let patch_resp = chess_response_u8_patch(
        image.data,
        image.width,
        image.height,
        chess_params,
        Roi { x0, y0, x1, y1 },
    );

    if patch_resp.w == 0 || patch_resp.h == 0 {
        return None;
    }

    // Build an ImageView with origin = (x0, y0) so the refiner reads the
    // correct global pixels even though the response map has local coords.
    let refine_view = ImageView::with_origin(
        image.width,
        image.height,
        image.data,
        [x0 as i32, y0 as i32],
    )?;

    let mut refiner = Refiner::from_kind(chess_params.refiner.clone());
    let raw_corners = detect_corners_from_response_with_refiner(
        &patch_resp,
        chess_params,
        Some(refine_view),
        &mut refiner,
    );

    // Shift patch-local coordinates back to global image coordinates and pick
    // the strongest corner that is within roi_half_px of the seed.
    let seed_x = seed.x;
    let seed_y = seed.y;
    let max_dist2 = (roi_half_px as f32) * (roi_half_px as f32);

    raw_corners
        .into_iter()
        .map(|c| {
            let gx = c.xy[0] + x0 as f32;
            let gy = c.xy[1] + y0 as f32;
            (c.strength, gx, gy)
        })
        .filter(|&(_s, gx, gy)| {
            let dx = gx - seed_x;
            let dy = gy - seed_y;
            dx * dx + dy * dy <= max_dist2
        })
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_s, gx, gy)| Point2::new(gx, gy))
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Validate ChArUco corners against a board-to-image homography estimated from
/// all inlier marker corners, and replace any that are geometrically inconsistent.
///
/// # Algorithm
///
/// 1. Collect `(board_corner, image_corner)` from all inlier markers.
/// 2. Estimate homography `H: board → image` via DLT on all correspondences.
///    If fewer than 4 correspondences or estimation fails → skip validation.
/// 3. For each ChArUco corner with a known board position:
///    a. Predict image position: `seed = H.apply(board_corner)`.
///    b. `|corner - seed| ≤ threshold_px` → consistent, keep unchanged.
///    c. `|corner - seed| > threshold_px` → false corner detected:
///       - Run `redetect_corner_in_roi` centred on `seed`.
///       - Re-detection succeeds → replace position.
///       - Re-detection fails → discard corner.
///
/// `threshold_px = threshold_rel * px_per_square`
/// `roi_half_px  = clamp(threshold_px * 3, 8, px_per_square * 0.5)`
pub(crate) fn validate_and_fix_corners(
    detection: TargetDetection,
    board: &CharucoBoard,
    markers: &[MarkerDetection],
    alignment: &CharucoAlignment,
    image: &GrayImageView<'_>,
    cfg: &CornerValidationConfig<'_>,
) -> TargetDetection {
    // Fast path: validation disabled or no markers to consult.
    if cfg.threshold_rel.is_infinite() || markers.is_empty() {
        return detection;
    }

    let threshold_px = cfg.threshold_rel * cfg.px_per_square;
    let threshold_sq = threshold_px * threshold_px;
    let roi_half_px = (threshold_px * 3.0)
        .round()
        .max(8.0)
        .min(cfg.px_per_square * 0.5) as i32;

    // Collect board↔image correspondences and estimate a global homography.
    let (board_pts, image_pts) = collect_board_to_image_correspondences(board, markers, alignment);

    let homography = match estimate_homography_rect_to_img(&board_pts, &image_pts) {
        Some(h) => h,
        None => {
            // Not enough correspondences or degenerate configuration — skip.
            return detection;
        }
    };

    let mut out_corners: Vec<LabeledCorner> = Vec::with_capacity(detection.corners.len());

    for corner in detection.corners {
        // Only validate corners that have a board position (via target_position
        // which encodes the board coordinate) and a ChArUco ID.  Corners
        // without an ID cannot be validated — keep as-is.
        let board_pos = match corner.target_position {
            Some(p) => p,
            None => {
                out_corners.push(corner);
                continue;
            }
        };

        if corner.id.is_none() {
            out_corners.push(corner);
            continue;
        }

        // Predict image position from the board homography.
        let seed = homography.apply(board_pos);

        let dx = corner.position.x - seed.x;
        let dy = corner.position.y - seed.y;
        if dx * dx + dy * dy <= threshold_sq {
            // Corner is geometrically consistent with the board homography.
            out_corners.push(corner);
            continue;
        }

        // Corner is a candidate false positive — attempt local re-detection.
        match redetect_corner_in_roi(image, seed, roi_half_px, cfg.chess_params) {
            Some(new_pos) => {
                // Replace the false position with the re-detected one.
                let mut fixed = corner;
                fixed.position = new_pos;
                out_corners.push(fixed);
            }
            None => {
                // No valid corner found near the seed — discard.
            }
        }
    }

    TargetDetection {
        kind: TargetKind::Charuco,
        corners: out_corners,
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recover_gc0_rotation_0() {
        use calib_targets_aruco::{GridCell, MarkerDetection};
        use nalgebra::Point2;

        let marker = MarkerDetection {
            id: 0,
            gc: GridCell { gx: 3, gy: 5 },
            rotation: 0,
            hamming: 0,
            score: 1.0,
            border_score: 1.0,
            code: 0,
            inverted: false,
            corners_rect: [Point2::new(0.0, 0.0); 4],
            corners_img: None,
        };
        assert_eq!(recover_gc0(&marker), (3, 5));
    }

    #[test]
    fn recover_gc0_rotation_1() {
        use calib_targets_aruco::{GridCell, MarkerDetection};
        use nalgebra::Point2;

        // gc = gc0 + (1, 0) for rotation 1, so gc0 = gc - (1, 0)
        let marker = MarkerDetection {
            id: 0,
            gc: GridCell { gx: 4, gy: 5 },
            rotation: 1,
            hamming: 0,
            score: 1.0,
            border_score: 1.0,
            code: 0,
            inverted: false,
            corners_rect: [Point2::new(0.0, 0.0); 4],
            corners_img: None,
        };
        assert_eq!(recover_gc0(&marker), (3, 5));
    }

    #[test]
    fn recover_gc0_rotation_2() {
        use calib_targets_aruco::{GridCell, MarkerDetection};
        use nalgebra::Point2;

        let marker = MarkerDetection {
            id: 0,
            gc: GridCell { gx: 4, gy: 6 },
            rotation: 2,
            hamming: 0,
            score: 1.0,
            border_score: 1.0,
            code: 0,
            inverted: false,
            corners_rect: [Point2::new(0.0, 0.0); 4],
            corners_img: None,
        };
        assert_eq!(recover_gc0(&marker), (3, 5));
    }

    #[test]
    fn recover_gc0_rotation_3() {
        use calib_targets_aruco::{GridCell, MarkerDetection};
        use nalgebra::Point2;

        let marker = MarkerDetection {
            id: 0,
            gc: GridCell { gx: 3, gy: 6 },
            rotation: 3,
            hamming: 0,
            score: 1.0,
            border_score: 1.0,
            code: 0,
            inverted: false,
            corners_rect: [Point2::new(0.0, 0.0); 4],
            corners_img: None,
        };
        assert_eq!(recover_gc0(&marker), (3, 5));
    }
}
