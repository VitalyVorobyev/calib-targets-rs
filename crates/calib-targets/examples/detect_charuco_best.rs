use calib_targets::aruco::builtins;
use calib_targets::charuco::{CharucoBoardSpec, CharucoParams, MarkerLayout};
use calib_targets::detect;
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing_subscriber();

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_charuco_best <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();

    let board = CharucoBoardSpec::new(22, 22, 1.0, 0.75, builtins::DICT_4X4_1000)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);

    // Use the built-in three-config sweep: base, high-threshold, low-threshold.
    let configs = CharucoParams::sweep_for_board(&board);

    let result = detect::detect_charuco_best(&img, &configs)?;
    println!(
        "detected {} corners, {} markers",
        result.corners.len(),
        result.markers.len(),
    );

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
