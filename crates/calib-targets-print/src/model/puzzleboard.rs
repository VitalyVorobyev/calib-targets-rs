//! Printable PuzzleBoard target specification.

use calib_targets_puzzleboard::{MASTER_COLS, MASTER_ROWS};
use serde::{Deserialize, Serialize};

use super::chessboard::validate_square_size;
use super::error::PrintableTargetError;

pub(super) fn default_puzzleboard_dot_diameter_rel() -> f64 {
    // Paper recommends 1/3 (1.0 / 3.0).
    1.0 / 3.0
}

/// Printable PuzzleBoard target.
///
/// A PuzzleBoard is a standard checkerboard of `rows × cols` squares with a
/// small colour-coded dot at every interior edge midpoint. The dots encode
/// one bit each (white = 0, black = 1) via the two cyclic sub-perfect maps
/// shipped in `calib-targets-puzzleboard`. The printable board is a
/// contiguous sub-rectangle of the 501×501 master pattern anchored at
/// `(origin_row, origin_col)`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PuzzleBoardTargetSpec {
    /// Number of board squares vertically.
    pub rows: u32,
    /// Number of board squares horizontally.
    pub cols: u32,
    /// Side length of one square in millimeters.
    pub square_size_mm: f64,
    /// Row offset into the 501×501 master pattern the board is cut from.
    #[serde(default)]
    pub origin_row: u32,
    /// Column offset into the 501×501 master pattern the board is cut from.
    #[serde(default)]
    pub origin_col: u32,
    /// Edge-dot diameter as a fraction of the square side.
    #[serde(default = "default_puzzleboard_dot_diameter_rel")]
    pub dot_diameter_rel: f64,
}

pub(crate) fn validate_puzzleboard_spec(
    spec: &PuzzleBoardTargetSpec,
) -> Result<(), PrintableTargetError> {
    if spec.rows < 4 || spec.cols < 4 || spec.rows > MASTER_ROWS || spec.cols > MASTER_COLS {
        return Err(PrintableTargetError::InvalidPuzzleBoardSize);
    }
    if spec.origin_row + spec.rows > MASTER_ROWS || spec.origin_col + spec.cols > MASTER_COLS {
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
