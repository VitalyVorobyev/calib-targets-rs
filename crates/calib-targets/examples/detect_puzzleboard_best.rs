use calib_targets::detect;
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use image::ImageReader;

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing(false);

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_puzzleboard_best <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();

    let spec = PuzzleBoardSpec::new(12, 12, 1.0)?;
    let configs = PuzzleBoardParams::sweep_for_board(&spec);
    let result = detect::detect_puzzleboard_best(&img, &configs)?;

    println!(
        "best of {} configs: {} corners, mean-confidence={:.3}",
        configs.len(),
        result.detection.corners.len(),
        result.decode.mean_confidence,
    );

    Ok(())
}
