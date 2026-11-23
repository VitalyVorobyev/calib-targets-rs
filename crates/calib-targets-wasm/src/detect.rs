//! Single-pass and multi-config sweep detector WASM exports.

use calib_targets_charuco::CharucoParams;
use calib_targets_chessboard::{
    ChessCorner, ChessboardDetector as ChessDetector, ChessboardParams,
};
use calib_targets_core::DetectorConfig;
use calib_targets_marker::{MarkerBoardDetector, MarkerBoardParams};
use calib_targets_puzzleboard::{PuzzleBoardParams, PuzzlePoleParams};
use wasm_bindgen::prelude::*;

use crate::{
    apply_chess_cfg_override, detection_to_js, from_js, gray_view, resolve_chess_cfg, to_js,
};

// ---------------------------------------------------------------------------
// Corner detection
// ---------------------------------------------------------------------------

/// Detect ChESS corners in a grayscale image.
///
/// Returns an array of `{ position: [x, y], orientation, strength }`.
#[wasm_bindgen]
pub fn detect_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let cfg: DetectorConfig = from_js(chess_cfg)?;
    let corners = calib_targets_chessboard::detect_corners(&view, &cfg);
    to_js(&corners)
}

// ---------------------------------------------------------------------------
// Chessboard detection
// ---------------------------------------------------------------------------

/// Detect a chessboard grid in a grayscale image.
///
/// Returns a `ChessboardDetection` JS object, or `null` if no board found.
/// If `chess_cfg` is provided, it overrides `params.chess`.
#[wasm_bindgen]
pub fn detect_chessboard(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let cb_params: ChessboardParams = from_js(params)?;
    let chess = resolve_chess_cfg(chess_cfg)?;

    // The frame's noise level goes onto the result's `quality`, which the
    // corner-cloud detector cannot know.
    let frame = calib_targets_chessboard::detect_corner_frame(&view, &chess);
    let detector = ChessDetector::new(cb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let result = detector.detect(&frame.corners).map(|mut d| {
        d.quality = d.quality.with_noise_sigma(frame.noise_sigma);
        d
    });
    to_js(&result)
}

/// Detect all chessboard components in a grayscale image.
///
/// Like `detect_chessboard` but returns every same-board component the detector
/// recovers (up to `params.max_components`), rather than just the first one.
///
/// Returns a JS array of `ChessboardDetection` objects (may be empty).
/// If `chess_cfg` is provided, it overrides `params.chess`.
#[wasm_bindgen]
pub fn detect_chessboard_all(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let cb_params: ChessboardParams = from_js(params)?;
    let chess = resolve_chess_cfg(chess_cfg)?;

    let frame = calib_targets_chessboard::detect_corner_frame(&view, &chess);
    let detector = ChessDetector::new(cb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let mut results = detector.detect_all(&frame.corners);
    for d in &mut results {
        d.quality = d.quality.with_noise_sigma(frame.noise_sigma);
    }
    to_js(&results)
}

// ---------------------------------------------------------------------------
// ChArUco detection
// ---------------------------------------------------------------------------

/// Detect a ChArUco board in a grayscale image.
///
/// Returns a `CharucoDetection` JS object, or `null` if no board is found.
/// Throws on an invalid image, board spec or parameters.
/// If `chess_cfg` is provided, it overrides `params.chess`.
///
/// Delegates to [`calib_targets::detect::detect_charuco`], which runs the
/// corner front-end from `params.chess` itself. Use
/// [`detect_charuco_with_corners`] when the caller already has a corner
/// cloud (e.g. shared across several detectors).
#[wasm_bindgen]
pub fn detect_charuco(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let mut charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    apply_chess_cfg_override(&mut charuco_params.chess, chess_cfg)?;

    detection_to_js(calib_targets::detect::detect_charuco(
        &view,
        &charuco_params,
    ))
}

/// Detect a ChArUco board from a pre-detected corner cloud (see
/// [`detect_corners`]).
///
/// Returns a `CharucoDetection` JS object, or `null` if no board is found.
/// Throws on an invalid image, board spec or parameters. `params.chess`
/// is not read: `corners` is taken as given.
#[wasm_bindgen]
pub fn detect_charuco_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    detection_to_js(calib_targets::detect::detect_charuco_with_corners(
        &view,
        &corners,
        &charuco_params,
    ))
}

// ---------------------------------------------------------------------------
// Marker board detection
// ---------------------------------------------------------------------------

/// Detect a checkerboard+circles marker board in a grayscale image.
///
/// Returns a `MarkerBoardDetection` JS object, or `null` if not found.
/// If `chess_cfg` is provided, it overrides `params.chess`.
///
/// `MarkerBoardDetector::detect_with_corners` now returns `Result`, not
/// `Option`, and its error type (`MarkerBoardDetectError`) does not
/// implement `Serialize`. The error is dropped via `.ok()` rather than
/// forwarded, so a miss still reaches JS as `null` — exactly the old
/// `Option`-returning contract. Bad `params` (detector construction
/// failure) still throws, unchanged from before.
#[wasm_bindgen]
pub fn detect_marker_board(
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
    let result = detector.detect(&view).ok();
    to_js(&result)
}

/// Detect a checkerboard+circles marker board from a pre-detected corner
/// cloud (see [`detect_corners`]).
///
/// Returns a `MarkerBoardDetection` JS object, or `null` if not found.
/// `params.chess` is not read: `corners` is taken as given. See
/// [`detect_marker_board`] for the null-on-miss / throw-on-bad-params
/// contract.
#[wasm_bindgen]
pub fn detect_marker_board_with_corners(
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
    let result = detector.detect_with_corners(&view, &corners).ok();
    to_js(&result)
}

// ---------------------------------------------------------------------------
// PuzzleBoard detection
// ---------------------------------------------------------------------------

/// Detect a PuzzleBoard in a grayscale image.
///
/// Returns a `PuzzleBoardDetection` JS object, or `null` if no board is found.
/// Throws on an invalid image, board spec or parameters.
/// If `chess_cfg` is provided, it overrides `params.chess`.
///
/// The returned `decode` block carries the compact decode summary.
/// Soft-mode runner-up scoring evidence is available from
/// `diagnose_puzzleboard`.
///
/// Delegates to [`calib_targets::detect::detect_puzzleboard`], which runs
/// the corner front-end from `params.chess` itself. Use
/// [`detect_puzzleboard_with_corners`] when the caller already has a corner
/// cloud (e.g. shared across several detectors).
#[wasm_bindgen]
pub fn detect_puzzleboard(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let mut puzzle_params: PuzzleBoardParams = from_js(params)?;
    apply_chess_cfg_override(&mut puzzle_params.chess, chess_cfg)?;

    detection_to_js(calib_targets::detect::detect_puzzleboard(
        &view,
        &puzzle_params,
    ))
}

/// Detect a PuzzleBoard from a pre-detected corner cloud (see
/// [`detect_corners`]).
///
/// Returns a `PuzzleBoardDetection` JS object, or `null` if no board is found.
/// Throws on an invalid image, board spec or parameters.
/// `params.chess` is not read: `corners` is taken as given.
#[wasm_bindgen]
pub fn detect_puzzleboard_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let puzzle_params: PuzzleBoardParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    detection_to_js(calib_targets::detect::detect_puzzleboard_with_corners(
        &view,
        &corners,
        &puzzle_params,
    ))
}

// ---------------------------------------------------------------------------
// Multi-config sweep detection
// ---------------------------------------------------------------------------

// Each `detect_*_best` deserialises its JS config array into `Vec<{X}Params>`,
// borrows a zero-copy `GrayImageView`, and delegates to the matching
// `calib_targets::detect::detect_*_best` facade helper. The facade honours
// each config's own `chess` front-end and memoises the corner pass across
// configs that share one, so the browser gets the same unified sweep
// semantics as the native and Python surfaces (rather than a single
// workspace-default corner pass).

/// Try multiple chessboard parameter configs, return the best result (most corners).
///
/// Returns a `ChessboardDetection` JS object, or `null` if no board found
/// with any config.
///
/// The chessboard detector is a corner-cloud consumer, so its front-end is
/// passed explicitly: `chess_cfg` (when provided) is used for corner detection
/// across every sweep config; otherwise the workspace-default ChESS settings
/// are used.
#[wasm_bindgen]
pub fn detect_chessboard_best(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    configs: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let configs: Vec<ChessboardParams> = from_js(configs)?;
    let chess = resolve_chess_cfg(chess_cfg)?;
    detection_to_js(calib_targets::detect::detect_chessboard_best(
        &view, &chess, &configs,
    ))
}

/// Try multiple ChArUco parameter configs, return the best result
/// (most markers, then most corners), or `null` if no config finds the board.
///
/// Each config carries its own `chess` front-end (`params.chess`); the corner
/// pass is deduplicated across configs that share one, so a sweep may freely
/// mix front-ends (e.g. a default pass alongside an upscaled pass) without
/// paying for a redundant corner pass when they agree.
#[wasm_bindgen]
pub fn detect_charuco_best(
    width: u32,
    height: u32,
    pixels: &[u8],
    configs: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let configs: Vec<CharucoParams> = from_js(configs)?;
    detection_to_js(calib_targets::detect::detect_charuco_best(&view, &configs))
}

/// Try multiple marker board parameter configs, return the best result (most corners).
///
/// Returns a `MarkerBoardDetection` JS object, or `null` if no board found
/// with any config.
///
/// Each config carries its own `chess` front-end (`params.chess`); the corner
/// pass is deduplicated across configs that share one.
#[wasm_bindgen]
pub fn detect_marker_board_best(
    width: u32,
    height: u32,
    pixels: &[u8],
    configs: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let configs: Vec<MarkerBoardParams> = from_js(configs)?;
    detection_to_js(calib_targets::detect::detect_marker_board_best(
        &view, &configs,
    ))
}

/// Try multiple PuzzleBoard parameter configs, return the best result
/// (most labelled corners, then mean bit confidence), or `null` if no config
/// finds the board.
///
/// Each config carries its own `chess` front-end (`params.chess`, deduplicated
/// across shared front-ends) and may choose its own `decode.search_mode` /
/// `decode.scoring_mode` / `decode.symmetry_mode`.
#[wasm_bindgen]
pub fn detect_puzzleboard_best(
    width: u32,
    height: u32,
    pixels: &[u8],
    configs: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let configs: Vec<PuzzleBoardParams> = from_js(configs)?;
    detection_to_js(calib_targets::detect::detect_puzzleboard_best(
        &view, &configs,
    ))
}

// ---------------------------------------------------------------------------
// PuzzlePole detection & diagnostics
// ---------------------------------------------------------------------------

/// Detect a PuzzlePole in a grayscale image.
#[wasm_bindgen]
pub fn detect_puzzlepole(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let mut pole_params: PuzzlePoleParams = from_js(params)?;
    apply_chess_cfg_override(&mut pole_params.chess, chess_cfg)?;

    detection_to_js(calib_targets::detect::detect_puzzlepole(
        &view,
        &pole_params,
    ))
}

/// Detect a PuzzlePole from a pre-detected corner cloud (see [`detect_corners`]).
///
/// `params.chess` is not read: `corners` is taken as given.
#[wasm_bindgen]
pub fn detect_puzzlepole_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let pole_params: PuzzlePoleParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    detection_to_js(calib_targets::detect::detect_puzzlepole_with_corners(
        &view,
        &corners,
        &pole_params,
    ))
}

/// Try multiple PuzzlePole parameter configs, returning the best detection.
#[wasm_bindgen]
pub fn detect_puzzlepole_best(
    width: u32,
    height: u32,
    pixels: &[u8],
    configs: JsValue,
) -> Result<JsValue, JsError> {
    let view = gray_view(pixels, width, height)?;
    let configs: Vec<PuzzlePoleParams> = from_js(configs)?;
    detection_to_js(calib_targets::detect::detect_puzzlepole_best(
        &view, &configs,
    ))
}
