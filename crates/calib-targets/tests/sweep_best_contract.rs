//! Contract tests for the shared `detect_*_best` sweep engine.

#![cfg(feature = "image")]

use std::path::PathBuf;

use calib_targets::aruco::builtins;
use calib_targets::charuco::{CharucoBoardSpec, CharucoParams, MarkerLayout};
use calib_targets::chessboard::ChessboardParams;
use calib_targets::detect::{self, default_chess_config, gray_view, DetectError};
use calib_targets::marker::{
    CellCoords, CirclePolarity, MarkerBoardParams, MarkerBoardSpec, MarkerCircleSpec,
};
use calib_targets::puzzleboard::{PuzzlePoleParams, PuzzlePoleSpec};

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

fn charuco_board() -> CharucoBoardSpec {
    CharucoBoardSpec::new(22, 22, 5.2, 0.75, builtins::DICT_4X4_250)
        .with_marker_layout(MarkerLayout::OpenCvCharuco)
}

fn marker_board() -> MarkerBoardSpec {
    MarkerBoardSpec::new(
        22,
        22,
        [
            MarkerCircleSpec::new(CellCoords::new(11, 11), CirclePolarity::White),
            MarkerCircleSpec::new(CellCoords::new(12, 11), CirclePolarity::Black),
            MarkerCircleSpec::new(CellCoords::new(12, 12), CirclePolarity::White),
        ],
    )
    .with_cell_size(1.0)
}

#[test]
fn empty_sweep_is_misuse_for_every_detector() {
    let px = vec![128u8; 64 * 64];
    let view = detect::gray_view_from_slice(64, 64, &px).expect("view");
    let chess = default_chess_config();

    let errors: Vec<DetectError> = vec![
        detect::detect_chessboard_best(&view, &chess, &[]).unwrap_err(),
        detect::detect_charuco_best(&view, &[]).unwrap_err(),
        detect::detect_puzzleboard_best(&view, &[]).unwrap_err(),
        detect::detect_puzzlepole_best(&view, &[]).unwrap_err(),
        detect::detect_marker_board_best(&view, &[]).unwrap_err(),
    ];
    for err in errors {
        assert!(!err.is_not_found(), "{err} should be a misuse");
        assert!(matches!(err, DetectError::EmptySweep { .. }), "{err}");
    }
}

#[test]
fn invalid_config_after_miss_propagates_misuse_immediately() {
    let px = vec![128u8; 128 * 128];
    let view = detect::gray_view_from_slice(128, 128, &px).expect("view");

    // ChArUco: invalid second config (`border_bits == 0`).
    let ok_charuco = CharucoParams::for_board(charuco_board());
    let bad_charuco = CharucoParams::for_board(charuco_board().with_border_bits(0));
    let err = detect::detect_charuco_best(&view, &[ok_charuco, bad_charuco]).unwrap_err();
    assert!(!err.is_not_found(), "{err}");

    // MarkerBoard: invalid second config (`rows == 1`).
    let ok_marker = MarkerBoardParams::for_board(marker_board());
    let mut bad_marker = MarkerBoardParams::for_board(marker_board());
    bad_marker.board.rows = 1;
    let err = detect::detect_marker_board_best(&view, &[ok_marker, bad_marker]).unwrap_err();
    assert!(!err.is_not_found(), "{err}");
}

#[test]
fn all_miss_sweep_returns_last_miss() {
    let px = vec![128u8; 128 * 128];
    let view = detect::gray_view_from_slice(128, 128, &px).expect("view");

    let err = detect::detect_chessboard_best(
        &view,
        &default_chess_config(),
        &[ChessboardParams::default()],
    )
    .unwrap_err();
    assert!(err.is_not_found());

    let p = PuzzlePoleSpec::new(24, 12, 12.0).expect("pole");
    let err = detect::detect_puzzlepole_best(&view, &[PuzzlePoleParams::for_pole(p)]).unwrap_err();
    assert!(err.is_not_found());
}

#[test]
fn ties_resolve_first_wins_and_strictly_better_wins_on_marker_and_chessboard() {
    let img = load_gray("markerboard.png");
    let view = gray_view(&img);

    let chess_cfg = default_chess_config();
    let mut loose = ChessboardParams::default();
    loose.min_corner_strength = 0.0;
    let mut strict = ChessboardParams::default();
    strict.min_corner_strength = 500.0;
    // Strict misses or finds fewer; loose finds more even when listed second.
    let single_loose = detect::detect_chessboard(&view, &chess_cfg, &loose).expect("loose");
    let swept = detect::detect_chessboard_best(&view, &chess_cfg, &[strict, loose]).expect("best");
    assert_eq!(swept.corners.len(), single_loose.corners.len());

    // A tie on corner count returns a detection of that count (the
    // first-of-tied rule itself is pinned by the unit tests in
    // `detect/sweep.rs`).
    let mut first = MarkerBoardParams::for_board(marker_board());
    first.match_params.max_candidates_per_polarity = 6;
    let mut second = MarkerBoardParams::for_board(marker_board());
    second.match_params.max_candidates_per_polarity = 7;
    let det_first = detect::detect_marker_board(&view, &first).expect("first");
    let det_second = detect::detect_marker_board(&view, &second).expect("second");
    assert_eq!(det_first.corners.len(), det_second.corners.len());
    let swept_marker = detect::detect_marker_board_best(&view, &[first, second]).expect("sweep");
    assert_eq!(swept_marker.corners.len(), det_first.corners.len());
}
