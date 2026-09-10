//! PuzzlePole detection entry points.
//!
//! A separate module rather than more of `lib.rs`: that file is already past
//! the size this workspace splits at, and the pole is a self-contained slice —
//! three detect entry points, a sweep preset and a defaults constructor, all
//! reading the shared conversion helpers next door.
//!
//! There is deliberately no `diagnose_puzzlepole` here, because there is none
//! on the Rust facade either. The decode evidence a caller needs is already on
//! the result; what a `diagnose_*` adds is the raw per-edge dump, which is its
//! own slice with its own binding surface.

use ::calib_targets::{detect, puzzleboard};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::{
    apply_chess_cfg_override, chess_corners_from_py, from_py_json, gray_image_from_py, json_to_py,
    sweep_preset_to_py, value_error,
};

/// Parse the required `params` argument.
///
/// Required rather than defaulted, as for a PuzzleBoard: a pole's parameters
/// carry the spec, and there is no such thing as a default pole — the
/// circumference has to be one that closes seamlessly and the caller is the
/// only one who knows which cylinder they wrapped.
fn params_from_py(obj: &Bound<'_, PyAny>) -> PyResult<puzzleboard::PuzzlePoleParams> {
    if obj.is_none() {
        return Err(value_error("params is required for PuzzlePole detection"));
    }
    from_py_json(obj, "params")
}

/// Serialize a detection result into a Python dict.
fn detection_to_py(
    py: Python<'_>,
    result: Result<puzzleboard::PuzzlePoleDetection, detect::DetectError>,
) -> PyResult<Py<PyAny>> {
    let result = result.map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(result).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

/// Detect a PuzzlePole in a grayscale image.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   chess_cfg: dict with DetectorConfig fields, or None for defaults.
///     If provided, overrides `params.chess`.
///   params: dict with PuzzlePoleParams fields (must include `pole`).
///
/// Returns:
///   dict with detection data — each corner carries both `surface_position`
///   (the unrolled strip, in mm) and `object_position` (the 3-D point on the
///   cylinder, in mm). Raises RuntimeError on detection errors.
#[pyfunction]
#[pyo3(signature = (image, *, chess_cfg=None, params))]
pub(crate) fn detect_puzzlepole(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut params = params_from_py(params)?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let result = py.detach(move || detect::detect_puzzlepole(&img, &params));
    detection_to_py(py, result)
}

/// Detect a PuzzlePole from a pre-detected corner cloud.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   corners: list of `ChessCorner`-shaped dicts (see
///     `trace_chessboard_topological`'s `"corners"` output, or a custom
///     upstream). `params.chess` is not read: `corners` is taken as given.
///   params: dict with PuzzlePoleParams fields (must include `pole`).
///
/// Returns:
///   dict with detection data. Raises RuntimeError on detection errors.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params))]
pub(crate) fn detect_puzzlepole_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = params_from_py(params)?;

    let result = py.detach(move || detect::detect_puzzlepole_with_corners(&img, &corners, &params));
    detection_to_py(py, result)
}

/// Try multiple PuzzlePole parameter configs, return the best result
/// (most identified corners).
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   configs: list of dicts with PuzzlePoleParams fields.
///
/// Returns:
///   dict with detection data. Raises RuntimeError if all configs fail.
#[pyfunction]
#[pyo3(signature = (image, configs))]
pub(crate) fn detect_puzzlepole_best(
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
        params_vec.push(from_py_json::<puzzleboard::PuzzlePoleParams>(
            &item,
            "configs[]",
        )?);
    }

    let result = py.detach(move || detect::detect_puzzlepole_best(&img, &params_vec));
    detection_to_py(py, result)
}

/// Multi-config sweep preset for a pole.
///
/// Args:
///   pole: dict with PuzzlePoleSpec fields.
///
/// Returns:
///   list of dicts with PuzzlePoleParams fields.
#[pyfunction]
#[pyo3(signature = (pole))]
pub(crate) fn puzzlepole_sweep_for_pole(
    py: Python<'_>,
    pole: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let spec: puzzleboard::PuzzlePoleSpec = from_py_json(pole, "pole")?;
    sweep_preset_to_py(py, &puzzleboard::PuzzlePoleParams::sweep_for_pole(&spec))
}

/// Return Rust-side default PuzzlePole parameters for a pole.
///
/// `circumference_squares` must be one of the seamless periods — see
/// `puzzlepole_periods`. Raises RuntimeError otherwise, rather than silently
/// substituting a period that does close.
#[pyfunction]
#[pyo3(signature = (circumference_squares, axial_squares, cell_size_mm))]
pub(crate) fn default_puzzlepole_params(
    py: Python<'_>,
    circumference_squares: u32,
    axial_squares: u32,
    cell_size_mm: f32,
) -> PyResult<Py<PyAny>> {
    let spec = puzzleboard::PuzzlePoleSpec::new(circumference_squares, axial_squares, cell_size_mm)
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let params = puzzleboard::PuzzlePoleParams::for_pole(spec);
    let json =
        serde_json::to_value(params).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}

/// Every pole of this shape that can be cut from the master without sharing a
/// corner with another.
///
/// Exposed rather than left to the caller's arithmetic: the stride is the
/// pole's corner-column count, not its piece count, and getting that wrong
/// yields poles that overlap by one column and therefore share corners.
#[pyfunction]
#[pyo3(signature = (circumference_squares, axial_squares, cell_size_mm))]
pub(crate) fn puzzlepole_distinct_poles(
    py: Python<'_>,
    circumference_squares: u32,
    axial_squares: u32,
    cell_size_mm: f32,
) -> PyResult<Py<PyAny>> {
    let poles = puzzleboard::PuzzlePoleSpec::distinct_poles(
        circumference_squares,
        axial_squares,
        cell_size_mm,
    )
    .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let json =
        serde_json::to_value(poles).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &json)
}
