//! Phase B contract dry-run for `projective-grid`.
//!
//! Detects a synthetic PuzzleBoard with the existing pipeline, converts its
//! `(position, grid)` corner output into the new crate's evidence shape
//! (point features plus caller-supplied coordinate hypotheses), and feeds
//! the result through `check_consistency`. The goal is to exercise the new
//! evidence/result contract with a real consumer's data shape before any
//! detection algorithm is ported onto `detect_grid`. The puzzleboard
//! library code itself is not modified — this is a test-only integration.

use calib_targets_chessboard::ChessCorner as TargetCorner;
use calib_targets_core::{grid_coords_to_next, GrayImageView};
use calib_targets_print::{PageSize, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec};
use calib_targets_puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use chess_corners::{CornerDescriptor, Detector as ChessDetector, DetectorConfig, Threshold};
use image::{ImageBuffer, Luma};
use nalgebra::Point2;
use projective_grid::{
    check_consistency, ConsistencyParams, ConsistencyRequest, CoordinateHypothesis, LatticeKind,
    PointFeature,
};

fn adapt(c: &CornerDescriptor) -> TargetCorner {
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

fn render_png_to_gray_image(bundle_bytes: &[u8]) -> ImageBuffer<Luma<u8>, Vec<u8>> {
    let img = image::load_from_memory(bundle_bytes).expect("decode PNG");
    img.to_luma8()
}

/// Puzzleboard detector output converts cleanly into the new crate's
/// evidence types, and `check_consistency` accepts the resulting
/// hypothesis set under a square lattice without rejecting anything.
#[test]
fn puzzleboard_corners_pass_check_consistency_square_lattice() {
    // 1) Render a synthetic puzzleboard. The board must be large enough that the
    //    detector's decodable interior window clears the bounded-distance floor
    //    (`min_window = 7` → 84 interior edges; Gap 19): the ChESS detector drops
    //    the outermost corner ring, so a board of `w×w` squares yields roughly a
    //    `(w-2)×(w-2)` decodable corner grid. A 10×10 board leaves an ~8×8
    //    interior, comfortably above the 7×7 floor, while still being small
    //    enough to exercise the conversion + fit shape.
    let spec = PuzzleBoardTargetSpec::new(10, 10, 12.0);
    let mut doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(spec.clone()));
    doc.page.size = PageSize::Custom {
        width_mm: 200.0,
        height_mm: 200.0,
    };
    doc.page.margin_mm = 5.0;
    doc.render.png_dpi = 300;

    let bundle = calib_targets_print::render_target_bundle(&doc).expect("render");
    let gray = render_png_to_gray_image(&bundle.png_bytes);

    // 2) Detect ChESS corners and feed into the puzzleboard detector.
    let cfg = DetectorConfig::chess()
        .with_threshold(Threshold::Relative(0.15))
        .with_chess(|c| c.nms_radius = 3);
    let mut chess_detector = ChessDetector::new(cfg).expect("build ChESS detector");
    let descriptors = chess_detector.detect(&gray).expect("ChESS detection");
    let corners: Vec<TargetCorner> = descriptors.iter().map(adapt).collect();

    let board_spec = PuzzleBoardSpec::with_origin(
        spec.rows,
        spec.cols,
        spec.square_size_mm as f32,
        spec.origin_row,
        spec.origin_col,
    )
    .expect("board");
    let params = PuzzleBoardParams::for_board(&board_spec);
    let detector = PuzzleBoardDetector::new(params).expect("detector");
    let view = GrayImageView {
        width: gray.width() as usize,
        height: gray.height() as usize,
        data: gray.as_raw(),
    };
    let result = detector
        .detect(&view, &corners)
        .expect("puzzleboard decode");

    // The contract dry-run needs at least four hypotheses (the consistency
    // checker rejects fewer with `InsufficientEvidence`); enforce a tighter
    // floor so a regression in the puzzleboard detector doesn't accidentally
    // make this test pass on a trivially small input.
    assert!(
        result.corners.len() >= 8,
        "puzzleboard returned only {} labelled corners on synthetic input — \
         dry-run input is too thin",
        result.corners.len()
    );

    // 3) Convert puzzleboard output into the new crate's evidence shape.
    //    Each corner's index in the vec is the natural `source_index`.
    let features: Vec<PointFeature> = result
        .corners
        .iter()
        .enumerate()
        .map(|(i, c)| PointFeature::new(i, c.position))
        .collect();
    let hypotheses: Vec<CoordinateHypothesis> = result
        .corners
        .iter()
        .enumerate()
        .map(|(i, c)| CoordinateHypothesis::new(i, grid_coords_to_next(c.grid), None))
        .collect();

    // 4) Run the new contract on this evidence. At 300 DPI with a 12 mm
    //    square the grid cell is ~142 px, so a genuine coordinate mislabel
    //    (off-by-one in i or j on a single corner) would manifest as a
    //    residual on the order of one cell. A 2.0 px gate is loose enough
    //    that ChESS sub-pixel jitter on the synthetic render never trips
    //    it but tight enough that any off-by-one would land in `rejected`.
    let params = ConsistencyParams::new(2.0_f32);
    let request =
        ConsistencyRequest::new(LatticeKind::Square, &features, &hypotheses, None, params);
    let report = check_consistency(request).expect("check_consistency");

    assert!(
        report.passed,
        "consistency check did not pass: max_residual_px={}, rejected_count={}",
        report
            .solution
            .fit
            .as_ref()
            .map(|f| f.residuals.max_px)
            .unwrap_or(f32::NAN),
        report.solution.rejected.len(),
    );
    assert!(
        report.solution.rejected.is_empty(),
        "expected no rejected hypotheses, got {} rejections",
        report.solution.rejected.len()
    );

    let fit = report
        .solution
        .fit
        .as_ref()
        .expect("consistency check should produce a fit on accepted input");
    // 1.5 px sits comfortably above the ~0.5-0.8 px sub-pixel jitter we
    // see from chess-corners on the 300 DPI synthetic render but well
    // below one puzzleboard cell (~142 px at 12 mm / 300 DPI), so any
    // genuine projective-fit divergence (off-by-one label or near-
    // singular homography) would clear this threshold.
    assert!(
        fit.residuals.max_px < 1.5_f32,
        "fit max_residual_px {} exceeds 1.5 px on synthetic input",
        fit.residuals.max_px
    );
    assert_eq!(fit.residuals.count, hypotheses.len());

    // Sanity: every accepted entry's source_index must round-trip back
    // to a corner in the puzzleboard result — the contract preserves
    // caller-owned indices through the fit.
    let n = result.corners.len();
    for entry in &report.solution.grid.entries {
        assert!(
            entry.source_index < n,
            "source_index {} out of range (n={})",
            entry.source_index,
            n
        );
    }
}
