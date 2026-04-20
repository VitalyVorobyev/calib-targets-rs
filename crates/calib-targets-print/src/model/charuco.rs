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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoTargetSpec {
    pub rows: u32,
    pub cols: u32,
    pub square_size_mm: f64,
    pub marker_size_rel: f64,
    pub dictionary: Dictionary,
    #[serde(default)]
    pub marker_layout: MarkerLayout,
    #[serde(default = "default_border_bits")]
    pub border_bits: usize,
}

impl PartialEq for CharucoTargetSpec {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
            && self.cols == other.cols
            && self.square_size_mm == other.square_size_mm
            && self.marker_size_rel == other.marker_size_rel
            && self.dictionary.name == other.dictionary.name
            && self.marker_layout == other.marker_layout
            && self.border_bits == other.border_bits
    }
}

impl CharucoTargetSpec {
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
        CharucoBoardSpec {
            rows: self.rows,
            cols: self.cols,
            cell_size: self.square_size_mm as f32,
            marker_size_rel: self.marker_size_rel as f32,
            dictionary: self.dictionary,
            marker_layout: self.marker_layout,
        }
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
    let available = spec.dictionary.codes.len();
    if available < needed {
        return Err(PrintableTargetError::NotEnoughDictionaryCodes { needed, available });
    }
    Ok(())
}
