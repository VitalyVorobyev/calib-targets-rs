//! Printable chessboard target specification.

use serde::{Deserialize, Serialize};

use super::error::PrintableTargetError;

/// Printable chessboard target.
///
/// `inner_rows × inner_cols` refers to the number of *inner corner
/// intersections* (not squares).
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChessboardTargetSpec {
    /// Number of inner corner-intersection rows.
    pub inner_rows: u32,
    /// Number of inner corner-intersection columns.
    pub inner_cols: u32,
    /// Side length of one square in millimeters.
    pub square_size_mm: f64,
    /// White inset square drawn centred inside every black square, as a
    /// fraction of the square side, in `[0.0, 1.0)`. `None` (the default)
    /// and `Some(0.0)` both mean no inset; the two are equivalent because
    /// downstream callers always send an explicit number with `0` meaning
    /// "off". Does not move any corner intersection — see
    /// [`crate::TargetSpec::resolved_points`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_square_rel: Option<f64>,
}

impl ChessboardTargetSpec {
    /// Build a printable chessboard target from its inner-corner grid size and
    /// square size in millimeters.
    pub fn new(inner_rows: u32, inner_cols: u32, square_size_mm: f64) -> Self {
        Self {
            inner_rows,
            inner_cols,
            square_size_mm,
            inner_square_rel: None,
        }
    }

    /// Draw a white square inset, centred inside every black square, whose
    /// side is `rel` times the square side.
    #[must_use]
    pub fn with_inner_square_rel(mut self, rel: f64) -> Self {
        self.inner_square_rel = Some(rel);
        self
    }
}

pub(crate) fn validate_inner_corner_grid(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
) -> Result<(), PrintableTargetError> {
    if inner_rows < 2 || inner_cols < 2 {
        return Err(PrintableTargetError::InvalidChessboardSize);
    }
    validate_square_size(square_size_mm)
}

/// Validate a full [`ChessboardTargetSpec`]: the inner-corner grid, the
/// square size, and the optional inner-square inset ratio.
pub(crate) fn validate_chessboard_spec(
    spec: &ChessboardTargetSpec,
) -> Result<(), PrintableTargetError> {
    validate_inner_corner_grid(spec.inner_rows, spec.inner_cols, spec.square_size_mm)?;
    validate_inner_square_rel(spec.inner_square_rel)
}

pub(crate) fn validate_square_size(square_size_mm: f64) -> Result<(), PrintableTargetError> {
    if !square_size_mm.is_finite() || square_size_mm <= 0.0 {
        return Err(PrintableTargetError::InvalidSquareSize);
    }
    Ok(())
}

/// Validate an optional inner-square inset ratio.
///
/// `None` and `Some(0.0)` are both accepted and mean "no inset". Any other
/// value must be finite and fall in `[0.0, 1.0)` — at `1.0` the inset would
/// erase the square it is cut from entirely.
pub(crate) fn validate_inner_square_rel(
    inner_square_rel: Option<f64>,
) -> Result<(), PrintableTargetError> {
    if let Some(rel) = inner_square_rel {
        if !rel.is_finite() || rel < 0.0 || rel >= 1.0 {
            return Err(PrintableTargetError::InvalidInnerSquareRel);
        }
    }
    Ok(())
}
