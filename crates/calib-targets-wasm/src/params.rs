//! Default detector parameter payloads, sweeps, and PuzzlePole period tables.

use calib_targets_aruco::builtins::BUILTIN_DICTIONARY_NAMES;
use calib_targets_charuco::CharucoParams;
use calib_targets_chessboard::ChessboardParams;
use calib_targets_marker::{MarkerBoardParams, MarkerBoardSpec};
use calib_targets_puzzleboard::{
    PuzzleBoardParams, PuzzleBoardSpec, PuzzlePoleParams, PuzzlePoleSpec, SUPPORTED_PERIODS,
};
use wasm_bindgen::prelude::*;

use crate::print::charuco_board_spec;
use crate::{from_js, to_js, workspace_default_chess_cfg};

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
/// [`default_charuco_params`] and [`render_charuco_png`](crate::render_charuco_png).
#[wasm_bindgen]
pub fn list_aruco_dictionaries() -> Result<JsValue, JsError> {
    to_js(&BUILTIN_DICTIONARY_NAMES)
}

// ---------------------------------------------------------------------------
// Multi-config sweep presets
// ---------------------------------------------------------------------------

/// Return the 3-config chessboard sweep preset (`ChessboardParams::sweep_default()`).
///
/// Pass the array directly to [`detect_chessboard_best`](crate::detect_chessboard_best).
#[wasm_bindgen]
pub fn chessboard_sweep_default() -> Result<JsValue, JsError> {
    to_js(&ChessboardParams::sweep_default())
}

/// Return the ChArUco sweep preset for a given board (`CharucoParams::sweep_for_board(&spec)`).
///
/// Pass the array directly to [`detect_charuco_best`](crate::detect_charuco_best).
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
/// Pass the array directly to [`detect_puzzleboard_best`](crate::detect_puzzleboard_best).
#[wasm_bindgen]
pub fn puzzleboard_sweep_for_board(rows: u32, cols: u32) -> Result<JsValue, JsError> {
    let spec = PuzzleBoardSpec::new(rows, cols, 1.0).map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&PuzzleBoardParams::sweep_for_board(&spec))
}

/// Return default `PuzzlePoleParams` for a pole as a JS object.
#[wasm_bindgen]
pub fn default_puzzlepole_params(
    circumference_squares: u32,
    axial_squares: u32,
    cell_size_mm: f32,
) -> Result<JsValue, JsError> {
    let spec = PuzzlePoleSpec::new(circumference_squares, axial_squares, cell_size_mm)
        .map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&PuzzlePoleParams::for_pole(spec))
}

/// Return the PuzzlePole sweep preset for a given pole spec
/// (`PuzzlePoleParams::sweep_for_pole(&pole)`).
///
/// Pass the array directly to [`detect_puzzlepole_best`](crate::detect_puzzlepole_best).
#[wasm_bindgen]
pub fn puzzlepole_sweep_for_pole(pole: JsValue) -> Result<JsValue, JsError> {
    let spec: PuzzlePoleSpec = from_js(pole)?;
    to_js(&PuzzlePoleParams::sweep_for_pole(&spec))
}

/// Return every pole spec of this shape that can be cut from the master
/// without sharing a corner with another (`PuzzlePoleSpec::distinct_poles`).
#[wasm_bindgen]
pub fn puzzlepole_distinct_poles(
    circumference_squares: u32,
    axial_squares: u32,
    cell_size_mm: f32,
) -> Result<JsValue, JsError> {
    let poles = PuzzlePoleSpec::distinct_poles(circumference_squares, axial_squares, cell_size_mm)
        .map_err(|e| JsError::new(&e.to_string()))?;
    to_js(&poles)
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
/// Pass the array directly to [`detect_marker_board_best`](crate::detect_marker_board_best).
#[wasm_bindgen]
pub fn marker_board_sweep_for_board(spec: JsValue) -> Result<JsValue, JsError> {
    let spec: MarkerBoardSpec = from_js(spec)?;
    to_js(&MarkerBoardParams::sweep_for_board(&spec))
}

/// Every `[circumference_squares, start_row]` pair that closes seamlessly.
///
/// Exposed rather than duplicated in TypeScript: the table is derived from the
/// shipped code maps and pinned by a Rust test, so a copy could drift from the
/// pattern it describes.
#[wasm_bindgen]
pub fn puzzlepole_periods() -> Result<JsValue, JsError> {
    let pairs: Vec<[u32; 2]> = SUPPORTED_PERIODS
        .iter()
        .map(|p| [p.squares, p.start_row])
        .collect();
    to_js(&pairs)
}
