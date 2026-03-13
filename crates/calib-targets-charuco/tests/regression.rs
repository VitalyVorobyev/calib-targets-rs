use calib_targets_aruco::builtins;
use calib_targets_charuco::{
    split_composite_rects, CharucoBoardSpec, CharucoDetector, CharucoDetectorParams, DatasetConfig,
    MarkerLayout,
};
use calib_targets_chessboard::{ChessboardDetector, ChessboardParams, GridGraphParams};
use calib_targets_core::{
    estimate_homography_rect_to_img, Corner as TargetCorner, GrayImageView, TargetKind,
};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::{imageops::crop_imm, ImageReader};
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

fn detect_3536119669_strip(
    image_name: &str,
    strip_index: usize,
) -> calib_targets_charuco::CharucoDetectionRun {
    let dataset_cfg =
        DatasetConfig::load_json(testdata_path("3536119669/config.json")).expect("dataset config");
    let board = dataset_cfg.board_spec().expect("board spec");
    let expected_strip_size = dataset_cfg.strip_size().expect("strip size");

    let composite = load_gray(&testdata_path(&format!("3536119669/{image_name}")));
    let rects = split_composite_rects(
        composite.width(),
        composite.height(),
        Some(expected_strip_size),
    )
    .expect("strip rects");
    let rect = rects[strip_index];
    let crop = crop_imm(&composite, rect.x, rect.y, rect.width, rect.height).to_image();

    let raw_corners = detect_corners(&crop);
    let corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    let mut params = CharucoDetectorParams::for_board(&board);
    params.px_per_square = 60.0;
    let detector = CharucoDetector::new(params).expect("detector");
    let src_view = GrayImageView {
        width: crop.width() as usize,
        height: crop.height() as usize,
        data: crop.as_raw(),
    };
    detector.detect_with_diagnostics(&src_view, &corners)
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

    let mut params = CharucoDetectorParams::for_board(&board);
    params.px_per_square = 60.0;
    params.chessboard.min_corners = 50;
    params.graph.min_spacing_pix = 40.0;
    params.graph.max_spacing_pix = 160.0;
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
        let sample = ranked
            .iter()
            .find(|entry| entry.id == known_bad_id)
            .expect("known problematic id should be present in large.png detection");
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

    let mut params = CharucoDetectorParams::for_board(&board);
    params.px_per_square = 60.0;
    params.chessboard.min_corners = 10;
    params.chessboard.completeness_threshold = 0.02;
    params.graph.min_spacing_pix = 5.0;
    params.graph.max_spacing_pix = 60.0;
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
fn algo_002_preserves_good_strip_marker_support() {
    let strip_1 = detect_3536119669_strip("target_0.png", 1);
    assert!(
        strip_1.result.is_ok(),
        "target_0 strip_1 should stay successful"
    );
    assert!(
        strip_1.diagnostics.decoded_marker_count >= 8,
        "target_0 strip_1 decoded markers regressed: {}",
        strip_1.diagnostics.decoded_marker_count
    );
    assert!(
        strip_1
            .diagnostics
            .marker_path
            .inferred
            .selected_marker_count
            >= 1,
        "target_0 strip_1 inferred selected markers regressed: {}",
        strip_1
            .diagnostics
            .marker_path
            .inferred
            .selected_marker_count
    );
    assert!(
        strip_1
            .diagnostics
            .marker_path
            .inferred
            .expected_id_match_count
            >= 1,
        "target_0 strip_1 inferred expected-id matches regressed: {}",
        strip_1
            .diagnostics
            .marker_path
            .inferred
            .expected_id_match_count
    );

    let strip_5 = detect_3536119669_strip("target_2.png", 5);
    assert!(
        strip_5.result.is_ok(),
        "target_2 strip_5 should stay successful"
    );
    assert!(
        strip_5.diagnostics.decoded_marker_count >= 11,
        "target_2 strip_5 decoded markers regressed: {}",
        strip_5.diagnostics.decoded_marker_count
    );
    assert!(
        strip_5
            .diagnostics
            .marker_path
            .inferred
            .cells_with_any_decode_count
            >= 3,
        "target_2 strip_5 inferred any-decode count regressed: {}",
        strip_5
            .diagnostics
            .marker_path
            .inferred
            .cells_with_any_decode_count
    );
    assert!(
        strip_5
            .diagnostics
            .marker_path
            .inferred
            .selected_marker_count
            >= 2,
        "target_2 strip_5 inferred selected markers regressed: {}",
        strip_5
            .diagnostics
            .marker_path
            .inferred
            .selected_marker_count
    );
    assert!(
        strip_5
            .diagnostics
            .marker_path
            .inferred
            .expected_id_match_count
            >= 2,
        "target_2 strip_5 inferred expected-id matches regressed: {}",
        strip_5
            .diagnostics
            .marker_path
            .inferred
            .expected_id_match_count
    );
}

#[test]
fn algo_002_keeps_target_2_strip_3_gain() {
    let strip_3 = detect_3536119669_strip("target_2.png", 3);
    assert!(
        strip_3
            .diagnostics
            .marker_path
            .inferred
            .cells_with_any_decode_count
            >= 3,
        "target_2 strip_3 should keep the inferred any-decode gain: {}",
        strip_3
            .diagnostics
            .marker_path
            .inferred
            .cells_with_any_decode_count
    );
    assert!(
        strip_3
            .diagnostics
            .marker_path
            .inferred
            .selected_marker_count
            >= 3,
        "target_2 strip_3 should keep the inferred selected-marker gain: {}",
        strip_3
            .diagnostics
            .marker_path
            .inferred
            .selected_marker_count
    );
    assert!(
        strip_3
            .diagnostics
            .marker_path
            .inferred
            .expected_id_match_count
            >= 1,
        "target_2 strip_3 should keep at least one inferred expected-id match: {}",
        strip_3
            .diagnostics
            .marker_path
            .inferred
            .expected_id_match_count
    );
}

#[test]
fn detect_with_diagnostics_matches_detect_on_small_png() {
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

    let mut params = CharucoDetectorParams::for_board(&board);
    params.px_per_square = 60.0;
    params.chessboard.min_corners = 10;
    params.chessboard.completeness_threshold = 0.02;
    params.graph.min_spacing_pix = 5.0;
    params.graph.max_spacing_pix = 60.0;
    params.min_marker_inliers = 12;

    let detector = CharucoDetector::new(params).expect("detector");
    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let direct = detector.detect(&src_view, &corners).expect("direct detect");
    let run = detector.detect_with_diagnostics(&src_view, &corners);
    let diagnostics = run.diagnostics.clone();
    let with_diag = run.result.expect("diagnostic detect");

    assert_eq!(direct.alignment, with_diag.alignment);
    assert_eq!(direct.markers.len(), with_diag.markers.len());
    assert_eq!(
        direct.detection.corners.len(),
        with_diag.detection.corners.len()
    );

    let direct_ids: Vec<u32> = direct
        .detection
        .corners
        .iter()
        .map(|corner| corner.id.expect("id"))
        .collect();
    let diag_ids: Vec<u32> = with_diag
        .detection
        .corners
        .iter()
        .map(|corner| corner.id.expect("id"))
        .collect();
    assert_eq!(direct_ids, diag_ids);

    assert!(diagnostics.candidate_cell_count >= diagnostics.decoded_marker_count);
    assert!(diagnostics.decoded_marker_count >= diagnostics.aligned_marker_count);
    assert_eq!(diagnostics.aligned_marker_count, with_diag.markers.len());
    assert_eq!(
        diagnostics.final_corner_count,
        with_diag.detection.corners.len()
    );
    assert!(diagnostics.marker_path.expected_id_accounted);
    assert!(diagnostics.marker_path.covers_selected_evaluation);
    assert_eq!(
        diagnostics.marker_path.complete.candidate_cell_count
            + diagnostics.marker_path.inferred.candidate_cell_count,
        diagnostics.candidate_cell_count
    );
    assert!(
        diagnostics.marker_path.complete.candidate_cell_count
            >= diagnostics.marker_path.complete.cells_with_any_decode_count
    );
    assert!(
        diagnostics.marker_path.complete.cells_with_any_decode_count
            >= diagnostics.marker_path.complete.selected_marker_count
    );
    assert!(
        diagnostics.marker_path.inferred.candidate_cell_count
            >= diagnostics.marker_path.inferred.cells_with_any_decode_count
    );
    assert!(
        diagnostics.marker_path.inferred.cells_with_any_decode_count
            >= diagnostics.marker_path.inferred.selected_marker_count
    );
    assert_eq!(
        diagnostics
            .marker_path
            .complete
            .selected_hamming
            .histogram
            .iter()
            .sum::<usize>(),
        diagnostics.marker_path.complete.selected_marker_count
    );
    assert_eq!(
        diagnostics
            .marker_path
            .inferred
            .selected_hamming
            .histogram
            .iter()
            .sum::<usize>(),
        diagnostics.marker_path.inferred.selected_marker_count
    );
    assert_eq!(
        diagnostics.marker_path.complete.expected_id_match_count
            + diagnostics
                .marker_path
                .complete
                .expected_id_contradiction_count
            + diagnostics
                .marker_path
                .complete
                .non_marker_confident_decode_count,
        diagnostics.marker_path.complete.selected_marker_count
    );
    assert_eq!(
        diagnostics.marker_path.inferred.expected_id_match_count
            + diagnostics
                .marker_path
                .inferred
                .expected_id_contradiction_count
            + diagnostics
                .marker_path
                .inferred
                .non_marker_confident_decode_count,
        diagnostics.marker_path.inferred.selected_marker_count
    );
    let validation = diagnostics
        .corner_validation
        .expect("corner validation diagnostics");
    assert_eq!(
        validation.kept_corner_count + validation.corrected_corner_count,
        diagnostics.final_corner_count
    );
    assert_eq!(
        validation.kept_corner_count
            + validation.corrected_corner_count
            + validation.dropped_corner_count,
        diagnostics.mapped_corner_count_before_validation
    );
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
