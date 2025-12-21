use calib_targets_aruco::builtins;
use calib_targets_charuco::{
    CharucoBoard, CharucoBoardSpec, CharucoDetector, CharucoDetectorParams, MarkerLayout,
};
use calib_targets_chessboard::{ChessboardDetector, ChessboardParams, GridGraphParams};
use calib_targets_core::{Corner as TargetCorner, GrayImageView, TargetKind};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use nalgebra::Point2;
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
    chess_cfg.params.threshold_rel = 0.2;
    chess_cfg.params.nms_radius = 2;
    find_chess_corners_image(img, &chess_cfg)
}

fn adapt_chess_corner(c: &CornerDescriptor) -> TargetCorner {
    TargetCorner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
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

#[test]
fn detects_charuco_on_large_png() {
    let img_path = testdata_path("large.png");
    let img = load_gray(&img_path);
    let raw_corners = detect_corners(&img);
    let corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    let dict = builtins::builtin_dictionary("DICT_4X4_1000").expect("builtin dict");
    let board = CharucoBoard::new(CharucoBoardSpec {
        rows: 22,
        cols: 22,
        cell_size: 1.0,
        marker_size_rel: 0.75,
        dictionary: dict,
        marker_layout: MarkerLayout::OpenCvCharuco,
    })
    .expect("board spec");

    let mut params = CharucoDetectorParams::for_board(&board);
    params.px_per_square = 60.0;
    params.chessboard.min_corners = 50;
    params.graph.min_spacing_pix = 40.0;
    params.graph.max_spacing_pix = 160.0;
    params.min_marker_inliers = 64;

    let detector = CharucoDetector::new(board, params);

    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let res = detector.detect(&src_view, &corners).expect("detect");
    assert_eq!(res.detection.kind, TargetKind::Charuco);
    assert!(res.alignment.marker_inliers.len() >= 100);
    assert!(res.detection.corners.len() >= 200);
    assert_unique_ids(&res, 22 * 22);
}

#[test]
fn detects_charuco_on_small_png() {
    let img_path = testdata_path("small.png");
    let img = load_gray(&img_path);
    let raw_corners = detect_corners(&img);
    let corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    let dict = builtins::builtin_dictionary("DICT_4X4_250").expect("builtin dict");
    let board = CharucoBoard::new(CharucoBoardSpec {
        rows: 10,
        cols: 10,
        cell_size: 1.0,
        marker_size_rel: 0.75,
        dictionary: dict,
        marker_layout: MarkerLayout::OpenCvCharuco,
    })
    .expect("board spec");

    let mut params = CharucoDetectorParams::for_board(&board);
    params.px_per_square = 60.0;
    params.chessboard.min_corners = 10;
    params.chessboard.completeness_threshold = 0.02;
    params.graph.min_spacing_pix = 5.0;
    params.graph.max_spacing_pix = 60.0;
    params.min_marker_inliers = 12;

    let detector = CharucoDetector::new(board, params);

    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let res = detector.detect(&src_view, &corners).expect("detect");
    assert_eq!(res.detection.kind, TargetKind::Charuco);
    assert!(res.alignment.marker_inliers.len() >= 20);
    assert!(res.detection.corners.len() >= 60);
    assert_unique_ids(&res, 22 * 22);
}

#[test]
fn detects_plain_chessboard_on_mid_png() {
    let img_path = testdata_path("mid.png");
    let img = load_gray(&img_path);
    let raw_corners = detect_corners(&img);
    let corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    let chessboard = ChessboardParams {
        min_corner_strength: 0.5,
        min_corners: 20,
        expected_rows: Some(7),
        expected_cols: Some(11),
        completeness_threshold: 0.9,
        ..ChessboardParams::default()
    };
    let graph = GridGraphParams {
        min_spacing_pix: 10.0,
        max_spacing_pix: 120.0,
        k_neighbors: 8,
        orientation_tolerance_deg: 22.5,
    };
    let detector = ChessboardDetector::new(chessboard).with_grid_search(graph);
    let res = detector
        .detect_from_corners(&corners)
        .expect("chessboard detect");
    assert_eq!(res.detection.kind, TargetKind::Chessboard);

    let mut max_i = 0;
    let mut max_j = 0;
    for c in &res.detection.corners {
        let g = c.grid.expect("grid coords");
        max_i = max_i.max(g.i);
        max_j = max_j.max(g.j);
    }

    assert_eq!(max_i + 1, 11, "expected 11 inner-corner columns");
    assert_eq!(max_j + 1, 7, "expected 7 inner-corner rows");
    assert_eq!(res.detection.corners.len(), 11 * 7);
}
