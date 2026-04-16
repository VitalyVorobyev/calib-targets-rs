use calib_targets::detect;
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use image::ImageReader;

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing(false);

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_puzzleboard <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();

    // Default 12×12 PuzzleBoard anchored at master origin (0, 0).
    let spec = PuzzleBoardSpec::new(12, 12, 1.0)?;
    let params = PuzzleBoardParams::for_board(&spec);
    let result = detect::detect_puzzleboard(&img, &params)?;

    println!(
        "detected {} labelled corners (mean confidence = {:.3}, bit-error rate = {:.3})",
        result.detection.corners.len(),
        result.decode.mean_confidence,
        result.decode.bit_error_rate,
    );
    println!(
        "master origin for local (0, 0): ({}, {})",
        result.decode.master_origin_row, result.decode.master_origin_col,
    );

    Ok(())
}
