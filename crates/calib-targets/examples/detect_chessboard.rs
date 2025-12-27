use calib_targets::chessboard::{ChessboardParams, GridGraphParams};
use calib_targets::detect;
use image::ImageReader;

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing(false);

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_chessboard <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();
    let chess_cfg = detect::default_chess_config();
    let params = ChessboardParams::default();
    let graph = GridGraphParams::default();

    let result = detect::detect_chessboard(&img, &chess_cfg, params, graph);
    match result {
        Some(found) => println!("detected {} corners", found.detection.corners.len()),
        None => println!("no board detected"),
    }

    Ok(())
}
