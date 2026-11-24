use ::calib_targets::blur as ct_blur;
use ::calib_targets::{charuco, chessboard, marker, puzzleboard};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use serde::Serialize;

use crate::{from_py_json, gray_image_from_py, json_to_py};

fn board_photometric_options_from_py(
    obj: Option<&Bound<'_, PyAny>>,
) -> PyResult<ct_blur::BoardPhotometricOptions> {
    let Some(obj) = obj else {
        return Ok(ct_blur::BoardPhotometricOptions::default());
    };
    if obj.is_none() {
        return Ok(ct_blur::BoardPhotometricOptions::default());
    }
    from_py_json(obj, "options")
}

fn refinement_response_to_py<T: Serialize>(
    py: Python<'_>,
    detection: &T,
    refinement: &ct_blur::BoardPhotometricRefinement,
) -> PyResult<Py<PyAny>> {
    let payload = serde_json::json!({
        "detection": detection,
        "refinement": refinement,
    });
    json_to_py(py, &payload)
}

/// Refine chessboard corners with the joint blur-aware photometric model.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   detection: dict with ChessboardDetection fields.
///   options: optional dict with BoardPhotometricOptions fields, or None.
///
/// Returns:
///   dict {"detection": <updated ChessboardDetection>, "refinement": <BoardPhotometricRefinement>}.
#[pyfunction]
#[pyo3(signature = (image, detection, *, options=None))]
pub(crate) fn refine_chessboard_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    detection: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut det: chessboard::ChessboardDetection = from_py_json(detection, "detection")?;
    let opts = board_photometric_options_from_py(options)?;
    let (det, refinement) = py.detach(move || {
        let refinement = ct_blur::refine_chessboard_corners(&img.view(), &mut det, &opts);
        (det, refinement)
    });
    refinement_response_to_py(py, &det, &refinement)
}

/// Refine ChArUco corners with the joint blur-aware photometric model.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   board: dict with CharucoBoardSpec fields.
///   detection: dict with CharucoDetection fields.
///   options: optional dict with BoardPhotometricOptions fields, or None.
///
/// Returns:
///   dict {"detection": <updated CharucoDetection>, "refinement": <BoardPhotometricRefinement>}.
#[pyfunction]
#[pyo3(signature = (image, board, detection, *, options=None))]
pub(crate) fn refine_charuco_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    board: &Bound<'_, PyAny>,
    detection: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let board: charuco::CharucoBoardSpec = from_py_json(board, "board")?;
    let mut det: charuco::CharucoDetection = from_py_json(detection, "detection")?;
    let opts = board_photometric_options_from_py(options)?;
    let (det, refinement) = py.detach(move || {
        let refinement = ct_blur::refine_charuco_corners(&img.view(), &board, &mut det, &opts);
        (det, refinement)
    });
    refinement_response_to_py(py, &det, &refinement)
}

/// Refine PuzzleBoard corners with the joint blur-aware photometric model.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   detection: dict with PuzzleBoardDetection fields.
///   options: optional dict with BoardPhotometricOptions fields, or None.
///
/// Returns:
///   dict {"detection": <updated PuzzleBoardDetection>, "refinement": <BoardPhotometricRefinement>}.
#[pyfunction]
#[pyo3(signature = (image, detection, *, options=None))]
pub(crate) fn refine_puzzleboard_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    detection: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let mut det: puzzleboard::PuzzleBoardDetection = from_py_json(detection, "detection")?;
    let opts = board_photometric_options_from_py(options)?;
    let (det, refinement) = py.detach(move || {
        let refinement = ct_blur::refine_puzzleboard_corners(&img.view(), &mut det, &opts);
        (det, refinement)
    });
    refinement_response_to_py(py, &det, &refinement)
}

/// Refine marker-board corners with the joint blur-aware photometric model.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   board: dict with MarkerBoardSpec fields.
///   detection: dict with MarkerBoardDetection fields.
///   options: optional dict with BoardPhotometricOptions fields, or None.
///
/// Returns:
///   dict {"detection": <updated MarkerBoardDetection>, "refinement": <BoardPhotometricRefinement>}.
#[pyfunction]
#[pyo3(signature = (image, board, detection, *, options=None))]
pub(crate) fn refine_marker_board_corners(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    board: &Bound<'_, PyAny>,
    detection: &Bound<'_, PyAny>,
    options: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let board: marker::MarkerBoardSpec = from_py_json(board, "board")?;
    let mut det: marker::MarkerBoardDetection = from_py_json(detection, "detection")?;
    let opts = board_photometric_options_from_py(options)?;
    let (det, refinement) = py.detach(move || {
        let refinement = ct_blur::refine_marker_board_corners(&img.view(), &board, &mut det, &opts);
        (det, refinement)
    });
    refinement_response_to_py(py, &det, &refinement)
}

/// Estimate per-corner PSF blur kernels for a detected ChArUco board.
///
/// Args:
///   image: 2D numpy.ndarray[uint8] (H, W) grayscale image.
///   board: dict with CharucoBoardSpec fields.
///   detection: dict with CharucoDetection fields.
///
/// Returns:
///   list of {"Ok": CornerBlur} or {"Err": BlurDecline} dicts in corner order.
#[pyfunction]
#[pyo3(signature = (image, board, detection))]
pub(crate) fn charuco_blur(
    py: Python<'_>,
    image: &Bound<'_, PyAny>,
    board: &Bound<'_, PyAny>,
    detection: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let img = gray_image_from_py(image)?;
    let board: charuco::CharucoBoardSpec = from_py_json(board, "board")?;
    let det: charuco::CharucoDetection = from_py_json(detection, "detection")?;
    let blurs = py.detach(move || ct_blur::charuco_blur(&img.view(), &board, &det));
    let val =
        serde_json::to_value(blurs).map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    json_to_py(py, &val)
}
