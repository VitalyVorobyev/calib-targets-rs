use calib_targets::detect;
use calib_targets::marker::{
    CellCoords, CirclePolarity, MarkerBoardLayout, MarkerBoardParams, MarkerCircleSpec,
};
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_markerboard <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();

    let layout = MarkerBoardLayout {
        rows: 6,
        cols: 8,
        cell_size: None,
        circles: [
            MarkerCircleSpec {
                cell: CellCoords { i: 2, j: 2 },
                polarity: CirclePolarity::White,
            },
            MarkerCircleSpec {
                cell: CellCoords { i: 3, j: 2 },
                polarity: CirclePolarity::Black,
            },
            MarkerCircleSpec {
                cell: CellCoords { i: 2, j: 3 },
                polarity: CirclePolarity::White,
            },
        ],
    };

    let params = MarkerBoardParams::new(layout);
    let result = detect::detect_marker_board_default(&img, params);
    println!("detected: {}", result.is_some());

    Ok(())
}
