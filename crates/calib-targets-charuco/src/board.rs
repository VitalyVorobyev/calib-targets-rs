//! Board specification and layout helpers for ChArUco.

use calib_targets_aruco::Dictionary;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

/// Marker placement scheme for the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MarkerLayout {
    /// OpenCV-style ChArUco layout:
    /// - markers are placed on white squares only (assuming top-left square is black),
    /// - marker IDs are assigned sequentially in row-major order over those squares.
    #[serde(rename = "opencv_charuco", alias = "open_cv_charuco")]
    #[default]
    OpenCvCharuco,
}

/// Static ChArUco board specification.
///
/// `rows`/`cols` are **square counts** (not inner corner counts).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CharucoBoardSpec {
    pub rows: u32,
    pub cols: u32,
    pub cell_size: f32,
    pub marker_size_rel: f32,
    pub dictionary: Dictionary,
    #[serde(default)]
    pub marker_layout: MarkerLayout,
}

/// Board specification validation errors.
#[derive(thiserror::Error, Debug)]
pub enum CharucoBoardError {
    #[error("rows and cols must be >= 2")]
    InvalidSize,
    #[error("cell_size must be > 0")]
    InvalidCellSize,
    #[error("marker_size_rel must be in (0, 1]")]
    InvalidMarkerSizeRel,
    #[error("dictionary has no codes")]
    EmptyDictionary,
    #[error("board needs {needed} markers, dictionary has {available}")]
    NotEnoughDictionaryCodes { needed: usize, available: usize },
}

/// Precomputed board mapping helpers.
#[derive(Clone, Debug)]
pub struct CharucoBoard {
    spec: CharucoBoardSpec,
    marker_positions: Vec<[i32; 2]>,
}

impl CharucoBoard {
    /// Validate and create a board from a spec.
    pub fn new(spec: CharucoBoardSpec) -> Result<Self, CharucoBoardError> {
        if spec.rows < 2 || spec.cols < 2 {
            return Err(CharucoBoardError::InvalidSize);
        }
        if !spec.cell_size.is_finite() || spec.cell_size <= 0.0 {
            return Err(CharucoBoardError::InvalidCellSize);
        }
        if !spec.marker_size_rel.is_finite()
            || spec.marker_size_rel <= 0.0
            || spec.marker_size_rel > 1.0
        {
            return Err(CharucoBoardError::InvalidMarkerSizeRel);
        }
        if spec.dictionary.codes.is_empty() {
            return Err(CharucoBoardError::EmptyDictionary);
        }

        let marker_positions = match spec.marker_layout {
            MarkerLayout::OpenCvCharuco => open_cv_charuco_marker_positions(spec.rows, spec.cols),
        };

        let needed = marker_positions.len();
        let available = spec.dictionary.codes.len();
        if available < needed {
            return Err(CharucoBoardError::NotEnoughDictionaryCodes { needed, available });
        }

        Ok(Self {
            spec,
            marker_positions,
        })
    }

    /// Return the underlying board specification.
    #[inline]
    pub fn spec(&self) -> CharucoBoardSpec {
        self.spec
    }

    /// Expected number of *inner* chessboard corners in vertical direction.
    #[inline]
    pub fn expected_inner_rows(&self) -> u32 {
        self.spec.rows - 1
    }

    /// Expected number of *inner* chessboard corners in horizontal direction.
    #[inline]
    pub fn expected_inner_cols(&self) -> u32 {
        self.spec.cols - 1
    }

    /// Mapping from marker id -> board cell (square) coordinates.
    #[inline]
    pub fn marker_position(&self, id: u32) -> Option<[i32; 2]> {
        self.marker_positions.get(id as usize).copied()
    }

    /// Number of markers on the board.
    #[inline]
    pub fn marker_count(&self) -> usize {
        self.marker_positions.len()
    }

    /// Convert a board **corner coordinate** `(i, j)` into a ChArUco corner id.
    ///
    /// Returns `None` if the corner is outside the inner corner range.
    pub fn charuco_corner_id_from_board_corner(&self, i: i32, j: i32) -> Option<u32> {
        let cols = i32::try_from(self.spec.cols).ok()?;
        let rows = i32::try_from(self.spec.rows).ok()?;

        if i <= 0 || j <= 0 || i >= cols || j >= rows {
            return None;
        }

        let inner_cols = cols - 1;
        let ii = i - 1;
        let jj = j - 1;
        Some((jj as u32) * (inner_cols as u32) + (ii as u32))
    }

    /// Physical 2D point (board plane) for a ChArUco corner id.
    ///
    /// Coordinates are in the board reference frame with origin at the top-left board corner.
    pub fn charuco_object_xy(&self, id: u32) -> Option<Point2<f32>> {
        let cols = self.spec.cols.checked_sub(1)?; // inner corner cols
        let rows = self.spec.rows.checked_sub(1)?; // inner corner rows
        let count = cols.checked_mul(rows)?;
        if id >= count {
            return None;
        }
        let i = (id % cols) as f32 + 1.0;
        let j = (id / cols) as f32 + 1.0;
        Some(Point2::new(
            i * self.spec.cell_size,
            j * self.spec.cell_size,
        ))
    }
}

fn open_cv_charuco_marker_positions(rows: u32, cols: u32) -> Vec<[i32; 2]> {
    let mut out = Vec::new();
    for j in 0..(rows as i32) {
        for i in 0..(cols as i32) {
            // OpenCV: top-left square is black => white squares have (i+j) odd.
            if ((i + j) & 1) == 1 {
                out.push([i, j]);
            }
        }
    }
    out
}
