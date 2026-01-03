use calib_targets::aruco::builtins;
use calib_targets::charuco::{CharucoBoardSpec, MarkerLayout};
use calib_targets::detect;
use image::ImageReader;

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing(false);

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_charuco <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();

    let board = CharucoBoardSpec {
        rows: 22,
        cols: 22,
        cell_size: 1.0,
        marker_size_rel: 0.75,
        dictionary: builtins::DICT_4X4_1000,
        marker_layout: MarkerLayout::OpenCvCharuco,
    };

    let params = calib_targets::charuco::CharucoDetectorParams::for_board(&board);
    let result = detect::detect_charuco_default(&img, params)?;
    println!(
        "detected {} charuco corners",
        result.detection.corners.len()
    );

    Ok(())
}
