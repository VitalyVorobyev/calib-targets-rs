use ::calib_targets::detect::{self as ct_detect, DetectorConfig};
use ::calib_targets::{charuco, chessboard, marker, printable as ct_printable, puzzleboard};
use numpy::{PyArrayDyn, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::conversion::IntoPyObjectExt;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyString, PyTuple};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Number, Value};

mod detect;
mod diagnose;
mod pole;
mod printable;

pub(crate) use diagnose::diagnostics_payload;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

pub(crate) fn value_error(msg: impl Into<String>) -> PyErr {
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

pub(crate) fn json_to_py(py: Python<'_>, value: &Value) -> PyResult<Py<PyAny>> {
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

pub(crate) fn from_py_json<T: DeserializeOwned>(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<T> {
    let value = py_to_json(obj, name)?;
    serde_json::from_value(value).map_err(|err| value_error(format!("{name}: {err}")))
}

// ---------------------------------------------------------------------------
// Image conversion
// ---------------------------------------------------------------------------

pub(crate) fn gray_image_from_py(
    image: &Bound<'_, PyAny>,
) -> PyResult<::calib_targets::core::GrayImage> {
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
    let height_u32 = u32::try_from(height).map_err(|_| value_error("image height is too large"))?;
    let width_u32 = u32::try_from(width).map_err(|_| value_error("image width is too large"))?;
    if width_u32 == 0 || height_u32 == 0 {
        return Err(value_error(format!(
            "invalid grayscale image dimensions (width={width_u32}, height={height_u32})"
        )));
    }
    let expected = width
        .checked_mul(height)
        .ok_or_else(|| value_error("image dimensions overflow"))?;
    let data = match view.as_slice() {
        Some(slice) => slice.to_vec(),
        None => view.iter().copied().collect(),
    };
    if data.len() != expected {
        return Err(value_error(format!(
            "invalid grayscale image buffer length (expected {expected} bytes, got {})",
            data.len()
        )));
    }
    Ok(::calib_targets::core::GrayImage {
        width,
        height,
        data,
    })
}

// ---------------------------------------------------------------------------
// Config extraction
// ---------------------------------------------------------------------------

/// Apply an explicit `chess_cfg=` argument over a params struct's own
/// `chess` field.
///
/// The ChArUco / PuzzleBoard / marker-board entry points accept both: the
/// `chess` key travels with the serialized params, while `chess_cfg=` is a
/// per-call override. An absent (or `None`) argument leaves the params' own
/// value in place, so the two can never silently disagree — whichever is used
/// is the one the corner pass runs with.
pub(crate) fn apply_chess_cfg_override(
    dst: &mut DetectorConfig,
    obj: Option<&Bound<'_, PyAny>>,
) -> PyResult<()> {
    let Some(obj) = obj else {
        return Ok(());
    };
    if obj.is_none() {
        return Ok(());
    }
    *dst = from_py_json(obj, "chess_cfg")?;
    Ok(())
}

pub(crate) fn chess_cfg_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<DetectorConfig> {
    let Some(obj) = obj else {
        return Ok(ct_detect::default_chess_config());
    };
    if obj.is_none() {
        return Ok(ct_detect::default_chess_config());
    }
    from_py_json(obj, "chess_cfg")
}

pub(crate) fn chessboard_params_from_py(
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

pub(crate) fn charuco_params_from_py(
    obj: Option<&Bound<'_, PyAny>>,
) -> PyResult<charuco::CharucoParams> {
    let Some(obj) = obj else {
        return Err(value_error("params is required for ChArUco detection"));
    };
    if obj.is_none() {
        return Err(value_error("params is required for ChArUco detection"));
    }
    let params: charuco::CharucoParams = from_py_json(obj, "params")?;
    Ok(params)
}

pub(crate) fn marker_board_params_from_py(
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

pub(crate) fn puzzleboard_params_from_py(
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

pub(crate) fn printable_document_from_py(
    obj: &Bound<'_, PyAny>,
) -> PyResult<ct_printable::PrintableTargetDocument> {
    from_py_json(obj, "document")
}

/// Parse a pre-detected corner cloud for the `*_with_corners` entry points.
///
/// `obj` must be a list of `ChessCorner`-shaped dicts:
/// `{"position": [x, y], "axes": [{"angle": .., "sigma": ..}, {..}], "strength": ..}`
/// plus an optional `"position_covariance": {"xx", "xy", "yy"} | None`
/// — the same shape `serde_json` emits for `calib_targets_chessboard::ChessCorner`.
/// A corner cloud obtained from `trace_chessboard_topological`'s `"corners"`
/// field (which additionally carries an `"index"` key) is accepted as-is: the
/// extra key is ignored.
pub(crate) fn chess_corners_from_py(
    obj: &Bound<'_, PyAny>,
) -> PyResult<Vec<chessboard::ChessCorner>> {
    from_py_json(obj, "corners")
}

// ---------------------------------------------------------------------------
// Detection functions
// ---------------------------------------------------------------------------

/// A facade detection result as Python sees it: the detection as a dict, or
/// `None` for a miss (`DetectError::is_not_found`). Any other error is a
/// misuse — a malformed image, an invalid board spec or parameters — and
/// raises `ValueError` with its message. Every `detect_*` function follows
/// this one convention.
pub(crate) fn detection_result_to_py<T: serde::Serialize>(
    py: Python<'_>,
    result: Result<T, ct_detect::DetectError>,
) -> PyResult<Option<Py<PyAny>>> {
    match result {
        Ok(detection) => {
            let json = serde_json::to_value(detection)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        Err(err) if err.is_not_found() => Ok(None),
        Err(err) => Err(value_error(err.to_string())),
    }
}

/// Serialize a Rust-built sweep preset into a Python list of config dicts.
///
/// The emitted shape is exactly `serde_json`'s encoding of the params struct —
/// the same wire shape the typed Python dataclasses round-trip through
/// (`to_dict` / `from_dict`) and that the `detect_*_best` entry points read
/// back — so the returned list can be handed straight to `configs=`.
///
/// Every preset below is *computed by Rust*: the Python `sweep_*`
/// classmethods parse these dicts rather than re-deriving the config list, so
/// the two surfaces cannot explore different configuration spaces.
pub(crate) fn sweep_preset_to_py<T: Serialize>(
    py: Python<'_>,
    configs: &[T],
) -> PyResult<Py<PyAny>> {
    let json =
        serde_json::to_value(configs).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

#[pymodule]
fn _core(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(detect::detect_charuco, m)?)?;
    m.add_function(wrap_pyfunction!(detect::detect_charuco_with_corners, m)?)?;
    m.add_function(wrap_pyfunction!(detect::detect_chessboard, m)?)?;
    m.add_function(wrap_pyfunction!(detect::detect_chessboard_all, m)?)?;
    m.add_function(wrap_pyfunction!(detect::trace_chessboard_topological, m)?)?;
    m.add_function(wrap_pyfunction!(detect::detect_marker_board, m)?)?;
    m.add_function(wrap_pyfunction!(
        detect::detect_marker_board_with_corners,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(detect::detect_puzzleboard, m)?)?;
    m.add_function(wrap_pyfunction!(
        detect::detect_puzzleboard_with_corners,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(pole::detect_puzzlepole, m)?)?;
    m.add_function(wrap_pyfunction!(pole::detect_puzzlepole_with_corners, m)?)?;
    m.add_function(wrap_pyfunction!(diagnose::diagnose_charuco, m)?)?;
    m.add_function(wrap_pyfunction!(
        diagnose::diagnose_charuco_with_corners,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(diagnose::diagnose_marker_board, m)?)?;
    m.add_function(wrap_pyfunction!(
        diagnose::diagnose_marker_board_with_corners,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(diagnose::diagnose_puzzleboard, m)?)?;
    m.add_function(wrap_pyfunction!(
        diagnose::diagnose_puzzleboard_with_corners,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(pole::diagnose_puzzlepole, m)?)?;
    m.add_function(wrap_pyfunction!(pole::diagnose_puzzlepole_with_corners, m)?)?;
    m.add_function(wrap_pyfunction!(detect::detect_chessboard_best, m)?)?;
    m.add_function(wrap_pyfunction!(detect::detect_charuco_best, m)?)?;
    m.add_function(wrap_pyfunction!(detect::detect_marker_board_best, m)?)?;
    m.add_function(wrap_pyfunction!(detect::detect_puzzleboard_best, m)?)?;
    m.add_function(wrap_pyfunction!(pole::detect_puzzlepole_best, m)?)?;
    m.add_function(wrap_pyfunction!(detect::chessboard_sweep_default, m)?)?;
    m.add_function(wrap_pyfunction!(detect::charuco_sweep_for_board, m)?)?;
    m.add_function(wrap_pyfunction!(detect::marker_board_sweep_for_board, m)?)?;
    m.add_function(wrap_pyfunction!(detect::puzzleboard_sweep_for_board, m)?)?;
    m.add_function(wrap_pyfunction!(detect::default_puzzleboard_params, m)?)?;
    m.add_function(wrap_pyfunction!(pole::puzzlepole_sweep_for_pole, m)?)?;
    m.add_function(wrap_pyfunction!(pole::default_puzzlepole_params, m)?)?;
    m.add_function(wrap_pyfunction!(pole::puzzlepole_distinct_poles, m)?)?;
    m.add_function(wrap_pyfunction!(printable::puzzlepole_periods, m)?)?;
    m.add_function(wrap_pyfunction!(
        printable::puzzlepole_canonical_start_row,
        m
    )?)?;
    m.add_function(wrap_pyfunction!(printable::render_target_bundle, m)?)?;
    m.add_function(wrap_pyfunction!(printable::render_screen_target, m)?)?;
    m.add_function(wrap_pyfunction!(printable::write_target_bundle, m)?)?;
    Ok(())
}
