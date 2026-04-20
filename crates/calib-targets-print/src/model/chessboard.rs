//! Printable chessboard target specification.

use serde::{Deserialize, Serialize};

use super::error::PrintableTargetError;

/// Printable chessboard target.
///
/// `inner_rows × inner_cols` refers to the number of *inner corner
/// intersections* (not squares).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChessboardTargetSpec {
    pub inner_rows: u32,
    pub inner_cols: u32,
    pub square_size_mm: f64,
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
