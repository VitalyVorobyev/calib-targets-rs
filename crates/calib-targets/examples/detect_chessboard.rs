use calib_targets::chessboard::ChessboardParams;
use calib_targets::detect;
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing_subscriber();

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_chessboard <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();
    let params = ChessboardParams::default();

    let result = detect::detect_chessboard(&img, &detect::default_chess_config(), &params);
    match result {
        Ok(found) => println!("detected {} corners", found.corners.len()),
        Err(err) => println!("no board detected: {err}"),
    }

    Ok(())
}

/// Install a minimal `tracing` subscriber reading `RUST_LOG` (default
/// `info`). Examples no longer depend on the removed
/// `calib_targets_core::init_tracing` helper.
#[cfg(feature = "tracing")]
fn init_tracing_subscriber() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
