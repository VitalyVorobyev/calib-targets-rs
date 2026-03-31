//! WebAssembly bindings for `calib-targets` calibration target detectors.
//!
//! Exposes stateless detection functions that accept grayscale `&[u8]` buffers
//! and JS config objects (deserialized via `serde-wasm-bindgen`).

mod convert;
mod gray;

use calib_targets_charuco::CharucoDetector;
use calib_targets_chessboard::{ChessboardDetector, ChessboardParams};
use calib_targets_core::{ChessConfig, Corner, ThresholdMode};
use calib_targets_marker::{MarkerBoardDetector, MarkerBoardParams};
use chess_corners::find_chess_corners_u8;
use wasm_bindgen::prelude::*;

use convert::{adapt_chess_corner, to_chess_corners_config};
use gray::make_view;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn from_js<T: serde::de::DeserializeOwned>(val: JsValue) -> Result<T, JsError> {
    serde_wasm_bindgen::from_value(val).map_err(|e| JsError::new(&e.to_string()))
}

fn to_js<T: serde::Serialize>(val: &T) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(val).map_err(|e| JsError::new(&e.to_string()))
}

fn validate_gray(pixels: &[u8], width: u32, height: u32) -> Result<(), JsError> {
    let expected = (width as usize) * (height as usize);
    if pixels.len() != expected {
        return Err(JsError::new(&format!(
            "pixel buffer length {} does not match {}x{} = {}",
            pixels.len(),
            width,
            height,
            expected
        )));
    }
    Ok(())
}

fn detect_corners_impl(pixels: &[u8], width: u32, height: u32, cfg: &ChessConfig) -> Vec<Corner> {
    let cc_cfg = to_chess_corners_config(cfg);
    find_chess_corners_u8(pixels, width, height, &cc_cfg)
        .iter()
        .map(adapt_chess_corner)
        .collect()
}

// ---------------------------------------------------------------------------
// Default configs (exported so the JS side can populate UI with defaults)
// ---------------------------------------------------------------------------

/// Return the default `ChessConfig` as a JS object.
#[wasm_bindgen]
pub fn default_chess_config() -> Result<JsValue, JsError> {
    let cfg = ChessConfig {
        threshold_mode: ThresholdMode::Relative,
        threshold_value: 0.2,
        nms_radius: 2,
        ..ChessConfig::single_scale()
    };
    to_js(&cfg)
}

/// Return the default `ChessboardParams` as a JS object.
#[wasm_bindgen]
pub fn default_chessboard_params() -> Result<JsValue, JsError> {
    to_js(&ChessboardParams::default())
}

/// Return the default `MarkerBoardParams` (with a minimal placeholder layout) as a JS object.
#[wasm_bindgen]
pub fn default_marker_board_params() -> Result<JsValue, JsError> {
    to_js(&MarkerBoardParams::default())
}

// ---------------------------------------------------------------------------
// RGBA → grayscale utility
// ---------------------------------------------------------------------------

/// Convert an RGBA pixel buffer to grayscale (BT.601 weights).
///
/// Input: RGBA buffer of length `4 * width * height`.
/// Returns: grayscale buffer of length `width * height`.
#[wasm_bindgen]
pub fn rgba_to_gray(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, JsError> {
    let expected = 4 * (width as usize) * (height as usize);
    if rgba.len() != expected {
        return Err(JsError::new(&format!(
            "RGBA buffer length {} does not match 4*{}*{} = {}",
            rgba.len(),
            width,
            height,
            expected
        )));
    }
    Ok(gray::rgba_to_grayscale(rgba, width, height))
}

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
    validate_gray(pixels, width, height)?;
    let cfg: ChessConfig = from_js(chess_cfg)?;
    let corners = detect_corners_impl(pixels, width, height, &cfg);
    to_js(&corners)
}

// ---------------------------------------------------------------------------
// Chessboard detection
// ---------------------------------------------------------------------------

/// Detect a chessboard grid in a grayscale image.
///
/// Returns a `ChessboardDetectionResult` JS object, or `null` if no board found.
#[wasm_bindgen]
pub fn detect_chessboard(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let cfg: ChessConfig = from_js(chess_cfg)?;
    let cb_params: ChessboardParams = from_js(params)?;

    let corners = detect_corners_impl(pixels, width, height, &cfg);
    let detector = ChessboardDetector::new(cb_params);
    let result = detector.detect_from_corners(&corners);
    to_js(&result)
}

// ---------------------------------------------------------------------------
// ChArUco detection
// ---------------------------------------------------------------------------

/// Detect a ChArUco board in a grayscale image.
///
/// Returns a `CharucoDetectionResult` JS object. Throws on error.
#[wasm_bindgen]
pub fn detect_charuco(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let cfg: ChessConfig = from_js(chess_cfg)?;
    let charuco_params: calib_targets_charuco::CharucoDetectorParams = from_js(params)?;

    let corners = detect_corners_impl(pixels, width, height, &cfg);
    let detector =
        CharucoDetector::new(charuco_params).map_err(|e| JsError::new(&e.to_string()))?;
    let view = make_view(pixels, width, height);
    let result = detector
        .detect(&view, &corners)
        .map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&result)
}

// ---------------------------------------------------------------------------
// Marker board detection
// ---------------------------------------------------------------------------

/// Detect a checkerboard+circles marker board in a grayscale image.
///
/// Returns a `MarkerBoardDetectionResult` JS object, or `null` if not found.
#[wasm_bindgen]
pub fn detect_marker_board(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let cfg: ChessConfig = from_js(chess_cfg)?;
    let mb_params: MarkerBoardParams = from_js(params)?;

    let corners = detect_corners_impl(pixels, width, height, &cfg);
    let detector = MarkerBoardDetector::new(mb_params);
    let view = make_view(pixels, width, height);
    let result = detector.detect_from_image_and_corners(&view, &corners);
    to_js(&result)
}
