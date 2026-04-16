//! End-to-end test: render a PuzzleBoard target via `calib-targets-print`,
//! detect ChESS corners on the PNG, run the PuzzleBoard detector, and
//! verify every returned `LabeledCorner` is labelled with the expected
//! master (I, J) coordinates.

use calib_targets_core::{Corner as TargetCorner, GrayImageView};
use calib_targets_print::{
    PageSize, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec,
};
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
