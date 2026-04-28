//! WebAssembly bindings for `calib-targets` calibration target detectors.
//!
//! Exposes stateless detection functions that accept grayscale `&[u8]` buffers
//! and JS config objects (deserialized via `serde-wasm-bindgen`).

mod convert;
mod gray;

use calib_targets_aruco::builtins::{builtin_dictionary, BUILTIN_DICTIONARY_NAMES};
use calib_targets_charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
use calib_targets_chessboard::{Detector as ChessDetector, DetectorParams};
use calib_targets_core::{ChessConfig, Corner, ThresholdMode};
use calib_targets_marker::{MarkerBoardDetector, MarkerBoardParams};
use calib_targets_print::{
    render_target_bundle, CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec, PageSize,
    PageSpec, PrintableTargetDocument, PuzzleBoardTargetSpec, RenderOptions, TargetSpec,
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
        .unwrap_or_default()
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
///
/// The returned payload includes the PuzzleBoard decode sub-config, with
/// `search_mode = {"kind": "full"}` and
/// `scoring_mode = {"kind": "soft_log_likelihood"}` by default.
#[wasm_bindgen]
pub fn default_puzzleboard_params(rows: u32, cols: u32) -> Result<JsValue, JsError> {
    let spec = PuzzleBoardSpec::new(rows, cols, 1.0).map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&PuzzleBoardParams::for_board(&spec))
}

/// Return default `CharucoParams` for the given board geometry.
///
/// `rows` / `cols` are **square counts** (not inner-corner counts).
/// `marker_size_rel` ∈ (0, 1] is the marker side length relative to the
/// square. `dictionary_name` is one of [`list_aruco_dictionaries`] (e.g.
/// `"DICT_4X4_50"`).
#[wasm_bindgen]
pub fn default_charuco_params(
    rows: u32,
    cols: u32,
    marker_size_rel: f64,
    dictionary_name: &str,
) -> Result<JsValue, JsError> {
    let spec = charuco_board_spec(rows, cols, marker_size_rel, dictionary_name)?;
    to_js(&CharucoParams::for_board(&spec))
}

/// List the names of every built-in ArUco / AprilTag dictionary.
///
/// The returned strings are valid `dictionary_name` arguments for
/// [`default_charuco_params`] and [`render_charuco_png`].
#[wasm_bindgen]
pub fn list_aruco_dictionaries() -> Result<JsValue, JsError> {
    to_js(&BUILTIN_DICTIONARY_NAMES)
}

// ---------------------------------------------------------------------------
// Multi-config sweep presets
// ---------------------------------------------------------------------------

/// Return the 3-config chessboard sweep preset (`DetectorParams::sweep_default()`).
///
/// Pass the array directly to [`detect_chessboard_best`].
#[wasm_bindgen]
pub fn chessboard_sweep_default() -> Result<JsValue, JsError> {
    to_js(&DetectorParams::sweep_default())
}

/// Return the ChArUco sweep preset for a given board (`CharucoParams::sweep_for_board(&spec)`).
///
/// Pass the array directly to [`detect_charuco_best`].
#[wasm_bindgen]
pub fn charuco_sweep_for_board(
    rows: u32,
    cols: u32,
    marker_size_rel: f64,
    dictionary_name: &str,
) -> Result<JsValue, JsError> {
    let spec = charuco_board_spec(rows, cols, marker_size_rel, dictionary_name)?;
    to_js(&CharucoParams::sweep_for_board(&spec))
}

/// Return the PuzzleBoard sweep preset for a given board (`PuzzleBoardParams::sweep_for_board(&spec)`).
///
/// Pass the array directly to [`detect_puzzleboard_best`].
#[wasm_bindgen]
pub fn puzzleboard_sweep_for_board(rows: u32, cols: u32) -> Result<JsValue, JsError> {
    let spec = PuzzleBoardSpec::new(rows, cols, 1.0).map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&PuzzleBoardParams::sweep_for_board(&spec))
}

// ---------------------------------------------------------------------------
// Synthetic target generation
// ---------------------------------------------------------------------------

/// Build a `CharucoBoardSpec` from JS-friendly arguments.
fn charuco_board_spec(
    rows: u32,
    cols: u32,
    marker_size_rel: f64,
    dictionary_name: &str,
) -> Result<CharucoBoardSpec, JsError> {
    let dictionary = builtin_dictionary(dictionary_name).ok_or_else(|| {
        JsError::new(&format!(
            "unknown dictionary {:?}; call list_aruco_dictionaries() for valid names",
            dictionary_name
        ))
    })?;
    Ok(CharucoBoardSpec {
        rows,
        cols,
        cell_size: 1.0,
        marker_size_rel: marker_size_rel as f32,
        dictionary,
        marker_layout: MarkerLayout::default(),
    })
}

/// Wrap a target spec in a `PrintableTargetDocument` sized to fit the board
/// exactly (board extent + 20 mm margin), at `dpi`.
fn fitted_document(
    target: TargetSpec,
    width_mm: f64,
    height_mm: f64,
    dpi: u32,
) -> PrintableTargetDocument {
    let mut doc = PrintableTargetDocument::new(target);
    doc.page = PageSpec {
        size: PageSize::Custom {
            width_mm: width_mm + 20.0,
            height_mm: height_mm + 20.0,
        },
        margin_mm: 5.0,
        ..PageSpec::default()
    };
    doc.render = RenderOptions {
        debug_annotations: false,
        png_dpi: dpi,
    };
    doc
}

/// Synthesise a chessboard target PNG in memory.
///
/// `inner_rows` / `inner_cols` are the **inner-corner** counts (each ≥ 2). The
/// printed board has `(inner_cols + 1) × (inner_rows + 1)` squares of side
/// `square_size_mm`. Returns raw PNG bytes for a tightly-cropped page.
#[wasm_bindgen]
pub fn render_chessboard_png(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<Vec<u8>, JsError> {
    let target = TargetSpec::Chessboard(ChessboardTargetSpec {
        inner_rows,
        inner_cols,
        square_size_mm,
    });
    let w = f64::from(inner_cols + 1) * square_size_mm;
    let h = f64::from(inner_rows + 1) * square_size_mm;
    let bundle = render_target_bundle(&fitted_document(target, w, h, dpi))
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(bundle.png_bytes)
}

/// Synthesise a ChArUco target PNG in memory.
///
/// `rows` / `cols` are **square counts** (≥ 2 each). `marker_size_rel` ∈ (0, 1]
/// sets the marker side length relative to the square; `dictionary_name` is
/// one of [`list_aruco_dictionaries`] (e.g. `"DICT_4X4_50"`).
#[wasm_bindgen]
pub fn render_charuco_png(
    rows: u32,
    cols: u32,
    square_size_mm: f64,
    marker_size_rel: f64,
    dictionary_name: &str,
    dpi: u32,
) -> Result<Vec<u8>, JsError> {
    let dictionary = builtin_dictionary(dictionary_name).ok_or_else(|| {
        JsError::new(&format!(
            "unknown dictionary {:?}; call list_aruco_dictionaries() for valid names",
            dictionary_name
        ))
    })?;
    let target = TargetSpec::Charuco(CharucoTargetSpec {
        rows,
        cols,
        square_size_mm,
        marker_size_rel,
        dictionary,
        marker_layout: MarkerLayout::default(),
        border_bits: 1,
    });
    let w = f64::from(cols) * square_size_mm;
    let h = f64::from(rows) * square_size_mm;
    let bundle = render_target_bundle(&fitted_document(target, w, h, dpi))
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(bundle.png_bytes)
}

/// Synthesise a marker-board target PNG in memory.
///
/// `inner_rows` / `inner_cols` are the **inner-corner** counts. The default
/// 3-circle layout from `MarkerBoardTargetSpec::default_circles` is used; for
/// custom circle placement, call the Rust facade directly.
#[wasm_bindgen]
pub fn render_marker_board_png(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<Vec<u8>, JsError> {
    let target = TargetSpec::MarkerBoard(MarkerBoardTargetSpec {
        inner_rows,
        inner_cols,
        square_size_mm,
        circles: MarkerBoardTargetSpec::default_circles(inner_rows, inner_cols),
        circle_diameter_rel: 0.5,
    });
    let w = f64::from(inner_cols + 1) * square_size_mm;
    let h = f64::from(inner_rows + 1) * square_size_mm;
    let bundle = render_target_bundle(&fitted_document(target, w, h, dpi))
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(bundle.png_bytes)
}

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
    let target = TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec {
        rows,
        cols,
        square_size_mm,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    });
    let w = f64::from(cols) * square_size_mm;
    let h = f64::from(rows) * square_size_mm;
    let bundle = render_target_bundle(&fitted_document(target, w, h, dpi))
        .map_err(|e| JsError::new(&e.to_string()))?;
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

/// Detect all chessboard components in a grayscale image.
///
/// Like `detect_chessboard` but returns every same-board component the detector
/// recovers (up to `params.max_components`), rather than just the first one.
///
/// Returns a JS array of `ChessboardDetectionResult` objects (may be empty).
/// If `chess_cfg` is provided, it overrides `params.chess`.
#[wasm_bindgen]
pub fn detect_chessboard_all(
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
    let results = detector.detect_all(&corners);
    to_js(&results)
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
///
/// The returned `decode` block mirrors the Rust `serde_json` shape,
/// including soft-mode diagnostics such as `score_margin` and the
/// runner-up hypothesis when available.
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
///
/// Each config may choose its own `decode.search_mode` / `decode.scoring_mode`.
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
