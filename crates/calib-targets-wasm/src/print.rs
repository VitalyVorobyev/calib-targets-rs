//! Printable target generation (bundle JSON/SVG/PNG/DXF and 1:1 screen PNG).

use calib_targets_aruco::builtins::builtin_dictionary;
use calib_targets_charuco::CharucoBoardSpec;
use calib_targets_print::{
    render_screen_target as render_screen, render_target_bundle, CharucoTargetSpec,
    ChessboardTargetSpec, GeneratedTargetBundle, MarkerBoardTargetSpec, PageSize, PageSpec,
    PrintableTargetDocument, PuzzleBoardTargetSpec, PuzzlePoleTargetSpec, RenderOptions,
    TargetSpec,
};
use wasm_bindgen::prelude::*;

use crate::from_js;

// ---------------------------------------------------------------------------
// Synthetic target generation
// ---------------------------------------------------------------------------

/// Build a `CharucoBoardSpec` from JS-friendly arguments.
pub(crate) fn charuco_board_spec(
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
    let obj = png_json_to_js(&bundle.png_bytes, &bundle.json_text)?;
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("svg_text"),
        &bundle.svg_text.as_str().into(),
    )
    .map_err(|_| JsError::new("failed to set svg_text on bundle object"))?;
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("dxf_text"),
        &bundle.dxf_text.as_str().into(),
    )
    .map_err(|_| JsError::new("failed to set dxf_text on bundle object"))?;
    Ok(obj.into())
}

fn png_json_to_js(bytes: &[u8], json: &str) -> Result<js_sys::Object, JsError> {
    let obj = js_sys::Object::new();
    let png = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
    png.copy_from(bytes);
    js_sys::Reflect::set(&obj, &JsValue::from_str("json_text"), &json.into())
        .map_err(|_| JsError::new("failed to set json_text on bundle object"))?;
    js_sys::Reflect::set(&obj, &JsValue::from_str("png_bytes"), &png.into())
        .map_err(|_| JsError::new("failed to set png_bytes on bundle object"))?;
    Ok(obj)
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

/// Build a PuzzlePole wrap-strip target and its board extent.
///
/// Returns `None` when the circumference is not a period that closes
/// seamlessly. A pole's diameter is quantised, so there is no nearest fit to
/// fall back to — rounding would produce a target whose ends do not meet.
fn puzzlepole_target_and_extent(
    circumference_squares: u32,
    axial_squares: u32,
    square_size_mm: f64,
) -> Option<(TargetSpec, f64, f64)> {
    let spec = PuzzlePoleTargetSpec::new(circumference_squares, axial_squares, square_size_mm)?;
    // Page x is the cylinder axis, page y the circumference — and the strip is
    // two pieces longer than it wraps, for the trim and the glue overlap.
    let w = f64::from(axial_squares) * square_size_mm;
    let h = f64::from(spec.printed_strip_squares()) * square_size_mm;
    Some((TargetSpec::PuzzlePole(spec), w, h))
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

/// Fit and render a Chessboard, ChArUco, or PuzzleBoard for 1:1 screen display.
///
/// Takes a target spec and native screen options and returns the 8-bit
/// grayscale PNG as a `Uint8Array` plus the calibration sidecar as JSON text.
#[wasm_bindgen]
pub fn render_screen_target(target: JsValue, screen: JsValue) -> Result<JsValue, JsError> {
    let target = from_js(target)?;
    let screen = from_js(screen)?;
    let generated = render_screen(&target, &screen).map_err(|e| JsError::new(&e.to_string()))?;
    let json = generated
        .metadata
        .to_json_pretty()
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(png_json_to_js(&generated.png_bytes, &json)?.into())
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
/// one of [`list_aruco_dictionaries`](crate::list_aruco_dictionaries) (e.g. `"DICT_4X4_50"`). Returns a
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

/// Render a PuzzlePole wrap strip into a JSON / SVG / PNG / DXF bundle.
///
/// Throws when `circumference_squares` is not a supported period; call
/// [`puzzlepole_periods`](crate::puzzlepole_periods) for the ones that are.
#[wasm_bindgen]
pub fn render_puzzlepole_bundle(
    circumference_squares: u32,
    axial_squares: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<JsValue, JsError> {
    let (target, w, h) =
        puzzlepole_target_and_extent(circumference_squares, axial_squares, square_size_mm)
            .ok_or_else(|| unsupported_period(circumference_squares))?;
    render_bundle_to_js(target, w, h, dpi)
}

/// Synthesise a PuzzlePole wrap-strip PNG in memory.
#[wasm_bindgen]
pub fn render_puzzlepole_png(
    circumference_squares: u32,
    axial_squares: u32,
    square_size_mm: f64,
    dpi: u32,
) -> Result<Vec<u8>, JsError> {
    let (target, w, h) =
        puzzlepole_target_and_extent(circumference_squares, axial_squares, square_size_mm)
            .ok_or_else(|| unsupported_period(circumference_squares))?;
    let bundle = render_target_bundle(&fitted_document(target, w, h, dpi))
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(bundle.png_bytes)
}

fn unsupported_period(circumference_squares: u32) -> JsError {
    JsError::new(&format!(
        "{circumference_squares} pieces around the circumference is not a PuzzlePole period \
         that closes seamlessly; call puzzlepole_periods() for the supported ones"
    ))
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
/// one of [`list_aruco_dictionaries`](crate::list_aruco_dictionaries) (e.g. `"DICT_4X4_50"`). Use
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
/// can be fed back into [`detect_puzzleboard`](crate::detect_puzzleboard) for a round-trip demo. Use
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
