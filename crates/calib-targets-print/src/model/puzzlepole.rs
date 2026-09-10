//! Printable PuzzlePole target specification.

use calib_targets_puzzleboard::{PuzzlePolePeriod, MASTER_COLS};
use serde::{Deserialize, Serialize};

use super::chessboard::validate_square_size;
use super::error::PrintableTargetError;
use super::puzzleboard::{default_puzzleboard_dot_diameter_rel, PuzzleBoardTargetSpec};

/// Printable PuzzlePole wrap strip.
///
/// A PuzzlePole is a PuzzleBoard strip wrapped around a cylinder. The strip is
/// cut from the master pattern along a **circumference period** — one of the
/// few row spacings at which the pattern genuinely repeats — so that its two
/// ends meet without creating any local code the master does not already
/// contain. See `calib_targets_puzzleboard::pole` for the construction.
///
/// # Assembling it
///
/// The strip is printed `circumference_squares + 2` pieces tall, two more than
/// wraps. Trim it through the **mid-line of the first and last pieces** — the
/// line through those pieces' vertical-edge dots — leaving one piece of spare
/// material. Wrap it around the cylinder and lay that last piece exactly over
/// the first.
///
/// The overlap is invisible only because the seam repeats **two** consecutive
/// rows of the master rather than one: the overlapping band has to reproduce
/// the band it covers, and two rows is exactly what that costs. Trimming
/// mid-piece rather than at a piece boundary is what keeps every edge dot
/// whole — a dot sits *on* the joint, so cutting there would halve it.
///
/// # Printing
///
/// Print at 100 % scale — the pole's diameter is fixed by
/// `circumference_squares * square_size_mm / pi`, and a scaled print silently
/// changes it. The strip runs *around* the cylinder down the page and *along*
/// the cylinder across it: page **y** is the circumference, page **x** the
/// cylinder axis.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PuzzlePoleTargetSpec {
    /// Pieces around the circumference. Must be a supported period.
    pub circumference_squares: u32,
    /// Master row the strip is cut from — the seam.
    pub start_row: u32,
    /// Master column the strip is cut from.
    #[serde(default)]
    pub axial_start_col: u32,
    /// Pieces along the cylinder axis.
    pub axial_squares: u32,
    /// Side length of one piece in millimeters.
    pub square_size_mm: f64,
    /// Edge-dot diameter as a fraction of the piece side.
    #[serde(default = "default_puzzleboard_dot_diameter_rel")]
    pub dot_diameter_rel: f64,
}

impl PuzzlePoleTargetSpec {
    /// Build a wrap strip at the canonical start row for `circumference_squares`,
    /// cut from the start of the master.
    ///
    /// Returns `None` when the circumference is not a supported period — the
    /// diameter of a PuzzlePole is quantised, and there is no sensible
    /// fallback. Override the master column with
    /// [`PuzzlePoleTargetSpec::with_axial_start_col`] and the dot diameter with
    /// [`PuzzlePoleTargetSpec::with_dot_diameter_rel`].
    pub fn new(
        circumference_squares: u32,
        axial_squares: u32,
        square_size_mm: f64,
    ) -> Option<Self> {
        let period = PuzzlePolePeriod::canonical(circumference_squares)?;
        Some(Self::at_period(period, axial_squares, square_size_mm))
    }

    /// Build a wrap strip at an already-verified period.
    ///
    /// Unlike [`PuzzlePoleTargetSpec::new`] this cannot fail: a
    /// [`PuzzlePolePeriod`] only exists for a circumference that closes. Use it
    /// to pick a strip other than the canonical one — several circumferences
    /// admit more than one, and each is a different pattern, so a different
    /// pole.
    #[must_use]
    pub fn at_period(period: PuzzlePolePeriod, axial_squares: u32, square_size_mm: f64) -> Self {
        Self {
            circumference_squares: period.squares,
            start_row: period.start_row,
            axial_start_col: 0,
            axial_squares,
            square_size_mm,
            dot_diameter_rel: default_puzzleboard_dot_diameter_rel(),
        }
    }

    /// Set the master column the strip is cut from.
    ///
    /// Strips cut from disjoint column windows share no corner, which is how
    /// several poles are told apart.
    #[must_use]
    pub fn with_axial_start_col(mut self, axial_start_col: u32) -> Self {
        self.axial_start_col = axial_start_col;
        self
    }

    /// Override the edge-dot diameter as a fraction of the piece side.
    #[must_use]
    pub fn with_dot_diameter_rel(mut self, dot_diameter_rel: f64) -> Self {
        self.dot_diameter_rel = dot_diameter_rel;
        self
    }

    /// Pieces printed around the circumference: two more than wraps, leaving
    /// one piece of overlap after trimming mid-piece at both ends.
    #[must_use]
    pub const fn printed_strip_squares(&self) -> u32 {
        self.circumference_squares + 2
    }

    /// Diameter of the cylinder the strip is meant for, in millimetres.
    #[must_use]
    pub fn diameter_mm(&self) -> f64 {
        self.circumference_squares as f64 * self.square_size_mm / core::f64::consts::PI
    }

    /// The equivalent PuzzleBoard sub-rectangle.
    ///
    /// A wrap strip *is* a sub-rectangle of the master — the periodicity is
    /// what makes its two ends meet, not any change to the pattern — so the
    /// renderer and the layout both go through the PuzzleBoard path rather
    /// than a parallel implementation.
    pub(crate) fn as_board(&self) -> PuzzleBoardTargetSpec {
        PuzzleBoardTargetSpec {
            rows: self.printed_strip_squares(),
            cols: self.axial_squares,
            square_size_mm: self.square_size_mm,
            origin_row: self.start_row,
            origin_col: self.axial_start_col,
            dot_diameter_rel: self.dot_diameter_rel,
        }
    }
}

pub(crate) fn validate_puzzlepole_spec(
    spec: &PuzzlePoleTargetSpec,
) -> Result<(), PrintableTargetError> {
    if PuzzlePolePeriod::lookup(spec.circumference_squares, spec.start_row).is_none() {
        return Err(PrintableTargetError::UnsupportedPuzzlePolePeriod {
            circumference_squares: spec.circumference_squares,
            start_row: spec.start_row,
        });
    }
    // The strip is measured in corner columns: `axial_squares` pieces span
    // `axial_squares + 1` corners, and all of them must exist on the master.
    if spec.axial_squares < 4 || spec.axial_start_col + spec.axial_squares + 1 > MASTER_COLS {
        return Err(PrintableTargetError::InvalidPuzzlePoleAxialExtent {
            axial_start_col: spec.axial_start_col,
            axial_squares: spec.axial_squares,
        });
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
