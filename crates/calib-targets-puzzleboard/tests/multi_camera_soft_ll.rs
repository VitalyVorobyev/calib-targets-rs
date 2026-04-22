//! Cross-camera regression for PuzzleBoard multi-view consistency.
//!
//! Renders a single large board, extracts six overlapping image crops that
//! simulate six cameras observing the same physical target from different
//! viewpoints, and asserts every successful per-camera decode yields the
//! same `(D4, master origin)` alignment. Any corner observed by two or more
//! "cameras" must receive byte-identical `target_position` across them —
//! the central contract that regressed on real data before the D4-aware
//! decoder fix.
//!
//! The same 6-view pass also runs under `HardWeighted` as a parity check, so
//! future scoring-mode changes do not silently reintroduce a class-dependent
//! coordinate split.

use std::collections::HashMap;

use calib_targets_core::{Corner as TargetCorner, GrayImageView};

type PerViewOrigin = Option<(i32, i32)>;
type TargetPositionMap = HashMap<(i32, i32), (f32, f32)>;
type SixViewResult = (Vec<PerViewOrigin>, Vec<TargetPositionMap>);
use calib_targets_print::{PageSize, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec};
use calib_targets_puzzleboard::{
    PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardScoringMode, PuzzleBoardSearchMode,
    PuzzleBoardSpec,
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

/// Run the 6-view consistency pass under the given scoring mode and return
/// `(per_view_alignment, per_view_target_positions)`. `None` entries mean
/// the view failed to decode under that mode.
fn run_six_views(mode: PuzzleBoardScoringMode) -> SixViewResult {
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
    let descriptors = find_chess_corners_image(&gray, &cfg);
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
    params.decode.scoring_mode = mode;
    let detector = PuzzleBoardDetector::new(params).expect("detector");

    // Six overlapping "camera" subsets of the corner cloud. Chosen so every
    // pair shares at least one rectangular strip — the multi-view contract
    // is only meaningful on shared corners.
    let w = gray.width() as f32;
    let h = gray.height() as f32;
    let view_boxes = [
        // upper-left three-quarters
        (0.00 * w, 0.00 * h, 0.75 * w, 0.75 * h),
        // upper-right three-quarters
        (0.25 * w, 0.00 * h, 1.00 * w, 0.75 * h),
        // lower-left three-quarters
        (0.00 * w, 0.25 * h, 0.75 * w, 1.00 * h),
        // lower-right three-quarters
        (0.25 * w, 0.25 * h, 1.00 * w, 1.00 * h),
        // horizontal middle band
        (0.00 * w, 0.25 * h, 1.00 * w, 0.75 * h),
        // vertical middle band
        (0.25 * w, 0.00 * h, 0.75 * w, 1.00 * h),
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

    let mut alignments: Vec<PerViewOrigin> = Vec::new();
    let mut per_view_positions: Vec<TargetPositionMap> = Vec::new();
    for (i, subset) in subsets.iter().enumerate() {
        assert!(
            subset.len() >= 12,
            "view {i} has too few corners ({}) — harness miscalibrated",
            subset.len()
        );
        match detector.detect(&view, subset) {
            Ok(res) => {
                alignments.push(Some((
                    res.decode.master_origin_row,
                    res.decode.master_origin_col,
                )));
                let mut m = HashMap::new();
                for lc in &res.detection.corners {
                    if let Some(tp) = lc.target_position {
                        // Quantise pixel position to a stable key so two
                        // views observing the same subpixel corner still
                        // collide.
                        let key = (
                            (lc.position.x * 0.5).round() as i32,
                            (lc.position.y * 0.5).round() as i32,
                        );
                        m.insert(key, (tp.x, tp.y));
                    }
                }
                per_view_positions.push(m);
            }
            Err(_) => {
                alignments.push(None);
                per_view_positions.push(HashMap::new());
            }
        }
    }
    (alignments, per_view_positions)
}

/// Count how many (ordered) view pairs disagree on `target_position` for
/// any shared physical corner. Returns `(disagreements, overlap_checks)`.
fn count_target_position_disagreements(positions: &[TargetPositionMap]) -> (usize, usize) {
    let mut overlap_checks = 0usize;
    let mut disagreements = 0usize;
    for i in 0..positions.len() {
        for j in (i + 1)..positions.len() {
            for (key, pi) in &positions[i] {
                if let Some(pj) = positions[j].get(key) {
                    overlap_checks += 1;
                    if (pi.0 - pj.0).abs() > 1e-3 || (pi.1 - pj.1).abs() > 1e-3 {
                        disagreements += 1;
                    }
                }
            }
        }
    }
    (disagreements, overlap_checks)
}

#[test]
fn soft_ll_six_views_agree_on_target_positions_for_shared_corners() {
    // Central multi-camera contract: a corner observed in two or more
    // views must receive byte-identical `target_position` across them.
    // Each view has its own chessboard-local origin (so `master_origin`
    // legitimately varies per view), but the master-frame `target_position`
    // of any shared corner is shift-invariant.
    let (alignments, positions) = run_six_views(PuzzleBoardScoringMode::SoftLogLikelihood);

    eprintln!("soft-ll per-view master origins:");
    for (i, a) in alignments.iter().enumerate() {
        eprintln!("  view {i}: {a:?}");
    }

    for (i, a) in alignments.iter().enumerate() {
        assert!(a.is_some(), "soft: view {i} failed to decode");
    }

    let (disagreements, overlap_checks) = count_target_position_disagreements(&positions);
    eprintln!(
        "soft-ll: {} disagreements across {} overlapping-corner comparisons",
        disagreements, overlap_checks
    );
    assert!(
        overlap_checks >= 50,
        "need at least 50 overlap comparisons, got {overlap_checks}"
    );
    assert_eq!(
        disagreements, 0,
        "soft-ll target_position inconsistency across views — see stderr"
    );
}

#[test]
fn hard_vs_soft_visibility_diagnostic_on_six_views() {
    // Diagnostic regression: both scoring modes should now preserve
    // target_position consistency on this synthetic six-view harness.
    // Keeping the comparison explicit makes future score-shape changes
    // visible in stderr even when they do not change the winning decode.
    let (_, hard_positions) = run_six_views(PuzzleBoardScoringMode::HardWeighted);
    let (_, soft_positions) = run_six_views(PuzzleBoardScoringMode::SoftLogLikelihood);
    let (hard_disagree, hard_checks) = count_target_position_disagreements(&hard_positions);
    let (soft_disagree, soft_checks) = count_target_position_disagreements(&soft_positions);
    eprintln!("hard: {hard_disagree}/{hard_checks} target_position disagreements");
    eprintln!("soft: {soft_disagree}/{soft_checks} target_position disagreements");
    assert_eq!(hard_disagree, 0, "hard-weighted must stay consistent here");
    assert_eq!(soft_disagree, 0, "soft-ll must stay consistent");
}
