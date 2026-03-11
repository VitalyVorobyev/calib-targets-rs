//! Homography-based corner validation for ChArUco detection.
//!
//! After the chessboard grid corners have been mapped to ChArUco IDs, this
//! module validates each corner's pixel position against a board-to-image
//! homography estimated from all inlier marker corners. A corner that deviates
//! by more than a relative threshold from the homography-predicted position is
//! identified as a false positive (typically a marker-interior corner picked up
//! by ChESS), and is corrected by running a local ChESS re-detection centred on
//! the homography-predicted seed.

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
use serde::{Deserialize, Serialize};

/// Configuration for the corner validation stage.
///
/// Groups the three tuning parameters so that `validate_and_fix_corners` stays
/// within the argument-count limit.
pub(crate) struct CornerValidationConfig<'a> {
    /// Side length of one board square in pixels (used to scale thresholds).
    pub px_per_square: f32,
    /// Maximum allowed deviation from the homography-predicted position,
    /// expressed as a fraction of `px_per_square`. Set to `f32::INFINITY`
    /// to disable validation entirely.
    pub threshold_rel: f32,
    /// ChESS detector parameters used for local re-detection.
    pub chess_params: &'a ChessParams,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CornerValidationSkippedReason {
    #[default]
    None,
    Disabled,
    NoMarkers,
    HomographyUnavailable,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CornerValidationDiagnostics {
    pub input_corner_count: usize,
    pub kept_corner_count: usize,
    pub corrected_corner_count: usize,
    pub dropped_corner_count: usize,
    pub skipped_reason: CornerValidationSkippedReason,
}

#[derive(Debug)]
pub(crate) struct CornerValidationRun {
    pub detection: TargetDetection,
    pub diagnostics: CornerValidationDiagnostics,
}

impl CornerValidationDiagnostics {
    fn skipped(count: usize, skipped_reason: CornerValidationSkippedReason) -> Self {
        Self {
            input_corner_count: count,
            kept_corner_count: count,
            corrected_corner_count: 0,
            dropped_corner_count: 0,
            skipped_reason,
        }
    }
}

/// Grid-corner offsets matching `corners_img` indices from `build_marker_cells`.
///
/// `corners_img = [p(gc0_x, gc0_y), p(gc0_x+1, gc0_y), p(gc0_x+1, gc0_y+1), p(gc0_x, gc0_y+1)]`
const GRID_CORNER_OFFSETS: [(i32, i32); 4] = [(0, 0), (1, 0), (1, 1), (0, 1)];

/// Recover the grid-frame cell TL coordinate (`gc0`) from `marker.gc` and
/// `marker.rotation`.
#[inline]
fn recover_gc0(marker: &MarkerDetection) -> (i32, i32) {
    match marker.rotation {
        1 => (marker.gc.gx - 1, marker.gc.gy),
        2 => (marker.gc.gx - 1, marker.gc.gy - 1),
        3 => (marker.gc.gx, marker.gc.gy - 1),
        _ => (marker.gc.gx, marker.gc.gy),
    }
}

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

fn redetect_corner_in_roi(
    image: &GrayImageView<'_>,
    seed: Point2<f32>,
    roi_half_px: i32,
    chess_params: &ChessParams,
) -> Option<Point2<f32>> {
    let x0 = ((seed.x as i32) - roi_half_px).max(0) as usize;
    let y0 = ((seed.y as i32) - roi_half_px).max(0) as usize;
    let x1 = ((seed.x as i32) + roi_half_px + 1).min(image.width as i32) as usize;
    let y1 = ((seed.y as i32) + roi_half_px + 1).min(image.height as i32) as usize;

    if x1 <= x0 || y1 <= y0 {
        return None;
    }

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

/// Validate ChArUco corners against a board-to-image homography estimated from
/// all inlier marker corners, and replace any that are geometrically inconsistent.
pub(crate) fn validate_and_fix_corners(
    detection: TargetDetection,
    board: &CharucoBoard,
    markers: &[MarkerDetection],
    alignment: &CharucoAlignment,
    image: &GrayImageView<'_>,
    cfg: &CornerValidationConfig<'_>,
) -> CornerValidationRun {
    let input_corner_count = detection.corners.len();
    if cfg.threshold_rel.is_infinite() {
        return CornerValidationRun {
            detection,
            diagnostics: CornerValidationDiagnostics::skipped(
                input_corner_count,
                CornerValidationSkippedReason::Disabled,
            ),
        };
    }
    if markers.is_empty() {
        return CornerValidationRun {
            detection,
            diagnostics: CornerValidationDiagnostics::skipped(
                input_corner_count,
                CornerValidationSkippedReason::NoMarkers,
            ),
        };
    }

    let threshold_px = cfg.threshold_rel * cfg.px_per_square;
    let threshold_sq = threshold_px * threshold_px;
    let roi_half_px = (threshold_px * 3.0)
        .round()
        .max(8.0)
        .min(cfg.px_per_square * 0.5) as i32;

    let (board_pts, image_pts) = collect_board_to_image_correspondences(board, markers, alignment);
    let homography = match estimate_homography_rect_to_img(&board_pts, &image_pts) {
        Some(h) => h,
        None => {
            return CornerValidationRun {
                detection,
                diagnostics: CornerValidationDiagnostics::skipped(
                    input_corner_count,
                    CornerValidationSkippedReason::HomographyUnavailable,
                ),
            };
        }
    };

    let mut diagnostics = CornerValidationDiagnostics {
        input_corner_count,
        ..CornerValidationDiagnostics::default()
    };
    let mut out_corners: Vec<LabeledCorner> = Vec::with_capacity(input_corner_count);

    for corner in detection.corners {
        let board_pos = match corner.target_position {
            Some(p) => p,
            None => {
                diagnostics.kept_corner_count += 1;
                out_corners.push(corner);
                continue;
            }
        };

        if corner.id.is_none() {
            diagnostics.kept_corner_count += 1;
            out_corners.push(corner);
            continue;
        }

        let seed = homography.apply(board_pos);
        let dx = corner.position.x - seed.x;
        let dy = corner.position.y - seed.y;
        if dx * dx + dy * dy <= threshold_sq {
            diagnostics.kept_corner_count += 1;
            out_corners.push(corner);
            continue;
        }

        match redetect_corner_in_roi(image, seed, roi_half_px, cfg.chess_params) {
            Some(new_pos) => {
                diagnostics.corrected_corner_count += 1;
                let mut fixed = corner;
                fixed.position = new_pos;
                out_corners.push(fixed);
            }
            None => {
                diagnostics.dropped_corner_count += 1;
            }
        }
    }

    CornerValidationRun {
        detection: TargetDetection {
            kind: TargetKind::Charuco,
            corners: out_corners,
        },
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{CharucoBoardSpec, MarkerLayout};
    use calib_targets_aruco::{builtins, GridCell};
    use calib_targets_core::GridAlignment;
    use nalgebra::Point2;

    fn build_board() -> CharucoBoard {
        let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("dict");
        CharucoBoard::new(CharucoBoardSpec {
            rows: 4,
            cols: 4,
            cell_size: 1.0,
            marker_size_rel: 0.75,
            dictionary: dict,
            marker_layout: MarkerLayout::OpenCvCharuco,
        })
        .expect("board")
    }

    fn blank_image(width: usize, height: usize) -> Vec<u8> {
        vec![127u8; width * height]
    }

    fn image_view(data: &[u8], width: usize, height: usize) -> GrayImageView<'_> {
        GrayImageView {
            width,
            height,
            data,
        }
    }

    fn internal_marker(board: &CharucoBoard) -> (u32, MarkerDetection, [usize; 4]) {
        for marker_id in 0..board.marker_count() as u32 {
            let Some(corner_ids) = board.marker_surrounding_charuco_corners(marker_id as i32)
            else {
                continue;
            };
            let Some((sx, sy)) = board.marker_cell(marker_id as i32) else {
                continue;
            };
            let marker = MarkerDetection {
                id: marker_id,
                gc: GridCell {
                    gx: sx as i32,
                    gy: sy as i32,
                },
                rotation: 0,
                hamming: 0,
                score: 1.0,
                border_score: 1.0,
                code: 0,
                inverted: false,
                corners_rect: [Point2::new(0.0, 0.0); 4],
                corners_img: Some([
                    Point2::new(20.0, 20.0),
                    Point2::new(40.0, 20.0),
                    Point2::new(40.0, 40.0),
                    Point2::new(20.0, 40.0),
                ]),
            };
            return (marker_id, marker, corner_ids);
        }
        panic!("expected at least one internal marker");
    }

    fn charuco_corner(
        board: &CharucoBoard,
        corner_id: usize,
        position: Point2<f32>,
    ) -> LabeledCorner {
        LabeledCorner {
            position,
            grid: None,
            id: Some(corner_id as u32),
            target_position: board.charuco_object_xy(corner_id as u32),
            score: 1.0,
        }
    }

    fn validation_cfg<'a>(chess_params: &'a ChessParams) -> CornerValidationConfig<'a> {
        CornerValidationConfig {
            px_per_square: 40.0,
            threshold_rel: 0.1,
            chess_params,
        }
    }

    fn seed_for_corner(
        board: &CharucoBoard,
        marker: &MarkerDetection,
        corner_id: usize,
    ) -> Point2<f32> {
        let alignment = CharucoAlignment {
            alignment: GridAlignment::IDENTITY,
            marker_inliers: vec![0],
        };
        let (board_pts, image_pts) =
            collect_board_to_image_correspondences(board, std::slice::from_ref(marker), &alignment);
        let homography =
            estimate_homography_rect_to_img(&board_pts, &image_pts).expect("homography");
        homography.apply(board.charuco_object_xy(corner_id as u32).expect("target"))
    }

    fn sample_real_corner_seed() -> (image::GrayImage, Point2<f32>) {
        use chess_corners::{find_chess_corners_image, ChessConfig};
        use image::ImageReader;
        use std::path::Path;

        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata")
            .join("mid.png");
        let image = ImageReader::open(path)
            .expect("open image")
            .decode()
            .expect("decode image")
            .to_luma8();
        let mut chess_cfg = ChessConfig::single_scale();
        chess_cfg.params.threshold_rel = 0.2;
        chess_cfg.params.nms_radius = 2;
        let corners = find_chess_corners_image(&image, &chess_cfg);
        let center = Point2::new(image.width() as f32 * 0.5, image.height() as f32 * 0.5);
        let seed = corners
            .iter()
            .min_by(|a, b| {
                let da = (a.x - center.x).powi(2) + (a.y - center.y).powi(2);
                let db = (b.x - center.x).powi(2) + (b.y - center.y).powi(2);
                da.total_cmp(&db)
            })
            .map(|c| Point2::new(c.x, c.y))
            .expect("at least one real corner");
        (image, seed)
    }

    #[test]
    fn recover_gc0_rotation_0() {
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

    #[test]
    fn validation_reports_keep() {
        let board = build_board();
        let (_marker_id, marker, corner_ids) = internal_marker(&board);
        let corner_id = corner_ids[2];
        let expected = seed_for_corner(&board, &marker, corner_id);
        let detection = TargetDetection {
            kind: TargetKind::Charuco,
            corners: vec![charuco_corner(&board, corner_id, expected)],
        };
        let pixels = blank_image(64, 64);
        let chess_params = crate::detector::params::default_redetect_params();
        let run = validate_and_fix_corners(
            detection,
            &board,
            std::slice::from_ref(&marker),
            &CharucoAlignment {
                alignment: GridAlignment::IDENTITY,
                marker_inliers: vec![0],
            },
            &image_view(&pixels, 64, 64),
            &validation_cfg(&chess_params),
        );

        assert_eq!(run.diagnostics.kept_corner_count, 1);
        assert_eq!(run.diagnostics.corrected_corner_count, 0);
        assert_eq!(run.diagnostics.dropped_corner_count, 0);
        assert_eq!(
            run.diagnostics.skipped_reason,
            CornerValidationSkippedReason::None
        );
        assert_eq!(run.detection.corners.len(), 1);
    }

    #[test]
    fn validation_reports_correct() {
        let board = build_board();
        let (_marker_id, marker, corner_ids) = internal_marker(&board);
        let corner_id = corner_ids[2];
        let (image, target_seed) = sample_real_corner_seed();
        let mut marker = marker;
        marker.corners_img = Some([
            Point2::new(target_seed.x - 20.0, target_seed.y - 20.0),
            Point2::new(target_seed.x, target_seed.y - 20.0),
            Point2::new(target_seed.x, target_seed.y),
            Point2::new(target_seed.x - 20.0, target_seed.y),
        ]);
        let expected = seed_for_corner(&board, &marker, corner_id);
        let detection = TargetDetection {
            kind: TargetKind::Charuco,
            corners: vec![charuco_corner(
                &board,
                corner_id,
                Point2::new(expected.x + 6.0, expected.y),
            )],
        };
        let chess_params = crate::detector::params::default_redetect_params();
        let preview = redetect_corner_in_roi(
            &image_view(
                image.as_raw(),
                image.width() as usize,
                image.height() as usize,
            ),
            expected,
            12,
            &chess_params,
        );
        assert!(
            preview.is_some(),
            "expected preview re-detect at {expected:?}"
        );
        let run = validate_and_fix_corners(
            detection,
            &board,
            std::slice::from_ref(&marker),
            &CharucoAlignment {
                alignment: GridAlignment::IDENTITY,
                marker_inliers: vec![0],
            },
            &image_view(
                image.as_raw(),
                image.width() as usize,
                image.height() as usize,
            ),
            &validation_cfg(&chess_params),
        );

        assert_eq!(run.diagnostics.kept_corner_count, 0);
        assert_eq!(run.diagnostics.corrected_corner_count, 1);
        assert_eq!(run.diagnostics.dropped_corner_count, 0);
        assert_eq!(run.detection.corners.len(), 1);
        let corrected = run.detection.corners[0].position;
        assert!((corrected.x - expected.x).abs() <= 1.5);
        assert!((corrected.y - expected.y).abs() <= 1.5);
    }

    #[test]
    fn validation_reports_drop() {
        let board = build_board();
        let (_marker_id, marker, corner_ids) = internal_marker(&board);
        let corner_id = corner_ids[2];
        let expected = seed_for_corner(&board, &marker, corner_id);
        let detection = TargetDetection {
            kind: TargetKind::Charuco,
            corners: vec![charuco_corner(
                &board,
                corner_id,
                Point2::new(expected.x + 6.0, expected.y),
            )],
        };
        let pixels = blank_image(64, 64);
        let chess_params = crate::detector::params::default_redetect_params();
        let run = validate_and_fix_corners(
            detection,
            &board,
            std::slice::from_ref(&marker),
            &CharucoAlignment {
                alignment: GridAlignment::IDENTITY,
                marker_inliers: vec![0],
            },
            &image_view(&pixels, 64, 64),
            &validation_cfg(&chess_params),
        );

        assert_eq!(run.diagnostics.corrected_corner_count, 0);
        assert_eq!(run.diagnostics.dropped_corner_count, 1);
        assert!(run.detection.corners.is_empty());
    }

    #[test]
    fn validation_reports_disabled_skip() {
        let board = build_board();
        let (_marker_id, marker, corner_ids) = internal_marker(&board);
        let corner_id = corner_ids[2];
        let expected = seed_for_corner(&board, &marker, corner_id);
        let detection = TargetDetection {
            kind: TargetKind::Charuco,
            corners: vec![charuco_corner(&board, corner_id, expected)],
        };
        let pixels = blank_image(64, 64);
        let chess_params = crate::detector::params::default_redetect_params();
        let run = validate_and_fix_corners(
            detection,
            &board,
            std::slice::from_ref(&marker),
            &CharucoAlignment {
                alignment: GridAlignment::IDENTITY,
                marker_inliers: vec![0],
            },
            &image_view(&pixels, 64, 64),
            &CornerValidationConfig {
                px_per_square: 20.0,
                threshold_rel: f32::INFINITY,
                chess_params: &chess_params,
            },
        );

        assert_eq!(
            run.diagnostics.skipped_reason,
            CornerValidationSkippedReason::Disabled
        );
        assert_eq!(run.diagnostics.kept_corner_count, 1);
    }

    #[test]
    fn validation_reports_no_marker_skip() {
        let board = build_board();
        let (_marker_id, marker, corner_ids) = internal_marker(&board);
        let corner_id = corner_ids[2];
        let expected = seed_for_corner(&board, &marker, corner_id);
        let detection = TargetDetection {
            kind: TargetKind::Charuco,
            corners: vec![charuco_corner(&board, corner_id, expected)],
        };
        let pixels = blank_image(64, 64);
        let chess_params = crate::detector::params::default_redetect_params();
        let run = validate_and_fix_corners(
            detection,
            &board,
            &[],
            &CharucoAlignment {
                alignment: GridAlignment::IDENTITY,
                marker_inliers: vec![],
            },
            &image_view(&pixels, 64, 64),
            &validation_cfg(&chess_params),
        );

        assert_eq!(
            run.diagnostics.skipped_reason,
            CornerValidationSkippedReason::NoMarkers
        );
        assert_eq!(run.diagnostics.kept_corner_count, 1);
    }

    #[test]
    fn validation_reports_homography_skip() {
        let board = build_board();
        let (_marker_id, mut marker, corner_ids) = internal_marker(&board);
        marker.corners_img = None;
        let corner_id = corner_ids[2];
        let detection = TargetDetection {
            kind: TargetKind::Charuco,
            corners: vec![charuco_corner(&board, corner_id, Point2::new(40.0, 40.0))],
        };
        let pixels = blank_image(64, 64);
        let chess_params = crate::detector::params::default_redetect_params();
        let run = validate_and_fix_corners(
            detection,
            &board,
            std::slice::from_ref(&marker),
            &CharucoAlignment {
                alignment: GridAlignment::IDENTITY,
                marker_inliers: vec![0],
            },
            &image_view(&pixels, 64, 64),
            &validation_cfg(&chess_params),
        );

        assert_eq!(
            run.diagnostics.skipped_reason,
            CornerValidationSkippedReason::HomographyUnavailable
        );
        assert_eq!(run.diagnostics.kept_corner_count, 1);
    }
}
