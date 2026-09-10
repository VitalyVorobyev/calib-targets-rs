//! A PuzzlePole wrap strip, printed and photographed **flat**, must decode as
//! the PuzzleBoard sub-rectangle it is.
//!
//! Worth asserting before any cylindrical decode exists, because it separates
//! two failures that would otherwise look alike. If a pole later fails to
//! decode, this says whether the *printable* was ever a valid, decodable
//! pattern — and it is the strongest statement available about the generated
//! strip that depends on no pole-specific decode at all.
//!
//! It also pins the property the whole construction is for: the corner rows the
//! strip repeats at its two ends really are the same rows of the code, so the
//! decoder reads them as master rows exactly one period apart.

use calib_targets::detect::default_chess_config;
use calib_targets_chessboard::ChessCorner as TargetCorner;
use calib_targets_core::GrayImageView;
use calib_targets_print::{
    render_target_bundle, PageSize, PrintableTargetDocument, PuzzlePoleTargetSpec, TargetSpec,
};
use calib_targets_puzzleboard::{
    PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec, PuzzlePolePeriod, SUPPORTED_PERIODS,
};
use chess_corners::{CornerDescriptor, Detector as ChessDetector};
use nalgebra::Point2;

const PIECE_MM: f64 = 12.0;
const AXIAL: u32 = 8;

fn adapt(c: &CornerDescriptor) -> TargetCorner {
    TargetCorner::new(
        Point2::new(c.x, c.y),
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

/// Render one pole strip flat and decode it with the ordinary planar detector.
/// Returns the labelled corners as `(master_row, master_col)`.
fn decode_strip_flat(period: PuzzlePolePeriod) -> Vec<(i32, i32)> {
    let spec = PuzzlePoleTargetSpec::at_period(period, AXIAL, PIECE_MM);
    let strip_pieces = spec.printed_strip_squares();

    let target = TargetSpec::PuzzlePole(spec);
    let (w, h) = target.board_size_mm().expect("valid spec");
    let mut doc = PrintableTargetDocument::new(target);
    doc.page.size = PageSize::Custom {
        width_mm: w + 20.0,
        height_mm: h + 20.0,
    };
    doc.page.margin_mm = 10.0;
    doc.render.png_dpi = 300;

    let bundle = render_target_bundle(&doc).expect("render");
    let gray = image::load_from_memory(&bundle.png_bytes)
        .expect("decode PNG")
        .to_luma8();

    let cfg = default_chess_config().with_detection(|d| d.nms_radius = 3);
    let descriptors = ChessDetector::new(cfg)
        .expect("build ChESS detector")
        .detect(&gray)
        .expect("ChESS detection");

    // The strip *is* a master sub-rectangle, so the planar detector needs no
    // knowledge of poles to read it. That is the point of this test.
    let board = PuzzleBoardSpec::with_origin(
        strip_pieces,
        AXIAL,
        PIECE_MM as f32,
        period.start_row,
        0,
    )
    .expect("a wrap strip is a valid planar sub-board");
    let detector =
        PuzzleBoardDetector::new(PuzzleBoardParams::for_board(board)).expect("detector");

    let view = GrayImageView {
        width: gray.width() as usize,
        height: gray.height() as usize,
        data: gray.as_raw(),
    };
    let corners: Vec<TargetCorner> = descriptors.iter().map(adapt).collect();
    let found = detector
        .detect_with_corners(&view, &corners)
        .unwrap_or_else(|e| {
            panic!(
                "period {} at start row {} did not decode as a flat board: {e}",
                period.squares, period.start_row
            )
        });

    // `grid` is `(u, v)` = `(master col, master row)`; report it row-first to
    // match the code-map convention the rest of this crate speaks.
    found.corners.iter().map(|c| (c.grid.v, c.grid.u)).collect()
}

/// Every shipped period must produce a strip the planar detector can read. A
/// period that generates an undecodable printable is useless however well its
/// seam closes.
#[test]
fn every_supported_period_prints_a_decodable_strip() {
    for period in SUPPORTED_PERIODS {
        let labelled = decode_strip_flat(*period);
        let expected_inner = (period.squares + 1) as usize * (AXIAL - 1) as usize;
        assert!(
            labelled.len() >= expected_inner / 2,
            "period {} decoded only {} of ~{expected_inner} inner corners",
            period.squares,
            labelled.len()
        );
    }
}

/// The seam, read back through the decoder rather than asserted from the maps:
/// the strip's first and last labelled corner rows sit exactly one period apart
/// on the master, which is what makes them the same row once wrapped.
#[test]
fn the_strips_two_ends_decode_one_period_apart() {
    let period = PuzzlePolePeriod::canonical(12).expect("period 12 is supported");
    let labelled = decode_strip_flat(period);

    let rows: Vec<i32> = {
        let mut r: Vec<i32> = labelled.iter().map(|(row, _)| *row).collect();
        r.sort_unstable();
        r.dedup();
        r
    };
    let (first, last) = (rows[0], rows[rows.len() - 1]);
    assert!(
        last - first >= period.squares as i32,
        "the strip spans {} master rows, less than its own period {}",
        last - first,
        period.squares
    );

    // Rows `first` and `first + p` are the same row of the code. The decoder
    // labelled them as distinct master rows -- as it must, decoding a flat
    // board -- and it is the *wrap* that identifies them.
    assert!(
        rows.contains(&(first + period.squares as i32)),
        "the row one period on from {first} was not labelled; the strip does \
         not carry both ends of the seam"
    );
}
