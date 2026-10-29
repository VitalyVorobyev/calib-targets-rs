//! `DetectError::is_not_found` separates a miss (no identifiable board) from a
//! misuse (a bad image, board spec or parameters). The Python and WASM
//! bindings turn the first into `None` / `null` and raise on the second, so
//! the split is pinned here at its source.

use calib_targets::aruco::builtins::builtin_dictionary;
use calib_targets::charuco::{CharucoBoardSpec, CharucoParams};
use calib_targets::chessboard::ChessboardParams;
use calib_targets::detect::{self, DetectError};
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use image::GrayImage;

fn blank() -> GrayImage {
    GrayImage::from_pixel(320, 240, image::Luma([128]))
}

fn charuco_board() -> CharucoBoardSpec {
    let dict = builtin_dictionary("DICT_4X4_50").expect("builtin dictionary");
    CharucoBoardSpec::new(5, 7, 20.0, 0.75, dict)
}

#[test]
fn a_blank_image_is_a_miss_for_every_detector() {
    let img = blank();
    let chess = detect::default_chess_config();
    let errors: Vec<DetectError> = vec![
        detect::detect_chessboard(&img, &chess, &ChessboardParams::default()).unwrap_err(),
        detect::detect_charuco(&img, &CharucoParams::for_board(charuco_board())).unwrap_err(),
        detect::detect_puzzleboard(
            &img,
            &PuzzleBoardParams::for_board(PuzzleBoardSpec::new(10, 10, 12.0).expect("spec")),
        )
        .unwrap_err(),
    ];
    for err in errors {
        assert!(err.is_not_found(), "{err} should be a miss");
    }
}

#[test]
fn an_invalid_board_spec_is_a_misuse() {
    // A marker needs a black ring around its payload.
    let params = CharucoParams::for_board(charuco_board().with_border_bits(0));
    let err = detect::detect_charuco(&blank(), &params).unwrap_err();
    assert!(!err.is_not_found(), "{err} should be a misuse");
}

#[test]
fn a_malformed_image_buffer_is_a_misuse() {
    let err = detect::gray_image_from_slice(4, 4, &[0; 3]).unwrap_err();
    assert!(!err.is_not_found(), "{err} should be a misuse");
}
