//! WebAssembly bindings for `calib-targets` calibration target detectors.
//!
//! Exposes stateless detection functions that accept grayscale `&[u8]` buffers
//! and JS config objects (deserialized via `serde-wasm-bindgen`).

mod convert;
mod gray;

use calib_targets_aruco::builtins::{builtin_dictionary, BUILTIN_DICTIONARY_NAMES};
use calib_targets_charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams};
use calib_targets_chessboard::ChessCorner;
use calib_targets_chessboard::{Detector as ChessDetector, DetectorParams, GraphBuildAlgorithm};
use calib_targets_core::DetectorConfig;
use calib_targets_marker::{MarkerBoardDetector, MarkerBoardParams};
use calib_targets_print::{
    render_target_bundle, CharucoTargetSpec, ChessboardTargetSpec, GeneratedTargetBundle,
    MarkerBoardTargetSpec, PageSize, PageSpec, PrintableTargetDocument, PuzzleBoardTargetSpec,
    RenderOptions, TargetSpec,
};
use calib_targets_puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use chess_corners::{Detector as ChessCornerDetector, Threshold};
use wasm_bindgen::prelude::*;

use convert::adapt_chess_corner;
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

fn detect_corners_impl(
    pixels: &[u8],
    width: u32,
    height: u32,
    cfg: &DetectorConfig,
) -> Vec<ChessCorner> {
    let Ok(mut detector) = ChessCornerDetector::new(*cfg) else {
        return Vec::new();
    };
    detector
        .detect_u8(pixels, width, height)
        .unwrap_or_default()
        .iter()
        .map(adapt_chess_corner)
        .collect()
}

/// Build the workspace-default ChESS detector config. Mirrors the threshold
/// override applied in `calib_targets::detect::default_chess_config` — see
/// the rustdoc on that function for the rationale and sweep evidence.
fn workspace_default_chess_cfg() -> DetectorConfig {
    DetectorConfig::chess().with_threshold(Threshold::Absolute(15.0))
}

/// Resolve a ChESS detector config, falling back to the workspace default
/// when JS supplies `undefined` / `null`.
fn resolve_chess_cfg(chess_cfg: JsValue) -> Result<DetectorConfig, JsError> {
    if !chess_cfg.is_undefined() && !chess_cfg.is_null() {
        from_js(chess_cfg)
    } else {
        Ok(workspace_default_chess_cfg())
    }
}

// ---------------------------------------------------------------------------
// Default configs (exported so the JS side can populate UI with defaults)
// ---------------------------------------------------------------------------

/// Return the default `DetectorConfig` as a JS object.
#[wasm_bindgen]
pub fn default_chess_config() -> Result<JsValue, JsError> {
    to_js(&workspace_default_chess_cfg())
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
    Ok(CharucoBoardSpec::new(
        rows,
        cols,
        1.0,
        marker_size_rel as f32,
        dictionary,
    ))
}

/// Wrap a target spec in a `PrintableTargetDocument` sized to fit the board
/// exactly (board extent + 20 mm margin), at `dpi`.
fn fitted_document(
    target: TargetSpec,
    width_mm: f64,
    height_mm: f64,
    dpi: u32,
) -> PrintableTargetDocument {
    let page = PageSpec::default()
        .with_size(PageSize::Custom {
            width_mm: width_mm + 20.0,
            height_mm: height_mm + 20.0,
        })
        .with_margin_mm(5.0);
    let render = RenderOptions::default()
        .with_debug_annotations(false)
        .with_png_dpi(dpi);
    PrintableTargetDocument::new(target)
        .with_page(page)
        .with_render(render)
}

/// Build a target spec + fitted document and render the full
/// JSON/SVG/PNG/DXF bundle, returning a JS object with
/// `json_text` / `svg_text` / `png_bytes` (as a `Uint8Array`) / `dxf_text`.
///
/// `png_bytes` is materialised as a `Uint8Array` rather than a plain JS
/// array so binary data crosses the boundary as a typed array
/// (single-buffer copy, browser-friendly).
fn render_bundle_to_js(
    target: TargetSpec,
    width_mm: f64,
    height_mm: f64,
    dpi: u32,
) -> Result<JsValue, JsError> {
    let bundle = render_target_bundle(&fitted_document(target, width_mm, height_mm, dpi))
        .map_err(|e| JsError::new(&e.to_string()))?;
    bundle_to_js(&bundle)
}

fn bundle_to_js(bundle: &GeneratedTargetBundle) -> Result<JsValue, JsError> {
    let obj = js_sys::Object::new();
    let png = js_sys::Uint8Array::new_with_length(bundle.png_bytes.len() as u32);
    png.copy_from(&bundle.png_bytes);
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("json_text"),
        &bundle.json_text.as_str().into(),
    )
    .map_err(|_| JsError::new("failed to set json_text on bundle object"))?;
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("svg_text"),
        &bundle.svg_text.as_str().into(),
    )
    .map_err(|_| JsError::new("failed to set svg_text on bundle object"))?;
    js_sys::Reflect::set(&obj, &JsValue::from_str("png_bytes"), &png.into())
        .map_err(|_| JsError::new("failed to set png_bytes on bundle object"))?;
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("dxf_text"),
        &bundle.dxf_text.as_str().into(),
    )
    .map_err(|_| JsError::new("failed to set dxf_text on bundle object"))?;
    Ok(obj.into())
}

fn chessboard_target_and_extent(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
) -> (TargetSpec, f64, f64) {
    let target = TargetSpec::Chessboard(ChessboardTargetSpec::new(
        inner_rows,
        inner_cols,
        square_size_mm,
    ));
    let w = f64::from(inner_cols + 1) * square_size_mm;
    let h = f64::from(inner_rows + 1) * square_size_mm;
    (target, w, h)
}

fn charuco_target_and_extent(
    rows: u32,
    cols: u32,
    square_size_mm: f64,
    marker_size_rel: f64,
    dictionary_name: &str,
) -> Result<(TargetSpec, f64, f64), JsError> {
    let dictionary = builtin_dictionary(dictionary_name).ok_or_else(|| {
        JsError::new(&format!(
            "unknown dictionary {:?}; call list_aruco_dictionaries() for valid names",
            dictionary_name
        ))
    })?;
    let target = TargetSpec::Charuco(CharucoTargetSpec::new(
        rows,
        cols,
        square_size_mm,
        marker_size_rel,
        dictionary,
    ));
    let w = f64::from(cols) * square_size_mm;
    let h = f64::from(rows) * square_size_mm;
    Ok((target, w, h))
}

fn marker_board_target_and_extent(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
) -> (TargetSpec, f64, f64) {
    let target = TargetSpec::MarkerBoard(MarkerBoardTargetSpec::new(
        inner_rows,
        inner_cols,
        square_size_mm,
        MarkerBoardTargetSpec::default_circles(inner_rows, inner_cols),
    ));
    let w = f64::from(inner_cols + 1) * square_size_mm;
    let h = f64::from(inner_rows + 1) * square_size_mm;
    (target, w, h)
}

fn puzzleboard_target_and_extent(
    rows: u32,
    cols: u32,
    square_size_mm: f64,
) -> (TargetSpec, f64, f64) {
    let target = TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec::new(rows, cols, square_size_mm));
    let w = f64::from(cols) * square_size_mm;
    let h = f64::from(rows) * square_size_mm;
    (target, w, h)
}

/// Synthesise a chessboard target as a full JSON / SVG / PNG / DXF bundle.
///
/// `inner_rows` / `inner_cols` are the **inner-corner** counts (each ≥ 2). The
/// printed board has `(inner_cols + 1) × (inner_rows + 1)` squares of side
/// `square_size_mm`. Returns a `GeneratedTargetBundle` JS object — see the
/// TypeScript type declaration in `typescript-extras.d.ts`.
#[wasm_bindgen]
pub fn render_chessboard_bundle(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<JsValue, JsError> {
    let (target, w, h) = chessboard_target_and_extent(inner_rows, inner_cols, square_size_mm);
    render_bundle_to_js(target, w, h, dpi)
}

/// Synthesise a ChArUco target as a full JSON / SVG / PNG / DXF bundle.
///
/// `rows` / `cols` are **square counts** (≥ 2 each). `marker_size_rel` ∈ (0, 1]
/// sets the marker side length relative to the square; `dictionary_name` is
/// one of [`list_aruco_dictionaries`] (e.g. `"DICT_4X4_50"`). Returns a
/// `GeneratedTargetBundle` JS object.
#[wasm_bindgen]
pub fn render_charuco_bundle(
    rows: u32,
    cols: u32,
    square_size_mm: f64,
    marker_size_rel: f64,
    dictionary_name: &str,
    dpi: u32,
) -> Result<JsValue, JsError> {
    let (target, w, h) =
        charuco_target_and_extent(rows, cols, square_size_mm, marker_size_rel, dictionary_name)?;
    render_bundle_to_js(target, w, h, dpi)
}

/// Synthesise a marker-board target as a full JSON / SVG / PNG / DXF bundle.
///
/// `inner_rows` / `inner_cols` are the **inner-corner** counts. The default
/// 3-circle layout from `MarkerBoardTargetSpec::default_circles` is used; for
/// custom circle placement, call the Rust facade directly. Returns a
/// `GeneratedTargetBundle` JS object.
#[wasm_bindgen]
pub fn render_marker_board_bundle(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<JsValue, JsError> {
    let (target, w, h) = marker_board_target_and_extent(inner_rows, inner_cols, square_size_mm);
    render_bundle_to_js(target, w, h, dpi)
}

/// Synthesise a PuzzleBoard target as a full JSON / SVG / PNG / DXF bundle.
///
/// Returns a `GeneratedTargetBundle` JS object for a `rows × cols` board at
/// the given DPI. Callers that only need the PNG can use
/// [`render_puzzleboard_png`] instead.
#[wasm_bindgen]
pub fn render_puzzleboard_bundle(
    rows: u32,
    cols: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<JsValue, JsError> {
    let (target, w, h) = puzzleboard_target_and_extent(rows, cols, square_size_mm);
    render_bundle_to_js(target, w, h, dpi)
}

/// Synthesise a chessboard target PNG in memory.
///
/// `inner_rows` / `inner_cols` are the **inner-corner** counts (each ≥ 2). The
/// printed board has `(inner_cols + 1) × (inner_rows + 1)` squares of side
/// `square_size_mm`. Returns raw PNG bytes for a tightly-cropped page. Use
/// [`render_chessboard_bundle`] for the full JSON / SVG / PNG / DXF output.
#[wasm_bindgen]
pub fn render_chessboard_png(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<Vec<u8>, JsError> {
    let (target, w, h) = chessboard_target_and_extent(inner_rows, inner_cols, square_size_mm);
    let bundle = render_target_bundle(&fitted_document(target, w, h, dpi))
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(bundle.png_bytes)
}

/// Synthesise a ChArUco target PNG in memory.
///
/// `rows` / `cols` are **square counts** (≥ 2 each). `marker_size_rel` ∈ (0, 1]
/// sets the marker side length relative to the square; `dictionary_name` is
/// one of [`list_aruco_dictionaries`] (e.g. `"DICT_4X4_50"`). Use
/// [`render_charuco_bundle`] for the full JSON / SVG / PNG / DXF output.
#[wasm_bindgen]
pub fn render_charuco_png(
    rows: u32,
    cols: u32,
    square_size_mm: f64,
    marker_size_rel: f64,
    dictionary_name: &str,
    dpi: u32,
) -> Result<Vec<u8>, JsError> {
    let (target, w, h) =
        charuco_target_and_extent(rows, cols, square_size_mm, marker_size_rel, dictionary_name)?;
    let bundle = render_target_bundle(&fitted_document(target, w, h, dpi))
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(bundle.png_bytes)
}

/// Synthesise a marker-board target PNG in memory.
///
/// `inner_rows` / `inner_cols` are the **inner-corner** counts. The default
/// 3-circle layout from `MarkerBoardTargetSpec::default_circles` is used; for
/// custom circle placement, call the Rust facade directly. Use
/// [`render_marker_board_bundle`] for the full JSON / SVG / PNG / DXF output.
#[wasm_bindgen]
pub fn render_marker_board_png(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<Vec<u8>, JsError> {
    let (target, w, h) = marker_board_target_and_extent(inner_rows, inner_cols, square_size_mm);
    let bundle = render_target_bundle(&fitted_document(target, w, h, dpi))
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(bundle.png_bytes)
}

/// Synthesise a PuzzleBoard target PNG in memory.
///
/// Returns the raw PNG bytes for a `rows × cols` board at the given DPI.
/// The caller typically hands these to an `<img>` or `createImageBitmap`
/// for display, then rasterises to a canvas to obtain an RGBA buffer that
/// can be fed back into [`detect_puzzleboard`] for a round-trip demo. Use
/// [`render_puzzleboard_bundle`] for the full JSON / SVG / PNG / DXF output.
#[wasm_bindgen]
pub fn render_puzzleboard_png(
    rows: u32,
    cols: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<Vec<u8>, JsError> {
    let (target, w, h) = puzzleboard_target_and_extent(rows, cols, square_size_mm);
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
    let cfg: DetectorConfig = from_js(chess_cfg)?;
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
    let detector = ChessDetector::new(cb_params).map_err(|e| JsError::new(&e.to_string()))?;
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
    let detector = ChessDetector::new(cb_params).map_err(|e| JsError::new(&e.to_string()))?;
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
    let mut charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    // ChArUco only supports seed-and-grow; re-pin in case the config omitted
    // the (now topological-defaulting) chessboard algorithm.
    charuco_params.chessboard.graph_build_algorithm = GraphBuildAlgorithm::SeedAndGrow;
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
    let detector = MarkerBoardDetector::new(mb_params).map_err(|e| JsError::new(&e.to_string()))?;
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
/// The returned `decode` block carries the compact decode summary.
/// Soft-mode runner-up scoring evidence is available from
/// `detect_puzzleboard_with_diagnostics`.
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
// Diagnostics-channel detection
//
// Each `detect_*_with_diagnostics` runs the detector's `*_with_diagnostics`
// Rust path and returns a `{ result, diagnostics }` JS object. `result` is
// the same payload the corresponding `detect_*` function returns; on a
// failed detection it is `null`. `diagnostics` mirrors the Rust diagnostics
// struct's `serde_json` shape and carries a looser stability promise than
// the result API. See `typescript-extras.d.ts` for the object shapes.
// ---------------------------------------------------------------------------

/// Detect a chessboard grid and additionally return the diagnostics channel.
///
/// Returns a `{ result, diagnostics }` object. `result` is a
/// `ChessboardDetectionResult` (or `null` when no board is found);
/// `diagnostics` is the `ChessboardDebugFrame` introspection payload, which
/// also embeds the detection under its own `detection` field.
#[wasm_bindgen]
pub fn detect_chessboard_with_diagnostics(
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
    let detector = ChessDetector::new(cb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let frame = detector.detect_with_diagnostics(&corners);
    to_js(&serde_json::json!({
        "result": frame.detection,
        "diagnostics": frame,
    }))
}

/// Detect a ChArUco board and additionally return the diagnostics channel.
///
/// Returns a `{ result, diagnostics }` object. `result` is a
/// `CharucoDetectionResult` (or `null` when detection fails);
/// `diagnostics` is the `CharucoDetectDiagnostics` payload — produced even
/// on a failed frame so callers can render the failure mode.
#[wasm_bindgen]
pub fn detect_charuco_with_diagnostics(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let mut charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    // ChArUco only supports seed-and-grow; re-pin in case the config omitted
    // the (now topological-defaulting) chessboard algorithm.
    charuco_params.chessboard.graph_build_algorithm = GraphBuildAlgorithm::SeedAndGrow;
    let chess = resolve_chess_cfg(chess_cfg)?;

    let corners = detect_corners_impl(pixels, width, height, &chess);
    let detector =
        CharucoDetector::new(charuco_params).map_err(|e| JsError::new(&e.to_string()))?;
    let view = make_view(pixels, width, height);
    let (result, diagnostics) = detector.detect_with_diagnostics(&view, &corners);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// Detect a marker board and additionally return the diagnostics channel.
///
/// Returns a `{ result, diagnostics }` object. `result` is a
/// `MarkerBoardDetectionResult` (or `null` when no board is found).
/// `diagnostics` is the `MarkerBoardDiagnostics` payload, or `null` when
/// detection fails — the marker-board diagnostics channel only yields
/// evidence on a successful detection.
#[wasm_bindgen]
pub fn detect_marker_board_with_diagnostics(
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
    let detector = MarkerBoardDetector::new(mb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let view = make_view(pixels, width, height);
    match detector.detect_from_image_and_corners_with_diagnostics(&view, &corners) {
        Some((result, diagnostics)) => to_js(&serde_json::json!({
            "result": result,
            "diagnostics": diagnostics,
        })),
        None => to_js(&serde_json::json!({
            "result": serde_json::Value::Null,
            "diagnostics": serde_json::Value::Null,
        })),
    }
}

/// Detect a PuzzleBoard and additionally return the diagnostics channel.
///
/// Returns a `{ result, diagnostics }` object. `result` is a
/// `PuzzleBoardDetectionResult` (or `null` when detection fails);
/// `diagnostics` is the `PuzzleBoardDiagnostics` payload — produced even on
/// a failed decode so callers can render the sampled edge observations.
#[wasm_bindgen]
pub fn detect_puzzleboard_with_diagnostics(
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
    let (result, diagnostics) = detector.detect_with_diagnostics(&view, &corners);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

// ---------------------------------------------------------------------------
// Multi-config sweep detection
// ---------------------------------------------------------------------------

/// Try multiple chessboard parameter configs, return the best result (most corners).
///
/// Returns a `ChessboardDetectionResult` JS object, or `null` if no board found
/// with any config.
/// If `chess_cfg` is provided, it is used for corner detection across every
/// sweep config; otherwise the workspace-default ChESS settings are used.
#[wasm_bindgen]
pub fn detect_chessboard_best(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: JsValue,
    configs: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let configs: Vec<DetectorParams> = from_js(configs)?;

    // The sweep configs only vary chessboard-detector tuning, not the
    // ChESS corner detector; resolve one ChESS config and reuse the
    // detected corners across every sweep config.
    let chess = resolve_chess_cfg(chess_cfg)?;
    let corners = detect_corners_impl(pixels, width, height, &chess);

    let best = configs
        .iter()
        .filter_map(|params| ChessDetector::new(params.clone()).ok()?.detect(&corners))
        .max_by_key(|d| d.corners.len());
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
    let mut configs: Vec<CharucoParams> = from_js(configs)?;
    // ChArUco only supports seed-and-grow; re-pin in case a config omitted
    // the (now topological-defaulting) chessboard algorithm.
    for cfg in &mut configs {
        cfg.chessboard.graph_build_algorithm = GraphBuildAlgorithm::SeedAndGrow;
    }

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
                    (b.markers.len(), b.corners.len())
                        >= (result.markers.len(), result.corners.len())
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
            let detector = MarkerBoardDetector::new(params.clone()).ok()?;
            let view = make_view(pixels, width, height);
            detector.detect_from_image_and_corners(&view, &corners)
        })
        .max_by_key(|r| r.corners.len());
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
                    let new_key = (result.corners.len(), result.decode.mean_confidence);
                    let old_key = (b.corners.len(), b.decode.mean_confidence);
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
