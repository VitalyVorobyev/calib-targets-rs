//! WebAssembly bindings for `calib-targets` calibration target detectors.
//!
//! Exposes stateless detection functions that accept grayscale `&[u8]` buffers
//! and JS config objects (deserialized via `serde-wasm-bindgen`).

mod gray;

use calib_targets_aruco::builtins::{builtin_dictionary, BUILTIN_DICTIONARY_NAMES};
use calib_targets_charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams};
use calib_targets_chessboard::ChessCorner;
use calib_targets_chessboard::{ChessboardDetector as ChessDetector, ChessboardParams};
use calib_targets_core::DetectorConfig;
use calib_targets_marker::{MarkerBoardDetector, MarkerBoardParams, MarkerBoardSpec};
use calib_targets_print::{
    render_target_bundle, CharucoTargetSpec, ChessboardTargetSpec, GeneratedTargetBundle,
    MarkerBoardTargetSpec, PageSize, PageSpec, PrintableTargetDocument, PuzzleBoardTargetSpec,
    RenderOptions, TargetSpec,
};
use calib_targets_puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use wasm_bindgen::prelude::*;

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

/// The workspace-default ChESS detector config.
///
/// Delegates to `calib_targets_core::default_chess_config` rather than
/// restating the threshold override, so the value has one definition.
fn workspace_default_chess_cfg() -> DetectorConfig {
    calib_targets_core::default_chess_config()
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

/// Apply an explicit `chess_cfg` argument over a params struct's own `chess`
/// field, leaving it untouched when JS supplies `undefined` / `null`.
fn apply_chess_cfg_override(dst: &mut DetectorConfig, chess_cfg: JsValue) -> Result<(), JsError> {
    if !chess_cfg.is_undefined() && !chess_cfg.is_null() {
        *dst = from_js(chess_cfg)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Default configs (exported so the JS side can populate UI with defaults)
// ---------------------------------------------------------------------------

/// Return the default `DetectorConfig` as a JS object.
#[wasm_bindgen]
pub fn default_chess_config() -> Result<JsValue, JsError> {
    to_js(&workspace_default_chess_cfg())
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

/// Return default `PuzzleBoardParams` for a `rows × cols` board as a JS object.
///
/// The returned payload includes the PuzzleBoard decode sub-config, with
/// `search_mode = {"kind": "full"}`,
/// `scoring_mode = {"kind": "soft_log_likelihood"}`, and
/// `symmetry_mode = {"kind": "rotations"}` by default.
#[wasm_bindgen]
pub fn default_puzzleboard_params(rows: u32, cols: u32) -> Result<JsValue, JsError> {
    let spec = PuzzleBoardSpec::new(rows, cols, 1.0).map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&PuzzleBoardParams::for_board(spec))
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
    to_js(&CharucoParams::for_board(spec))
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

/// Return the 3-config chessboard sweep preset (`ChessboardParams::sweep_default()`).
///
/// Pass the array directly to [`detect_chessboard_best`].
#[wasm_bindgen]
pub fn chessboard_sweep_default() -> Result<JsValue, JsError> {
    to_js(&ChessboardParams::sweep_default())
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

/// Return the marker-board sweep preset for a given board
/// (`MarkerBoardParams::sweep_for_board(&spec)`).
///
/// Unlike [`charuco_sweep_for_board`] / [`puzzleboard_sweep_for_board`], a
/// marker-board layout is not reducible to a `(rows, cols)` pair — the three
/// circle placements are load-bearing — so this takes the full
/// `MarkerBoardSpec` JS object (start from `default_marker_board_params().board`
/// and override `rows` / `cols` / `circles`).
///
/// Pass the array directly to [`detect_marker_board_best`].
#[wasm_bindgen]
pub fn marker_board_sweep_for_board(spec: JsValue) -> Result<JsValue, JsError> {
    let spec: MarkerBoardSpec = from_js(spec)?;
    to_js(&MarkerBoardParams::sweep_for_board(&spec))
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

/// Render a complete `PrintableTargetDocument` into a JSON / SVG / PNG / DXF
/// bundle.
///
/// The `render_*_bundle` helpers below each take a fixed argument list and
/// wrap the spec in a page sized to fit the board. That is the right default
/// for a quick preview, but it puts page size, orientation, margin and every
/// spec field the helper does not name out of reach — a host application that
/// prints on Letter, or wants `border_bits = 2`, cannot express it and has to
/// reimplement the renderer to get there. This entry point takes the document
/// the Rust API already models, so the WASM surface stops being narrower than
/// the library behind it.
///
/// `doc` is the same `schema_version: 1` JSON the CLI reads and
/// `testdata/printable/*.json` holds:
///
/// ```json
/// {
///   "schema_version": 1,
///   "target": { "kind": "charuco", "rows": 8, "cols": 11, "square_size_mm": 20.0,
///               "marker_size_rel": 0.75, "dictionary": "DICT_4X4_250",
///               "marker_layout": "opencv_charuco", "border_bits": 1 },
///   "page":   { "size": { "kind": "a4" }, "orientation": "portrait", "margin_mm": 10.0 },
///   "render": { "debug_annotations": false, "png_dpi": 300 }
/// }
/// ```
///
/// Returns a `GeneratedTargetBundle` JS object. Throws when the document
/// fails to deserialise or the target spec is invalid (e.g. a board too small,
/// or one needing more markers than the dictionary holds).
#[wasm_bindgen]
pub fn render_target_bundle_json(doc: JsValue) -> Result<JsValue, JsError> {
    let doc: PrintableTargetDocument = from_js(doc)?;
    let bundle = render_target_bundle(&doc).map_err(|e| JsError::new(&e.to_string()))?;
    bundle_to_js(&bundle)
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
    let view = make_view(pixels, width, height);
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
    validate_gray(pixels, width, height)?;
    let cb_params: ChessboardParams = from_js(params)?;
    let chess = resolve_chess_cfg(chess_cfg)?;

    let view = make_view(pixels, width, height);
    let corners = calib_targets_chessboard::detect_corners(&view, &chess);
    let detector = ChessDetector::new(cb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let result = detector.detect(&corners);
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
    validate_gray(pixels, width, height)?;
    let cb_params: ChessboardParams = from_js(params)?;
    let chess = resolve_chess_cfg(chess_cfg)?;

    let view = make_view(pixels, width, height);
    let corners = calib_targets_chessboard::detect_corners(&view, &chess);
    let detector = ChessDetector::new(cb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let results = detector.detect_all(&corners);
    to_js(&results)
}

// ---------------------------------------------------------------------------
// ChArUco detection
// ---------------------------------------------------------------------------

/// Detect a ChArUco board in a grayscale image.
///
/// Returns a `CharucoDetection` JS object. Throws on error.
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
    validate_gray(pixels, width, height)?;
    let mut charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    apply_chess_cfg_override(&mut charuco_params.chess, chess_cfg)?;

    let img = calib_targets::detect::gray_image_from_slice(width, height, pixels)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let result = calib_targets::detect::detect_charuco(&img, &charuco_params)
        .map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&result)
}

/// Detect a ChArUco board from a pre-detected corner cloud (see
/// [`detect_corners`]).
///
/// Returns a `CharucoDetection` JS object. Throws on error. `params.chess`
/// is not read: `corners` is taken as given.
#[wasm_bindgen]
pub fn detect_charuco_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    let img = calib_targets::detect::gray_image_from_slice(width, height, pixels)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let result =
        calib_targets::detect::detect_charuco_with_corners(&img, &corners, &charuco_params)
            .map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&result)
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
    validate_gray(pixels, width, height)?;
    let mut mb_params: MarkerBoardParams = from_js(params)?;
    apply_chess_cfg_override(&mut mb_params.chess, chess_cfg)?;
    let chess = mb_params.chess;

    let view = make_view(pixels, width, height);
    let corners = calib_targets_chessboard::detect_corners(&view, &chess);
    let detector = MarkerBoardDetector::new(mb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let result = detector.detect_with_corners(&view, &corners).ok();
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
    validate_gray(pixels, width, height)?;
    let mb_params: MarkerBoardParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    let detector = MarkerBoardDetector::new(mb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let view = make_view(pixels, width, height);
    let result = detector.detect_with_corners(&view, &corners).ok();
    to_js(&result)
}

// ---------------------------------------------------------------------------
// PuzzleBoard detection
// ---------------------------------------------------------------------------

/// Detect a PuzzleBoard in a grayscale image.
///
/// Returns a `PuzzleBoardDetection` JS object. Throws on error.
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
    validate_gray(pixels, width, height)?;
    let mut puzzle_params: PuzzleBoardParams = from_js(params)?;
    apply_chess_cfg_override(&mut puzzle_params.chess, chess_cfg)?;

    let img = calib_targets::detect::gray_image_from_slice(width, height, pixels)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let result = calib_targets::detect::detect_puzzleboard(&img, &puzzle_params)
        .map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&result)
}

/// Detect a PuzzleBoard from a pre-detected corner cloud (see
/// [`detect_corners`]).
///
/// Returns a `PuzzleBoardDetection` JS object. Throws on error.
/// `params.chess` is not read: `corners` is taken as given.
#[wasm_bindgen]
pub fn detect_puzzleboard_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let puzzle_params: PuzzleBoardParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    let img = calib_targets::detect::gray_image_from_slice(width, height, pixels)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let result =
        calib_targets::detect::detect_puzzleboard_with_corners(&img, &corners, &puzzle_params)
            .map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&result)
}

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
    validate_gray(pixels, width, height)?;
    let mut charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    apply_chess_cfg_override(&mut charuco_params.chess, chess_cfg)?;
    let chess = charuco_params.chess;

    let view = make_view(pixels, width, height);
    let corners = calib_targets_chessboard::detect_corners(&view, &chess);
    let detector =
        CharucoDetector::new(charuco_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose_with_corners(&view, &corners);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// [`diagnose_charuco`] from a pre-detected corner cloud (see
/// [`detect_corners`]). `params.chess` is not read: `corners` is taken as
/// given.
#[wasm_bindgen]
pub fn diagnose_charuco_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let charuco_params: calib_targets_charuco::CharucoParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    let detector =
        CharucoDetector::new(charuco_params).map_err(|e| JsError::new(&e.to_string()))?;
    let view = make_view(pixels, width, height);
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
    validate_gray(pixels, width, height)?;
    let mut mb_params: MarkerBoardParams = from_js(params)?;
    apply_chess_cfg_override(&mut mb_params.chess, chess_cfg)?;
    let chess = mb_params.chess;

    let view = make_view(pixels, width, height);
    let corners = calib_targets_chessboard::detect_corners(&view, &chess);
    let detector = MarkerBoardDetector::new(mb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose_with_corners(&view, &corners);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// [`diagnose_marker_board`] from a pre-detected corner cloud (see
/// [`detect_corners`]). `params.chess` is not read: `corners` is taken as
/// given.
#[wasm_bindgen]
pub fn diagnose_marker_board_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let mb_params: MarkerBoardParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    let detector = MarkerBoardDetector::new(mb_params).map_err(|e| JsError::new(&e.to_string()))?;
    let view = make_view(pixels, width, height);
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
    validate_gray(pixels, width, height)?;
    let mut puzzle_params: PuzzleBoardParams = from_js(params)?;
    apply_chess_cfg_override(&mut puzzle_params.chess, chess_cfg)?;
    let chess = puzzle_params.chess;

    let view = make_view(pixels, width, height);
    let corners = calib_targets_chessboard::detect_corners(&view, &chess);
    let detector =
        PuzzleBoardDetector::new(puzzle_params).map_err(|e| JsError::new(&e.to_string()))?;
    let (result, diagnostics) = detector.diagnose_with_corners(&view, &corners);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

/// [`diagnose_puzzleboard`] from a pre-detected corner cloud (see
/// [`detect_corners`]). `params.chess` is not read: `corners` is taken as
/// given.
#[wasm_bindgen]
pub fn diagnose_puzzleboard_with_corners(
    width: u32,
    height: u32,
    pixels: &[u8],
    corners: JsValue,
    params: JsValue,
) -> Result<JsValue, JsError> {
    validate_gray(pixels, width, height)?;
    let puzzle_params: PuzzleBoardParams = from_js(params)?;
    let corners: Vec<ChessCorner> = from_js(corners)?;

    let detector =
        PuzzleBoardDetector::new(puzzle_params).map_err(|e| JsError::new(&e.to_string()))?;
    let view = make_view(pixels, width, height);
    let (result, diagnostics) = detector.diagnose_with_corners(&view, &corners);
    to_js(&serde_json::json!({
        "result": result.ok(),
        "diagnostics": diagnostics,
    }))
}

// ---------------------------------------------------------------------------
// Multi-config sweep detection
// ---------------------------------------------------------------------------

// Each `detect_*_best` deserialises its JS config array into `Vec<{X}Params>`,
// decodes the image once, and delegates to the matching
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
    validate_gray(pixels, width, height)?;
    let configs: Vec<ChessboardParams> = from_js(configs)?;
    let chess = resolve_chess_cfg(chess_cfg)?;
    let img = calib_targets::detect::gray_image_from_slice(width, height, pixels)
        .map_err(|e| JsError::new(&e.to_string()))?;
    // The facade now returns `Result`; a miss (`DetectError::NoDetection`)
    // maps back to `null`, exactly as this entry point returned before.
    let best = calib_targets::detect::detect_chessboard_best(&img, &chess, &configs).ok();
    to_js(&best)
}

/// Try multiple ChArUco parameter configs, return the best result
/// (most markers, then most corners). Throws if all configs fail.
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
    validate_gray(pixels, width, height)?;
    let configs: Vec<CharucoParams> = from_js(configs)?;
    let img = calib_targets::detect::gray_image_from_slice(width, height, pixels)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let result = calib_targets::detect::detect_charuco_best(&img, &configs)
        .map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&result)
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
    validate_gray(pixels, width, height)?;
    let configs: Vec<MarkerBoardParams> = from_js(configs)?;
    let img = calib_targets::detect::gray_image_from_slice(width, height, pixels)
        .map_err(|e| JsError::new(&e.to_string()))?;
    // The facade now returns `Result`; a miss (`DetectError::NoDetection`)
    // maps back to `null`, exactly as this entry point returned before.
    let best = calib_targets::detect::detect_marker_board_best(&img, &configs).ok();
    to_js(&best)
}

/// Try multiple PuzzleBoard parameter configs, return the best result
/// (most labelled corners, then mean bit confidence). Throws if all configs fail.
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
    validate_gray(pixels, width, height)?;
    let configs: Vec<PuzzleBoardParams> = from_js(configs)?;
    let img = calib_targets::detect::gray_image_from_slice(width, height, pixels)
        .map_err(|e| JsError::new(&e.to_string()))?;
    let result = calib_targets::detect::detect_puzzleboard_best(&img, &configs)
        .map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&result)
}
