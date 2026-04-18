//! Sweep PuzzleBoard detection across multiple board sizes and print a
//! per-stage diagnostic table.
//!
//! The main purpose is to surface *where* a detection fails — whether it's
//! the ChESS corner stage, the chessboard grid assembly, or the PuzzleBoard
//! decode — and to show per-stage timings. This is the human-readable
//! companion to `benches/puzzleboard_sizes.rs` and is how the 13 × 13 failure
//! mode gets diagnosed.
//!
//! Run with:
//! ```text
//! cargo run --release -p calib-targets --example puzzleboard_size_sweep
//! ```

use calib_targets::chessboard::ChessboardDetector;
use calib_targets::detect::{detect_corners, gray_view};
use calib_targets::printable::{
    render_target_bundle, PageSize, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec,
};
use calib_targets::puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use image::GrayImage;
use std::time::{Duration, Instant};

const SIZES: &[u32] = &[6, 8, 10, 12, 13, 16, 20, 30];

fn render_puzzleboard_gray(rows: u32, cols: u32, px_per_cell: u32) -> GrayImage {
    let square_size_mm: f64 = 12.0;
    let margin_mm: f64 = 5.0;
    let width_mm = cols as f64 * square_size_mm + 2.0 * margin_mm;
    let height_mm = rows as f64 * square_size_mm + 2.0 * margin_mm;
    let png_dpi = ((px_per_cell as f64) * 25.4 / square_size_mm).round() as u32;

    let spec = PuzzleBoardTargetSpec {
        rows,
        cols,
        square_size_mm,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    };
    let mut doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(spec));
    doc.page.size = PageSize::Custom {
        width_mm,
        height_mm,
    };
    doc.page.margin_mm = margin_mm;
    doc.render.png_dpi = png_dpi;

    let bundle = render_target_bundle(&doc).expect("render");
    image::load_from_memory(&bundle.png_bytes)
        .expect("decode PNG")
        .to_luma8()
}

struct StageTimings {
    corners: Duration,
    chessboard: Duration,
    decode: Duration,
}

struct Row {
    size: u32,
    img_px: u32,
    corners_found: usize,
    chessboard_ok: bool,
    grids: usize,
    labeled_corners: Option<usize>,
    edges_observed: Option<usize>,
    bit_error_rate: Option<f32>,
    timings: StageTimings,
    verdict: &'static str,
}

fn sweep_one(n: u32) -> Row {
    let params = {
        let spec = PuzzleBoardSpec::with_origin(n, n, 12.0, 0, 0).expect("spec");
        PuzzleBoardParams::for_board(&spec)
    };
    let img = render_puzzleboard_gray(n, n, 40);
    let img_px = img.width();

    // Stage 1 — ChESS corners.
    let t0 = Instant::now();
    let corners = detect_corners(&img, &params.chessboard.chess);
    let d_corners = t0.elapsed();

    // Stage 2 — chessboard grid assembly.
    let t1 = Instant::now();
    let chessboard = ChessboardDetector::new(params.chessboard.clone());
    let grids = chessboard.detect_all_from_corners(&corners);
    let d_chessboard = t1.elapsed();

    if grids.is_empty() {
        return Row {
            size: n,
            img_px,
            corners_found: corners.len(),
            chessboard_ok: false,
            grids: 0,
            labeled_corners: None,
            edges_observed: None,
            bit_error_rate: None,
            timings: StageTimings {
                corners: d_corners,
                chessboard: d_chessboard,
                decode: Duration::ZERO,
            },
            verdict: "chessboard FAIL",
        };
    }

    // Stage 3 — puzzleboard decode (edge sampling + decode + labels fused).
    let t2 = Instant::now();
    let detector = PuzzleBoardDetector::new(params).expect("detector");
    let result = detector.detect(&gray_view(&img), &corners);
    let d_decode = t2.elapsed();

    match result {
        Ok(r) => Row {
            size: n,
            img_px,
            corners_found: corners.len(),
            chessboard_ok: true,
            grids: grids.len(),
            labeled_corners: Some(r.detection.corners.len()),
            edges_observed: Some(r.decode.edges_observed),
            bit_error_rate: Some(r.decode.bit_error_rate),
            timings: StageTimings {
                corners: d_corners,
                chessboard: d_chessboard,
                decode: d_decode,
            },
            verdict: "ok",
        },
        Err(e) => {
            eprintln!("[{n}x{n}] decode error: {e}");
            Row {
                size: n,
                img_px,
                corners_found: corners.len(),
                chessboard_ok: true,
                grids: grids.len(),
                labeled_corners: None,
                edges_observed: None,
                bit_error_rate: None,
                timings: StageTimings {
                    corners: d_corners,
                    chessboard: d_chessboard,
                    decode: d_decode,
                },
                verdict: "decode FAIL",
            }
        }
    }
}

fn fmt_ms(d: Duration) -> String {
    format!("{:>7.1}", d.as_secs_f64() * 1000.0)
}

fn main() {
    println!(
        "| size | img_px |  corners | cb_ok | grids | labelled |  edges |    ber |   t_crn |    t_cb |   t_dec |   total | verdict |"
    );
    println!(
        "|------|--------|----------|-------|-------|----------|--------|--------|---------|---------|---------|---------|---------|"
    );
    for &n in SIZES {
        let row = sweep_one(n);
        let total = row.timings.corners + row.timings.chessboard + row.timings.decode;
        println!(
            "| {:>4} | {:>6} | {:>8} | {:>5} | {:>5} | {} | {} | {} | {} | {} | {} | {} | {} |",
            row.size,
            row.img_px,
            row.corners_found,
            if row.chessboard_ok { "yes" } else { "no" },
            row.grids,
            row.labeled_corners
                .map(|v| format!("{v:>8}"))
                .unwrap_or_else(|| format!("{:>8}", "—")),
            row.edges_observed
                .map(|v| format!("{v:>6}"))
                .unwrap_or_else(|| format!("{:>6}", "—")),
            row.bit_error_rate
                .map(|v| format!("{v:>6.3}"))
                .unwrap_or_else(|| format!("{:>6}", "—")),
            fmt_ms(row.timings.corners),
            fmt_ms(row.timings.chessboard),
            fmt_ms(row.timings.decode),
            fmt_ms(total),
            row.verdict,
        );
    }
}
