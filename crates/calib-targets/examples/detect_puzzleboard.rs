use calib_targets::detect;
use calib_targets::puzzleboard::{
    PuzzleBoardDetectConfig, PuzzleBoardDetectReport, PuzzleBoardParams, PuzzleBoardSpec,
};
use image::ImageReader;
use std::path::Path;

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing(false);

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_puzzleboard <image_path|config.json>");
        return Ok(());
    };

    if path.ends_with(".json") {
        return run_config(Path::new(&path));
    }

    // Default 12×12 PuzzleBoard anchored at master origin (0, 0).
    let spec = PuzzleBoardSpec::new(12, 12, 1.0)?;
    let params = PuzzleBoardParams::for_board(&spec);
    run_image(Path::new(&path), &params)?;

    Ok(())
}

fn run_config(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let cfg = PuzzleBoardDetectConfig::from_reader(file)?;
    let params = cfg.detector.clone();
    // The v2 chessboard detector no longer carries a nested ChESS config;
    // any `cfg.chess_config` override would have to be applied via the
    // standalone `detect::detect_corners` helper, not the params struct.
    let _ = cfg.chess_config;
    let result = run_image(&cfg.image_path, &params)?;
    if let Some(output_path) = cfg.output_path {
        let report = PuzzleBoardDetectReport {
            image_path: cfg.image_path,
            result,
        };
        let file = std::fs::File::create(output_path)?;
        serde_json::to_writer_pretty(file, &report)?;
    }

    Ok(())
}

fn run_image(
    path: &Path,
    params: &PuzzleBoardParams,
) -> Result<calib_targets::puzzleboard::PuzzleBoardDetectionResult, Box<dyn std::error::Error>> {
    let img = ImageReader::open(path)?.decode()?.to_luma8();
    let result = detect::detect_puzzleboard(&img, params)?;

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

    Ok(result)
}
