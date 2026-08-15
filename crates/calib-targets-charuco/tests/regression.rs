use calib_targets::detect::default_chess_config;
use calib_targets_aruco::builtins;
use calib_targets_charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
use calib_targets_chessboard::ChessCorner as TargetCorner;
use calib_targets_chessboard::{ChessboardDetector, ChessboardParams};
use calib_targets_core::GrayImageView;
use chess_corners::{CornerDescriptor, Detector as ChessDetector};
use image::ImageReader;
use nalgebra::Point2;
use projective_grid::{
    check_consistency, ConsistencyParams, ConsistencyRequest, CoordinateHypothesis, LatticeKind,
    PointFeature,
};
use std::path::Path;

fn load_gray(path: &Path) -> image::GrayImage {
    ImageReader::open(path)
        .expect("open image")
        .decode()
        .expect("decode image")
        .to_luma8()
}

fn detect_corners(img: &image::GrayImage) -> Vec<CornerDescriptor> {
    let chess_cfg = // chess-corners 1.0 removed relative thresholding for the ChESS
    // strategy (`threshold` is now an absolute floor on the raw response;
    // only Radon reads it as a fraction). Rather than invent an absolute
    // number to imitate the old adaptive cutoff, use the workspace
    // production default, so this exercises the config real callers get.
    default_chess_config().with_detection(|d| d.nms_radius = 2);
    let mut detector = ChessDetector::new(chess_cfg).expect("build ChESS detector");
    detector.detect(img).expect("ChESS detection")
}

fn adapt_chess_corner(c: &CornerDescriptor) -> TargetCorner {
    TargetCorner::new(
        Point2::new(c.x, c.y),
        // `axes` is `None` only when the upstream orientation fit is
        // skipped; these fixtures always fit it.
        c.axes
            .map(|a| {
                [
                    calib_targets_core::AxisEstimate {
                        angle: a[0].angle,
                        sigma: a[0].sigma,
                    },
                    calib_targets_core::AxisEstimate {
                        angle: a[1].angle,
                        sigma: a[1].sigma,
                    },
                ]
            })
            .expect("orientation fit enabled"),
        c.response,
    )
}

fn assert_unique_ids(res: &calib_targets_charuco::CharucoDetection, max_id: u32) {
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

/// Assert that a shipped detection is **projectively self-consistent**.
///
/// For a planar board the map `(grid.u, grid.v) -> position` is a homography by
/// construction, so a detection can be checked against itself with no ground
/// truth, no camera model and no calibration: fit the lattice-to-image map to
/// the detection's own pairs and look at the residual.
///
/// Two things are asserted:
///
/// 1. **Distinct positions.** Two lattice nodes at one pixel is a fold no
///    homography admits, and it is unrecoverable downstream — the consumer
///    cannot tell which of the labels was meant.
/// 2. **Residual small against the cell pitch.** Expressed as a fraction of the
///    detection's own cell pitch, so the bound is free of image scale, board
///    size and viewing distance.
///
///    The two regimes it separates are physically distinct, which is what makes
///    a single bound defensible here. A *global* homography cannot represent
///    lens distortion, so a healthy labelling on a wide-angle frame still shows
///    a residual — but a smooth, sub-cell one: measured 0.7 % of the pitch on
///    `large.png` and 5.2–5.4 % on the two `small*.png` frames, where the board
///    fills the sensor. A *mislabelled* corner, by contrast, is displaced by a
///    whole lattice step — on the order of 100 % of the pitch — and even a
///    small mislabelled minority drags the median past 10 % (the issue-#86
///    frames measured 11 %–600 %). The 8 % bound sits in the gap between
///    "smooth sub-cell distortion" and "displaced by a lattice step"; it is an
///    order-of-magnitude separator, not a value fitted to a frame.
///
///    This is a *fixture contract*, not the production gate. A global residual
///    bound cannot be a production precision gate precisely because it absorbs
///    distortion — the detector's own guard against this failure is the
///    parameter-free lattice-orientation parity invariant in `projective-grid`.
fn assert_projectively_consistent(res: &calib_targets_charuco::CharucoDetection, img_name: &str) {
    /// Coincidence radius as a fraction of the cell pitch, matching
    /// `projective-grid`'s duplicate-label guard.
    const DUP_PIXEL_FRAC: f32 = 0.2;
    /// Residual bound as a fraction of the cell pitch. See the doc above.
    const MAX_RESIDUAL_OVER_PITCH: f32 = 0.08;

    let corners = &res.corners;
    assert!(
        corners.len() >= 4,
        "{img_name}: {} corners is too few to check",
        corners.len()
    );

    // Cell pitch: median cardinal-edge length of the labelled set.
    let by_grid: std::collections::HashMap<(i32, i32), Point2<f32>> = corners
        .iter()
        .map(|c| ((c.grid.u, c.grid.v), c.position))
        .collect();
    let mut edges: Vec<f32> = Vec::new();
    for (&(u, v), &p) in &by_grid {
        for (du, dv) in [(1, 0), (0, 1)] {
            if let Some(&q) = by_grid.get(&(u + du, v + dv)) {
                edges.push((q - p).norm());
            }
        }
    }
    assert!(
        !edges.is_empty(),
        "{img_name}: no cardinal edges to size from"
    );
    edges.sort_by(|a, b| a.partial_cmp(b).expect("finite edge lengths"));
    let pitch = edges[edges.len() / 2];

    let eps = DUP_PIXEL_FRAC * pitch;
    for (a, ca) in corners.iter().enumerate() {
        for cb in &corners[a + 1..] {
            assert!(
                (ca.position - cb.position).norm() >= eps,
                "{img_name}: ids {} and {} collapsed onto one corner at {:?}",
                ca.id,
                cb.id,
                ca.position,
            );
        }
    }

    let features: Vec<PointFeature> = corners
        .iter()
        .enumerate()
        .map(|(i, c)| PointFeature::new(i, c.position))
        .collect();
    let hypotheses: Vec<CoordinateHypothesis> = corners
        .iter()
        .enumerate()
        .map(|(i, c)| CoordinateHypothesis::unweighted(i, c.grid))
        .collect();
    let report = check_consistency(ConsistencyRequest::new(
        LatticeKind::Square,
        &features,
        &hypotheses,
        None,
        // Infinite tolerance: the fit is what we want, the judgement is below.
        ConsistencyParams::new(f32::INFINITY),
    ))
    .unwrap_or_else(|e| panic!("{img_name}: self-consistency fit failed: {e}"));

    let mut residuals: Vec<f32> = report
        .grid()
        .entries()
        .iter()
        .filter_map(|e| e.residual_px)
        .collect();
    residuals.sort_by(|a, b| a.partial_cmp(b).expect("finite residuals"));
    let median = residuals[residuals.len() / 2];
    assert!(
        median <= MAX_RESIDUAL_OVER_PITCH * pitch,
        "{img_name}: labelling is not projectively self-consistent — median \
         residual {median:.2} px is {:.1}% of the {pitch:.1} px cell pitch \
         (bound {:.0}%)",
        100.0 * median / pitch,
        100.0 * MAX_RESIDUAL_OVER_PITCH,
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

    let mut params = CharucoParams::for_board(board);
    params.px_per_square = 60.0;
    // The board-level matcher is its own inlier gate — keep the downstream
    // min_marker_inliers low so the matcher's margin gate is what decides
    // accept/reject.
    params.min_marker_inliers = 1;
    // `min_secondary_marker_inliers` is now an advanced knob; its default (1)
    // already matches what this test wants, so no override is needed.

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
    assert_projectively_consistent(&res, img_name);
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

    let mut params = CharucoParams::for_board(board);
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
        max_i = max_i.max(c.grid.u);
        max_j = max_j.max(c.grid.v);
    }

    assert_eq!(max_i + 1, 11, "expected 11 inner-corner columns");
    assert_eq!(max_j + 1, 7, "expected 7 inner-corner rows");
    assert_eq!(res.corners.len(), 11 * 7);
}
