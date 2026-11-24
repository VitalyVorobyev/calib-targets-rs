//! Photometric / blur-aware corner refinement WASM exports.

use calib_targets::blur as ct_blur;
use calib_targets_charuco::{CharucoBoardSpec, CharucoDetection};
use calib_targets_chessboard::ChessboardDetection;
use calib_targets_core::blur::BoardPhotometricOptions;
use calib_targets_marker::{MarkerBoardDetection, MarkerBoardSpec};
use calib_targets_puzzleboard::PuzzleBoardDetection;
use wasm_bindgen::prelude::*;

use crate::{from_js, gray_view, to_js};

fn options_from_js(options: JsValue) -> Result<BoardPhotometricOptions, JsError> {
    if options.is_undefined() || options.is_null() {
        Ok(BoardPhotometricOptions::default())
    } else {
        from_js(options)
    }
}

/// Return the default `BoardPhotometricOptions` as a JS object.
#[wasm_bindgen]
pub fn default_board_photometric_options() -> Result<JsValue, JsError> {
    to_js(&BoardPhotometricOptions::default())
}

/// Refine detected chessboard corners using the joint blur-aware photometric model.
///
/// Returns `{ detection, refinement }` where `detection` is the updated
/// `ChessboardDetection` and `refinement` is a `BoardPhotometricRefinement`.
#[wasm_bindgen]
pub fn refine_chessboard_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    detection: JsValue,
    options: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let mut det: ChessboardDetection = from_js(detection)?;
    let opts = options_from_js(options)?;
    let refinement = ct_blur::refine_chessboard_corners(&view, &mut det, &opts);
    to_js(&serde_json::json!({
        "detection": det,
        "refinement": refinement,
    }))
}

/// Refine detected ChArUco corners using the joint blur-aware photometric model.
///
/// Returns `{ detection, refinement }` where `detection` is the updated
/// `CharucoDetection` and `refinement` is a `BoardPhotometricRefinement`.
#[wasm_bindgen]
pub fn refine_charuco_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    board: JsValue,
    detection: JsValue,
    options: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let board_spec: CharucoBoardSpec = from_js(board)?;
    let mut det: CharucoDetection = from_js(detection)?;
    let opts = options_from_js(options)?;
    let refinement = ct_blur::refine_charuco_corners(&view, &board_spec, &mut det, &opts);
    to_js(&serde_json::json!({
        "detection": det,
        "refinement": refinement,
    }))
}

/// Refine detected PuzzleBoard corners using the joint blur-aware photometric model.
///
/// Returns `{ detection, refinement }` where `detection` is the updated
/// `PuzzleBoardDetection` and `refinement` is a `BoardPhotometricRefinement`.
#[wasm_bindgen]
pub fn refine_puzzleboard_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    detection: JsValue,
    options: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let mut det: PuzzleBoardDetection = from_js(detection)?;
    let opts = options_from_js(options)?;
    let refinement = ct_blur::refine_puzzleboard_corners(&view, &mut det, &opts);
    to_js(&serde_json::json!({
        "detection": det,
        "refinement": refinement,
    }))
}

/// Refine detected marker-board corners using the joint blur-aware photometric model.
///
/// Returns `{ detection, refinement }` where `detection` is the updated
/// `MarkerBoardDetection` and `refinement` is a `BoardPhotometricRefinement`.
#[wasm_bindgen]
pub fn refine_marker_board_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    board: JsValue,
    detection: JsValue,
    options: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let board_spec: MarkerBoardSpec = from_js(board)?;
    let mut det: MarkerBoardDetection = from_js(detection)?;
    let opts = options_from_js(options)?;
    let refinement = ct_blur::refine_marker_board_corners(&view, &board_spec, &mut det, &opts);
    to_js(&serde_json::json!({
        "detection": det,
        "refinement": refinement,
    }))
}
