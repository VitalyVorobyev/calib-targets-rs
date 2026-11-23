//! Shared CLI target geometry flags and document builders for `init`, `gen`,
//! and `gen-screen`.

use calib_targets_aruco::builtins::builtin_dictionary;
use calib_targets_print::{MarkerBoardTargetSpec, PrintableTargetDocument};
use clap::Args;

use super::args::{parse_circles, MarkerLayoutArg};
use super::error::CliError;
use crate::generate::{
    charuco_document, chessboard_document, marker_board_document_with_circles,
    puzzleboard_document, puzzlepole_document,
};

#[derive(Args, Debug)]
pub struct ChessboardGeometryArgs {
    /// Number of inner chessboard corners vertically.
    #[arg(long)]
    pub inner_rows: u32,
    /// Number of inner chessboard corners horizontally.
    #[arg(long)]
    pub inner_cols: u32,
    /// Square side length in millimeters.
    #[arg(long)]
    pub square_size_mm: f64,
    /// White inset square drawn centred inside every black square, as a
    /// fraction of the square side, in `[0, 1)`. Omitted or `0` means no
    /// inset.
    #[arg(long)]
    pub inner_square_rel: Option<f64>,
}

#[derive(Args, Debug)]
pub struct CharucoGeometryArgs {
    /// Number of board squares vertically.
    #[arg(long)]
    pub rows: u32,
    /// Number of board squares horizontally.
    #[arg(long)]
    pub cols: u32,
    /// Square side length in millimeters.
    #[arg(long)]
    pub square_size_mm: f64,
    /// Marker side length relative to square size.
    #[arg(long)]
    pub marker_size_rel: f64,
    /// Built-in dictionary name. Use `list-dictionaries` to discover valid values.
    #[arg(long)]
    pub dictionary: String,
    /// Marker placement scheme.
    #[arg(long, value_enum, default_value_t = MarkerLayoutArg::OpencvCharuco)]
    pub marker_layout: MarkerLayoutArg,
    /// Marker border width in bit cells.
    #[arg(long, default_value_t = 1)]
    pub border_bits: usize,
    /// White inset square drawn centred inside every plain black checker
    /// square (never inside an ArUco marker's bit cells), as a fraction of
    /// the square side, in `[0, 1)`. Omitted or `0` means no inset.
    #[arg(long)]
    pub inner_square_rel: Option<f64>,
}

#[derive(Args, Debug)]
pub struct PuzzleboardGeometryArgs {
    /// Number of board squares vertically (4..=501).
    #[arg(long)]
    pub rows: u32,
    /// Number of board squares horizontally (4..=501).
    #[arg(long)]
    pub cols: u32,
    /// Square side length in millimeters.
    #[arg(long)]
    pub square_size_mm: f64,
    /// Row anchor in the 501×501 master pattern.
    #[arg(long, default_value_t = 0)]
    pub origin_row: u32,
    /// Column anchor in the 501×501 master pattern.
    #[arg(long, default_value_t = 0)]
    pub origin_col: u32,
    /// Dot diameter relative to square size (defaults to 1/3).
    #[arg(long)]
    pub dot_diameter_rel: Option<f64>,
}

#[derive(Args, Debug)]
pub struct PuzzlepoleGeometryArgs {
    /// Pieces around the circumference. Must be a supported period.
    #[arg(long)]
    pub circumference_squares: u32,
    /// Pieces along the cylinder axis.
    #[arg(long)]
    pub axial_squares: u32,
    /// Piece side length in millimeters.
    #[arg(long)]
    pub square_size_mm: f64,
    /// Column anchor in the 501-column master pattern. Strips cut from
    /// disjoint column windows are distinguishable poles.
    #[arg(long, default_value_t = 0)]
    pub axial_start_col: u32,
    /// Dot diameter relative to piece size (defaults to 1/3).
    #[arg(long)]
    pub dot_diameter_rel: Option<f64>,
}

#[derive(Args, Debug)]
pub struct MarkerBoardGeometryArgs {
    /// Number of inner chessboard corners vertically.
    #[arg(long)]
    pub inner_rows: u32,
    /// Number of inner chessboard corners horizontally.
    #[arg(long)]
    pub inner_cols: u32,
    /// Square side length in millimeters.
    #[arg(long)]
    pub square_size_mm: f64,
    /// Circle diameter relative to square size.
    #[arg(long, default_value_t = 0.5)]
    pub circle_diameter_rel: f64,
    /// Marker circles as `i,j,polarity`; repeat exactly three times when overriding defaults.
    #[arg(long = "circle", value_name = "I,J,POLARITY")]
    pub circles: Vec<String>,
    /// White inset square drawn centred inside every black checkerboard
    /// square, as a fraction of the square side, in `[0, 1)`. Omitted or
    /// `0` means no inset.
    #[arg(long)]
    pub inner_square_rel: Option<f64>,
}

pub(super) fn chessboard_from_args(args: ChessboardGeometryArgs) -> PrintableTargetDocument {
    let mut doc = chessboard_document(args.inner_rows, args.inner_cols, args.square_size_mm);
    if let Some(rel) = args.inner_square_rel {
        if let calib_targets_print::TargetSpec::Chessboard(spec) = &mut doc.target {
            spec.inner_square_rel = Some(rel);
        }
    }
    doc
}

pub(super) fn charuco_from_args(
    args: CharucoGeometryArgs,
) -> Result<PrintableTargetDocument, CliError> {
    let dictionary = builtin_dictionary(&args.dictionary)
        .ok_or_else(|| CliError::UnknownDictionary(args.dictionary.clone()))?;
    let mut doc = charuco_document(
        args.rows,
        args.cols,
        args.square_size_mm,
        args.marker_size_rel,
        dictionary,
    );
    if let calib_targets_print::TargetSpec::Charuco(spec) = &mut doc.target {
        spec.border_bits = args.border_bits;
        spec.marker_layout = match args.marker_layout {
            MarkerLayoutArg::OpencvCharuco => calib_targets_charuco::MarkerLayout::OpenCvCharuco,
        };
        if let Some(rel) = args.inner_square_rel {
            spec.inner_square_rel = Some(rel);
        }
    }
    Ok(doc)
}

pub(super) fn puzzleboard_from_args(args: PuzzleboardGeometryArgs) -> PrintableTargetDocument {
    let mut doc = puzzleboard_document(args.rows, args.cols, args.square_size_mm);
    if let calib_targets_print::TargetSpec::PuzzleBoard(spec) = &mut doc.target {
        spec.origin_row = args.origin_row;
        spec.origin_col = args.origin_col;
        if let Some(dot) = args.dot_diameter_rel {
            spec.dot_diameter_rel = dot;
        }
    }
    doc
}

pub(super) fn puzzlepole_from_args(
    args: PuzzlepoleGeometryArgs,
) -> Result<PrintableTargetDocument, CliError> {
    let mut doc = puzzlepole_document(
        args.circumference_squares,
        args.axial_squares,
        args.square_size_mm,
    )
    .ok_or(CliError::UnsupportedPuzzlePolePeriod {
        circumference_squares: args.circumference_squares,
    })?;
    if let calib_targets_print::TargetSpec::PuzzlePole(spec) = &mut doc.target {
        spec.axial_start_col = args.axial_start_col;
        if let Some(dot) = args.dot_diameter_rel {
            spec.dot_diameter_rel = dot;
        }
    }
    Ok(doc)
}

pub(super) fn marker_board_from_args(
    args: MarkerBoardGeometryArgs,
) -> Result<PrintableTargetDocument, CliError> {
    let circles = if args.circles.is_empty() {
        MarkerBoardTargetSpec::default_circles(args.inner_rows, args.inner_cols)
    } else {
        parse_circles(&args.circles)?
    };
    let mut doc = marker_board_document_with_circles(
        args.inner_rows,
        args.inner_cols,
        args.square_size_mm,
        circles,
    );
    if let calib_targets_print::TargetSpec::MarkerBoard(spec) = &mut doc.target {
        spec.circle_diameter_rel = args.circle_diameter_rel;
        if let Some(rel) = args.inner_square_rel {
            spec.inner_square_rel = Some(rel);
        }
    }
    Ok(doc)
}
