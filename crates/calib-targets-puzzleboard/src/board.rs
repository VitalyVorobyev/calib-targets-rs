//! Board specification for a PuzzleBoard target.
//!
//! A *PuzzleBoard* is physically a checkerboard of `rows × cols` squares
//! (inner chessboard corners: `(rows - 1) × (cols - 1)`). The committed master
//! pattern is [`MASTER_ROWS`]×[`MASTER_COLS`] = 501×501 squares. Any printable
//! board is a contiguous sub-rectangle of the master pattern, anchored at
//! `(origin_row, origin_col)` on the master.

use serde::{Deserialize, Serialize};

/// Number of rows in the master PuzzleBoard pattern.
pub const MASTER_ROWS: u32 = 501;
/// Number of columns in the master PuzzleBoard pattern.
pub const MASTER_COLS: u32 = 501;

/// Specification of a printable PuzzleBoard.
///
/// `rows` and `cols` are **square counts** — the inner-corner grid has
/// `(rows - 1) × (cols - 1)` corners.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PuzzleBoardSpec {
    /// Number of squares vertically.
    pub rows: u32,
    /// Number of squares horizontally.
    pub cols: u32,
    /// Physical size of one square (typically millimetres in board frame).
    pub cell_size: f32,
    /// Row offset into the 501×501 master pattern from which this board is cut.
    #[serde(default)]
    pub origin_row: u32,
    /// Column offset into the 501×501 master pattern from which this board is cut.
    #[serde(default)]
    pub origin_col: u32,
}

/// Errors returned by [`PuzzleBoardSpec::new`].
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum PuzzleBoardSpecError {
    #[error("rows and cols must be >= 4")]
    TooSmall,
    #[error("rows * cols exceeds the master 501×501 pattern")]
    TooLarge,
    #[error(
        "origin (rows={origin_row}, cols={origin_col}) plus board size exceeds master pattern"
    )]
    OriginOutOfRange { origin_row: u32, origin_col: u32 },
    #[error("cell_size must be finite and > 0")]
    InvalidCellSize,
}

impl PuzzleBoardSpec {
    /// Build a spec anchored at origin `(0, 0)` on the master pattern.
    pub fn new(rows: u32, cols: u32, cell_size: f32) -> Result<Self, PuzzleBoardSpecError> {
        Self::with_origin(rows, cols, cell_size, 0, 0)
    }

    /// Build a spec anchored at an arbitrary origin on the master pattern.
    pub fn with_origin(
        rows: u32,
        cols: u32,
        cell_size: f32,
        origin_row: u32,
        origin_col: u32,
    ) -> Result<Self, PuzzleBoardSpecError> {
        if rows < 4 || cols < 4 {
            return Err(PuzzleBoardSpecError::TooSmall);
        }
        if rows > MASTER_ROWS || cols > MASTER_COLS {
            return Err(PuzzleBoardSpecError::TooLarge);
        }
        if origin_row + rows > MASTER_ROWS || origin_col + cols > MASTER_COLS {
            return Err(PuzzleBoardSpecError::OriginOutOfRange {
                origin_row,
                origin_col,
            });
        }
        if !cell_size.is_finite() || cell_size <= 0.0 {
            return Err(PuzzleBoardSpecError::InvalidCellSize);
        }
        Ok(Self {
            rows,
            cols,
            cell_size,
            origin_row,
            origin_col,
        })
    }

    /// Expected number of **inner** corner rows (`rows - 1`).
    #[inline]
    pub fn inner_rows(&self) -> u32 {
        self.rows - 1
    }

    /// Expected number of **inner** corner columns (`cols - 1`).
    #[inline]
    pub fn inner_cols(&self) -> u32 {
        self.cols - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_tiny_board() {
        assert!(matches!(
            PuzzleBoardSpec::new(3, 10, 1.0),
            Err(PuzzleBoardSpecError::TooSmall)
        ));
    }

    #[test]
    fn rejects_out_of_master() {
        assert!(matches!(
            PuzzleBoardSpec::new(502, 10, 1.0),
            Err(PuzzleBoardSpecError::TooLarge)
        ));
    }

    #[test]
    fn origin_must_fit() {
        assert!(matches!(
            PuzzleBoardSpec::with_origin(10, 10, 1.0, 495, 0),
            Err(PuzzleBoardSpecError::OriginOutOfRange { .. })
        ));
    }

    #[test]
    fn cell_size_positive() {
        assert!(matches!(
            PuzzleBoardSpec::new(10, 10, 0.0),
            Err(PuzzleBoardSpecError::InvalidCellSize)
        ));
    }

    #[test]
    fn valid_spec() {
        let s = PuzzleBoardSpec::new(12, 12, 1.0).unwrap();
        assert_eq!(s.inner_rows(), 11);
        assert_eq!(s.inner_cols(), 11);
    }
}
