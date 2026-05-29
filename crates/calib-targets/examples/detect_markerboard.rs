use calib_targets::detect;
use calib_targets::marker::{
    CellCoords, CirclePolarity, MarkerBoardParams, MarkerBoardSpec, MarkerCircleSpec,
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

    let layout = MarkerBoardSpec::new(
        22,
        22,
        [
            MarkerCircleSpec::new(CellCoords { i: 11, j: 11 }, CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords { i: 12, j: 11 }, CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords { i: 12, j: 12 }, CirclePolarity::White),
        ],
    )
    .with_cell_size(1.0);

    let params = MarkerBoardParams::new(layout);
    let result = detect::detect_marker_board(&img, &params);
    println!("detected: {}", result.is_some());

    Ok(())
}
