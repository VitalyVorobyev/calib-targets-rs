//! PuzzlePole — the PuzzleBoard pattern wrapped around a cylinder.
//!
//! A *PuzzlePole* is a strip of the master PuzzleBoard, chosen so that its two
//! ends meet without creating any pattern the code does not already contain,
//! wrapped around a cylinder. Because the pattern then closes, a pole is
//! recognisable from any direction — which is what a planar target cannot do.
//!
//! Introduced by Zach & Stelldinger, *PuzzlePoles: Cylindrical Fiducial Markers
//! Based on the PuzzleBoard Pattern* (arXiv:2511.19448), on top of the
//! PuzzleBoard pattern of Stelldinger, Schönherr & Biermann (arXiv:2409.20127).
//!
//! # What this module is, and is not
//!
//! It is a *specification and geometry* layer plus a seam-aware decode. It
//! reuses the planar PuzzleBoard detector wholesale — the same ChESS corners,
//! the same grid assembly, the same edge sampling, the same code maps — and
//! changes exactly one thing: the code's period along the wrapped axis.
//!
//! It is **not** a pose solver. Detection yields, per corner, a sub-pixel image
//! point and a 3-D object point in millimetres; feeding those to a PnP solver
//! is the caller's job. See [`geometry`] for the object frame.
//!
//! # Which axis wraps
//!
//! The circumference runs along the **master row** axis and the cylinder axis
//! along the **master column** axis. [`periods`] explains why the maps leave no
//! choice about that.
//!
//! # Choosing a pole
//!
//! Only the circumferences in [`periods::SUPPORTED_PERIODS`] close seamlessly,
//! so a pole's diameter is quantised: with pieces of side `cell`, a `p`-piece
//! circumference gives a cylinder of diameter `p * cell / pi`. Pick the period
//! that lands nearest the tube you have, then let [`PuzzlePoleSpec::new`]
//! report the diameter you must actually hit.

pub mod geometry;
pub mod periods;

use serde::{Deserialize, Serialize};

use crate::board::MASTER_COLS;
use nalgebra::{Point2, Point3};
use periods::PuzzlePolePeriod;

/// Smallest axial extent a pole may declare, in pieces.
///
/// The same floor [`PuzzleBoardSpec`](crate::PuzzleBoardSpec) applies to a
/// planar board, and for the same reason: below four pieces the master code
/// does not distinguish positions even at a fixed orientation. A *detector*
/// needs more than this — see
/// [`min_window`](crate::PuzzleBoardDecodeConfig::min_window).
pub const MIN_AXIAL_SQUARES: u32 = 4;

/// Specification of a physical PuzzlePole.
///
/// Construct with [`PuzzlePoleSpec::new`] for the paper's canonical strip at a
/// given circumference, or [`PuzzlePoleSpec::with_placement`] to choose the
/// strip explicitly — which is how two poles are made distinguishable.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PuzzlePoleSpec {
    /// Pieces around the circumference. One of
    /// [`periods::SUPPORTED_PERIODS`].
    pub circumference_squares: u32,
    /// Master row the wrapped strip starts at — the seam, and `theta = 0`.
    pub start_row: u32,
    /// Master column the pole's axial window starts at.
    ///
    /// This is what makes one pole different from another: two poles cut from
    /// disjoint column windows share no corner, so a corner's ID identifies its
    /// pole as well as its place on it. See [`PuzzlePoleSpec::distinct_poles`].
    pub axial_start_col: u32,
    /// Pieces along the cylinder axis. The pole is `axial_squares + 1` corner
    /// columns tall.
    pub axial_squares: u32,
    /// Physical size of one piece, in millimetres.
    pub cell_size_mm: f32,
}

/// Errors returned when building a [`PuzzlePoleSpec`].
#[non_exhaustive]
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PuzzlePoleSpecError {
    /// The `(circumference, start row)` pair is not one that closes seamlessly.
    ///
    /// Only the pairs in [`periods::SUPPORTED_PERIODS`] are accepted. A pair
    /// that merely satisfies the seam predicate is not enough — shipping a
    /// period means its decode uniqueness has been measured too.
    #[error(
        "no seamless PuzzlePole strip with {squares} pieces around the \
         circumference starting at master row {start_row}"
    )]
    UnsupportedPeriod {
        /// The requested circumference, in pieces.
        squares: u32,
        /// The requested start row.
        start_row: u32,
    },
    /// No supported strip exists at the requested circumference at all.
    #[error("{squares} pieces around the circumference is not a supported period")]
    UnsupportedCircumference {
        /// The requested circumference, in pieces.
        squares: u32,
    },
    /// The axial extent is below [`MIN_AXIAL_SQUARES`].
    #[error("axial_squares must be >= {MIN_AXIAL_SQUARES}")]
    AxialTooSmall,
    /// The pole's axial window runs off the master pattern.
    #[error(
        "a pole starting at master column {axial_start_col} and spanning \
         {axial_squares} pieces runs off the 501-column master"
    )]
    AxialOutOfRange {
        /// The requested axial start column.
        axial_start_col: u32,
        /// The requested axial extent, in pieces.
        axial_squares: u32,
    },
    /// `cell_size_mm` is not a positive, finite length.
    #[error("cell_size_mm must be finite and > 0")]
    InvalidCellSize,
}

impl PuzzlePoleSpec {
    /// A pole at the canonical strip for `circumference_squares`, cut from the
    /// start of the master.
    ///
    /// The canonical strip is the paper's, so a pole built this way is the same
    /// physical pattern as one built from the paper's parameters.
    pub fn new(
        circumference_squares: u32,
        axial_squares: u32,
        cell_size_mm: f32,
    ) -> Result<Self, PuzzlePoleSpecError> {
        let period = PuzzlePolePeriod::canonical(circumference_squares).ok_or(
            PuzzlePoleSpecError::UnsupportedCircumference {
                squares: circumference_squares,
            },
        )?;
        Self::with_placement(
            circumference_squares,
            period.start_row,
            0,
            axial_squares,
            cell_size_mm,
        )
    }

    /// A pole at an explicitly chosen strip and axial window.
    pub fn with_placement(
        circumference_squares: u32,
        start_row: u32,
        axial_start_col: u32,
        axial_squares: u32,
        cell_size_mm: f32,
    ) -> Result<Self, PuzzlePoleSpecError> {
        if PuzzlePolePeriod::lookup(circumference_squares, start_row).is_none() {
            return Err(PuzzlePoleSpecError::UnsupportedPeriod {
                squares: circumference_squares,
                start_row,
            });
        }
        if axial_squares < MIN_AXIAL_SQUARES {
            return Err(PuzzlePoleSpecError::AxialTooSmall);
        }
        // The window is measured in corner columns: `axial_squares` pieces span
        // `axial_squares + 1` corners, and all of them must exist on the master.
        if axial_start_col + axial_squares + 1 > MASTER_COLS {
            return Err(PuzzlePoleSpecError::AxialOutOfRange {
                axial_start_col,
                axial_squares,
            });
        }
        if !cell_size_mm.is_finite() || cell_size_mm <= 0.0 {
            return Err(PuzzlePoleSpecError::InvalidCellSize);
        }
        Ok(Self {
            circumference_squares,
            start_row,
            axial_start_col,
            axial_squares,
            cell_size_mm,
        })
    }

    /// The verified period this pole is built on.
    #[must_use]
    pub fn period(&self) -> PuzzlePolePeriod {
        PuzzlePolePeriod::lookup(self.circumference_squares, self.start_row)
            .expect("a constructed spec always names a supported period")
    }

    /// Distinct corner rows around the circumference: `circumference_squares`.
    ///
    /// Distinct is the operative word — a wrapped pole has exactly this many,
    /// however many the printed sheet shows.
    #[must_use]
    pub const fn circumference_corner_rows(&self) -> u32 {
        self.circumference_squares
    }

    /// Pieces on the printable strip: `circumference_squares + 1`.
    ///
    /// One more than wraps, because the renderer only draws a dot on an edge
    /// that has a square on both sides. Printing the extra piece gives the seam
    /// edge its dot, and that piece is a duplicate of the first — so it doubles
    /// as the glue overlap. The paper's Table 1 asks its generator for two
    /// extra rows for the same reason.
    #[must_use]
    pub const fn printed_strip_squares(&self) -> u32 {
        self.circumference_squares + 1
    }

    /// Corner columns along the cylinder axis: `axial_squares + 1`.
    #[must_use]
    pub const fn axial_corner_cols(&self) -> u32 {
        self.axial_squares + 1
    }

    /// Distinct corners on the pole, once the duplicated seam row is collapsed.
    #[must_use]
    pub const fn corner_count(&self) -> u32 {
        self.circumference_squares * self.axial_corner_cols()
    }

    /// Nominal circumference, in millimetres.
    #[must_use]
    pub fn circumference_mm(&self) -> f32 {
        self.circumference_squares as f32 * self.cell_size_mm
    }

    /// Nominal cylinder radius, in millimetres.
    #[must_use]
    pub fn radius_mm(&self) -> f32 {
        geometry::radius_mm(self.circumference_squares, self.cell_size_mm)
    }

    /// Nominal cylinder diameter, in millimetres — the tube to buy.
    #[must_use]
    pub fn diameter_mm(&self) -> f32 {
        2.0 * self.radius_mm()
    }

    /// Extent along the cylinder axis, lowest to highest corner, in millimetres.
    #[must_use]
    pub fn axial_extent_mm(&self) -> f32 {
        self.axial_squares as f32 * self.cell_size_mm
    }

    /// Where corner `(axial, cyclic)` sits on the unrolled strip, in
    /// millimetres.
    ///
    /// Ordered `(z, arc)`, matching a planar board's `(col, row)`
    /// `target_position` — see [`geometry`] on index roles.
    #[must_use]
    pub fn surface_position(&self, axial: u32, cyclic: u32) -> Point2<f32> {
        geometry::surface_position(
            axial,
            cyclic % self.circumference_squares,
            self.cell_size_mm,
        )
    }

    /// Where corner `(axial, cyclic)` sits in the pole's 3-D object frame, in
    /// millimetres.
    #[must_use]
    pub fn object_position(&self, axial: u32, cyclic: u32) -> Point3<f32> {
        geometry::object_position(self.circumference_squares, axial, cyclic, self.cell_size_mm)
    }

    /// The master `(row, col)` corner `(axial, cyclic)` is cut from.
    ///
    /// The row wraps at the period; the column does not.
    #[must_use]
    pub fn master_corner(&self, axial: u32, cyclic: u32) -> (u32, u32) {
        (
            self.start_row + cyclic % self.circumference_squares,
            self.axial_start_col + axial,
        )
    }

    /// Dense logical id of corner `(axial, cyclic)`.
    ///
    /// Bijective with the `(axial, cyclic)` pair over the pole, so an id
    /// identifies a physical corner and nothing else.
    #[must_use]
    pub fn corner_id(&self, axial: u32, cyclic: u32) -> u32 {
        (cyclic % self.circumference_squares) * self.axial_corner_cols() + axial
    }

    /// Every pole of this shape that can be cut from the master without sharing
    /// a corner with another.
    ///
    /// Poles are laid out along the axial axis in disjoint corner-column
    /// windows, so no corner belongs to two of them and a decoded corner
    /// identifies its pole. This reproduces the paper's counts: a 7-corner-
    /// column pole yields `501 / 7 = 71` distinct poles.
    pub fn distinct_poles(
        circumference_squares: u32,
        axial_squares: u32,
        cell_size_mm: f32,
    ) -> Result<Vec<Self>, PuzzlePoleSpecError> {
        // Validate once at column 0 so a bad circumference or cell size is
        // reported as itself rather than as an empty list.
        let first = Self::new(circumference_squares, axial_squares, cell_size_mm)?;
        let stride = first.axial_corner_cols();
        Ok((0..MASTER_COLS / stride)
            .map(|n| Self {
                axial_start_col: n * stride,
                ..first
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_papers_pole_round_trips() {
        // "constructed from a 7 x 12 periodic PuzzleBoard pattern with the y
        // corner point IDs from 73 to 85. The edge length of a Puzzle piece is
        // 3 centimeters. Thus, the PuzzlePoles each have a diameter of 11.46cm
        // and a height ... of 18cm."
        let pole = PuzzlePoleSpec::new(12, 6, 30.0).expect("period 12 is supported");
        assert_eq!(pole.start_row, 73);
        // Corner rows 73..=85 is 13 labels with the last the same as the first.
        assert_eq!(pole.circumference_corner_rows(), 12);
        assert_eq!(pole.master_corner(0, 0).0, 73);
        assert_eq!(pole.master_corner(0, 12).0, 73);
        assert_eq!(pole.axial_corner_cols(), 7);
        assert!((pole.diameter_mm() - 114.59).abs() < 0.01);
        assert!((pole.axial_extent_mm() - 180.0).abs() < 1e-3);
    }

    #[test]
    fn distinct_poles_reproduce_the_papers_counts() {
        // A 7-corner-column pole: 501 / 7 = 71.
        let short = PuzzlePoleSpec::distinct_poles(12, 6, 30.0).expect("valid");
        assert_eq!(short.len(), 71);
        // A 21-corner-column pole: 501 / 21 = 23.
        let tall = PuzzlePoleSpec::distinct_poles(12, 20, 30.0).expect("valid");
        assert_eq!(tall.len(), 23);
    }

    #[test]
    fn distinct_poles_share_no_corner() {
        let poles = PuzzlePoleSpec::distinct_poles(12, 6, 30.0).expect("valid");
        let mut columns: Vec<u32> = Vec::new();
        for pole in &poles {
            for axial in 0..pole.axial_corner_cols() {
                columns.push(pole.master_corner(axial, 0).1);
            }
        }
        let before = columns.len();
        columns.sort_unstable();
        columns.dedup();
        assert_eq!(
            before,
            columns.len(),
            "two poles claim the same master column"
        );
    }

    #[test]
    fn the_last_strip_row_is_the_first_row_again() {
        let pole = PuzzlePoleSpec::new(12, 6, 30.0).expect("valid");
        let seam = pole.circumference_corner_rows();
        assert_eq!(pole.master_corner(2, 0), pole.master_corner(2, seam));
        assert_eq!(pole.object_position(2, 0), pole.object_position(2, seam));
        assert_eq!(pole.corner_id(2, 0), pole.corner_id(2, seam));
    }

    #[test]
    fn an_unsupported_circumference_is_rejected() {
        assert_eq!(
            PuzzlePoleSpec::new(13, 6, 30.0),
            Err(PuzzlePoleSpecError::UnsupportedCircumference { squares: 13 })
        );
    }

    /// The paper's period-36 start row does not close on these maps, and the
    /// spec must say so rather than silently building a broken pole.
    #[test]
    fn the_papers_uncorrected_period_36_start_row_is_rejected() {
        assert_eq!(
            PuzzlePoleSpec::with_placement(36, 325 % 167, 0, 6, 30.0),
            Err(PuzzlePoleSpecError::UnsupportedPeriod {
                squares: 36,
                start_row: 325 % 167,
            })
        );
        assert!(PuzzlePoleSpec::with_placement(36, 327 % 167, 0, 6, 30.0).is_ok());
    }

    #[test]
    fn a_pole_that_runs_off_the_master_is_rejected() {
        let err = PuzzlePoleSpec::with_placement(12, 73, 495, 6, 30.0).unwrap_err();
        assert!(matches!(err, PuzzlePoleSpecError::AxialOutOfRange { .. }));
        // 494 + 6 + 1 = 501 corner columns exactly — the last legal placement.
        assert!(PuzzlePoleSpec::with_placement(12, 73, 494, 6, 30.0).is_ok());
    }

    #[test]
    fn a_degenerate_cell_size_is_rejected() {
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert_eq!(
                PuzzlePoleSpec::new(12, 6, bad),
                Err(PuzzlePoleSpecError::InvalidCellSize),
                "cell size {bad} was accepted"
            );
        }
    }

    #[test]
    fn too_short_a_pole_is_rejected() {
        assert_eq!(
            PuzzlePoleSpec::new(12, MIN_AXIAL_SQUARES - 1, 30.0),
            Err(PuzzlePoleSpecError::AxialTooSmall)
        );
        assert!(PuzzlePoleSpec::new(12, MIN_AXIAL_SQUARES, 30.0).is_ok());
    }

    #[test]
    fn the_spec_serde_round_trips() {
        let pole = PuzzlePoleSpec::new(18, 8, 12.5).expect("valid");
        let json = serde_json::to_string(&pole).expect("serialize");
        let back: PuzzlePoleSpec = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(pole, back);
    }

    #[test]
    fn corner_count_collapses_the_duplicated_seam_row() {
        let pole = PuzzlePoleSpec::new(12, 6, 30.0).expect("valid");
        // 13 strip corner rows x 7 columns printed, but row 12 == row 0.
        assert_eq!(pole.corner_count(), 12 * 7);
    }
}
