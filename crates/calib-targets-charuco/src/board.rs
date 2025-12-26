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

    /// Square-cell coordinates `(sx, sy)` for the given marker id.
    ///
    /// These are chessboard square indices in the board coordinate system.
    pub fn marker_cell(&self, marker_id: i32) -> Option<(usize, usize)> {
        let id = u32::try_from(marker_id).ok()?;
        let [sx, sy] = self.marker_position(id)?;
        let sx = usize::try_from(sx).ok()?;
        let sy = usize::try_from(sy).ok()?;
        Some((sx, sy))
    }

    /// Return the four surrounding ChArUco corner ids for a marker (TL, TR, BR, BL).
    ///
    /// Returns `None` if the marker is unknown or lies on the board border
    /// (i.e. not surrounded by 4 internal intersections).
    pub fn marker_surrounding_charuco_corners(&self, marker_id: i32) -> Option<[usize; 4]> {
        let (sx, sy) = self.marker_cell(marker_id)?;
        marker_surrounding_charuco_corners_for_cell(
            self.spec.cols as usize,
            self.spec.rows as usize,
            sx,
            sy,
        )
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

/// True if `(ix, iy)` is an internal intersection for a board with `squares_x` × `squares_y`.
pub fn is_internal_intersection(squares_x: usize, squares_y: usize, ix: usize, iy: usize) -> bool {
    squares_x >= 2
        && squares_y >= 2
        && (1..=squares_x - 1).contains(&ix)
        && (1..=squares_y - 1).contains(&iy)
}

/// Row-major ChArUco corner id for an internal intersection `(ix, iy)`.
pub fn charuco_corner_id(
    squares_x: usize,
    squares_y: usize,
    ix: usize,
    iy: usize,
) -> Option<usize> {
    if !is_internal_intersection(squares_x, squares_y, ix, iy) {
        return None;
    }
    let stride = squares_x.checked_sub(1)?;
    let ix0 = ix.checked_sub(1)?;
    let iy0 = iy.checked_sub(1)?;
    Some(iy0 * stride + ix0)
}

fn marker_surrounding_charuco_corners_for_cell(
    squares_x: usize,
    squares_y: usize,
    sx: usize,
    sy: usize,
) -> Option<[usize; 4]> {
    if squares_x < 2 || squares_y < 2 {
        return None;
    }
    if sx == 0 || sy == 0 {
        return None;
    }
    if sx + 1 >= squares_x || sy + 1 >= squares_y {
        return None;
    }
    let tl = charuco_corner_id(squares_x, squares_y, sx, sy)?;
    let tr = charuco_corner_id(squares_x, squares_y, sx + 1, sy)?;
    let br = charuco_corner_id(squares_x, squares_y, sx + 1, sy + 1)?;
    let bl = charuco_corner_id(squares_x, squares_y, sx, sy + 1)?;
    Some([tl, tr, br, bl])
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

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_aruco::builtins;

    fn build_board() -> CharucoBoard {
        let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("dict");
        CharucoBoard::new(CharucoBoardSpec {
            rows: 5,
            cols: 6,
            cell_size: 1.0,
            marker_size_rel: 0.75,
            dictionary: dict,
            marker_layout: MarkerLayout::OpenCvCharuco,
        })
        .expect("board")
    }

    #[test]
    fn marker_surrounding_charuco_corners_matches_expected() {
        let board = build_board();
        let marker_id = 4;
        let cell = board.marker_cell(marker_id).expect("marker cell");
        assert_eq!(cell, (2, 1));

        let corners = board
            .marker_surrounding_charuco_corners(marker_id)
            .expect("corners");
        assert_eq!(corners, [1, 2, 7, 6]);
    }

    #[test]
    fn border_marker_has_no_four_corner_neighborhood() {
        let board = build_board();
        let marker_id = 0;
        let cell = board.marker_cell(marker_id).expect("marker cell");
        assert_eq!(cell, (1, 0));
        assert!(board
            .marker_surrounding_charuco_corners(marker_id)
            .is_none());
    }
}
