use ::calib_targets::detect::DetectorConfig;
use ::calib_targets::{charuco, chessboard, detect, marker, printable, puzzleboard};
use numpy::{PyArrayDyn, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::conversion::IntoPyObjectExt;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList, PyString, PyTuple};
use serde::de::DeserializeOwned;
use serde::Serialize;
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

/// Apply an explicit `chess_cfg=` argument over a params struct's own
/// `chess` field.
///
/// The ChArUco / PuzzleBoard / marker-board entry points accept both: the
/// `chess` key travels with the serialized params, while `chess_cfg=` is a
/// per-call override. An absent (or `None`) argument leaves the params' own
/// value in place, so the two can never silently disagree — whichever is used
/// is the one the corner pass runs with.
fn apply_chess_cfg_override(
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

fn chess_cfg_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<DetectorConfig> {
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
    let params: charuco::CharucoParams = from_py_json(obj, "params")?;
    Ok(params)
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

/// Parse a pre-detected corner cloud for the `*_with_corners` entry points.
///
/// `obj` must be a list of `ChessCorner`-shaped dicts:
/// `{"position": [x, y], "axes": [{"angle": .., "sigma": ..}, {..}], "strength": ..}`
/// — the same shape `serde_json` emits for `calib_targets_chessboard::ChessCorner`.
/// A corner cloud obtained from `trace_chessboard_topological`'s `"corners"`
/// field (which additionally carries an `"index"` key) is accepted as-is: the
/// extra key is ignored.
fn chess_corners_from_py(obj: &Bound<'_, PyAny>) -> PyResult<Vec<chessboard::ChessCorner>> {
    from_py_json(obj, "corners")
}

// ---------------------------------------------------------------------------
// Detection functions
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct TopologicalCornerPayload {
    index: usize,
    position: [f32; 2],
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
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;
    let result = py.detach(move || detect::detect_charuco(&img, &params));
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
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
///   dict with detection data, or raises RuntimeError on detection errors.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params))]
fn detect_charuco_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = charuco_params_from_py(Some(params))?;
    let result = py.detach(move || detect::detect_charuco_with_corners(&img, &corners, &params));
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
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
fn detect_chessboard(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let params = chessboard_params_from_py(params)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;

    let result = py.detach(move || {
        let corners = detect::detect_corners(&img, &chess_cfg);
        chessboard::ChessboardDetector::new(params.clone())
            .ok()
            .and_then(|d| d.detect(&corners))
    });
    match result {
        Some(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        None => Ok(None),
    }
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
fn detect_chessboard_all(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let params = chessboard_params_from_py(params)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;

    let results = py.detach(move || {
        let corners = detect::detect_corners(&img, &chess_cfg);
        chessboard::ChessboardDetector::new(params.clone())
            .map(|d| d.detect_all(&corners))
            .unwrap_or_default()
    });
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
fn trace_chessboard_topological(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let width = img.width();
    let height = img.height();
    let params = chessboard_params_from_py(params)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;

    let payload = py.detach(move || -> Result<Value, String> {
        let corners = detect::detect_corners(&img, &chess_cfg);
        let corner_payload: Vec<TopologicalCornerPayload> = corners
            .iter()
            .enumerate()
            .map(|(index, c)| TopologicalCornerPayload {
                index,
                position: [c.position.x, c.position.y],
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
fn detect_marker_board(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let mut params = marker_board_params_from_py(params)?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let result = py.detach(move || detect::detect_marker_board(&img, &params));
    match result {
        Ok(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        // A miss — no chessboard grid, or a grid whose circles did not match
        // the expected layout — is reported to Python as `None`, matching
        // this function's documented contract. Any other error (e.g. a
        // rejected `chessboard` sub-config) raises with the specific message.
        Err(detect::DetectError::MarkerBoardDetect(_)) => Ok(None),
        Err(err) => Err(PyRuntimeError::new_err(err.to_string())),
    }
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
fn detect_marker_board_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<Py<PyAny>>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = marker_board_params_from_py(params)?;

    let result =
        py.detach(move || detect::detect_marker_board_with_corners(&img, &corners, &params));
    match result {
        Ok(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        Err(detect::DetectError::MarkerBoardDetect(_)) => Ok(None),
        Err(err) => Err(PyRuntimeError::new_err(err.to_string())),
    }
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
fn detect_puzzleboard(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut params = puzzleboard_params_from_py(Some(params))?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let result = py.detach(move || detect::detect_puzzleboard(&img, &params));
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
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
fn detect_puzzleboard_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = puzzleboard_params_from_py(Some(params))?;

    let result =
        py.detach(move || detect::detect_puzzleboard_with_corners(&img, &corners, &params));
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

/// Build the `{"result": ..., "diagnostics": ...}` payload shared by every
/// `diagnose_*` pyfunction.
///
/// `diagnostics` is `None` only when the facade could not construct the
/// detector from the supplied params — the pipeline never ran, so there is
/// nothing to report but the construction error, and this raises. Whenever
/// the pipeline ran, `diagnostics` is `Some`, even on a failed detection:
/// `result` then serializes to JSON `null` while `diagnostics` still carries
/// best-effort evidence for overlay tooling.
fn diagnostics_payload<T: Serialize, D: Serialize>(
    result: Result<T, detect::DetectError>,
    diagnostics: Option<D>,
) -> PyResult<Value> {
    let Some(diagnostics) = diagnostics else {
        let err = result
            .err()
            .expect("diagnose_*: None diagnostics implies detector construction failed");
        return Err(PyRuntimeError::new_err(err.to_string()));
    };
    let result_json = match result {
        Ok(value) => {
            serde_json::to_value(value).map_err(|err| PyRuntimeError::new_err(err.to_string()))?
        }
        Err(_) => Value::Null,
    };
    let diagnostics_json = serde_json::to_value(diagnostics)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    Ok(serde_json::json!({
        "result": result_json,
        "diagnostics": diagnostics_json,
    }))
}

/// Detect a ChArUco board and additionally return the diagnostics channel.
///
/// Runs the same detection as `detect_charuco` but returns a dict
/// `{"result": ..., "diagnostics": ...}`. `result` is the
/// `CharucoDetection` dict (or `None` when detection fails);
/// `diagnostics` is the raw `CharucoDetectDiagnostics` payload (per-component
/// matcher decisions, per-cell scores, chosen/runner-up hypotheses, rejection
/// reasons). Diagnostics are produced even on a failed frame.
///
/// The `diagnostics` shape carries a looser stability promise than the
/// typed result API and may evolve between minor versions.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params))]
fn diagnose_charuco(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut params = charuco_params_from_py(Some(params))?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let (result, diagnostics) = py.detach(move || detect::diagnose_charuco(&img, &params));
    let payload = diagnostics_payload(result, diagnostics)?;
    json_to_py(py, &payload)
}

/// `diagnose_charuco` on a pre-detected corner cloud.
///
/// `params.chess` is not read: `corners` is taken as given. See
/// `detect_charuco_with_corners` for the `corners` shape.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params))]
fn diagnose_charuco_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = charuco_params_from_py(Some(params))?;

    let (result, diagnostics) =
        py.detach(move || detect::diagnose_charuco_with_corners(&img, &corners, &params));
    let payload = diagnostics_payload(result, diagnostics)?;
    json_to_py(py, &payload)
}

/// Detect a marker board and additionally return the diagnostics channel.
///
/// Runs the same detection as `detect_marker_board` but returns a dict
/// `{"result": ..., "diagnostics": ...}`. `result` is the
/// `MarkerBoardDetection` dict (or `None` when detection fails);
/// `diagnostics` is the raw `MarkerBoardDiagnostics` payload (scored circle
/// candidates, circle matches, per-corner provenance, alignment-inlier
/// count). Diagnostics are produced even on a failed detection — e.g. a grid
/// found but its circles not matching the expected layout.
///
/// The `diagnostics` shape carries a looser stability promise than the
/// typed result API and may evolve between minor versions.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
fn diagnose_marker_board(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut params = marker_board_params_from_py(params)?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let (result, diagnostics) = py.detach(move || detect::diagnose_marker_board(&img, &params));
    let payload = diagnostics_payload(result, diagnostics)?;
    json_to_py(py, &payload)
}

/// `diagnose_marker_board` on a pre-detected corner cloud.
///
/// `params.chess` is not read: `corners` is taken as given. See
/// `detect_marker_board_with_corners` for the `corners` shape.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params=None))]
fn diagnose_marker_board_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = marker_board_params_from_py(params)?;

    let (result, diagnostics) =
        py.detach(move || detect::diagnose_marker_board_with_corners(&img, &corners, &params));
    let payload = diagnostics_payload(result, diagnostics)?;
    json_to_py(py, &payload)
}

/// Detect a PuzzleBoard and additionally return the diagnostics channel.
///
/// Runs the same detection as `detect_puzzleboard` but returns a dict
/// `{"result": ..., "diagnostics": ...}`. `result` is the
/// `PuzzleBoardDetection` dict (or `None` when detection fails);
/// `diagnostics` is the raw `PuzzleBoardDiagnostics` payload (raw
/// pre-alignment per-edge bit observations and winner-vs-runner-up scoring
/// evidence). Diagnostics are produced even on a failed decode.
///
/// The `diagnostics` shape carries a looser stability promise than the
/// typed result API and may evolve between minor versions.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params))]
fn diagnose_puzzleboard(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut params = puzzleboard_params_from_py(Some(params))?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let (result, diagnostics) = py.detach(move || detect::diagnose_puzzleboard(&img, &params));
    let payload = diagnostics_payload(result, diagnostics)?;
    json_to_py(py, &payload)
}

/// `diagnose_puzzleboard` on a pre-detected corner cloud.
///
/// `params.chess` is not read: `corners` is taken as given. See
/// `detect_puzzleboard_with_corners` for the `corners` shape.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params))]
fn diagnose_puzzleboard_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = puzzleboard_params_from_py(Some(params))?;

    let (result, diagnostics) =
        py.detach(move || detect::diagnose_puzzleboard_with_corners(&img, &corners, &params));
    let payload = diagnostics_payload(result, diagnostics)?;
    json_to_py(py, &payload)
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
fn detect_chessboard_best(
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

    let result = py.detach(move || detect::detect_chessboard_best(&img, &chess_cfg, &params_vec));
    match result {
        Ok(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        // A miss is reported to Python as `None`, exactly as before the facade
        // unified on `Result`; any other error is a genuine failure and raises.
        Err(detect::DetectError::NoDetection { .. }) => Ok(None),
        Err(err) => Err(PyRuntimeError::new_err(err.to_string())),
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
        let cfg = from_py_json::<charuco::CharucoParams>(&item, "configs[]")?;
        params_vec.push(cfg);
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
        Ok(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        // A miss is reported to Python as `None`. `NoDetection` covers the
        // edge case where no config even attempted a detection (an empty
        // `configs`, or every config rejected by construction);
        // `MarkerBoardDetect` covers the ordinary case where a config ran and
        // found no board. Any other error is a genuine failure and raises.
        Err(
            detect::DetectError::NoDetection { .. } | detect::DetectError::MarkerBoardDetect(_),
        ) => Ok(None),
        Err(err) => Err(PyRuntimeError::new_err(err.to_string())),
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

// ---------------------------------------------------------------------------
// Multi-config sweep presets
// ---------------------------------------------------------------------------

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
fn sweep_preset_to_py<T: Serialize>(py: Python<'_>, configs: &[T]) -> PyResult<Py<PyAny>> {
    let json =
        serde_json::to_value(configs).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

/// Return the chessboard sweep preset (`ChessboardParams::sweep_default()`).
///
/// Three configs: the canonical tolerances plus a tighter and a looser
/// grid-graph angular bracket. Pass the list to `detect_chessboard_best`.
///
/// Returns:
///   list of dicts with ChessboardParams fields.
#[pyfunction]
#[pyo3(signature = ())]
fn chessboard_sweep_default(py: Python<'_>) -> PyResult<Py<PyAny>> {
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
fn charuco_sweep_for_board(py: Python<'_>, board: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
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
fn marker_board_sweep_for_board(py: Python<'_>, board: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
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
fn puzzleboard_sweep_for_board(py: Python<'_>, board: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    let spec: puzzleboard::PuzzleBoardSpec = from_py_json(board, "board")?;
    sweep_preset_to_py(py, &puzzleboard::PuzzleBoardParams::sweep_for_board(&spec))
}

/// Return Rust-side default PuzzleBoard parameters for a board size.
#[pyfunction]
#[pyo3(signature = (rows, cols))]
fn default_puzzleboard_params(py: Python<'_>, rows: u32, cols: u32) -> PyResult<Py<PyAny>> {
    let spec = puzzleboard::PuzzleBoardSpec::new(rows, cols, 1.0)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let params = puzzleboard::PuzzleBoardParams::for_board(spec);
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
    out.set_item("dxf_text", bundle.dxf_text)?;
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
    out.set_item("dxf_path", written.dxf_path.to_string_lossy().as_ref())?;
    Ok(out.into_any().unbind())
}

// ---------------------------------------------------------------------------
// Module
// ---------------------------------------------------------------------------

#[pymodule]
fn _core(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(detect_charuco, m)?)?;
    m.add_function(wrap_pyfunction!(detect_charuco_with_corners, m)?)?;
    m.add_function(wrap_pyfunction!(detect_chessboard, m)?)?;
    m.add_function(wrap_pyfunction!(detect_chessboard_all, m)?)?;
    m.add_function(wrap_pyfunction!(trace_chessboard_topological, m)?)?;
    m.add_function(wrap_pyfunction!(detect_marker_board, m)?)?;
    m.add_function(wrap_pyfunction!(detect_marker_board_with_corners, m)?)?;
    m.add_function(wrap_pyfunction!(detect_puzzleboard, m)?)?;
    m.add_function(wrap_pyfunction!(detect_puzzleboard_with_corners, m)?)?;
    m.add_function(wrap_pyfunction!(diagnose_charuco, m)?)?;
    m.add_function(wrap_pyfunction!(diagnose_charuco_with_corners, m)?)?;
    m.add_function(wrap_pyfunction!(diagnose_marker_board, m)?)?;
    m.add_function(wrap_pyfunction!(diagnose_marker_board_with_corners, m)?)?;
    m.add_function(wrap_pyfunction!(diagnose_puzzleboard, m)?)?;
    m.add_function(wrap_pyfunction!(diagnose_puzzleboard_with_corners, m)?)?;
    m.add_function(wrap_pyfunction!(detect_chessboard_best, m)?)?;
    m.add_function(wrap_pyfunction!(detect_charuco_best, m)?)?;
    m.add_function(wrap_pyfunction!(detect_marker_board_best, m)?)?;
    m.add_function(wrap_pyfunction!(detect_puzzleboard_best, m)?)?;
    m.add_function(wrap_pyfunction!(chessboard_sweep_default, m)?)?;
    m.add_function(wrap_pyfunction!(charuco_sweep_for_board, m)?)?;
    m.add_function(wrap_pyfunction!(marker_board_sweep_for_board, m)?)?;
    m.add_function(wrap_pyfunction!(puzzleboard_sweep_for_board, m)?)?;
    m.add_function(wrap_pyfunction!(default_puzzleboard_params, m)?)?;
    m.add_function(wrap_pyfunction!(render_target_bundle, m)?)?;
    m.add_function(wrap_pyfunction!(write_target_bundle, m)?)?;
    Ok(())
}
