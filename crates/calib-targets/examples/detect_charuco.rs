use calib_targets::aruco::builtins;
use calib_targets::charuco::{CharucoBoardSpec, MarkerLayout};
use calib_targets::detect;
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_charuco <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();

    let board = CharucoBoardSpec {
        rows: 5,
        cols: 7,
        cell_size: 1.0,
        marker_size_rel: 0.7,
        dictionary: builtins::DICT_4X4_50,
        marker_layout: MarkerLayout::OpenCvCharuco,
    };

    let result = detect::detect_charuco_default(&img, board)?;
    println!(
        "detected {} charuco corners",
        result.detection.corners.len()
    );

    Ok(())
}
