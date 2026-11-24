use ::calib_targets::detect;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use serde::Serialize;
use serde_json::Value;

use crate::{
    apply_chess_cfg_override, charuco_params_from_py, chess_corners_from_py, gray_image_from_py,
    json_to_py, marker_board_params_from_py, puzzleboard_params_from_py, value_error,
};

/// Build the `{"result": ..., "diagnostics": ...}` payload shared by every
/// `diagnose_*` pyfunction.
///
/// `diagnostics` is `None` only when the facade could not construct the
/// detector from the supplied params — the pipeline never ran, so there is
/// nothing to report but the construction error, and this raises. Whenever
/// the pipeline ran, `diagnostics` is `Some`, even on a failed detection:
/// `result` then serializes to JSON `null` while `diagnostics` still carries
/// best-effort evidence for overlay tooling.
pub(crate) fn diagnostics_payload<T: Serialize, D: Serialize>(
    result: Result<T, detect::DetectError>,
    diagnostics: Option<D>,
) -> PyResult<Value> {
    let Some(diagnostics) = diagnostics else {
        let err = result
            .err()
            .expect("diagnose_*: None diagnostics implies detector construction failed");
        return Err(value_error(err.to_string()));
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
pub(crate) fn diagnose_charuco(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut params = charuco_params_from_py(Some(params))?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let (result, diagnostics) = py.detach(move || detect::diagnose_charuco(&img.view(), &params));
    let payload = diagnostics_payload(result, diagnostics)?;
    json_to_py(py, &payload)
}

/// `diagnose_charuco` on a pre-detected corner cloud.
///
/// `params.chess` is not read: `corners` is taken as given. See
/// `detect_charuco_with_corners` for the `corners` shape.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params))]
pub(crate) fn diagnose_charuco_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = charuco_params_from_py(Some(params))?;

    let (result, diagnostics) =
        py.detach(move || detect::diagnose_charuco_with_corners(&img.view(), &corners, &params));
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
pub(crate) fn diagnose_marker_board(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut params = marker_board_params_from_py(params)?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let (result, diagnostics) =
        py.detach(move || detect::diagnose_marker_board(&img.view(), &params));
    let payload = diagnostics_payload(result, diagnostics)?;
    json_to_py(py, &payload)
}

/// `diagnose_marker_board` on a pre-detected corner cloud.
///
/// `params.chess` is not read: `corners` is taken as given. See
/// `detect_marker_board_with_corners` for the `corners` shape.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params=None))]
pub(crate) fn diagnose_marker_board_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = marker_board_params_from_py(params)?;

    let (result, diagnostics) = py
        .detach(move || detect::diagnose_marker_board_with_corners(&img.view(), &corners, &params));
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
pub(crate) fn diagnose_puzzleboard(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    chess_cfg: Option<&Bound<'_, PyAny>>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut params = puzzleboard_params_from_py(Some(params))?;
    apply_chess_cfg_override(&mut params.chess, chess_cfg)?;

    let (result, diagnostics) =
        py.detach(move || detect::diagnose_puzzleboard(&img.view(), &params));
    let payload = diagnostics_payload(result, diagnostics)?;
    json_to_py(py, &payload)
}

/// `diagnose_puzzleboard` on a pre-detected corner cloud.
///
/// `params.chess` is not read: `corners` is taken as given. See
/// `detect_puzzleboard_with_corners` for the `corners` shape.
#[pyfunction]
#[pyo3(signature = (image, corners, *, params))]
pub(crate) fn diagnose_puzzleboard_with_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    corners: &Bound<'_, PyAny>,
    params: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let corners = chess_corners_from_py(corners)?;
    let params = puzzleboard_params_from_py(Some(params))?;

    let (result, diagnostics) = py
        .detach(move || detect::diagnose_puzzleboard_with_corners(&img.view(), &corners, &params));
    let payload = diagnostics_payload(result, diagnostics)?;
    json_to_py(py, &payload)
}
