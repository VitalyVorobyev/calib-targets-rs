//! Printable ChArUco target specification.

use calib_targets_aruco::Dictionary;
use calib_targets_charuco::{CharucoBoard, CharucoBoardSpec, MarkerLayout};
use serde::{Deserialize, Serialize};

use super::chessboard::validate_square_size;
use super::error::PrintableTargetError;

pub(super) fn default_border_bits() -> usize {
    1
}

/// Printable ChArUco target.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoTargetSpec {
    /// Number of board squares vertically.
    pub rows: u32,
    /// Number of board squares horizontally.
    pub cols: u32,
    /// Side length of one square in millimeters.
    pub square_size_mm: f64,
    /// Marker side length as a fraction of the square side, in `(0, 1]`.
    pub marker_size_rel: f64,
    /// The ArUco dictionary the markers are drawn from.
    pub dictionary: Dictionary,
    /// How markers are placed and numbered on the board.
    #[serde(default)]
    pub marker_layout: MarkerLayout,
    /// Marker border width in cells.
    #[serde(default = "default_border_bits")]
    pub border_bits: usize,
}

impl PartialEq for CharucoTargetSpec {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
            && self.cols == other.cols
            && self.square_size_mm == other.square_size_mm
            && self.marker_size_rel == other.marker_size_rel
            && self.dictionary.name() == other.dictionary.name()
            && self.marker_layout == other.marker_layout
            && self.border_bits == other.border_bits
    }
}

impl CharucoTargetSpec {
    /// Build a printable ChArUco target from its square counts, square size
    /// (mm), marker scale, and dictionary. The marker layout and border-bit
    /// count default; override them with [`CharucoTargetSpec::with_marker_layout`]
    /// and [`CharucoTargetSpec::with_border_bits`].
    pub fn new(
        rows: u32,
        cols: u32,
        square_size_mm: f64,
        marker_size_rel: f64,
        dictionary: Dictionary,
    ) -> Self {
        Self {
            rows,
            cols,
            square_size_mm,
            marker_size_rel,
            dictionary,
            marker_layout: MarkerLayout::default(),
            border_bits: default_border_bits(),
        }
    }

    /// Override the marker placement / numbering scheme.
    #[must_use]
    pub fn with_marker_layout(mut self, marker_layout: MarkerLayout) -> Self {
        self.marker_layout = marker_layout;
        self
    }

    /// Override the marker border width in cells.
    #[must_use]
    pub fn with_border_bits(mut self, border_bits: usize) -> Self {
        self.border_bits = border_bits;
        self
    }

    /// Build a printable ChArUco target from a detector board spec whose
    /// `cell_size` is already expressed in millimeters.
    pub fn from_board_spec_mm(board: &CharucoBoardSpec) -> Self {
        Self {
            rows: board.rows,
            cols: board.cols,
            square_size_mm: f64::from(board.cell_size),
            marker_size_rel: f64::from(board.marker_size_rel),
            dictionary: board.dictionary,
            marker_layout: board.marker_layout,
            border_bits: default_border_bits(),
        }
    }

    /// Convert to the detector `CharucoBoardSpec`.
    pub fn to_board_spec(&self) -> CharucoBoardSpec {
        CharucoBoardSpec::new(
            self.rows,
            self.cols,
            self.square_size_mm as f32,
            self.marker_size_rel as f32,
            self.dictionary,
        )
        .with_marker_layout(self.marker_layout)
    }
}

pub(crate) fn validate_charuco_spec(spec: &CharucoTargetSpec) -> Result<(), PrintableTargetError> {
    if spec.rows < 2 || spec.cols < 2 {
        return Err(PrintableTargetError::InvalidCharucoSize);
    }
    validate_square_size(spec.square_size_mm)?;
    if !spec.marker_size_rel.is_finite()
        || spec.marker_size_rel <= 0.0
        || spec.marker_size_rel > 1.0
    {
        return Err(PrintableTargetError::InvalidMarkerSizeRel);
    }
    if spec.border_bits == 0 {
        return Err(PrintableTargetError::InvalidBorderBits);
    }
    let board = spec.to_board_spec();
    let board = CharucoBoard::new(board)?;
    let needed = board.marker_count();
    let available = spec.dictionary.codes().len();
    if available < needed {
        return Err(PrintableTargetError::NotEnoughDictionaryCodes { needed, available });
    }
    Ok(())
}
