use calib_targets_print::{write_target_bundle, PrintableTargetDocument};
use clap::{Args, Subcommand};
use std::path::PathBuf;

use super::args::{build_page_spec, build_render_options, PageArgs, RenderArgs};
use super::error::CliError;
use super::target_args::{
    charuco_from_args, chessboard_from_args, marker_board_from_args, puzzleboard_from_args,
    puzzlepole_from_args, CharucoGeometryArgs, ChessboardGeometryArgs, MarkerBoardGeometryArgs,
    PuzzleboardGeometryArgs, PuzzlepoleGeometryArgs,
};

#[derive(Subcommand, Debug)]
pub enum GenCommand {
    /// Generate a chessboard bundle directly from flags.
    Chessboard(ChessboardGenArgs),
    /// Generate a ChArUco bundle directly from flags.
    Charuco(CharucoGenArgs),
    /// Generate a PuzzleBoard bundle directly from flags.
    Puzzleboard(PuzzleboardGenArgs),
    /// Render a PuzzlePole wrap strip straight to a bundle.
    Puzzlepole(PuzzlepoleGenArgs),
    /// Generate a marker-board bundle directly from flags.
    MarkerBoard(MarkerBoardGenArgs),
}

#[derive(Args, Debug)]
pub struct ChessboardGenArgs {
    /// Output path stem; the CLI writes `stem.json`, `stem.svg`,
    /// `stem.png`, and `stem.dxf` (chrome-on-glass photolith handoff).
    #[arg(long)]
    pub out_stem: PathBuf,
    #[command(flatten)]
    pub geometry: ChessboardGeometryArgs,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct CharucoGenArgs {
    #[arg(long)]
    pub out_stem: PathBuf,
    #[command(flatten)]
    pub geometry: CharucoGeometryArgs,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct PuzzleboardGenArgs {
    #[arg(long)]
    pub out_stem: PathBuf,
    #[command(flatten)]
    pub geometry: PuzzleboardGeometryArgs,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct PuzzlepoleGenArgs {
    #[arg(long)]
    pub out_stem: PathBuf,
    #[command(flatten)]
    pub geometry: PuzzlepoleGeometryArgs,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct MarkerBoardGenArgs {
    #[arg(long)]
    pub out_stem: PathBuf,
    #[command(flatten)]
    pub geometry: MarkerBoardGeometryArgs,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

pub fn run(cmd: GenCommand) -> Result<(), CliError> {
    let (mut doc, out_stem, page, render) = match cmd {
        GenCommand::Chessboard(args) => (
            chessboard_from_args(args.geometry),
            args.out_stem,
            args.page,
            args.render,
        ),
        GenCommand::Charuco(args) => (
            charuco_from_args(args.geometry)?,
            args.out_stem,
            args.page,
            args.render,
        ),
        GenCommand::Puzzleboard(args) => (
            puzzleboard_from_args(args.geometry),
            args.out_stem,
            args.page,
            args.render,
        ),
        GenCommand::Puzzlepole(args) => (
            puzzlepole_from_args(args.geometry)?,
            args.out_stem,
            args.page,
            args.render,
        ),
        GenCommand::MarkerBoard(args) => (
            marker_board_from_args(args.geometry)?,
            args.out_stem,
            args.page,
            args.render,
        ),
    };
    doc.page = build_page_spec(&page)?;
    doc.render = build_render_options(&render);
    emit_bundle(&doc, out_stem)
}

fn emit_bundle(doc: &PrintableTargetDocument, out_stem: PathBuf) -> Result<(), CliError> {
    let written = write_target_bundle(doc, out_stem)?;
    println!("{}", written.json_path.display());
    println!("{}", written.svg_path.display());
    println!("{}", written.png_path.display());
    println!("{}", written.dxf_path.display());
    Ok(())
}
