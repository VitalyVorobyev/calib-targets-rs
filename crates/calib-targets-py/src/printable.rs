use ::calib_targets::{printable, puzzleboard};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use crate::{from_py_json, printable_document_from_py, value_error};

/// Every `(circumference_squares, start_row)` pair that closes seamlessly
/// around a cylinder.
///
/// Exposed rather than duplicated in Python: the table is derived from the
/// shipped code maps and pinned by a Rust test, so a Python copy could drift
/// from the pattern it describes.
#[pyfunction]
#[pyo3(signature = ())]
pub(crate) fn puzzlepole_periods() -> Vec<(u32, u32)> {
    puzzleboard::SUPPORTED_PERIODS
        .iter()
        .map(|p| (p.squares, p.start_row))
        .collect()
}

/// The canonical start row for a PuzzlePole circumference, or `None` if that
/// circumference has no seamless strip.
#[pyfunction]
#[pyo3(signature = (circumference_squares))]
pub(crate) fn puzzlepole_canonical_start_row(circumference_squares: u32) -> Option<u32> {
    puzzleboard::PuzzlePolePeriod::canonical(circumference_squares).map(|p| p.start_row)
}

// ---------------------------------------------------------------------------
// Printable target functions
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (document))]
pub(crate) fn render_target_bundle(
    py: Python<'_>,
    document: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
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
#[pyo3(signature = (target, screen))]
pub(crate) fn render_screen_target(
    py: Python<'_>,
    target: &Bound<'_, PyAny>,
    screen: &Bound<'_, PyAny>,
) -> PyResult<Py<PyAny>> {
    let target = from_py_json(target, "target")?;
    let screen = from_py_json(screen, "screen")?;
    let rendered = printable::render_screen_target(&target, &screen)
        .map_err(|err| value_error(err.to_string()))?;
    let json = rendered
        .metadata
        .to_json_pretty()
        .map_err(|err| PyRuntimeError::new_err(err.to_string()))?;
    let out = PyDict::new(py);
    out.set_item("json_text", json)?;
    out.set_item("png_bytes", PyBytes::new(py, &rendered.png_bytes))?;
    Ok(out.into_any().unbind())
}

#[pyfunction]
#[pyo3(signature = (document, output_stem))]
pub(crate) fn write_target_bundle(
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
