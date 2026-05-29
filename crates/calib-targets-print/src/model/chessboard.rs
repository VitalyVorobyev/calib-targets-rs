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
}

impl ChessboardTargetSpec {
    /// Build a printable chessboard target from its inner-corner grid size and
    /// square size in millimeters.
    pub fn new(inner_rows: u32, inner_cols: u32, square_size_mm: f64) -> Self {
        Self {
            inner_rows,
            inner_cols,
            square_size_mm,
        }
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

pub(crate) fn validate_square_size(square_size_mm: f64) -> Result<(), PrintableTargetError> {
    if !square_size_mm.is_finite() || square_size_mm <= 0.0 {
        return Err(PrintableTargetError::InvalidSquareSize);
    }
    Ok(())
}
