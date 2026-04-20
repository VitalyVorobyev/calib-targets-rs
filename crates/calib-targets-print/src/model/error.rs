//! Error type for printable target generation.

use calib_targets_charuco::CharucoBoardError;
use calib_targets_puzzleboard::{PuzzleBoardSpecError, MASTER_ROWS};

pub const SCHEMA_VERSION_V1: u32 = 1;

#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum PrintableTargetError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("unsupported schema_version {0}, expected {SCHEMA_VERSION_V1}")]
    UnsupportedSchemaVersion(u32),
    #[error("page width/height must be finite and > 0")]
    InvalidPageSize,
    #[error("page margin must be finite and >= 0")]
    InvalidMargin,
    #[error("printable area is empty after margins")]
    EmptyPrintableArea,
    #[error("png_dpi must be > 0")]
    InvalidPngDpi,
    #[error("inner_rows and inner_cols must be >= 2")]
    InvalidChessboardSize,
    #[error("rows and cols must be >= 2")]
    InvalidCharucoSize,
    #[error("square_size_mm must be finite and > 0")]
    InvalidSquareSize,
    #[error("marker_size_rel must be finite and in (0, 1]")]
    InvalidMarkerSizeRel,
    #[error("border_bits must be > 0")]
    InvalidBorderBits,
    #[error("circle_diameter_rel must be finite and in (0, 1]")]
    InvalidCircleDiameter,
    #[error("marker board layout needs cell_size in millimeters for printable conversion")]
    MissingMarkerBoardCellSize,
    #[error("marker circle coordinates must fall inside the board squares")]
    InvalidCircleCell,
    #[error("marker circle cells must be unique")]
    DuplicateCircleCells,
    #[error("board needs {needed} markers, dictionary has {available}")]
    NotEnoughDictionaryCodes { needed: usize, available: usize },
    #[error("board does not fit page: board {board_width_mm:.3}x{board_height_mm:.3} mm, printable area {printable_width_mm:.3}x{printable_height_mm:.3} mm")]
    BoardDoesNotFit {
        board_width_mm: f64,
        board_height_mm: f64,
        printable_width_mm: f64,
        printable_height_mm: f64,
    },
    #[error(transparent)]
    CharucoBoard(#[from] CharucoBoardError),
    #[error("puzzleboard: rows and cols must be in [4, {MASTER_ROWS}]")]
    InvalidPuzzleBoardSize,
    #[error("puzzleboard origin + size exceeds 501\u{d7}501 master pattern")]
    InvalidPuzzleBoardOrigin,
    #[error("puzzleboard dot_diameter_rel must be in (0, 1]")]
    InvalidPuzzleBoardDotDiameter,
    #[error(transparent)]
    PuzzleBoardSpec(#[from] PuzzleBoardSpecError),
}
