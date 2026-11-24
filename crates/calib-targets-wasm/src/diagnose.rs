//! Diagnostics-channel detector WASM exports (`{ result, diagnostics }`).

use calib_targets_charuco::CharucoDetector;
use calib_targets_chessboard::ChessCorner;
use calib_targets_marker::{MarkerBoardDetector, MarkerBoardParams};
use calib_targets_puzzleboard::{
    PuzzleBoardDetector, PuzzleBoardParams, PuzzlePoleDetector, PuzzlePoleParams,
};
use wasm_bindgen::prelude::*;

use crate::{apply_chess_cfg_override, from_js, gray_view, to_js};

// ---------------------------------------------------------------------------
// Diagnostics-channel detection
//
// Each `diagnose_*` runs the detector's `diagnose` / `diagnose_with_corners`
// Rust path and returns a `{ result, diagnostics }` JS object. `result` is
// the same payload the corresponding `detect_*` function returns; on a
// failed detection it is `null`. `diagnostics` mirrors the Rust diagnostics
// struct's `serde_json` shape and carries a looser stability promise than
// the result API — every detector's diagnostics channel now yields evidence
// even on a failed detection (best-effort), so `diagnostics` is only ever
// `null` here when `result` is present but the payload construction itself
// failed. See `typescript-extras.d.ts` for the object shapes.
// ---------------------------------------------------------------------------

/// Detect a ChArUco board and additionally return the diagnostics channel.
///
/// Returns a `{ result, diagnostics }` object. `result` is a
/// `CharucoDetection` (or `null` when detection fails);
/// `diagnostics` is the `CharucoDetectDiagnostics` payload — produced even
/// on a failed frame so callers can render the failure mode.
#[wasm_bindgen]
pub fn diagnose_charuco(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let mut charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    apply_chess_cfg_override(&mut charuco_params.chess, chess_cfg)?;

    let detector =
        CharucoDetector::new(charuco_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose(&view);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// [`diagnose_charuco`] from a pre-detected corner cloud (see
/// [`detect_corners`](crate::detect_corners)). `params.chess` is not read: `corners` is taken as
/// given.
#[wasm_bindgen]
pub fn diagnose_charuco_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    let detector =
        CharucoDetector::new(charuco_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose_with_corners(&view, &corners);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// Detect a marker board and additionally return the diagnostics channel.
///
/// Returns a `{ result, diagnostics }` object. `result` is a
/// `MarkerBoardDetection` (or `null` when no board is found).
/// `diagnostics` is the `MarkerBoardDiagnostics` payload, produced even on a
/// failed detection (best-effort, so overlay tools can render the circle
/// hypotheses that *were* scored) — `MarkerBoardDetector::diagnose_with_corners`
/// now returns diagnostics unconditionally rather than only on success.
#[wasm_bindgen]
pub fn diagnose_marker_board(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let mut mb_params: MarkerBoardParams = from_js(params)?;
    apply_chess_cfg_override(&mut mb_params.chess, chess_cfg)?;

    let detector = MarkerBoardDetector::new(mb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose(&view);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// [`diagnose_marker_board`] from a pre-detected corner cloud (see
/// [`detect_corners`](crate::detect_corners)). `params.chess` is not read: `corners` is taken as
/// given.
#[wasm_bindgen]
pub fn diagnose_marker_board_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let mb_params: MarkerBoardParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    let detector = MarkerBoardDetector::new(mb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose_with_corners(&view, &corners);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// Detect a PuzzleBoard and additionally return the diagnostics channel.
///
/// Returns a `{ result, diagnostics }` object. `result` is a
/// `PuzzleBoardDetection` (or `null` when detection fails);
/// `diagnostics` is the `PuzzleBoardDiagnostics` payload — produced even on
/// a failed decode so callers can render the sampled edge observations.
#[wasm_bindgen]
pub fn diagnose_puzzleboard(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let mut puzzle_params: PuzzleBoardParams = from_js(params)?;
    apply_chess_cfg_override(&mut puzzle_params.chess, chess_cfg)?;

    let detector =
        PuzzleBoardDetector::new(puzzle_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose(&view);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// [`diagnose_puzzleboard`] from a pre-detected corner cloud (see
/// [`detect_corners`](crate::detect_corners)). `params.chess` is not read: `corners` is taken as
/// given.
#[wasm_bindgen]
pub fn diagnose_puzzleboard_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let puzzle_params: PuzzleBoardParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    let detector =
        PuzzleBoardDetector::new(puzzle_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose_with_corners(&view, &corners);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// Detect a PuzzlePole and additionally return the diagnostics channel.
#[wasm_bindgen]
pub fn diagnose_puzzlepole(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let mut pole_params: PuzzlePoleParams = from_js(params)?;
    apply_chess_cfg_override(&mut pole_params.chess, chess_cfg)?;

    let detector =
        PuzzlePoleDetector::new(pole_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose(&view);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// [`diagnose_puzzlepole`] from a pre-detected corner cloud (see
/// [`detect_corners`](crate::detect_corners)). `params.chess` is not read: `corners` is taken as
/// given.
#[wasm_bindgen]
pub fn diagnose_puzzlepole_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let pole_params: PuzzlePoleParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    let detector =
        PuzzlePoleDetector::new(pole_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose_with_corners(&view, &corners);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}
