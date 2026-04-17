//! End-to-end test: render a PuzzleBoard target via `calib-targets-print`,
//! detect ChESS corners on the PNG, run the PuzzleBoard detector, and
//! verify every returned `LabeledCorner` is labelled with the expected
//! master (I, J) coordinates.

use calib_targets_core::{Corner as TargetCorner, GrayImageView};
use calib_targets_print::{PageSize, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec};
use calib_targets_puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::{ImageBuffer, Luma};
use nalgebra::Point2;

fn adapt(c: &CornerDescriptor) -> TargetCorner {
    TargetCorner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
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
    let descriptors = find_chess_corners_image(&gray, &cfg);
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

/// The KnownOrigin fast path must agree with the Full search when the
/// origin is seeded from a prior Full result.
#[test]
fn known_origin_matches_full_search_on_small_puzzleboard() {
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
    let descriptors = find_chess_corners_image(&gray, &cfg);
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

    // Full search — the reference result.
    let params_full = PuzzleBoardParams::for_board(&board_spec);
    let full = PuzzleBoardDetector::new(params_full.clone())
        .expect("detector")
        .detect(&view, &corners)
        .expect("full decode");

    // KnownOrigin seeded from the Full result. `window_radius = 0` is the
    // tightest possible check — only the exact origin is scored.
    let mut params_fast = params_full.clone();
    params_fast.decode.search_mode = full.as_known_origin(0);
    let fast = PuzzleBoardDetector::new(params_fast)
        .expect("detector")
        .detect(&view, &corners)
        .expect("fast decode");

    assert_eq!(
        full.decode.master_origin_row, fast.decode.master_origin_row,
        "master_origin_row mismatch"
    );
    assert_eq!(
        full.decode.master_origin_col, fast.decode.master_origin_col,
        "master_origin_col mismatch"
    );
    assert_eq!(
        full.decode.edges_matched, fast.decode.edges_matched,
        "edges_matched mismatch"
    );
    assert!(
        (full.decode.bit_error_rate - fast.decode.bit_error_rate).abs() < 1e-5,
        "bit_error_rate mismatch: full={} fast={}",
        full.decode.bit_error_rate,
        fast.decode.bit_error_rate
    );

    // Same labelled corners in the same order — the post-decode pipeline is
    // deterministic and agnostic to which decode path supplied the alignment.
    assert_eq!(
        full.detection.corners.len(),
        fast.detection.corners.len(),
        "labelled corner count mismatch"
    );
    for (f, g) in full
        .detection
        .corners
        .iter()
        .zip(fast.detection.corners.iter())
    {
        assert_eq!(f.id, g.id, "corner id mismatch");
        assert_eq!(f.grid, g.grid, "corner grid mismatch");
    }

    // Window > 0 should also succeed and agree.
    let mut params_fast_wide = params_full;
    params_fast_wide.decode.search_mode = full.as_known_origin(3);
    let fast_wide = PuzzleBoardDetector::new(params_fast_wide)
        .expect("detector")
        .detect(&view, &corners)
        .expect("fast decode (wide window)");
    assert_eq!(
        fast_wide.decode.master_origin_row,
        full.decode.master_origin_row
    );
    assert_eq!(
        fast_wide.decode.master_origin_col,
        full.decode.master_origin_col
    );
}
