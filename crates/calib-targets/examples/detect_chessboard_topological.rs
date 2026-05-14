use calib_targets::chessboard::DetectorParams;
use calib_targets::detect::{self, Threshold};
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_chessboard_topological <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();
    let chess_cfg = detect::default_chess_config().with_threshold(Threshold::Absolute(100.0));
    let params = DetectorParams::topological();

    let Some(result) = detect::detect_chessboard_with_config(&img, &chess_cfg, &params, 0.0) else {
        println!("no board detected");
        return Ok(());
    };

    println!(
        "detected {} labelled corners; cell size {:.2}px",
        result.target.corners.len(),
        result.cell_size
    );
    println!("i\tj\tx\ty");
    for corner in &result.target.corners {
        if let Some(grid) = corner.grid {
            println!(
                "{}\t{}\t{:.2}\t{:.2}",
                grid.i, grid.j, corner.position.x, corner.position.y
            );
        }
    }

    Ok(())
}
