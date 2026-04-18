//! End-to-end PuzzleBoard roundtrip:
//!
//! 1. **Synthesise** a PuzzleBoard target image in memory using
//!    `calib-targets-print` (no temp files).
//! 2. **Detect** the printed board from the PNG bytes with
//!    `calib_targets::detect::detect_puzzleboard`.
//! 3. **Verify** every returned corner carries an absolute master
//!    `(I, J)` label consistent with the alignment.
//!
//! Run with:
//!
//! ```text
//! cargo run --release -p calib-targets --example puzzleboard_roundtrip
//! ```
//!
//! Options via CLI flags (all optional):
//! `--rows <u32>`, `--cols <u32>`, `--dpi <u32>`, `--out <path.png>`.

use calib_targets::detect;
use calib_targets::printable::{
    render_target_bundle, PageSize, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec,
};
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use image::ImageReader;
use std::io::Cursor;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- CLI -----------------------------------------------------------
    let mut rows: u32 = 10;
    let mut cols: u32 = 10;
    let mut dpi: u32 = 300;
    let mut out_path: Option<String> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--rows" => rows = args.next().ok_or("missing --rows value")?.parse()?,
            "--cols" => cols = args.next().ok_or("missing --cols value")?.parse()?,
            "--dpi" => dpi = args.next().ok_or("missing --dpi value")?.parse()?,
            "--out" => out_path = Some(args.next().ok_or("missing --out value")?),
            "--help" | "-h" => {
                println!(
                    "Usage: puzzleboard_roundtrip [--rows N] [--cols N] [--dpi DPI] [--out path.png]"
                );
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }

    // --- 1. Synthesise --------------------------------------------------
    let target = PuzzleBoardTargetSpec {
        rows,
        cols,
        square_size_mm: 12.0,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    };
    let mut doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(target));
    doc.page.size = PageSize::Custom {
        width_mm: (cols as f64) * 12.0 + 20.0,
        height_mm: (rows as f64) * 12.0 + 20.0,
    };
    doc.page.margin_mm = 5.0;
    doc.render.png_dpi = dpi;

    let bundle = render_target_bundle(&doc)?;
    println!(
        "synthesised {}×{} PuzzleBoard at {} DPI ({} KB PNG)",
        rows,
        cols,
        dpi,
        bundle.png_bytes.len() / 1024,
    );

    if let Some(path) = &out_path {
        std::fs::write(path, &bundle.png_bytes)?;
        println!("wrote synthetic target to {path}");
    }

    // --- 2. Detect ------------------------------------------------------
    let img = ImageReader::new(Cursor::new(&bundle.png_bytes))
        .with_guessed_format()?
        .decode()?
        .to_luma8();

    let board = PuzzleBoardSpec::new(rows, cols, 12.0)?;
    let params = PuzzleBoardParams::for_board(&board);
    let result = detect::detect_puzzleboard(&img, &params)?;

    println!(
        "detected {} labelled corners (mean confidence = {:.3}, BER = {:.3})",
        result.detection.corners.len(),
        result.decode.mean_confidence,
        result.decode.bit_error_rate,
    );
    println!(
        "master origin for local (0, 0): ({}, {})",
        result.decode.master_origin_row, result.decode.master_origin_col,
    );

    // --- 3. Verify ------------------------------------------------------
    let inner = ((rows - 1) * (cols - 1)) as usize;
    let labelled = result.detection.corners.len();
    let coverage = labelled as f32 / inner as f32;
    println!(
        "coverage: {}/{} inner corners labelled ({:.1}%)",
        labelled,
        inner,
        coverage * 100.0,
    );

    let mut seen = std::collections::HashSet::new();
    for c in &result.detection.corners {
        assert!(c.id.is_some(), "missing id");
        let grid = c.grid.expect("missing grid");
        assert!(seen.insert((grid.i, grid.j)), "duplicate master coord");
    }
    println!("every labelled corner has a unique master (I, J) and ID");

    Ok(())
}
