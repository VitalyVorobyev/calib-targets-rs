use ::calib_targets::{charuco, chessboard, detect, marker};
use chess_corners::{ChessConfig, ChessParams, CoarseToFineParams};
use numpy::{PyArrayDyn, PyArrayMethods, PyUntypedArrayMethods};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyList, PyString, PyTuple};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{Map, Number, Value};

#[derive(Debug, Default, Deserialize)]
struct ChessConfigOverrides {
    #[serde(default)]
    params: Option<ChessParamsOverrides>,
    #[serde(default)]
    multiscale: Option<CoarseToFineOverrides>,
}

impl ChessConfigOverrides {
    fn apply(self, cfg: &mut ChessConfig) {
        if let Some(params) = self.params {
            params.apply(&mut cfg.params);
        }
        if let Some(multiscale) = self.multiscale {
            multiscale.apply(&mut cfg.multiscale);
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct ChessParamsOverrides {
    #[serde(default)]
    use_radius10: Option<bool>,
    #[serde(default)]
    descriptor_use_radius10: Option<bool>,
    #[serde(default)]
    threshold_rel: Option<f32>,
    #[serde(default)]
    threshold_abs: Option<f32>,
    #[serde(default)]
    nms_radius: Option<u32>,
    #[serde(default)]
    min_cluster_size: Option<u32>,
}

impl ChessParamsOverrides {
    fn apply(self, params: &mut ChessParams) {
        if let Some(use_radius10) = self.use_radius10 {
            params.use_radius10 = use_radius10;
        }
        if let Some(descriptor_use_radius10) = self.descriptor_use_radius10 {
            params.descriptor_use_radius10 = Some(descriptor_use_radius10);
        }
        if let Some(threshold_rel) = self.threshold_rel {
            params.threshold_rel = threshold_rel;
        }
        if let Some(threshold_abs) = self.threshold_abs {
            params.threshold_abs = Some(threshold_abs);
        }
        if let Some(nms_radius) = self.nms_radius {
            params.nms_radius = nms_radius;
        }
        if let Some(min_cluster_size) = self.min_cluster_size {
            params.min_cluster_size = min_cluster_size;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct CoarseToFineOverrides {
    #[serde(default)]
    pyramid: Option<PyramidOverrides>,
    #[serde(default)]
    refinement_radius: Option<u32>,
    #[serde(default)]
    merge_radius: Option<f32>,
}

impl CoarseToFineOverrides {
    fn apply(self, params: &mut CoarseToFineParams) {
        if let Some(pyramid) = self.pyramid {
            pyramid.apply(params);
        }
        if let Some(refinement_radius) = self.refinement_radius {
            params.refinement_radius = refinement_radius;
        }
        if let Some(merge_radius) = self.merge_radius {
            params.merge_radius = merge_radius;
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct PyramidOverrides {
    #[serde(default)]
    num_levels: Option<u8>,
    #[serde(default)]
    min_size: Option<usize>,
}

impl PyramidOverrides {
    fn apply(self, params: &mut CoarseToFineParams) {
        if let Some(num_levels) = self.num_levels {
            params.pyramid.num_levels = num_levels;
        }
        if let Some(min_size) = self.min_size {
            params.pyramid.min_size = min_size;
        }
    }
}

fn value_error(message: impl Into<String>) -> PyErr {
    PyValueError::new_err(message.into())
}

fn py_to_json(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }

    if obj.is_instance_of::<PyBool>() {
        return Ok(Value::Bool(obj.extract::<bool>()?));
    }

    if let Ok(dict) = obj.downcast::<PyDict>() {
        let mut out = Map::with_capacity(dict.len());
        for (key, value) in dict.iter() {
            let key_str: String = key
                .extract()
                .map_err(|_| value_error("dictionary keys must be strings for JSON conversion"))?;
            let value_json = py_to_json(&value)?;
            out.insert(key_str, value_json);
        }
        return Ok(Value::Object(out));
    }

    if let Ok(list) = obj.downcast::<PyList>() {
        let mut out = Vec::with_capacity(list.len());
        for item in list.iter() {
            out.push(py_to_json(&item)?);
        }
        return Ok(Value::Array(out));
    }

    if let Ok(tuple) = obj.downcast::<PyTuple>() {
        let mut out = Vec::with_capacity(tuple.len());
        for item in tuple.iter() {
            out.push(py_to_json(&item)?);
        }
        return Ok(Value::Array(out));
    }

    if obj.is_instance_of::<PyString>() {
        let text: String = obj.extract()?;
        return Ok(Value::String(text));
    }

    if let Ok(value) = obj.extract::<i64>() {
        return Ok(Value::Number(value.into()));
    }

    if let Ok(value) = obj.extract::<u64>() {
        return Ok(Value::Number(value.into()));
    }

    if let Ok(value) = obj.extract::<f64>() {
        let number = Number::from_f64(value)
            .ok_or_else(|| value_error("non-finite float is not JSON compatible"))?;
        return Ok(Value::Number(number));
    }

    Err(value_error("unsupported type for JSON conversion"))
}

fn json_to_py(py: Python<'_>, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::Null => Ok(py.None()),
        Value::Bool(v) => Ok(v.into_py(py)),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                Ok(i.into_py(py))
            } else if let Some(u) = v.as_u64() {
                Ok(u.into_py(py))
            } else if let Some(f) = v.as_f64() {
                Ok(f.into_py(py))
            } else {
                Ok(py.None())
            }
        }
        Value::String(s) => Ok(s.into_py(py)),
        Value::Array(values) => {
            let mut out = Vec::with_capacity(values.len());
            for item in values {
                out.push(json_to_py(py, item)?);
            }
            Ok(PyList::new_bound(py, out).into_py(py))
        }
        Value::Object(map) => {
            let dict = PyDict::new_bound(py);
            for (key, item) in map.iter() {
                let value = json_to_py(py, item)?;
                dict.set_item(key, value)?;
            }
            Ok(dict.into_py(py))
        }
    }
}

fn parse_optional<T: DeserializeOwned>(
    obj: Option<&Bound<'_, PyAny>>,
    name: &str,
) -> PyResult<Option<T>> {
    let Some(obj) = obj else {
        return Ok(None);
    };
    if obj.is_none() {
        return Ok(None);
    }
    let value = py_to_json(obj).map_err(|err| value_error(format!("{name}: {err}")))?;
    serde_json::from_value(value)
        .map(Some)
        .map_err(|err| value_error(format!("{name}: {err}")))
}

fn parse_required<T: DeserializeOwned>(obj: &Bound<'_, PyAny>, name: &str) -> PyResult<T> {
    if obj.is_none() {
        return Err(value_error(format!("{name} is required")));
    }
    let value = py_to_json(obj).map_err(|err| value_error(format!("{name}: {err}")))?;
    serde_json::from_value(value).map_err(|err| value_error(format!("{name}: {err}")))
}

fn chess_cfg_from_py(obj: Option<&Bound<'_, PyAny>>) -> PyResult<ChessConfig> {
    let mut cfg = detect::default_chess_config();
    if let Some(obj) = obj {
        if !obj.is_none() {
            let value = py_to_json(obj).map_err(|err| value_error(format!("chess_cfg: {err}")))?;
            let overrides: ChessConfigOverrides = serde_json::from_value(value)
                .map_err(|err| value_error(format!("chess_cfg: {err}")))?;
            overrides.apply(&mut cfg);
        }
    }
    Ok(cfg)
}

fn gray_image_from_py(image: &Bound<'_, PyAny>) -> PyResult<::image::GrayImage> {
    let array = image
        .downcast::<PyArrayDyn<u8>>()
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
    let pixels = view.to_owned().into_raw_vec();
    detect::gray_image_from_slice(width, height, &pixels)
        .map_err(|err| value_error(err.to_string()))
}

#[pyfunction]
#[pyo3(signature = (image, *, board, chess_cfg=None, params=None))]
fn detect_charuco(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    board: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyObject> {
    let img = gray_image_from_py(image)?;
    let board = parse_required::<charuco::CharucoBoardSpec>(board, "board")?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;
    let params = match parse_optional::<charuco::CharucoDetectorParams>(params, "params")? {
        Some(params) => params,
        None => charuco::CharucoDetectorParams::for_board(&board),
    };

    let result = py.allow_threads(move || detect::detect_charuco(&img, &chess_cfg, board, params));
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
fn detect_chessboard(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<PyObject>> {
    let img = gray_image_from_py(image)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;
    let params =
        parse_optional::<chessboard::ChessboardParams>(params, "params")?.unwrap_or_default();

    let result = py.allow_threads(move || detect::detect_chessboard(&img, &chess_cfg, params));
    match result {
        Some(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        None => Ok(None),
    }
}

#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params=None))]
fn detect_marker_board(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<PyObject>> {
    let img = gray_image_from_py(image)?;
    let chess_cfg = chess_cfg_from_py(chess_cfg)?;
    let params = parse_optional::<marker::MarkerBoardParams>(params, "params")?.unwrap_or_default();

    let result = py.allow_threads(move || detect::detect_marker_board(&img, &chess_cfg, params));
    match result {
        Some(res) => {
            let json = serde_json::to_value(res)
                .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
            Ok(Some(json_to_py(py, &json)?))
        }
        None => Ok(None),
    }
}

#[pymodule]
fn calib_targets(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction_bound!(detect_charuco, m)?)?;
    m.add_function(wrap_pyfunction_bound!(detect_chessboard, m)?)?;
    m.add_function(wrap_pyfunction_bound!(detect_marker_board, m)?)?;
    Ok(())
}
