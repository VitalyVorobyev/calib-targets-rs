use calib_targets::chessboard::ChessboardParams;
use calib_targets::detect;
use image::ImageReader;

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing(false);

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_chessboard_best <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();

    // Use the built-in three-config sweep: default, high-threshold, low-threshold.
    let configs = ChessboardParams::sweep_default();

    let result = detect::detect_chessboard_best(&img, &configs);
    match result {
        Some(found) => println!("detected {} corners", found.detection.corners.len()),
        None => println!("no board detected with any config"),
    }

    Ok(())
}
