use calib_targets::detect;
use calib_targets::marker::{
    CellCoords, CirclePolarity, MarkerBoardParams, MarkerBoardSpec, MarkerCircleSpec,
};
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing_subscriber();

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_markerboard <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();

    let board = MarkerBoardSpec::new(
        22,
        22,
        [
            MarkerCircleSpec::new(CellCoords { i: 11, j: 11 }, CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords { i: 12, j: 11 }, CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords { i: 12, j: 12 }, CirclePolarity::White),
        ],
    )
    .with_cell_size(1.0);

    let params = MarkerBoardParams::for_board(board);
    let result = detect::detect_marker_board(&img, &params);
    println!("detected: {}", result.is_ok());

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
