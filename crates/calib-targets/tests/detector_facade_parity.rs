//! Parity contract for the compound-detector entry points.
//!
//! Two identities define the ChArUco / PuzzleBoard / marker-board API, and
//! every other entry point is expressed in terms of them:
//!
//! 1. **Facade == detector.** `detect_t(img, &params)` is exactly
//!    `TDetector::new(params)?.detect(&gray_view(img))` — the facade helper
//!    adds an error type, not behaviour.
//! 2. **`detect` == `detect_with_corners` over its own corner pass.**
//!    `d.detect(&view)` is exactly
//!    `d.detect_with_corners(&view, &d.detect_corners(&view))` — the
//!    ergonomic entry point is the corner pass plus the corner-cloud entry
//!    point, nothing more.
//!
//! Outcomes are compared through `Debug`, which is total over every field of
//! the detection types (positions, grid labels, ids, target positions, scores,
//! decode summaries, alignments) and over the error variants, so the assertions
//! cannot pass on a partial match.

#![cfg(feature = "image")]

use calib_targets::aruco::builtins;
use calib_targets::charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
use calib_targets::detect::{self, gray_view};
use calib_targets::marker::{
    CellCoords, CirclePolarity, MarkerBoardDetector, MarkerBoardParams, MarkerBoardSpec,
    MarkerCircleSpec,
};
use calib_targets::puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use std::fmt::Debug;
use std::path::PathBuf;

fn load_gray(name: &str) -> image::GrayImage {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdata")
        .join(name);
    image::ImageReader::open(&path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()))
        .decode()
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
        .to_luma8()
}

/// Total textual rendering of a detection outcome, used to compare two paths.
/// `Ok` renders the whole detection; `Err` renders the error's `Display`, so
/// two paths that fail differently are not treated as equal.
fn outcome<T: Debug, E: std::fmt::Display>(r: &Result<T, E>) -> String {
    match r {
        Ok(v) => format!("Ok({v:?})"),
        Err(e) => format!("Err({e})"),
    }
}

fn charuco_params() -> CharucoParams {
    let board = CharucoBoardSpec::new(22, 22, 5.2, 0.75, builtins::DICT_4X4_250)
        .with_marker_layout(MarkerLayout::OpenCvCharuco);
    CharucoParams::for_board(board)
}

fn puzzleboard_params() -> PuzzleBoardParams {
    // testdata/puzzleboard_small.png is a 10x10 board at 12.0 mm cell size,
    // cut from the master at origin (0, 0).
    let board = PuzzleBoardSpec::with_origin(10, 10, 12.0, 0, 0).expect("board spec");
    PuzzleBoardParams::for_board(board)
}

fn marker_params() -> MarkerBoardParams {
    let board = MarkerBoardSpec::new(
        22,
        22,
        [
            MarkerCircleSpec::new(CellCoords { i: 11, j: 11 }, CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords { i: 12, j: 11 }, CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords { i: 12, j: 12 }, CirclePolarity::White),
        ],
    )
    .with_cell_size(1.0);
    MarkerBoardParams::for_board(board)
}

#[test]
fn charuco_facade_matches_detector() {
    let img = load_gray("small.png");
    let params = charuco_params();

    let via_facade = detect::detect_charuco(&img, &params);
    let detector = CharucoDetector::new(params).expect("detector");
    let via_detector = detector.detect(&gray_view(&img));

    assert!(
        via_detector.is_ok(),
        "small.png must detect, otherwise this parity check is vacuous: {}",
        outcome(&via_detector)
    );
    assert_eq!(
        outcome(&via_facade),
        outcome(&via_detector),
        "detect_charuco must equal CharucoDetector::detect"
    );
}

#[test]
fn charuco_detect_matches_detect_with_corners() {
    let img = load_gray("small.png");
    let detector = CharucoDetector::new(charuco_params()).expect("detector");
    let view = gray_view(&img);

    let via_detect = detector.detect(&view);
    let corners = detector.detect_corners(&view);
    let via_corners = detector.detect_with_corners(&view, &corners);

    assert!(
        !corners.is_empty(),
        "the corner front-end must produce corners on small.png"
    );
    assert!(
        via_detect.is_ok(),
        "small.png must detect, otherwise this parity check is vacuous: {}",
        outcome(&via_detect)
    );
    assert_eq!(
        outcome(&via_detect),
        outcome(&via_corners),
        "detect must equal detect_with_corners over detect_corners"
    );
}

#[test]
fn puzzleboard_facade_matches_detector() {
    let img = load_gray("puzzleboard_small.png");
    let params = puzzleboard_params();

    let via_facade = detect::detect_puzzleboard(&img, &params);
    let detector = PuzzleBoardDetector::new(params).expect("detector");
    let via_detector = detector.detect(&gray_view(&img));

    assert!(
        via_detector.is_ok(),
        "puzzleboard_small.png must decode, otherwise this parity check is vacuous: {}",
        outcome(&via_detector)
    );
    assert_eq!(
        outcome(&via_facade),
        outcome(&via_detector),
        "detect_puzzleboard must equal PuzzleBoardDetector::detect"
    );
}

#[test]
fn puzzleboard_detect_matches_detect_with_corners() {
    let img = load_gray("puzzleboard_small.png");
    let detector = PuzzleBoardDetector::new(puzzleboard_params()).expect("detector");
    let view = gray_view(&img);

    let via_detect = detector.detect(&view);
    let corners = detector.detect_corners(&view);
    let via_corners = detector.detect_with_corners(&view, &corners);

    assert!(
        !corners.is_empty(),
        "the corner front-end must produce corners on puzzleboard_small.png"
    );
    assert!(
        via_detect.is_ok(),
        "puzzleboard_small.png must decode, otherwise this parity check is vacuous: {}",
        outcome(&via_detect)
    );
    assert_eq!(
        outcome(&via_detect),
        outcome(&via_corners),
        "detect must equal detect_with_corners over detect_corners"
    );
}

#[test]
fn marker_board_facade_matches_detector() {
    let img = load_gray("markerboard.png");
    let params = marker_params();

    let via_facade = detect::detect_marker_board(&img, &params);
    let detector = MarkerBoardDetector::new(params).expect("detector");
    let via_detector = detector.detect(&gray_view(&img));

    assert!(
        via_detector.is_ok(),
        "markerboard.png must detect, otherwise this parity check is vacuous: {}",
        outcome(&via_detector)
    );
    assert_eq!(
        outcome(&via_facade),
        outcome(&via_detector),
        "detect_marker_board must equal MarkerBoardDetector::detect"
    );
}

#[test]
fn marker_board_detect_matches_detect_with_corners() {
    let img = load_gray("markerboard.png");
    let detector = MarkerBoardDetector::new(marker_params()).expect("detector");
    let view = gray_view(&img);

    let via_detect = detector.detect(&view);
    let corners = detector.detect_corners(&view);
    let via_corners = detector.detect_with_corners(&view, &corners);

    assert!(
        !corners.is_empty(),
        "the corner front-end must produce corners on markerboard.png"
    );
    assert!(
        via_detect.is_ok(),
        "markerboard.png must detect, otherwise this parity check is vacuous: {}",
        outcome(&via_detect)
    );
    assert_eq!(
        outcome(&via_detect),
        outcome(&via_corners),
        "detect must equal detect_with_corners over detect_corners"
    );
}
