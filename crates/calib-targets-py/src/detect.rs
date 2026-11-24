use ::calib_targets::{charuco, chessboard, detect, marker, puzzleboard};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyList;
use serde::Serialize;
use serde_json::Value;

use crate::{
    apply_chess_cfg_override, charuco_params_from_py, chess_cfg_from_py, chess_corners_from_py,
    chessboard_params_from_py, detection_result_to_py, from_py_json, gray_image_from_py,
    json_to_py, marker_board_params_from_py, puzzleboard_params_from_py, sweep_preset_to_py,
    value_error,
};

#[derive(Serialize)]
struct TopologicalCornerPayload {
    index: usize,
    position: [f32; 2],
    position_covariance: Option<::calib_targets::core::PositionCovariance>,
    axes: [::calib_targets::core::AxisEstimate; 2],
    strength: f32,
}

/// Detect a ChArUco board in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: dict with DetectorConfig fields, or None for defaults.
///     If provided, overrides `params.chessboard.chess`.
///   params: dict with CharucoParams fields (must include `board`).
///
/// Returns:
///   dict with detection data, or None when no board is found. Raises
///   ValueError on an invalid image, board spec or parameters.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params))]
pub(crate) fn detect_charuco(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let mut params = charuco_params_from_py(Some(params))?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;
    let result = py.detach(move || detect::detect_charuco(&img.view(), &params));
    detection_result_to_py(py, result)
}

/// Detect a ChArUco board from a pre-detected corner cloud.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   corners: list of `ChessCorner`-shaped dicts (see
///     `trace_chessboard_topological`'s `"corners"` output, or a custom
///     upstream). `params.chess` is not read: `corners` is taken as given.
///   params: dict with CharucoParams fields (must include `board`).
///
/// Returns:
///   dict with detection data, or None when no board is found. Raises
///   ValueError on an invalid image, board spec or parameters.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params))]
pub(crate) fn detect_charuco_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = charuco_params_from_py(Some(params))?;
    let result =
        py.detach(move || detect::detect_charuco_with_corners(&img.view(), &corners, &params));
    detection_result_to_py(py, result)
}

/// Detect a chessboard in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: dict with DetectorConfig fields, or None for defaults.
///     If provided, overrides `params.chess`.
///   params: dict with ChessboardParams fields, or None for defaults.
///
/// Returns:
///   dict with detection data, or None if no board is found.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
pub(crate) fn detect_chessboard(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let params = chessboard_params_from_py(params)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;

    // The facade helper also records the frame's noise level on the result's
    // `quality`.
    let result = py.detach(move || detect::detect_chessboard(&img.view(), &chess_cfg, &params));
    detection_result_to_py(py, result)
}

/// Detect all chessboard components in a grayscale image.
///
/// Like `detect_chessboard` but returns every same-board component the detector
/// recovers (up to `params.max_components`), rather than just the first one.
/// Useful when the board is partially occluded and multiple disjoint patches
/// are visible.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: dict with DetectorConfig fields, or None for defaults.
///     If provided, overrides `params.chess`.
///   params: dict with ChessboardParams fields, or None for defaults.
///
/// Returns:
///   list of dicts, each with the `ChessboardDetectionResult` schema.
///   Empty list if no board components are found.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
pub(crate) fn detect_chessboard_all(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let params = chessboard_params_from_py(params)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;

    let results =
        py.detach(move || detect::detect_chessboard_all(&img.view(), &chess_cfg, &params));
    let json =
        serde_json::to_value(results).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

/// Run ChESS corner detection plus the topological grid trace.
///
/// This is an offline diagnostics / visualization entry point. It applies
/// the topological detector defaults, then returns the raw corners, the
/// `projective-grid` topological trace when at least three usable corners are
/// available, and the final merged detections produced by the chessboard
/// detector.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
pub(crate) fn trace_chessboard_topological(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let width = img.width as u32;
    let height = img.height as u32;
    let params = chessboard_params_from_py(params)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;

    let payload = py.detach(move || -> Result<Value, String> {
        let corners = detect::detect_corners(&img.view(), &chess_cfg);
        let corner_payload: Vec<TopologicalCornerPayload> = corners
            .iter()
            .enumerate()
            .map(|(index, c)| TopologicalCornerPayload {
                index,
                position: [c.position.x, c.position.y],
                position_covariance: c.position_covariance,
                axes: c.axes,
                strength: c.strength,
            })
            .collect();

        let trace_result = chessboard::trace_topological_detection(&corners, &params);

        let mut payload = serde_json::json!({
            "schema": 1,
            "image": {
                "width": width,
                "height": height,
            },
            "corners": corner_payload,
        });
        let obj = payload
            .as_object_mut()
            .expect("topological trace payload is an object");
        match trace_result {
            Ok(trace) => {
                obj.insert(
                    "trace".to_string(),
                    serde_json::to_value(&trace.projective_grid).map_err(|e| e.to_string())?,
                );
                obj.insert(
                    "chessboard_stages".to_string(),
                    serde_json::to_value(&trace.chessboard).map_err(|e| e.to_string())?,
                );
                obj.insert(
                    "detections".to_string(),
                    serde_json::to_value(&trace.detections).map_err(|e| e.to_string())?,
                );
                obj.insert("error".to_string(), Value::Null);
            }
            Err(err) => {
                obj.insert("trace".to_string(), Value::Null);
                obj.insert("chessboard_stages".to_string(), Value::Null);
                obj.insert("detections".to_string(), Value::Array(Vec::new()));
                obj.insert("error".to_string(), Value::String(err.to_string()));
            }
        }
        Ok(payload)
    });
    let payload = payload.map_err(PyRuntimeError::new_err)?;
    json_to_py(py, &payload)
}

/// Detect a marker-board target in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: dict with DetectorConfig fields, or None for defaults.
///     If provided, overrides `params.chessboard.chess`.
///   params: dict with MarkerBoardParams fields, or None for defaults.
///
/// Returns:
///   dict with detection data, or None if no board is found.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
pub(crate) fn detect_marker_board(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let mut params = marker_board_params_from_py(params)?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let result = py.detach(move || detect::detect_marker_board(&img.view(), &params));
    detection_result_to_py(py, result)
}

/// Detect a marker-board target from a pre-detected corner cloud.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   corners: list of `ChessCorner`-shaped dicts (see
///     `trace_chessboard_topological`'s `"corners"` output, or a custom
///     upstream). `params.chess` is not read: `corners` is taken as given.
///   params: dict with MarkerBoardParams fields, or None for defaults.
///
/// Returns:
///   dict with detection data, or None if no board is found.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params=None))]
pub(crate) fn detect_marker_board_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = marker_board_params_from_py(params)?;

    let result =
        py.detach(move || detect::detect_marker_board_with_corners(&img.view(), &corners, &params));
    detection_result_to_py(py, result)
}

/// Detect a PuzzleBoard in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: dict with DetectorConfig fields, or None for defaults.
///     If provided, overrides `params.chessboard.chess`.
///   params: dict with PuzzleBoardParams fields (must include `board`).
///
/// Returns:
///   dict with detection data. Raises RuntimeError on detection errors.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params))]
pub(crate) fn detect_puzzleboard(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let mut params = puzzleboard_params_from_py(Some(params))?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let result = py.detach(move || detect::detect_puzzleboard(&img.view(), &params));
    detection_result_to_py(py, result)
}

/// Detect a PuzzleBoard from a pre-detected corner cloud.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   corners: list of `ChessCorner`-shaped dicts (see
///     `trace_chessboard_topological`'s `"corners"` output, or a custom
///     upstream). `params.chess` is not read: `corners` is taken as given.
///   params: dict with PuzzleBoardParams fields (must include `board`).
///
/// Returns:
///   dict with detection data. Raises RuntimeError on detection errors.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params))]
pub(crate) fn detect_puzzleboard_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = puzzleboard_params_from_py(Some(params))?;

    let result =
        py.detach(move || detect::detect_puzzleboard_with_corners(&img.view(), &corners, &params));
    detection_result_to_py(py, result)
}

// ---------------------------------------------------------------------------
// Multi-config sweep detection
// ---------------------------------------------------------------------------

/// Try multiple chessboard parameter configs, return the best result (most corners).
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   configs: list of dicts with ChessboardParams fields.
///   chess_cfg: dict with DetectorConfig fields, or None for defaults.
///     If provided, overrides `params.chess`.
///
/// Returns:
///   dict with detection data, or None if no board is found with any config.
#[pyfunction]
#[pyo3(signature = (image, configs, *, chess_cfg=None))]
pub(crate) fn detect_chessboard_best(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    configs: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let list = configs
        .cast::<PyList>()
        .map_err(|_| value_error("configs must be a list"))?;
    let mut params_vec = Vec::with_capacity(list.len());
    for item in list.iter() {
        params_vec.push(from_py_json::<chessboard::ChessboardParams>(
            &item,
            "configs[]",
        )?);
    }
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;

    let result =
        py.detach(move || detect::detect_chessboard_best(&img.view(), &chess_cfg, &params_vec));
    detection_result_to_py(py, result)
}

/// Try multiple ChArUco parameter configs, return the best result
/// (most markers, then most corners).
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   configs: list of dicts with CharucoParams fields.
///
/// Returns:
///   dict with detection data. Raises RuntimeError if all configs fail.
#[pyfunction]
#[pyo3(signature = (image, configs))]
pub(crate) fn detect_charuco_best(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    configs: &Bound<'_, PyAny>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let list = configs
        .cast::<PyList>()
        .map_err(|_| value_error("configs must be a list"))?;
    let mut params_vec = Vec::with_capacity(list.len());
    for item in list.iter() {
        let cfg = from_py_json::<charuco::CharucoParams>(&item, "configs[]")?;
        params_vec.push(cfg);
    }

    let result = py.detach(move || detect::detect_charuco_best(&img.view(), &params_vec));
    detection_result_to_py(py, result)
}

/// Try multiple marker board parameter configs, return the best result (most corners).
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   configs: list of dicts with MarkerBoardParams fields.
///
/// Returns:
///   dict with detection data, or None if no board is found with any config.
#[pyfunction]
#[pyo3(signature = (image, configs))]
pub(crate) fn detect_marker_board_best(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    configs: &Bound<'_, PyAny>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let list = configs
        .cast::<PyList>()
        .map_err(|_| value_error("configs must be a list"))?;
    let mut params_vec = Vec::with_capacity(list.len());
    for item in list.iter() {
        params_vec.push(from_py_json::<marker::MarkerBoardParams>(
            &item,
            "configs[]",
        )?);
    }

    let result = py.detach(move || detect::detect_marker_board_best(&img.view(), &params_vec));
    detection_result_to_py(py, result)
}

/// Try multiple PuzzleBoard parameter configs, return the best result
/// (most labelled corners, then mean bit confidence).
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   configs: list of dicts with PuzzleBoardParams fields.
///
/// Returns:
///   dict with detection data. Raises RuntimeError if all configs fail.
#[pyfunction]
#[pyo3(signature = (image, configs))]
pub(crate) fn detect_puzzleboard_best(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    configs: &Bound<'_, PyAny>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let list = configs
        .cast::<PyList>()
        .map_err(|_| value_error("configs must be a list"))?;
    let mut params_vec = Vec::with_capacity(list.len());
    for item in list.iter() {
        params_vec.push(from_py_json::<puzzleboard::PuzzleBoardParams>(
            &item,
            "configs[]",
        )?);
    }

    let result = py.detach(move || detect::detect_puzzleboard_best(&img.view(), &params_vec));
    detection_result_to_py(py, result)
}

// ---------------------------------------------------------------------------
// Multi-config sweep presets
// ---------------------------------------------------------------------------

/// Return the chessboard sweep preset (`ChessboardParams::sweep_default()`).
///
/// Three configs: the canonical tolerances plus a tighter and a looser
/// grid-graph angular bracket. Pass the list to `detect_chessboard_best`.
///
/// Returns:
///   list of dicts with ChessboardParams fields.
#[pyfunction]
#[pyo3(signature = ())]
pub(crate) fn chessboard_sweep_default(py: Python<'_>) -> PyResult<Py<PyAny>> {
    sweep_preset_to_py(py, &chessboard::ChessboardParams::sweep_default())
}

/// Return the ChArUco sweep preset (`CharucoParams::sweep_for_board(&spec)`).
///
/// Args:
///   board: dict with CharucoBoardSpec fields.
///
/// Returns:
///   list of dicts with CharucoParams fields.
#[pyfunction]
#[pyo3(signature = (board))]
pub(crate) fn charuco_sweep_for_board(
    py: Python<'_>,
    board: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let spec: charuco::CharucoBoardSpec = from_py_json(board, "board")?;
    sweep_preset_to_py(py, &charuco::CharucoParams::sweep_for_board(&spec))
}

/// Return the marker-board sweep preset
/// (`MarkerBoardParams::sweep_for_board(&spec)`).
///
/// Takes the full board spec rather than a `(rows, cols)` pair: a marker-board
/// layout is not reducible to a size, since the three circle placements are
/// load-bearing.
///
/// Args:
///   board: dict with MarkerBoardSpec fields.
///
/// Returns:
///   list of dicts with MarkerBoardParams fields.
#[pyfunction]
#[pyo3(signature = (board))]
pub(crate) fn marker_board_sweep_for_board(
    py: Python<'_>,
    board: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let spec: marker::MarkerBoardSpec = from_py_json(board, "board")?;
    sweep_preset_to_py(py, &marker::MarkerBoardParams::sweep_for_board(&spec))
}

/// Return the PuzzleBoard sweep preset
/// (`PuzzleBoardParams::sweep_for_board(&spec)`).
///
/// Takes the full board spec so the caller's `cell_size` and board origin
/// travel into every returned config.
///
/// Args:
///   board: dict with PuzzleBoardSpec fields.
///
/// Returns:
///   list of dicts with PuzzleBoardParams fields.
#[pyfunction]
#[pyo3(signature = (board))]
pub(crate) fn puzzleboard_sweep_for_board(
    py: Python<'_>,
    board: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let spec: puzzleboard::PuzzleBoardSpec = from_py_json(board, "board")?;
    sweep_preset_to_py(py, &puzzleboard::PuzzleBoardParams::sweep_for_board(&spec))
}

/// Return Rust-side default PuzzleBoard parameters for a board size.
#[pyfunction]
#[pyo3(signature = (rows, cols))]
pub(crate) fn default_puzzleboard_params(
    py: Python<'_>,
    rows: u32,
    cols: u32,
) -> PyResult<Py<PyAny>> {
    let spec = puzzleboard::PuzzleBoardSpec::new(rows, cols, 1.0)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let params = puzzleboard::PuzzleBoardParams::for_board(spec);
    let json =
        serde_json::to_value(params).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}
