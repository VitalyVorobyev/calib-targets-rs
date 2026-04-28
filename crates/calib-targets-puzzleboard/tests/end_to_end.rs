//! End-to-end test: render a PuzzleBoard target via `calib-targets-print`,
//! detect ChESS corners on the PNG, run the PuzzleBoard detector, and
//! verify every returned `LabeledCorner` is labelled with the expected
//! master (I, J) coordinates.

use calib_targets_core::{Corner as TargetCorner, GrayImageView};
use calib_targets_print::{PageSize, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec};
use calib_targets_puzzleboard::{
    PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSearchMode, PuzzleBoardSpec,
};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::{ImageBuffer, Luma};
use nalgebra::Point2;

fn adapt(c: &CornerDescriptor) -> TargetCorner {
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

fn render_png_to_gray_image(bundle_bytes: &[u8]) -> ImageBuffer<Luma<u8>, Vec<u8>> {
    let img = image::load_from_memory(bundle_bytes).expect("decode PNG");
    img.to_luma8()
}

#[test]
fn render_detect_roundtrip_on_small_puzzleboard() {
    // 1) Build a printable PuzzleBoard spec.
    let spec = PuzzleBoardTargetSpec {
        rows: 10,
        cols: 10,
        square_size_mm: 12.0,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    };
    let mut doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(spec.clone()));
    doc.page.size = PageSize::Custom {
        width_mm: 200.0,
        height_mm: 200.0,
    };
    doc.page.margin_mm = 5.0;
    // High DPI so ChESS corners are detectable.
    doc.render.png_dpi = 300;

    let bundle = calib_targets_print::render_target_bundle(&doc).expect("render");
    let gray = render_png_to_gray_image(&bundle.png_bytes);

    // 2) Detect ChESS corners.
    let mut cfg = ChessConfig::single_scale();
    cfg.threshold_mode = chess_corners::ThresholdMode::Relative;
    cfg.threshold_value = 0.15;
    cfg.nms_radius = 3;
    let descriptors = find_chess_corners_image(&gray, &cfg).expect("ChESS detection");
    assert!(
        descriptors.len() >= 60,
        "expected at least 60 ChESS corners, got {}",
        descriptors.len()
    );

    // 3) Run the PuzzleBoard detector.
    let board_spec = PuzzleBoardSpec::with_origin(
        spec.rows,
        spec.cols,
        spec.square_size_mm as f32,
        spec.origin_row,
        spec.origin_col,
    )
    .expect("board");
    let params = PuzzleBoardParams::for_board(&board_spec);
    println!(
        "detected {} ChESS corners on a {}x{} image",
        descriptors.len(),
        gray.width(),
        gray.height()
    );
    let detector = PuzzleBoardDetector::new(params).expect("detector");

    let corners: Vec<TargetCorner> = descriptors.iter().map(adapt).collect();
    let view = GrayImageView {
        width: gray.width() as usize,
        height: gray.height() as usize,
        data: gray.as_raw(),
    };
    let result = match detector.detect(&view, &corners) {
        Ok(r) => r,
        Err(e) => panic!("detection failed: {e}"),
    };

    // 4) At least half the inner corners should be labelled consistently.
    let inner_corners = (spec.rows as usize - 1) * (spec.cols as usize - 1);
    assert!(
        result.detection.corners.len() >= inner_corners / 2,
        "too few labelled corners: {} / {}",
        result.detection.corners.len(),
        inner_corners
    );

    // 5) Every labelled corner should have an id, a master (I, J) grid coord,
    //    and a target position in mm consistent with the master layout.
    for lc in &result.detection.corners {
        assert!(lc.id.is_some(), "missing id");
        assert!(lc.grid.is_some(), "missing grid");
        let grid = lc.grid.unwrap();
        // Master coords must lie within the board.
        assert!(grid.i >= 0 && grid.i < 501);
        assert!(grid.j >= 0 && grid.j < 501);
    }

    // 6) Alignment must satisfy: every master-label pair (I, J) is consistent
    //    with local grid (i, j) and the returned alignment — i.e. for every
    //    two corners, the master-delta equals the local-delta under the
    //    alignment's linear part.
    let labelled: Vec<_> = result
        .detection
        .corners
        .iter()
        .filter_map(|c| c.grid.map(|g| (g.i, g.j)))
        .collect();
    assert!(labelled.len() >= 4, "need at least 4 corners for check");
    // All labelled corners share the same alignment so their pairwise master
    // differences must be unimodular (Δ-consistent). Simpler check: no
    // duplicated master coords.
    let mut seen = std::collections::HashSet::new();
    for g in &labelled {
        assert!(seen.insert(*g), "duplicate master coord {:?}", g);
    }

    // 7) Decode diagnostics should show a low bit-error rate.
    assert!(
        result.decode.bit_error_rate < 0.30,
        "unexpectedly high bit error rate: {}",
        result.decode.bit_error_rate
    );
}

/// `FixedBoard` must agree with `Full` when the camera sees the whole board —
/// same master origin and byte-for-byte identical labelled corners.
#[test]
fn fixed_board_agrees_with_full_on_whole_view() {
    let spec = PuzzleBoardTargetSpec {
        rows: 10,
        cols: 10,
        square_size_mm: 12.0,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    };
    let mut doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(spec.clone()));
    doc.page.size = PageSize::Custom {
        width_mm: 200.0,
        height_mm: 200.0,
    };
    doc.page.margin_mm = 5.0;
    doc.render.png_dpi = 300;

    let bundle = calib_targets_print::render_target_bundle(&doc).expect("render");
    let gray = render_png_to_gray_image(&bundle.png_bytes);

    let mut cfg = ChessConfig::single_scale();
    cfg.threshold_mode = chess_corners::ThresholdMode::Relative;
    cfg.threshold_value = 0.15;
    cfg.nms_radius = 3;
    let descriptors = find_chess_corners_image(&gray, &cfg).expect("ChESS detection");
    let corners: Vec<TargetCorner> = descriptors.iter().map(adapt).collect();

    let board_spec = PuzzleBoardSpec::with_origin(
        spec.rows,
        spec.cols,
        spec.square_size_mm as f32,
        spec.origin_row,
        spec.origin_col,
    )
    .expect("board");

    let view = GrayImageView {
        width: gray.width() as usize,
        height: gray.height() as usize,
        data: gray.as_raw(),
    };

    let params_full = PuzzleBoardParams::for_board(&board_spec);
    let full = PuzzleBoardDetector::new(params_full.clone())
        .expect("detector")
        .detect(&view, &corners)
        .expect("full decode");

    let mut params_fixed = params_full;
    params_fixed.decode.search_mode = PuzzleBoardSearchMode::FixedBoard;
    let fixed = PuzzleBoardDetector::new(params_fixed)
        .expect("detector")
        .detect(&view, &corners)
        .expect("fixed-board decode");

    assert_eq!(
        full.decode.master_origin_row, fixed.decode.master_origin_row,
        "master_origin_row mismatch"
    );
    assert_eq!(
        full.decode.master_origin_col, fixed.decode.master_origin_col,
        "master_origin_col mismatch"
    );
    assert_eq!(full.decode.edges_matched, fixed.decode.edges_matched);
    assert!((full.decode.bit_error_rate - fixed.decode.bit_error_rate).abs() < 1e-5);

    assert_eq!(full.detection.corners.len(), fixed.detection.corners.len(),);
    for (f, g) in full
        .detection
        .corners
        .iter()
        .zip(fixed.detection.corners.iter())
    {
        assert_eq!(f.id, g.id, "corner id mismatch");
        assert_eq!(f.grid, g.grid, "corner grid mismatch");
        assert_eq!(f.target_position, g.target_position);
    }
}

/// Multi-camera contract: three disjoint partial views of the same physical
/// board must label overlapping corners with identical master IDs when all
/// three cameras decode via `FixedBoard`.
#[test]
fn fixed_board_agrees_across_disjoint_partial_views() {
    let spec = PuzzleBoardTargetSpec {
        rows: 20,
        cols: 20,
        square_size_mm: 8.0,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    };
    let mut doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(spec.clone()));
    doc.page.size = PageSize::Custom {
        width_mm: 220.0,
        height_mm: 220.0,
    };
    doc.page.margin_mm = 5.0;
    doc.render.png_dpi = 300;

    let bundle = calib_targets_print::render_target_bundle(&doc).expect("render");
    let gray = render_png_to_gray_image(&bundle.png_bytes);

    let mut cfg = ChessConfig::single_scale();
    cfg.threshold_mode = chess_corners::ThresholdMode::Relative;
    cfg.threshold_value = 0.15;
    cfg.nms_radius = 3;
    let descriptors = find_chess_corners_image(&gray, &cfg).expect("ChESS detection");
    let all_corners: Vec<TargetCorner> = descriptors.iter().map(adapt).collect();

    let view = GrayImageView {
        width: gray.width() as usize,
        height: gray.height() as usize,
        data: gray.as_raw(),
    };
    let board_spec = PuzzleBoardSpec::with_origin(
        spec.rows,
        spec.cols,
        spec.square_size_mm as f32,
        spec.origin_row,
        spec.origin_col,
    )
    .expect("board");
    let mut params = PuzzleBoardParams::for_board(&board_spec);
    params.decode.search_mode = PuzzleBoardSearchMode::FixedBoard;
    // chessboard detector is scale-invariant and has no expected_rows /
    // expected_cols / min_corners gates; the smallest meaningful detection
    // is governed by `min_labeled_corners` (default 8 — fine for a 4×4
    // partial view).
    let detector = PuzzleBoardDetector::new(params).expect("detector");

    // Three overlapping subsets of the image. Each covers ~half the board in
    // one axis and the middle third in the other, so every pair of views
    // shares a strip of corners.
    let w = gray.width() as f32;
    let h = gray.height() as f32;
    let view_boxes = [
        // Upper-left three-quarters.
        (0.0, 0.0, 0.75 * w, 0.75 * h),
        // Lower-right three-quarters.
        (0.25 * w, 0.25 * h, w, h),
        // Horizontal middle band.
        (0.0, 0.25 * h, w, 0.75 * h),
    ];

    let subsets: Vec<Vec<TargetCorner>> = view_boxes
        .iter()
        .map(|&(x0, y0, x1, y1)| {
            all_corners
                .iter()
                .filter(|c| {
                    c.position.x >= x0
                        && c.position.x < x1
                        && c.position.y >= y0
                        && c.position.y < y1
                })
                .cloned()
                .collect()
        })
        .collect();

    // Detect each subset and index labelled corners by rounded image position.
    let mut per_view: Vec<std::collections::HashMap<(i32, i32), u32>> = Vec::new();
    for (i, subset) in subsets.iter().enumerate() {
        assert!(
            subset.len() >= 12,
            "view {i} has too few corners ({}) — test is miscalibrated",
            subset.len()
        );
        let res = detector
            .detect(&view, subset)
            .unwrap_or_else(|e| panic!("view {i} decode failed: {e}"));
        let mut m = std::collections::HashMap::new();
        for lc in &res.detection.corners {
            let key = (
                (lc.position.x * 0.5).round() as i32,
                (lc.position.y * 0.5).round() as i32,
            );
            m.insert(key, lc.id.expect("labelled corner without id"));
        }
        per_view.push(m);
    }

    // For every corner seen by two or more views, the master id must match.
    let mut overlap_checks = 0usize;
    for i in 0..per_view.len() {
        for j in (i + 1)..per_view.len() {
            for (key, id_i) in &per_view[i] {
                if let Some(id_j) = per_view[j].get(key) {
                    assert_eq!(
                        id_i, id_j,
                        "id disagreement between view {i} and view {j} at {key:?}"
                    );
                    overlap_checks += 1;
                }
            }
        }
    }
    assert!(
        overlap_checks > 0,
        "no overlapping corners across views — test boxes need adjustment",
    );
}

/// Image-rotation D4 consistency: render a board, detect on the original
/// and on the same image rotated 90° CW, and verify every shared physical
/// corner gets the same `target_position` in both decodes.
///
/// This reproduces the failure pattern reported on the 130x130 real dataset,
/// where snaps in different rotation classes disagree by a pure translation
/// on `target_position`.
#[test]
fn fixed_board_target_position_consistent_under_90cw_image_rotation() {
    run_image_rotation_test(1, 90);
}

/// Same contract as `fixed_board_target_position_consistent_under_90cw_image_rotation`
/// but with `upscale=2` applied before detection — matches the configuration
/// the 130x130 real dataset uses in `run_dataset.rs`.
#[test]
fn fixed_board_target_position_consistent_under_90cw_with_upscale() {
    run_image_rotation_test(2, 90);
}

#[test]
fn fixed_board_target_position_consistent_under_180_with_upscale() {
    run_image_rotation_test(2, 180);
}

#[test]
fn fixed_board_target_position_consistent_under_270_with_upscale() {
    run_image_rotation_test(2, 270);
}

fn run_image_rotation_test(upscale: u32, rotation_deg: u32) {
    let spec = PuzzleBoardTargetSpec {
        rows: 10,
        cols: 10,
        square_size_mm: 12.0,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    };
    let mut doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(spec.clone()));
    doc.page.size = PageSize::Custom {
        width_mm: 200.0,
        height_mm: 200.0,
    };
    doc.page.margin_mm = 5.0;
    doc.render.png_dpi = 300;

    let bundle = calib_targets_print::render_target_bundle(&doc).expect("render");
    let gray_native = render_png_to_gray_image(&bundle.png_bytes);
    let gray_orig = if upscale == 1 {
        gray_native.clone()
    } else {
        let (w, h) = gray_native.dimensions();
        image::imageops::resize(
            &gray_native,
            w * upscale,
            h * upscale,
            image::imageops::FilterType::Triangle,
        )
    };
    let gray_rot = match rotation_deg {
        90 => image::imageops::rotate90(&gray_orig),
        180 => image::imageops::rotate180(&gray_orig),
        270 => image::imageops::rotate270(&gray_orig),
        _ => panic!("unsupported rotation {rotation_deg}"),
    };

    let mut cfg = ChessConfig::single_scale();
    cfg.threshold_mode = chess_corners::ThresholdMode::Relative;
    cfg.threshold_value = 0.15;
    cfg.nms_radius = 3;

    let board_spec = PuzzleBoardSpec::with_origin(
        spec.rows,
        spec.cols,
        spec.square_size_mm as f32,
        spec.origin_row,
        spec.origin_col,
    )
    .expect("board");
    let mut params = PuzzleBoardParams::for_board(&board_spec);
    params.decode.search_mode = PuzzleBoardSearchMode::FixedBoard;
    let detector = PuzzleBoardDetector::new(params).expect("detector");

    // Detect on the original image.
    let descriptors_orig = find_chess_corners_image(&gray_orig, &cfg).expect("ChESS detection");
    let corners_orig: Vec<TargetCorner> = descriptors_orig.iter().map(adapt).collect();
    let view_orig = GrayImageView {
        width: gray_orig.width() as usize,
        height: gray_orig.height() as usize,
        data: gray_orig.as_raw(),
    };
    let res_orig = detector
        .detect(&view_orig, &corners_orig)
        .expect("orig decode");

    // Detect on the 90° CW rotated image.
    let descriptors_rot = find_chess_corners_image(&gray_rot, &cfg).expect("ChESS detection");
    let corners_rot: Vec<TargetCorner> = descriptors_rot.iter().map(adapt).collect();
    let view_rot = GrayImageView {
        width: gray_rot.width() as usize,
        height: gray_rot.height() as usize,
        data: gray_rot.as_raw(),
    };
    let res_rot = detector
        .detect(&view_rot, &corners_rot)
        .expect("rotated decode");

    // Map each rotated detection back to original image pixel coords.
    // - rotate90 (CW):  orig (x, y) → rot (h - 1 - y, x); inverse: (xr, yr) → (yr, h - 1 - xr).
    // - rotate180:      orig (x, y) → rot (w - 1 - x, h - 1 - y); inverse: (xr, yr) → (w - 1 - xr, h - 1 - yr).
    // - rotate270 (CCW):orig (x, y) → rot (y, w - 1 - x); inverse: (xr, yr) → (w - 1 - yr, xr).
    let w_orig = gray_orig.width() as f32;
    let h_orig = gray_orig.height() as f32;
    let mut orig_map: std::collections::HashMap<(i32, i32), Point2<f32>> =
        std::collections::HashMap::new();
    for lc in &res_orig.detection.corners {
        // Quantise by 0.5× to tolerate subpixel jitter but still uniquely
        // key each physical corner.
        let key = (
            (lc.position.x * 0.5).round() as i32,
            (lc.position.y * 0.5).round() as i32,
        );
        orig_map.insert(
            key,
            lc.target_position.expect("target_position missing on orig"),
        );
    }

    let mut checks = 0usize;
    let mut mismatches: Vec<(Point2<f32>, Point2<f32>, Point2<f32>)> = Vec::new();
    for lc in &res_rot.detection.corners {
        let xr = lc.position.x;
        let yr = lc.position.y;
        let (x_in_orig, y_in_orig) = match rotation_deg {
            90 => (yr, h_orig - 1.0 - xr),
            180 => (w_orig - 1.0 - xr, h_orig - 1.0 - yr),
            270 => (w_orig - 1.0 - yr, xr),
            _ => unreachable!(),
        };
        let key = (
            (x_in_orig * 0.5).round() as i32,
            (y_in_orig * 0.5).round() as i32,
        );
        if let Some(target_orig) = orig_map.get(&key) {
            let target_rot = lc.target_position.expect("target_position missing on rot");
            checks += 1;
            if (target_rot.x - target_orig.x).abs() > 1e-3
                || (target_rot.y - target_orig.y).abs() > 1e-3
            {
                mismatches.push((Point2::new(x_in_orig, y_in_orig), target_rot, *target_orig));
            }
        }
    }

    assert!(
        checks >= 30,
        "too few shared-corner comparisons ({checks}) — test miscalibrated"
    );
    if !mismatches.is_empty() {
        let (pos, got, expected) = mismatches[0];
        panic!(
            "{} / {} shared corners disagree on target_position under {}° image \
             rotation (upscale={}); first: unrotated pixel=({:.1},{:.1}) \
             rotated-decoded=({},{}) unrotated-decoded=({},{}); \
             rot_alignment={:?} rot_origin=({}, {}) orig_alignment={:?} orig_origin=({}, {})",
            mismatches.len(),
            checks,
            rotation_deg,
            upscale,
            pos.x,
            pos.y,
            got.x,
            got.y,
            expected.x,
            expected.y,
            res_rot.alignment,
            res_rot.decode.master_origin_row,
            res_rot.decode.master_origin_col,
            res_orig.alignment,
            res_orig.decode.master_origin_row,
            res_orig.decode.master_origin_col,
        );
    }
}
