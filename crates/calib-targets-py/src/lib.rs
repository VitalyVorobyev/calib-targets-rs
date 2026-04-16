use ::calib_targets::detect::ChessConfig;
use ::calib_targets::{charuco, chessboard, detect, marker, printable, puzzleboard};
use numpy::{PyArrayDyn, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::conversion::IntoPyObjectExt;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList, PyString, PyTuple};
use serde::de::DeserializeOwned;
use serde_json::{Map, Number, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn value_error(msg: impl Into<String>) -> PyErr {
    PyValueError::new_err(msg.into())
}

fn py_to_json(obj: &Bound<'_, PyAny>, path: &str) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }
    if obj.is_instance_of::<PyBool>() {
        return Ok(Value::Bool(obj.extract::<bool>()?));
    }
    if let Ok(dict) = obj.cast::<PyDict>() {
        let mut out = Map::with_capacity(dict.len());
        for (key, value) in dict.iter() {
            let key_str: String = key.extract().map_err(|_| {
                value_error(format!(
                    "{path}: dictionary keys must be strings for JSON conversion"
                ))
            })?;
            let child_path = format!("{path}.{key_str}");
            out.insert(key_str, py_to_json(&value, &child_path)?);
        }
        return Ok(Value::Object(out));
    }
    if let Ok(list) = obj.cast::<PyList>() {
        let mut out = Vec::with_capacity(list.len());
        for (idx, item) in list.iter().enumerate() {
            out.push(py_to_json(&item, &format!("{path}[{idx}]"))?);
        }
        return Ok(Value::Array(out));
    }
    if let Ok(tuple) = obj.cast::<PyTuple>() {
        let mut out = Vec::with_capacity(tuple.len());
        for (idx, item) in tuple.iter().enumerate() {
            out.push(py_to_json(&item, &format!("{path}[{idx}]"))?);
        }
        return Ok(Value::Array(out));
    }
    if obj.is_instance_of::<PyString>() {
        return Ok(Value::String(obj.extract()?));
    }
    if is_numpy_scalar(obj) {
        if let Ok(item) = obj.call_method0("item") {
            return py_to_json(&item, path);
        }
    }
    if let Ok(value) = obj.extract::<i64>() {
        return Ok(Value::Number(value.into()));
    }
    if let Ok(value) = obj.extract::<u64>() {
        return Ok(Value::Number(value.into()));
    }
    if let Ok(value) = obj.extract::<f64>() {
        let number = Number::from_f64(value).ok_or_else(|| {
            value_error(format!("{path}: non-finite float is not JSON compatible"))
        })?;
        return Ok(Value::Number(number));
    }
    Err(value_error(format!(
        "{path}: unsupported type for JSON conversion"
    )))
}

fn json_to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(v) => v.into_py_any(py),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                i.into_py_any(py)
            } else if let Some(u) = v.as_u64() {
                u.into_py_any(py)
            } else if let Some(f) = v.as_f64() {
                f.into_py_any(py)
            } else {
                Ok(py.None())
            }
        }
        Value::String(s) => s.into_py_any(py),
        Value::Array(values) => {
            let mut out = Vec::with_capacity(values.len());
            for item in values {
                out.push(json_to_py(py, item)?);
            }
            Ok(PyList::new(py, out)?.into_any().unbind())
        }
        Value::Object(map) => {
            let dict = PyDict::new(py);
            for (key, item) in map.iter() {
                dict.set_item(key, json_to_py(py, item)?)?;
            }
            Ok(dict.into_any().unbind())
        }
    }
}

fn is_numpy_scalar(obj: &Bound<'_, PyAny>) -> bool {
    obj.get_type()
        .qualname()
        .map(|name| {
            let s = name.to_string();
            s.starts_with("int") || s.starts_with("uint") || s.starts_with("float")
        })
        .unwrap_or(false)
}

fn from_py_json<T: DeserializeOwned>(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<T> {
    let value = py_to_json(obj, name)?;
    serde_json::from_value(value).map_err(|err| value_error(format!("{name}: {err}")))
}

// ---------------------------------------------------------------------------
// Image conversion
// ---------------------------------------------------------------------------

fn gray_image_from_py(image: &Bound<'_, PyAny>) -> PyResult<::image::GrayImage> {
    let array = image
        .cast::<PyArrayDyn<u8>>()
        .map_err(|_| value_error("image must be a numpy.ndarray with dtype=uint8"))?;
    if array.ndim() != 2 {
        return Err(value_error("image must be a 2D array"));
    }
    let readonly = array.readonly();
    let view = readonly.as_array();
    let shape = view.shape();
    let height = *shape
        .first()
        .ok_or_else(|| value_error("image has no height"))?;
    let width = *shape
        .get(1)
        .ok_or_else(|| value_error("image has no width"))?;
    let height = u32::try_from(height).map_err(|_| value_error("image height is too large"))?;
    let width = u32::try_from(width).map_err(|_| value_error("image width is too large"))?;
    let pixels = view.to_owned().into_raw_vec_and_offset().0;
    detect::gray_image_from_slice(width, height, &pixels)
        .map_err(|err| value_error(err.to_string()))
}

// ---------------------------------------------------------------------------
// Config extraction
// ---------------------------------------------------------------------------

fn chess_cfg_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<ChessConfig> {
    let Some(obj) = obj else {
        return Ok(detect::default_chess_config());
    };
    if obj.is_none() {
        return Ok(detect::default_chess_config());
    }
    from_py_json(obj, "chess_cfg")
}

fn chessboard_params_from_py(
    obj: Option<&Bound<'_, PyAny>>,
) -> PyResult<chessboard::ChessboardParams> {
    let Some(obj) = obj else {
        return Ok(chessboard::ChessboardParams::default());
    };
    if obj.is_none() {
        return Ok(chessboard::ChessboardParams::default());
    }
    from_py_json(obj, "params")
}

fn charuco_params_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<charuco::CharucoParams> {
    let Some(obj) = obj else {
        return Err(value_error("params is required for ChArUco detection"));
    };
    if obj.is_none() {
        return Err(value_error("params is required for ChArUco detection"));
    }
    from_py_json(obj, "params")
}

fn marker_board_params_from_py(
    obj: Option<&Bound<'_, PyAny>>,
) -> PyResult<marker::MarkerBoardParams> {
    let Some(obj) = obj else {
        return Ok(marker::MarkerBoardParams::default());
    };
    if obj.is_none() {
        return Ok(marker::MarkerBoardParams::default());
    }
    from_py_json(obj, "params")
}

fn puzzleboard_params_from_py(
    obj: Option<&Bound<'_, PyAny>>,
) -> PyResult<puzzleboard::PuzzleBoardParams> {
    let Some(obj) = obj else {
        return Err(value_error("params is required for PuzzleBoard detection"));
    };
    if obj.is_none() {
        return Err(value_error("params is required for PuzzleBoard detection"));
    }
    from_py_json(obj, "params")
}

fn printable_document_from_py(
    obj: &Bound<'_, PyAny>,
) -> PyResult<printable::PrintableTargetDocument> {
    from_py_json(obj, "document")
}

// ---------------------------------------------------------------------------
// Detection functions
// ---------------------------------------------------------------------------

/// Detect a ChArUco board in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: dict with ChessConfig fields, or None for defaults.
///     If provided, overrides `params.chessboard.chess`.
///   params: dict with CharucoParams fields (must include `board`).
///
/// Returns:
///   dict with detection data, or raises RuntimeError on detection errors.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params))]
fn detect_charuco(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut params = charuco_params_from_py(Some(params))?;
    if chess_cfg.is_some() {
        params.chessboard.chess = chess_cfg_from_py(chess_cfg)?;
    }

    let result = py.detach(move || detect::detect_charuco(&img, &params));
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

/// Detect a chessboard in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: dict with ChessConfig fields, or None for defaults.
///     If provided, overrides `params.chess`.
///   params: dict with ChessboardParams fields, or None for defaults.
///
/// Returns:
///   dict with detection data, or None if no board is found.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
fn detect_chessboard(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let mut params = chessboard_params_from_py(params)?;
    if chess_cfg.is_some() {
        params.chess = chess_cfg_from_py(chess_cfg)?;
    }

    let result = py.detach(move || detect::detect_chessboard(&img, &params));
    match result {
        Some(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        None => Ok(None),
    }
}

/// Detect a marker-board target in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: dict with ChessConfig fields, or None for defaults.
///     If provided, overrides `params.chessboard.chess`.
///   params: dict with MarkerBoardParams fields, or None for defaults.
///
/// Returns:
///   dict with detection data, or None if no board is found.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
fn detect_marker_board(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let mut params = marker_board_params_from_py(params)?;
    if chess_cfg.is_some() {
        params.chessboard.chess = chess_cfg_from_py(chess_cfg)?;
    }

    let result = py.detach(move || detect::detect_marker_board(&img, &params));
    match result {
        Some(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        None => Ok(None),
    }
}

/// Detect a PuzzleBoard in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: dict with ChessConfig fields, or None for defaults.
///     If provided, overrides `params.chessboard.chess`.
///   params: dict with PuzzleBoardParams fields (must include `board`).
///
/// Returns:
///   dict with detection data. Raises RuntimeError on detection errors.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params))]
fn detect_puzzleboard(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut params = puzzleboard_params_from_py(Some(params))?;
    if chess_cfg.is_some() {
        params.chessboard.chess = chess_cfg_from_py(chess_cfg)?;
    }

    let result = py.detach(move || detect::detect_puzzleboard(&img, &params));
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

// ---------------------------------------------------------------------------
// Multi-config sweep detection
// ---------------------------------------------------------------------------

/// Try multiple chessboard parameter configs, return the best result (most corners).
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   configs: list of dicts with ChessboardParams fields.
///
/// Returns:
///   dict with detection data, or None if no board is found with any config.
#[pyfunction]
#[pyo3(signature = (image, configs))]
fn detect_chessboard_best(
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
        params_vec.push(from_py_json::<chessboard::ChessboardParams>(
            &item,
            "configs[]",
        )?);
    }

    let result = py.detach(move || detect::detect_chessboard_best(&img, &params_vec));
    match result {
        Some(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        None => Ok(None),
    }
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
fn detect_charuco_best(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    configs: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let list = configs
        .cast::<PyList>()
        .map_err(|_| value_error("configs must be a list"))?;
    let mut params_vec = Vec::with_capacity(list.len());
    for item in list.iter() {
        params_vec.push(from_py_json::<charuco::CharucoParams>(&item, "configs[]")?);
    }

    let result = py.detach(move || detect::detect_charuco_best(&img, &params_vec));
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
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
fn detect_marker_board_best(
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

    let result = py.detach(move || detect::detect_marker_board_best(&img, &params_vec));
    match result {
        Some(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        None => Ok(None),
    }
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
fn detect_puzzleboard_best(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    configs: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
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

    let result = py.detach(move || detect::detect_puzzleboard_best(&img, &params_vec));
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

/// Return Rust-side default PuzzleBoard parameters for a board size.
#[pyfunction]
#[pyo3(signature = (rows, cols))]
fn default_puzzleboard_params(py: Python<'_>, rows: u32, cols: u32) -> PyResult<Py<PyAny>> {
    let params = detect::default_puzzleboard_params(rows, cols)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(params).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

// ---------------------------------------------------------------------------
// Printable target functions
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (document))]
fn render_target_bundle(py: Python<'_>, document: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let document = printable_document_from_py(document)?;
    let bundle = printable::render_target_bundle(&document)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let out = PyDict::new(py);
    out.set_item("json_text", bundle.json_text)?;
    out.set_item("svg_text", bundle.svg_text)?;
    out.set_item("png_bytes", PyBytes::new(py, &bundle.png_bytes))?;
    Ok(out.into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (document, output_stem))]
fn write_target_bundle(
    py: Python<'_>,
    document: &Bound<'_, PyAny>,
    output_stem: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let document = printable_document_from_py(document)?;
    let output_stem = output_stem
        .extract::<String>()
        .map_err(|_| value_error("output_stem must be str"))?;
    let written = printable::write_target_bundle(&document, output_stem)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let out = PyDict::new(py);
    out.set_item("json_path", written.json_path.to_string_lossy().as_ref())?;
    out.set_item("svg_path", written.svg_path.to_string_lossy().as_ref())?;
    out.set_item("png_path", written.png_path.to_string_lossy().as_ref())?;
    Ok(out.into_any().unbind())
}

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

#[pymodule]
fn _core(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(detect_charuco, m)?)?;
    m.add_function(wrap_pyfunction!(detect_chessboard, m)?)?;
    m.add_function(wrap_pyfunction!(detect_marker_board, m)?)?;
    m.add_function(wrap_pyfunction!(detect_puzzleboard, m)?)?;
    m.add_function(wrap_pyfunction!(detect_chessboard_best, m)?)?;
    m.add_function(wrap_pyfunction!(detect_charuco_best, m)?)?;
    m.add_function(wrap_pyfunction!(detect_marker_board_best, m)?)?;
    m.add_function(wrap_pyfunction!(detect_puzzleboard_best, m)?)?;
    m.add_function(wrap_pyfunction!(default_puzzleboard_params, m)?)?;
    m.add_function(wrap_pyfunction!(render_target_bundle, m)?)?;
    m.add_function(wrap_pyfunction!(write_target_bundle, m)?)?;
    Ok(())
}
