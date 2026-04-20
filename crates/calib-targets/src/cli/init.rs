//! `init <target>` subcommands — write a printable spec JSON file.

use calib_targets_aruco::builtins::builtin_dictionary;
use calib_targets_print::{
    CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec, PrintableTargetDocument,
    PuzzleBoardTargetSpec, TargetSpec,
};
use clap::{Args, Subcommand};
use std::{fs, path::PathBuf};

use super::args::{
    build_page_spec, build_render_options, parse_circles, MarkerLayoutArg, PageArgs, RenderArgs,
};
use super::error::CliError;

#[derive(Subcommand, Debug)]
pub enum InitCommand {
    /// Initialize a chessboard printable spec.
    Chessboard(ChessboardInitArgs),
    /// Initialize a ChArUco printable spec.
    Charuco(CharucoInitArgs),
    /// Initialize a PuzzleBoard printable spec.
    Puzzleboard(PuzzleboardInitArgs),
    /// Initialize a checkerboard marker-board printable spec.
    MarkerBoard(MarkerBoardInitArgs),
}

#[derive(Args, Debug)]
pub struct ChessboardInitArgs {
    /// Path to the spec JSON file to write.
    #[arg(long)]
    pub out: PathBuf,
    /// Number of inner chessboard corners vertically.
    #[arg(long)]
    pub inner_rows: u32,
    /// Number of inner chessboard corners horizontally.
    #[arg(long)]
    pub inner_cols: u32,
    /// Square side length in millimeters.
    #[arg(long)]
    pub square_size_mm: f64,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct CharucoInitArgs {
    /// Path to the spec JSON file to write.
    #[arg(long)]
    pub out: PathBuf,
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
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct PuzzleboardInitArgs {
    /// Path to the spec JSON file to write.
    #[arg(long)]
    pub out: PathBuf,
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
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct MarkerBoardInitArgs {
    /// Path to the spec JSON file to write.
    #[arg(long)]
    pub out: PathBuf,
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
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

pub fn run(cmd: InitCommand) -> Result<(), CliError> {
    match cmd {
        InitCommand::Chessboard(args) => run_chessboard(args),
        InitCommand::Charuco(args) => run_charuco(args),
        InitCommand::Puzzleboard(args) => run_puzzleboard(args),
        InitCommand::MarkerBoard(args) => run_marker_board(args),
    }
}

fn run_chessboard(args: ChessboardInitArgs) -> Result<(), CliError> {
    let doc = PrintableTargetDocument {
        schema_version: 1,
        target: TargetSpec::Chessboard(ChessboardTargetSpec {
            inner_rows: args.inner_rows,
            inner_cols: args.inner_cols,
            square_size_mm: args.square_size_mm,
        }),
        page: build_page_spec(&args.page)?,
        render: build_render_options(&args.render),
    };
    write_document_json(&doc, args.out)
}

fn run_charuco(args: CharucoInitArgs) -> Result<(), CliError> {
    let dictionary = builtin_dictionary(&args.dictionary)
        .ok_or_else(|| CliError::UnknownDictionary(args.dictionary.clone()))?;
    let marker_layout = match args.marker_layout {
        MarkerLayoutArg::OpencvCharuco => calib_targets_charuco::MarkerLayout::OpenCvCharuco,
    };
    let doc = PrintableTargetDocument {
        schema_version: 1,
        target: TargetSpec::Charuco(CharucoTargetSpec {
            rows: args.rows,
            cols: args.cols,
            square_size_mm: args.square_size_mm,
            marker_size_rel: args.marker_size_rel,
            dictionary,
            marker_layout,
            border_bits: args.border_bits,
        }),
        page: build_page_spec(&args.page)?,
        render: build_render_options(&args.render),
    };
    write_document_json(&doc, args.out)
}

fn run_puzzleboard(args: PuzzleboardInitArgs) -> Result<(), CliError> {
    let doc = PrintableTargetDocument {
        schema_version: 1,
        target: TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec {
            rows: args.rows,
            cols: args.cols,
            square_size_mm: args.square_size_mm,
            origin_row: args.origin_row,
            origin_col: args.origin_col,
            dot_diameter_rel: args.dot_diameter_rel.unwrap_or(1.0 / 3.0),
        }),
        page: build_page_spec(&args.page)?,
        render: build_render_options(&args.render),
    };
    write_document_json(&doc, args.out)
}

fn run_marker_board(args: MarkerBoardInitArgs) -> Result<(), CliError> {
    let circles = if args.circles.is_empty() {
        MarkerBoardTargetSpec::default_circles(args.inner_rows, args.inner_cols)
    } else {
        parse_circles(&args.circles)?
    };
    let doc = PrintableTargetDocument {
        schema_version: 1,
        target: TargetSpec::MarkerBoard(MarkerBoardTargetSpec {
            inner_rows: args.inner_rows,
            inner_cols: args.inner_cols,
            square_size_mm: args.square_size_mm,
            circles,
            circle_diameter_rel: args.circle_diameter_rel,
        }),
        page: build_page_spec(&args.page)?,
        render: build_render_options(&args.render),
    };
    write_document_json(&doc, args.out)
}

fn write_document_json(doc: &PrintableTargetDocument, out: PathBuf) -> Result<(), CliError> {
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    doc.write_json(&out)?;
    println!("{}", out.display());
    Ok(())
}
