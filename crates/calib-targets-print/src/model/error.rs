//! Error type for printable target generation.

use calib_targets_charuco::CharucoBoardError;
use calib_targets_puzzleboard::{PuzzleBoardSpecError, MASTER_ROWS};

pub const SCHEMA_VERSION_V1: u32 = 1;

/// Error returned when validating a target specification or rendering a
/// printable target bundle.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum PrintableTargetError {
    /// An I/O operation failed while writing one of the output files.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The target document could not be serialized to or deserialized from JSON.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// The document declares a `schema_version` this build does not understand.
    #[error("unsupported schema_version {0}, expected {SCHEMA_VERSION_V1}")]
    UnsupportedSchemaVersion(u32),
    /// A custom page width or height is non-finite or not strictly positive.
    #[error("page width/height must be finite and > 0")]
    InvalidPageSize,
    /// The page margin is non-finite or negative.
    #[error("page margin must be finite and >= 0")]
    InvalidMargin,
    /// The page margins consume the entire page, leaving no printable area.
    #[error("printable area is empty after margins")]
    EmptyPrintableArea,
    /// The requested PNG render resolution is not strictly positive.
    #[error("png_dpi must be > 0")]
    InvalidPngDpi,
    /// A chessboard spec requests fewer than 2 inner rows or columns.
    #[error("inner_rows and inner_cols must be >= 2")]
    InvalidChessboardSize,
    /// A ChArUco spec requests fewer than 2 rows or columns.
    #[error("rows and cols must be >= 2")]
    InvalidCharucoSize,
    /// A square-size dimension is non-finite or not strictly positive.
    #[error("square_size_mm must be finite and > 0")]
    InvalidSquareSize,
    /// The marker-to-square size ratio is non-finite or outside `(0, 1]`.
    #[error("marker_size_rel must be finite and in (0, 1]")]
    InvalidMarkerSizeRel,
    /// The ArUco marker border width is not strictly positive.
    #[error("border_bits must be > 0")]
    InvalidBorderBits,
    /// The circle-to-square diameter ratio is non-finite or outside `(0, 1]`.
    #[error("circle_diameter_rel must be finite and in (0, 1]")]
    InvalidCircleDiameter,
    /// The inner-square inset ratio (fraction of the square side occupied by
    /// the centred white inset) is non-finite, negative, or `>= 1.0` (which
    /// would erase the square entirely).
    #[error("inner_square_rel must be finite and in [0.0, 1.0)")]
    InvalidInnerSquareRel,
    /// A marker board layout omits the millimeter cell size required to place
    /// it on a printable page.
    #[error("marker board layout needs cell_size in millimeters for printable conversion")]
    MissingMarkerBoardCellSize,
    /// A marker circle coordinate falls outside the board's square grid.
    #[error("marker circle coordinates must fall inside the board squares")]
    InvalidCircleCell,
    /// Two or more marker circles are placed in the same board cell.
    #[error("marker circle cells must be unique")]
    DuplicateCircleCells,
    /// A marker circle carries the same colour as the square it is drawn on,
    /// which renders nothing at all.
    ///
    /// The checkerboard's colour phase is fixed — square `(0, 0)` is black —
    /// so a disk's polarity is determined by its cell: white on an even
    /// `i + j`, black on an odd one. Nothing else is a marker.
    #[error("marker circle at ({i}, {j}) matches the square underneath and would render invisible: a white circle needs an even `i + j`, a black circle an odd one")]
    InvisibleCircle {
        /// Cell column index of the offending circle.
        i: u32,
        /// Cell row index of the offending circle.
        j: u32,
    },
    /// The chosen ArUco dictionary has fewer codes than the board needs.
    #[error("board needs {needed} markers, dictionary has {available}")]
    NotEnoughDictionaryCodes {
        /// Number of distinct marker codes the board layout requires.
        needed: usize,
        /// Number of marker codes available in the selected dictionary.
        available: usize,
    },
    /// The board's physical size exceeds the page's printable area.
    #[error("board does not fit page: board {board_width_mm:.3}x{board_height_mm:.3} mm, printable area {printable_width_mm:.3}x{printable_height_mm:.3} mm")]
    BoardDoesNotFit {
        /// Board width in millimeters.
        board_width_mm: f64,
        /// Board height in millimeters.
        board_height_mm: f64,
        /// Printable-area width in millimeters (page width minus margins).
        printable_width_mm: f64,
        /// Printable-area height in millimeters (page height minus margins).
        printable_height_mm: f64,
    },
    /// Validation of the underlying ChArUco board layout failed.
    #[error(transparent)]
    CharucoBoard(#[from] CharucoBoardError),
    /// A PuzzleBoard spec requests a row or column count outside the valid range.
    #[error("puzzleboard: rows and cols must be in [4, {MASTER_ROWS}]")]
    InvalidPuzzleBoardSize,
    /// A PuzzleBoard's origin plus size extends past the 501×501 master pattern.
    #[error("puzzleboard origin + size exceeds 501\u{d7}501 master pattern")]
    InvalidPuzzleBoardOrigin,
    /// The PuzzleBoard edge-dot diameter ratio is outside `(0, 1]`.
    #[error("puzzleboard dot_diameter_rel must be in (0, 1]")]
    InvalidPuzzleBoardDotDiameter,
    /// Validation of the underlying PuzzleBoard specification failed.
    #[error(transparent)]
    PuzzleBoardSpec(#[from] PuzzleBoardSpecError),
}
