use calib_targets::chessboard::DetectorParams;
use calib_targets::detect;
use chess_corners::Threshold;
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_chessboard_topological <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();
    let chess_cfg = detect::default_chess_config().with_threshold(Threshold::Absolute(100.0));
    let params = DetectorParams::topological();

    let Some(result) = detect::detect_chessboard(&img, &chess_cfg, &params) else {
        println!("no board detected");
        return Ok(());
    };

    println!("detected {} labelled corners", result.corners.len());
    println!("i\tj\tx\ty");
    for corner in &result.corners {
        println!(
            "{}\t{}\t{:.2}\t{:.2}",
            corner.grid.u, corner.grid.v, corner.position.x, corner.position.y
        );
    }

    Ok(())
}
