//! WebAssembly bindings for `calib-targets` calibration target detectors.
//!
//! Exposes stateless detection functions that accept grayscale `&[u8]` buffers
//! and JS config objects (deserialized via `serde-wasm-bindgen`).

mod convert;
mod gray;

use calib_targets_charuco::{CharucoDetector, CharucoParams};
use calib_targets_chessboard::{Detector as ChessDetector, DetectorParams};
use calib_targets_core::{ChessConfig, Corner, ThresholdMode};
use calib_targets_marker::{MarkerBoardDetector, MarkerBoardParams};
use calib_targets_print::{
    render_target_bundle, PageSize, PageSpec, PrintableTargetDocument, PuzzleBoardTargetSpec,
    RenderOptions, TargetSpec,
};
use calib_targets_puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
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
    let expected = (width as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| {
            JsError::new(&format!(
                "image dimensions {}x{} overflow usize",
                width, height
            ))
        })?;
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

/// Resolve a ChESS detector config, falling back to defaults when JS supplies
/// `undefined` / `null`. The chessboard detector no longer carries a
/// nested ChESS config in `DetectorParams`, so callers (or this helper) must
/// supply one for the corner-detection step.
fn resolve_chess_cfg(chess_cfg: JsValue) -> Result<ChessConfig, JsError> {
    if !chess_cfg.is_undefined() && !chess_cfg.is_null() {
        from_js(chess_cfg)
    } else {
        Ok(ChessConfig {
            threshold_mode: ThresholdMode::Relative,
            threshold_value: 0.2,
            nms_radius: 2,
            ..ChessConfig::single_scale()
        })
    }
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

/// Return the default `DetectorParams` as a JS object.
#[wasm_bindgen]
pub fn default_chessboard_params() -> Result<JsValue, JsError> {
    to_js(&DetectorParams::default())
}

/// Return the default `MarkerBoardParams` (with a minimal placeholder layout) as a JS object.
#[wasm_bindgen]
pub fn default_marker_board_params() -> Result<JsValue, JsError> {
    to_js(&MarkerBoardParams::default())
}

/// Return default `PuzzleBoardParams` for a `rows × cols` board as a JS object.
#[wasm_bindgen]
pub fn default_puzzleboard_params(rows: u32, cols: u32) -> Result<JsValue, JsError> {
    let spec = PuzzleBoardSpec::new(rows, cols, 1.0).map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&PuzzleBoardParams::for_board(&spec))
}

// ---------------------------------------------------------------------------
// Synthetic PuzzleBoard generation
// ---------------------------------------------------------------------------

/// Synthesise a PuzzleBoard target PNG in memory.
///
/// Returns the raw PNG bytes for a `rows × cols` board at the given DPI.
/// The caller typically hands these to an `<img>` or `createImageBitmap`
/// for display, then rasterises to a canvas to obtain an RGBA buffer that
/// can be fed back into [`detect_puzzleboard`] for a round-trip demo.
#[wasm_bindgen]
pub fn render_puzzleboard_png(
    rows: u32,
    cols: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<Vec<u8>, JsError> {
    let target = PuzzleBoardTargetSpec {
        rows,
        cols,
        square_size_mm,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    };
    let mut doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(target));
    doc.page = PageSpec {
        size: PageSize::Custom {
            width_mm: f64::from(cols) * square_size_mm + 20.0,
            height_mm: f64::from(rows) * square_size_mm + 20.0,
        },
        margin_mm: 5.0,
        ..PageSpec::default()
    };
    doc.render = RenderOptions {
        debug_annotations: false,
        png_dpi: dpi,
    };
    let bundle = render_target_bundle(&doc).map_err(|e| JsError::new(&e.to_string()))?;
    Ok(bundle.png_bytes)
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
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| {
            JsError::new(&format!(
                "image dimensions {}x{} overflow usize",
                width, height
            ))
        })?;
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
/// If `chess_cfg` is provided, it overrides `params.chess`.
#[wasm_bindgen]
pub fn detect_chessboard(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let cb_params: DetectorParams = from_js(params)?;
    let chess = resolve_chess_cfg(chess_cfg)?;

    let corners = detect_corners_impl(pixels, width, height, &chess);
    let detector = ChessDetector::new(cb_params);
    let result = detector.detect(&corners);
    to_js(&result)
}

// ---------------------------------------------------------------------------
// ChArUco detection
// ---------------------------------------------------------------------------

/// Detect a ChArUco board in a grayscale image.
///
/// Returns a `CharucoDetectionResult` JS object. Throws on error.
/// If `chess_cfg` is provided, it overrides `params.chessboard.chess`.
#[wasm_bindgen]
pub fn detect_charuco(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    let chess = resolve_chess_cfg(chess_cfg)?;

    let corners = detect_corners_impl(pixels, width, height, &chess);
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
/// If `chess_cfg` is provided, it overrides `params.chessboard.chess`.
#[wasm_bindgen]
pub fn detect_marker_board(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let mb_params: MarkerBoardParams = from_js(params)?;
    let chess = resolve_chess_cfg(chess_cfg)?;

    let corners = detect_corners_impl(pixels, width, height, &chess);
    let detector = MarkerBoardDetector::new(mb_params);
    let view = make_view(pixels, width, height);
    let result = detector.detect_from_image_and_corners(&view, &corners);
    to_js(&result)
}

// ---------------------------------------------------------------------------
// PuzzleBoard detection
// ---------------------------------------------------------------------------

/// Detect a PuzzleBoard in a grayscale image.
///
/// Returns a `PuzzleBoardDetectionResult` JS object. Throws on error.
/// If `chess_cfg` is provided, it overrides `params.chessboard.chess`.
#[wasm_bindgen]
pub fn detect_puzzleboard(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let puzzle_params: PuzzleBoardParams = from_js(params)?;
    let chess = resolve_chess_cfg(chess_cfg)?;

    let corners = detect_corners_impl(pixels, width, height, &chess);
    let detector =
        PuzzleBoardDetector::new(puzzle_params).map_err(|e| JsError::new(&e.to_string()))?;
    let view = make_view(pixels, width, height);
    let result = detector
        .detect(&view, &corners)
        .map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&result)
}

// ---------------------------------------------------------------------------
// Multi-config sweep detection
// ---------------------------------------------------------------------------

/// Try multiple chessboard parameter configs, return the best result (most corners).
///
/// Returns a `ChessboardDetectionResult` JS object, or `null` if no board found
/// with any config.
#[wasm_bindgen]
pub fn detect_chessboard_best(
    width: u32,
    height: u32,
    pixels: &[u8],
    configs: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let configs: Vec<DetectorParams> = from_js(configs)?;

    // The chessboard detector does not carry a ChESS config; reuse the
    // default ChESS settings for corner detection across every sweep config.
    let chess = resolve_chess_cfg(JsValue::UNDEFINED)?;
    let corners = detect_corners_impl(pixels, width, height, &chess);

    let best = configs
        .iter()
        .filter_map(|params| ChessDetector::new(params.clone()).detect(&corners))
        .max_by_key(|d| d.target.corners.len());
    to_js(&best)
}

/// Try multiple ChArUco parameter configs, return the best result
/// (most markers, then most corners). Throws if all configs fail.
#[wasm_bindgen]
pub fn detect_charuco_best(
    width: u32,
    height: u32,
    pixels: &[u8],
    configs: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let configs: Vec<CharucoParams> = from_js(configs)?;

    let mut best: Option<calib_targets_charuco::CharucoDetectionResult> = None;
    let mut last_err = None;

    let chess = resolve_chess_cfg(JsValue::UNDEFINED)?;
    let corners = detect_corners_impl(pixels, width, height, &chess);
    for params in &configs {
        let detector = match CharucoDetector::new(params.clone()) {
            Ok(d) => d,
            Err(e) => {
                last_err = Some(e.to_string());
                continue;
            }
        };
        let view = make_view(pixels, width, height);
        match detector.detect(&view, &corners) {
            Ok(result) => {
                let dominated = best.as_ref().is_some_and(|b| {
                    (b.markers.len(), b.detection.corners.len())
                        >= (result.markers.len(), result.detection.corners.len())
                });
                if !dominated {
                    best = Some(result);
                }
            }
            Err(e) => {
                last_err = Some(e.to_string());
            }
        }
    }

    match best {
        Some(result) => to_js(&result),
        None => Err(JsError::new(
            &last_err.unwrap_or_else(|| "no markers detected".to_string()),
        )),
    }
}

/// Try multiple marker board parameter configs, return the best result (most corners).
///
/// Returns a `MarkerBoardDetectionResult` JS object, or `null` if no board found
/// with any config.
#[wasm_bindgen]
pub fn detect_marker_board_best(
    width: u32,
    height: u32,
    pixels: &[u8],
    configs: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let configs: Vec<MarkerBoardParams> = from_js(configs)?;

    let chess = resolve_chess_cfg(JsValue::UNDEFINED)?;
    let corners = detect_corners_impl(pixels, width, height, &chess);
    let best = configs
        .iter()
        .filter_map(|params| {
            let detector = MarkerBoardDetector::new(params.clone());
            let view = make_view(pixels, width, height);
            detector.detect_from_image_and_corners(&view, &corners)
        })
        .max_by_key(|r| r.detection.corners.len());
    to_js(&best)
}

/// Try multiple PuzzleBoard parameter configs, return the best result
/// (most labelled corners, then mean bit confidence). Throws if all configs fail.
#[wasm_bindgen]
pub fn detect_puzzleboard_best(
    width: u32,
    height: u32,
    pixels: &[u8],
    configs: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let configs: Vec<PuzzleBoardParams> = from_js(configs)?;

    let mut best: Option<calib_targets_puzzleboard::PuzzleBoardDetectionResult> = None;
    let mut last_err = None;

    let chess = resolve_chess_cfg(JsValue::UNDEFINED)?;
    let corners = detect_corners_impl(pixels, width, height, &chess);
    for params in &configs {
        let detector = match PuzzleBoardDetector::new(params.clone()) {
            Ok(d) => d,
            Err(e) => {
                last_err = Some(e.to_string());
                continue;
            }
        };
        let view = make_view(pixels, width, height);
        match detector.detect(&view, &corners) {
            Ok(result) => {
                let dominated = best.as_ref().is_some_and(|b| {
                    let new_key = (
                        result.detection.corners.len(),
                        result.decode.mean_confidence,
                    );
                    let old_key = (b.detection.corners.len(), b.decode.mean_confidence);
                    old_key.0 > new_key.0 || (old_key.0 == new_key.0 && old_key.1 >= new_key.1)
                });
                if !dominated {
                    best = Some(result);
                }
            }
            Err(e) => {
                last_err = Some(e.to_string());
            }
        }
    }

    match best {
        Some(result) => to_js(&result),
        None => Err(JsError::new(
            &last_err.unwrap_or_else(|| "puzzleboard not detected".to_string()),
        )),
    }
}
