use calib_targets::detect;
use calib_targets::marker::{
    CellCoords, CirclePolarity, MarkerBoardLayout, MarkerBoardParams, MarkerCircleSpec,
};
use image::ImageReader;

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing(false);

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_markerboard <image_path>");
        return Ok(());
    };

    let img = ImageReader::open(path)?.decode()?.to_luma8();

    let layout = MarkerBoardLayout {
        rows: 22,
        cols: 22,
        cell_size: Some(1.0),
        circles: [
            MarkerCircleSpec {
                cell: CellCoords { i: 11, j: 11 },
                polarity: CirclePolarity::White,
            },
            MarkerCircleSpec {
                cell: CellCoords { i: 12, j: 11 },
                polarity: CirclePolarity::Black,
            },
            MarkerCircleSpec {
                cell: CellCoords { i: 12, j: 12 },
                polarity: CirclePolarity::White,
            },
        ],
    };

    let params = MarkerBoardParams::new(layout);
    let result = detect::detect_marker_board_default(&img, params);
    println!("detected: {}", result.is_some());

    Ok(())
}
