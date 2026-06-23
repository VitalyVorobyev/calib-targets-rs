use calib_targets_aruco::builtins;
use calib_targets_charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
use calib_targets_chessboard::ChessCorner as TargetCorner;
use calib_targets_chessboard::{
    Detector as ChessboardDetector, DetectorParams as ChessboardParams,
};
use calib_targets_core::GrayImageView;
use chess_corners::{CornerDescriptor, Detector as ChessDetector, DetectorConfig, Threshold};
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
    let chess_cfg = DetectorConfig::chess()
        .with_threshold(Threshold::Relative(0.2))
        .with_chess(|c| c.nms_radius = 2);
    let mut detector = ChessDetector::new(chess_cfg).expect("build ChESS detector");
    detector.detect(img).expect("ChESS detection")
}

fn adapt_chess_corner(c: &CornerDescriptor) -> TargetCorner {
    TargetCorner {
        position: Point2::new(c.x, c.y),
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
    let mut ids: Vec<u32> = res.corners.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(
        ids.len(),
        res.corners.len(),
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

struct PublicCase {
    img_name: &'static str,
    dict_name: &'static str,
    rows: u32,
    cols: u32,
    cell_size: f32,
    min_markers: usize,
    min_corners: usize,
}

/// Shared helper: run the detector on one public testdata image and
/// assert basic contracts (kind, minimum markers/corners, unique ids,
/// zero self-consistency wrong-id).
fn run_public_charuco(case: &PublicCase) {
    let img_name = case.img_name;
    let dict_name = case.dict_name;
    let rows = case.rows;
    let cols = case.cols;
    let cell_size = case.cell_size;
    let min_markers = case.min_markers;
    let min_corners = case.min_corners;
    let img_path = testdata_path(img_name);
    let img = load_gray(&img_path);
    let raw_corners = detect_corners(&img);
    let corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    let dict = builtins::builtin_dictionary(dict_name).expect("builtin dict");
    let board = CharucoBoardSpec::new(rows, cols, cell_size, 0.75, dict)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);

    let mut params = CharucoParams::for_board(&board);
    params.px_per_square = 60.0;
    // The board-level matcher is its own inlier gate — keep the downstream
    // min_marker_inliers low so the matcher's margin gate is what decides
    // accept/reject.
    params.min_marker_inliers = 1;
    params.min_secondary_marker_inliers = 1;

    let detector = CharucoDetector::new(params).expect("detector");
    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };
    let (res, diagnostics) = detector.detect_with_diagnostics(&src_view, &corners);
    let res = res.unwrap_or_else(|e| panic!("{img_name}: detect: {e}"));
    assert!(
        res.markers.len() >= min_markers,
        "{img_name}: markers {} < {}",
        res.markers.len(),
        min_markers,
    );
    assert!(
        res.corners.len() >= min_corners,
        "{img_name}: corners {} < {}",
        res.corners.len(),
        min_corners,
    );
    assert_unique_ids(&res, rows * cols);
    assert_eq!(
        diagnostics.raw_marker_wrong_id_count, 0,
        "{img_name}: wrong-id count must be 0",
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
        min_markers: 20,
        min_corners: 60,
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
        min_markers: 20,
        min_corners: 60,
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
        min_markers: 100,
        min_corners: 200,
    });
}

#[test]
fn detects_charuco_on_small_png() {
    let img_path = testdata_path("small.png");
    let img = load_gray(&img_path);
    let raw_corners = detect_corners(&img);
    let corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    let dict = builtins::builtin_dictionary("DICT_4X4_250").expect("builtin dict");
    let board = CharucoBoardSpec::new(22, 22, 5.2, 0.75, dict)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);

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
    assert!(res.markers.len() >= 20);
    assert!(res.corners.len() >= 60);
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
    let detector = ChessboardDetector::new(chessboard).expect("valid detector params");
    let res = detector.detect(&corners).expect("chessboard detect");

    let mut max_i = 0;
    let mut max_j = 0;
    for c in &res.corners {
        max_i = max_i.max(c.grid.i);
        max_j = max_j.max(c.grid.j);
    }

    assert_eq!(max_i + 1, 11, "expected 11 inner-corner columns");
    assert_eq!(max_j + 1, 7, "expected 7 inner-corner rows");
    assert_eq!(res.corners.len(), 11 * 7);
}
