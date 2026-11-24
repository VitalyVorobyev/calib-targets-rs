//! Printable PuzzleBoard target specification.

use calib_targets_puzzleboard::{PuzzleBoardSpec, MASTER_COLS, MASTER_ROWS};
use serde::{Deserialize, Serialize};

use super::chessboard::validate_square_size;
use super::error::PrintableTargetError;

pub(crate) fn default_puzzleboard_dot_diameter_rel() -> f64 {
    // Paper recommends 1/3 (1.0 / 3.0).
    1.0 / 3.0
}

/// Printable PuzzleBoard target.
///
/// A PuzzleBoard is a standard checkerboard of `rows × cols` squares with a
/// small colour-coded dot at every interior edge midpoint. The dots encode
/// one bit each (black = 0, white = 1) via the two cyclic sub-perfect maps
/// shipped in `calib-targets-puzzleboard`. The printable board is a
/// contiguous sub-rectangle of the 501×501 master pattern anchored at
/// `(origin_row, origin_col)`.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PuzzleBoardTargetSpec {
    /// Number of board squares vertically.
    #[cfg_attr(feature = "schemars", schemars(range(min = 4, max = 501)))]
    pub rows: u32,
    /// Number of board squares horizontally.
    #[cfg_attr(feature = "schemars", schemars(range(min = 4, max = 501)))]
    pub cols: u32,
    /// Side length of one square in millimeters.
    #[cfg_attr(feature = "schemars", schemars(extend("exclusiveMinimum" = 0.0)))]
    #[cfg_attr(feature = "schemars", schemars(extend("x-unit" = "mm")))]
    pub square_size_mm: f64,
    /// Row offset into the 501×501 master pattern the board is cut from.
    /// `origin_row + rows` must not exceed 501.
    #[serde(default)]
    #[cfg_attr(feature = "schemars", schemars(range(max = 500)))]
    pub origin_row: u32,
    /// Column offset into the 501×501 master pattern the board is cut from.
    /// `origin_col + cols` must not exceed 501.
    #[serde(default)]
    #[cfg_attr(feature = "schemars", schemars(range(max = 500)))]
    pub origin_col: u32,
    /// Edge-dot diameter as a fraction of the square side.
    #[serde(default = "default_puzzleboard_dot_diameter_rel")]
    #[cfg_attr(feature = "schemars", schemars(extend("exclusiveMinimum" = 0.0)))]
    #[cfg_attr(feature = "schemars", schemars(range(max = 1.0)))]
    pub dot_diameter_rel: f64,
}

impl PuzzleBoardTargetSpec {
    /// Build a printable PuzzleBoard target from its square counts and square
    /// size (mm), anchored at master-pattern origin `(0, 0)`. Override the
    /// origin with [`PuzzleBoardTargetSpec::with_origin`] and the dot diameter
    /// with [`PuzzleBoardTargetSpec::with_dot_diameter_rel`].
    pub fn new(rows: u32, cols: u32, square_size_mm: f64) -> Self {
        Self {
            rows,
            cols,
            square_size_mm,
            origin_row: 0,
            origin_col: 0,
            dot_diameter_rel: default_puzzleboard_dot_diameter_rel(),
        }
    }

    /// Set the `(origin_row, origin_col)` offset into the master pattern.
    #[must_use]
    pub fn with_origin(mut self, origin_row: u32, origin_col: u32) -> Self {
        self.origin_row = origin_row;
        self.origin_col = origin_col;
        self
    }

    /// Override the edge-dot diameter as a fraction of the square side.
    #[must_use]
    pub fn with_dot_diameter_rel(mut self, dot_diameter_rel: f64) -> Self {
        self.dot_diameter_rel = dot_diameter_rel;
        self
    }

    /// Build a printable PuzzleBoard target from a detector board spec whose
    /// `cell_size` is already expressed in millimetres.
    pub fn from_board_spec_mm(board: &PuzzleBoardSpec) -> Self {
        Self::new(board.rows, board.cols, f64::from(board.cell_size))
            .with_origin(board.origin_row, board.origin_col)
    }

    /// Convert to the detector [`PuzzleBoardSpec`].
    pub fn to_board_spec(&self) -> Result<PuzzleBoardSpec, PrintableTargetError> {
        Ok(PuzzleBoardSpec::with_origin(
            self.rows,
            self.cols,
            self.square_size_mm as f32,
            self.origin_row,
            self.origin_col,
        )?)
    }
}

pub(crate) fn validate_puzzleboard_spec(
    spec: &PuzzleBoardTargetSpec,
) -> Result<(), PrintableTargetError> {
    if spec.rows < 4 || spec.cols < 4 || spec.rows > MASTER_ROWS || spec.cols > MASTER_COLS {
        return Err(PrintableTargetError::InvalidPuzzleBoardSize);
    }
    if spec.origin_row > MASTER_ROWS - spec.rows || spec.origin_col > MASTER_COLS - spec.cols {
        return Err(PrintableTargetError::InvalidPuzzleBoardOrigin);
    }
    validate_square_size(spec.square_size_mm)?;
    if !spec.dot_diameter_rel.is_finite()
        || spec.dot_diameter_rel <= 0.0
        || spec.dot_diameter_rel > 1.0
    {
        return Err(PrintableTargetError::InvalidPuzzleBoardDotDiameter);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn board_spec_bridge_round_trips_and_rejects_invalid() {
        let target = PuzzleBoardTargetSpec::new(12, 16, 10.5).with_origin(20, 30);
        let board = target.to_board_spec().expect("valid board spec");
        assert_eq!(board.rows, 12);
        assert_eq!(board.cols, 16);
        assert!((board.cell_size - 10.5).abs() < 1e-5);
        assert_eq!(board.origin_row, 20);
        assert_eq!(board.origin_col, 30);

        let back = PuzzleBoardTargetSpec::from_board_spec_mm(&board);
        assert_eq!(back.rows, target.rows);
        assert_eq!(back.cols, target.cols);
        assert_eq!(back.origin_row, target.origin_row);
        assert_eq!(back.origin_col, target.origin_col);
        assert!((back.square_size_mm - target.square_size_mm).abs() < 1e-5);

        let invalid = PuzzleBoardTargetSpec::new(3, 10, 10.0);
        assert!(matches!(
            invalid.to_board_spec(),
            Err(PrintableTargetError::PuzzleBoardSpec(_))
        ));
    }
}
