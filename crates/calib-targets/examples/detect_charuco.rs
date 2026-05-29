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

    let board = CharucoBoardSpec::new(22, 22, 1.0, 0.75, builtins::DICT_4X4_1000)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);

    let params = calib_targets::charuco::CharucoParams::for_board(&board);
    let result = detect::detect_charuco(&img, &params)?;
    println!("detected {} charuco corners", result.corners.len());

    Ok(())
}
