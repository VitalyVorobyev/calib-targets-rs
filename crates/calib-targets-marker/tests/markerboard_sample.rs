//! The public marker-board sample, through the detector the demo runs.
//!
//! `testdata/markerboard.png` is a 22 × 22-corner board seen whole. Its top
//! edge has a T-junction — where a black and a white outer square meet the
//! margin — that the chessboard stage labels as a grid corner one row past the
//! board's first inner row. That corner used to ship labelled `(8, -1)`
//! without an id: a label outside the board, and negative. The board's layout
//! fixes its extent, so the detector now delivers only inner corners.

use calib_targets_marker::{
    CellCoords, CirclePolarity, GrayImageView, MarkerBoardDetector, MarkerBoardParams,
    MarkerBoardSpec, MarkerCircleSpec,
};
use std::path::PathBuf;

fn testdata(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdata")
        .join(name)
}

/// The board the sample shows — the layout of
/// `testdata/marker_detect_config.json`, and the one the demo seeds.
fn sample_board() -> MarkerBoardSpec {
    MarkerBoardSpec::new(
        22,
        22,
        [
            MarkerCircleSpec::new(CellCoords { i: 11, j: 11 }, CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords { i: 12, j: 11 }, CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords { i: 12, j: 12 }, CirclePolarity::White),
        ],
    )
}

/// The detector parameters stored next to the sample.
fn config_params() -> MarkerBoardParams {
    let text = std::fs::read_to_string(testdata("marker_detect_config.json")).expect("config");
    let config: serde_json::Value = serde_json::from_str(&text).expect("config JSON");
    serde_json::from_value(config["marker"].clone()).expect("marker params")
}

#[test]
fn every_delivered_corner_is_an_inner_corner_of_the_board() {
    let image = image::open(testdata("markerboard.png"))
        .expect("sample image")
        .to_luma8();
    let view = GrayImageView {
        width: image.width() as usize,
        height: image.height() as usize,
        data: image.as_raw(),
    };
    for (name, params) in [
        (
            "default params",
            MarkerBoardParams::for_board(sample_board()),
        ),
        ("sample config", config_params()),
    ] {
        let (rows, cols) = (params.board.rows as i32, params.board.cols as i32);
        let detection = MarkerBoardDetector::new(params)
            .expect("valid params")
            .detect(&view)
            .unwrap_or_else(|e| panic!("{name}: the sample must detect: {e}"));
        for corner in &detection.corners {
            let g = corner.grid;
            assert!(
                (0..cols).contains(&g.u) && (0..rows).contains(&g.v),
                "{name}: corner at ({:.2}, {:.2}) labelled ({}, {}) is off the {cols} x {rows} board",
                corner.position.x,
                corner.position.y,
                g.u,
                g.v
            );
            assert_eq!(corner.id, Some((g.v * cols + g.u) as u32), "{name}");
        }
        // The board is fully in view: every inner corner, each exactly once.
        let mut labels: Vec<_> = detection
            .corners
            .iter()
            .map(|c| (c.grid.v, c.grid.u))
            .collect();
        labels.dedup();
        assert_eq!(labels.len(), (rows * cols) as usize, "{name}");
        assert_eq!(detection.corners.len(), (rows * cols) as usize, "{name}");
    }
}
