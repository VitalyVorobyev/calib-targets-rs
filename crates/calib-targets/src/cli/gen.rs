//! `gen <target>` one-step subcommands — flags directly to JSON+SVG+PNG bundle.

use calib_targets_aruco::builtins::builtin_dictionary;
use calib_targets_print::{write_target_bundle, MarkerBoardTargetSpec, PrintableTargetDocument};
use clap::{Args, Subcommand};
use std::path::PathBuf;

use super::args::{
    build_page_spec, build_render_options, parse_circles, MarkerLayoutArg, PageArgs, RenderArgs,
};
use super::error::CliError;
use crate::generate::{
    charuco_document, chessboard_document, marker_board_document_with_circles, puzzleboard_document,
};

#[derive(Subcommand, Debug)]
pub enum GenCommand {
    /// Generate a chessboard bundle directly from flags.
    Chessboard(ChessboardGenArgs),
    /// Generate a ChArUco bundle directly from flags.
    Charuco(CharucoGenArgs),
    /// Generate a PuzzleBoard bundle directly from flags.
    Puzzleboard(PuzzleboardGenArgs),
    /// Generate a marker-board bundle directly from flags.
    MarkerBoard(MarkerBoardGenArgs),
}

#[derive(Args, Debug)]
pub struct ChessboardGenArgs {
    /// Output path stem; the CLI writes `stem.json`, `stem.svg`, and `stem.png`.
    #[arg(long)]
    pub out_stem: PathBuf,
    #[arg(long)]
    pub inner_rows: u32,
    #[arg(long)]
    pub inner_cols: u32,
    #[arg(long)]
    pub square_size_mm: f64,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct CharucoGenArgs {
    #[arg(long)]
    pub out_stem: PathBuf,
    #[arg(long)]
    pub rows: u32,
    #[arg(long)]
    pub cols: u32,
    #[arg(long)]
    pub square_size_mm: f64,
    #[arg(long)]
    pub marker_size_rel: f64,
    #[arg(long)]
    pub dictionary: String,
    #[arg(long, value_enum, default_value_t = MarkerLayoutArg::OpencvCharuco)]
    pub marker_layout: MarkerLayoutArg,
    #[arg(long, default_value_t = 1)]
    pub border_bits: usize,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct PuzzleboardGenArgs {
    #[arg(long)]
    pub out_stem: PathBuf,
    #[arg(long)]
    pub rows: u32,
    #[arg(long)]
    pub cols: u32,
    #[arg(long)]
    pub square_size_mm: f64,
    #[arg(long, default_value_t = 0)]
    pub origin_row: u32,
    #[arg(long, default_value_t = 0)]
    pub origin_col: u32,
    #[arg(long)]
    pub dot_diameter_rel: Option<f64>,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct MarkerBoardGenArgs {
    #[arg(long)]
    pub out_stem: PathBuf,
    #[arg(long)]
    pub inner_rows: u32,
    #[arg(long)]
    pub inner_cols: u32,
    #[arg(long)]
    pub square_size_mm: f64,
    #[arg(long, default_value_t = 0.5)]
    pub circle_diameter_rel: f64,
    #[arg(long = "circle", value_name = "I,J,POLARITY")]
    pub circles: Vec<String>,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

pub fn run(cmd: GenCommand) -> Result<(), CliError> {
    match cmd {
        GenCommand::Chessboard(args) => run_chessboard(args),
        GenCommand::Charuco(args) => run_charuco(args),
        GenCommand::Puzzleboard(args) => run_puzzleboard(args),
        GenCommand::MarkerBoard(args) => run_marker_board(args),
    }
}

fn run_chessboard(args: ChessboardGenArgs) -> Result<(), CliError> {
    let mut doc = chessboard_document(args.inner_rows, args.inner_cols, args.square_size_mm);
    doc.page = build_page_spec(&args.page)?;
    doc.render = build_render_options(&args.render);
    emit_bundle(&doc, args.out_stem)
}

fn run_charuco(args: CharucoGenArgs) -> Result<(), CliError> {
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
    }
    doc.page = build_page_spec(&args.page)?;
    doc.render = build_render_options(&args.render);
    emit_bundle(&doc, args.out_stem)
}

fn run_puzzleboard(args: PuzzleboardGenArgs) -> Result<(), CliError> {
    let mut doc = puzzleboard_document(args.rows, args.cols, args.square_size_mm);
    if let calib_targets_print::TargetSpec::PuzzleBoard(spec) = &mut doc.target {
        spec.origin_row = args.origin_row;
        spec.origin_col = args.origin_col;
        if let Some(dot) = args.dot_diameter_rel {
            spec.dot_diameter_rel = dot;
        }
    }
    doc.page = build_page_spec(&args.page)?;
    doc.render = build_render_options(&args.render);
    emit_bundle(&doc, args.out_stem)
}

fn run_marker_board(args: MarkerBoardGenArgs) -> Result<(), CliError> {
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
    }
    doc.page = build_page_spec(&args.page)?;
    doc.render = build_render_options(&args.render);
    emit_bundle(&doc, args.out_stem)
}

fn emit_bundle(doc: &PrintableTargetDocument, out_stem: PathBuf) -> Result<(), CliError> {
    let written = write_target_bundle(doc, out_stem)?;
    println!("{}", written.json_path.display());
    println!("{}", written.svg_path.display());
    println!("{}", written.png_path.display());
    Ok(())
}
