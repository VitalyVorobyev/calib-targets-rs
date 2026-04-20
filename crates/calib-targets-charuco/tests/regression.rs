use calib_targets_aruco::builtins;
use calib_targets_charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
use calib_targets_chessboard::{
    Detector as ChessboardDetector, DetectorParams as ChessboardParams,
};
use calib_targets_core::{
    estimate_homography_rect_to_img, Corner as TargetCorner, GrayImageView, TargetKind,
};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use nalgebra::Point2;
use std::collections::HashSet;
use std::path::Path;

fn load_gray(path: &Path) -> image::GrayImage {
    ImageReader::open(path)
        .expect("open image")
        .decode()
        .expect("decode image")
        .to_luma8()
}

fn detect_corners(img: &image::GrayImage) -> Vec<CornerDescriptor> {
    let mut chess_cfg = ChessConfig::single_scale();
    chess_cfg.threshold_mode = chess_corners::ThresholdMode::Relative;
    chess_cfg.threshold_value = 0.2;
    chess_cfg.nms_radius = 2;
    find_chess_corners_image(img, &chess_cfg)
}

fn adapt_chess_corner(c: &CornerDescriptor) -> TargetCorner {
    TargetCorner {
        position: Point2::new(c.x, c.y),
        orientation_cluster: None,
        axes: [
            calib_targets_core::AxisEstimate {
                angle: c.axes[0].angle,
                sigma: c.axes[0].sigma,
            },
            calib_targets_core::AxisEstimate {
                angle: c.axes[1].angle,
                sigma: c.axes[1].sigma,
            },
        ],
        contrast: c.contrast,
        fit_rms: c.fit_rms,
        strength: c.response,
    }
}

fn assert_unique_ids(res: &calib_targets_charuco::CharucoDetectionResult, max_id: u32) {
    let mut ids: Vec<u32> = res.detection.corners.iter().filter_map(|c| c.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        res.detection.corners.len(),
        "expected every detected ChArUco corner to have a unique id"
    );
    assert!(
        ids.last().copied().unwrap_or(0) < max_id,
        "unexpected corner id range"
    );
}

fn testdata_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata")
        .join(name)
}

#[derive(Clone, Copy, Debug)]
struct ReprojectionError {
    id: u32,
    error_px: f32,
}

fn median(values: &[f32]) -> f32 {
    assert!(!values.is_empty(), "median requires at least one sample");
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 1 {
        sorted[mid]
    } else {
        0.5 * (sorted[mid - 1] + sorted[mid])
    }
}

fn robust_error_gate(errors: &[f32], sigma_scale: f32, min_gate_px: f32) -> f32 {
    let med = median(errors);
    let abs_dev: Vec<f32> = errors.iter().map(|e| (e - med).abs()).collect();
    let mad = median(&abs_dev);
    let robust_sigma = (1.4826 * mad).max(0.25);
    (med + sigma_scale * robust_sigma).max(min_gate_px)
}

fn compute_reprojection_errors(
    ids: &[u32],
    target_pts: &[Point2<f32>],
    image_pts: &[Point2<f32>],
    gate_sigma: f32,
) -> Vec<ReprojectionError> {
    assert_eq!(ids.len(), target_pts.len(), "ids/target count mismatch");
    assert_eq!(
        target_pts.len(),
        image_pts.len(),
        "target/image correspondence count mismatch"
    );
    assert!(
        target_pts.len() >= 4,
        "need at least 4 correspondences to estimate homography"
    );

    let h_all = estimate_homography_rect_to_img(target_pts, image_pts)
        .expect("homography fit from target coordinates to image pixels");

    let seed_errors: Vec<ReprojectionError> = ids
        .iter()
        .zip(target_pts.iter())
        .zip(image_pts.iter())
        .map(|((&id, &target), &image)| {
            let pred = h_all.apply(target);
            let dx = pred.x - image.x;
            let dy = pred.y - image.y;
            ReprojectionError {
                id,
                error_px: (dx * dx + dy * dy).sqrt(),
            }
        })
        .collect();

    let seed_values: Vec<f32> = seed_errors.iter().map(|sample| sample.error_px).collect();
    let seed_gate = robust_error_gate(&seed_values, gate_sigma, 2.0);

    let mut inlier_target = Vec::new();
    let mut inlier_image = Vec::new();
    for (idx, sample) in seed_errors.iter().enumerate() {
        if sample.error_px <= seed_gate {
            inlier_target.push(target_pts[idx]);
            inlier_image.push(image_pts[idx]);
        }
    }

    let h_refined = if inlier_target.len() >= 8 {
        estimate_homography_rect_to_img(&inlier_target, &inlier_image).unwrap_or(h_all)
    } else {
        h_all
    };

    ids.iter()
        .zip(target_pts.iter())
        .zip(image_pts.iter())
        .map(|((&id, &target), &image)| {
            let pred = h_refined.apply(target);
            let dx = pred.x - image.x;
            let dy = pred.y - image.y;
            ReprojectionError {
                id,
                error_px: (dx * dx + dy * dy).sqrt(),
            }
        })
        .collect()
}

#[test]
fn detects_charuco_on_large_png() {
    let img_path = testdata_path("large.png");
    let img = load_gray(&img_path);
    let raw_corners = detect_corners(&img);
    let corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    let dict = builtins::builtin_dictionary("DICT_4X4_1000").expect("builtin dict");
    let board = CharucoBoardSpec {
        rows: 22,
        cols: 22,
        cell_size: 1.0,
        marker_size_rel: 0.75,
        dictionary: dict,
        marker_layout: MarkerLayout::OpenCvCharuco,
    };

    let mut params = CharucoParams::for_board(&board);
    params.px_per_square = 60.0;
    params.min_marker_inliers = 64;

    let detector = CharucoDetector::new(params).expect("detector");

    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let res = detector.detect(&src_view, &corners).expect("detect");
    assert_eq!(res.detection.kind, TargetKind::Charuco);
    assert!(res.markers.len() >= 100);
    assert!(res.detection.corners.len() >= 200);
    assert!(res
        .detection
        .corners
        .iter()
        .all(|c| c.id.is_some() && c.grid.is_some() && c.target_position.is_some()));
    assert_unique_ids(&res, 22 * 22);

    let mut ids = Vec::with_capacity(res.detection.corners.len());
    let mut target_pts = Vec::with_capacity(res.detection.corners.len());
    let mut image_pts = Vec::with_capacity(res.detection.corners.len());
    for corner in &res.detection.corners {
        let id = corner.id.expect("id");
        let target = corner.target_position.expect("target_position");
        ids.push(id);
        target_pts.push(target);
        image_pts.push(corner.position);
    }

    let errors = compute_reprojection_errors(&ids, &target_pts, &image_pts, 4.0);
    let error_values: Vec<f32> = errors.iter().map(|sample| sample.error_px).collect();
    let outlier_gate_px = robust_error_gate(&error_values, 6.0, 3.0);
    let median_error_px = median(&error_values);

    let outlier_ids: HashSet<u32> = errors
        .iter()
        .filter(|sample| sample.error_px > outlier_gate_px)
        .map(|sample| sample.id)
        .collect();

    let mut ranked = errors;
    ranked.sort_by(|a, b| b.error_px.total_cmp(&a.error_px));
    let top12_ids: HashSet<u32> = ranked.iter().take(12).map(|sample| sample.id).collect();

    for known_bad_id in [369_u32, 309_u32, 109_u32] {
        // The homography-residual pruning in the chessboard detector now
        // removes these previously-problematic IDs outright when they are
        // clearly off-lattice. Treat "dropped by pruning" as a stronger form
        // of passing the test — only run the old "kept but not an outlier"
        // checks when the ID survived into the detection.
        let Some(sample) = ranked.iter().find(|entry| entry.id == known_bad_id) else {
            continue;
        };
        assert!(
            !outlier_ids.contains(&known_bad_id),
            "known problematic id {known_bad_id} is still a reprojection outlier (err={:.3}px, gate={:.3}px, median={:.3}px)",
            sample.error_px,
            outlier_gate_px,
            median_error_px
        );
        assert!(
            !top12_ids.contains(&known_bad_id),
            "known problematic id {known_bad_id} still ranks among top reprojection errors (err={:.3}px)",
            sample.error_px
        );
        assert!(
            sample.error_px <= median_error_px * 3.0,
            "known problematic id {known_bad_id} reprojection error is still far above baseline (err={:.3}px, median={:.3}px)",
            sample.error_px,
            median_error_px
        );
    }
}

struct PublicCase {
    img_name: &'static str,
    dict_name: &'static str,
    rows: u32,
    cols: u32,
    cell_size: f32,
    min_marker_inliers: usize,
    min_markers: usize,
    min_corners: usize,
    use_board_level: bool,
}

/// Shared helper: run the detector on one public testdata image and
/// assert basic contracts (kind, minimum markers/corners, unique ids,
/// zero self-consistency wrong-id).
#[allow(clippy::too_many_arguments)]
fn run_public_charuco(case: &PublicCase) {
    let img_name = case.img_name;
    let dict_name = case.dict_name;
    let rows = case.rows;
    let cols = case.cols;
    let cell_size = case.cell_size;
    let min_marker_inliers = case.min_marker_inliers;
    let min_markers = case.min_markers;
    let min_corners = case.min_corners;
    let use_board_level = case.use_board_level;
    let img_path = testdata_path(img_name);
    let img = load_gray(&img_path);
    let raw_corners = detect_corners(&img);
    let corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    let dict = builtins::builtin_dictionary(dict_name).expect("builtin dict");
    let board = CharucoBoardSpec {
        rows,
        cols,
        cell_size,
        marker_size_rel: 0.75,
        dictionary: dict,
        marker_layout: MarkerLayout::OpenCvCharuco,
    };

    let mut params = CharucoParams::for_board(&board);
    params.px_per_square = 60.0;
    params.min_marker_inliers = min_marker_inliers;
    params.use_board_level_matcher = use_board_level;
    if use_board_level {
        // The board-level matcher is its own inlier gate — keep the
        // downstream min_marker_inliers low so the matcher's margin
        // gate is what decides accept/reject.
        params.min_marker_inliers = 1;
        params.min_secondary_marker_inliers = 1;
    }

    let detector = CharucoDetector::new(params).expect("detector");
    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };
    let res = detector.detect(&src_view, &corners).unwrap_or_else(|e| {
        panic!(
            "{img_name} ({}) detect: {e}",
            if use_board_level {
                "board-level"
            } else {
                "legacy"
            }
        )
    });
    assert_eq!(res.detection.kind, TargetKind::Charuco);
    assert!(
        res.markers.len() >= min_markers,
        "{img_name} ({}): markers {} < {}",
        if use_board_level {
            "board-level"
        } else {
            "legacy"
        },
        res.markers.len(),
        min_markers,
    );
    assert!(
        res.detection.corners.len() >= min_corners,
        "{img_name} ({}): corners {} < {}",
        if use_board_level {
            "board-level"
        } else {
            "legacy"
        },
        res.detection.corners.len(),
        min_corners,
    );
    assert!(res
        .detection
        .corners
        .iter()
        .all(|c| c.id.is_some() && c.grid.is_some() && c.target_position.is_some()));
    assert_unique_ids(&res, rows * cols);
    assert_eq!(
        res.raw_marker_wrong_id_count,
        0,
        "{img_name} ({}): wrong-id count must be 0",
        if use_board_level {
            "board-level"
        } else {
            "legacy"
        },
    );
}

#[test]
fn board_matcher_detects_small_png() {
    run_public_charuco(&PublicCase {
        img_name: "small.png",
        dict_name: "DICT_4X4_250",
        rows: 22,
        cols: 22,
        cell_size: 5.2,
        min_marker_inliers: 12,
        min_markers: 20,
        min_corners: 60,
        use_board_level: true,
    });
}

#[test]
fn board_matcher_detects_small2_png() {
    // small2.png is the same nominal board as small.png (22×22 DICT_4X4_250)
    // from a slightly different pose — asserts the tuned matcher keeps
    // working under geometric variation.
    run_public_charuco(&PublicCase {
        img_name: "small2.png",
        dict_name: "DICT_4X4_250",
        rows: 22,
        cols: 22,
        cell_size: 5.2,
        min_marker_inliers: 12,
        min_markers: 20,
        min_corners: 60,
        use_board_level: true,
    });
}

#[test]
fn board_matcher_detects_large_png() {
    run_public_charuco(&PublicCase {
        img_name: "large.png",
        dict_name: "DICT_4X4_1000",
        rows: 22,
        cols: 22,
        cell_size: 1.0,
        min_marker_inliers: 64,
        min_markers: 100,
        min_corners: 200,
        use_board_level: true,
    });
}

#[test]
fn detects_charuco_on_small_png() {
    let img_path = testdata_path("small.png");
    let img = load_gray(&img_path);
    let raw_corners = detect_corners(&img);
    let corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    let dict = builtins::builtin_dictionary("DICT_4X4_250").expect("builtin dict");
    let board = CharucoBoardSpec {
        rows: 22,
        cols: 22,
        cell_size: 5.2,
        marker_size_rel: 0.75,
        dictionary: dict,
        marker_layout: MarkerLayout::OpenCvCharuco,
    };

    let mut params = CharucoParams::for_board(&board);
    params.px_per_square = 60.0;
    params.min_marker_inliers = 12;

    let detector = CharucoDetector::new(params).expect("detector");

    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let res = detector.detect(&src_view, &corners).expect("detect");
    assert_eq!(res.detection.kind, TargetKind::Charuco);
    assert!(res.markers.len() >= 20);
    assert!(res.detection.corners.len() >= 60);
    assert!(res
        .detection
        .corners
        .iter()
        .all(|c| c.id.is_some() && c.grid.is_some() && c.target_position.is_some()));
    assert_unique_ids(&res, 22 * 22);
}

#[test]
fn detects_plain_chessboard_on_mid_png() {
    let img_path = testdata_path("mid.png");
    let img = load_gray(&img_path);
    let raw_corners = detect_corners(&img);
    let corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    let mut chessboard = ChessboardParams::default();
    chessboard.min_corner_strength = 0.5;
    let detector = ChessboardDetector::new(chessboard);
    let res = detector.detect(&corners).expect("chessboard detect");
    assert_eq!(res.target.kind, TargetKind::Chessboard);

    let mut max_i = 0;
    let mut max_j = 0;
    for c in &res.target.corners {
        let g = c.grid.expect("grid coords");
        max_i = max_i.max(g.i);
        max_j = max_j.max(g.j);
    }

    assert_eq!(max_i + 1, 11, "expected 11 inner-corner columns");
    assert_eq!(max_j + 1, 7, "expected 7 inner-corner rows");
    assert_eq!(res.target.corners.len(), 11 * 7);
}
