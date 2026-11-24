use calib_targets_print::PrintableTargetDocument;
use clap::{Args, Subcommand};
use std::{fs, path::PathBuf};

use super::args::{build_page_spec, build_render_options, PageArgs, RenderArgs};
use super::error::CliError;
use super::target_args::{
    charuco_from_args, chessboard_from_args, marker_board_from_args, puzzleboard_from_args,
    puzzlepole_from_args, CharucoGeometryArgs, ChessboardGeometryArgs, MarkerBoardGeometryArgs,
    PuzzleboardGeometryArgs, PuzzlepoleGeometryArgs,
};

#[derive(Subcommand, Debug)]
pub enum InitCommand {
    /// Initialize a chessboard printable spec.
    Chessboard(ChessboardInitArgs),
    /// Initialize a ChArUco printable spec.
    Charuco(CharucoInitArgs),
    /// Initialize a PuzzleBoard printable spec.
    Puzzleboard(PuzzleboardInitArgs),
    /// Write a PuzzlePole wrap-strip spec.
    Puzzlepole(PuzzlepoleInitArgs),
    /// Initialize a checkerboard marker-board printable spec.
    MarkerBoard(MarkerBoardInitArgs),
}

#[derive(Args, Debug)]
pub struct ChessboardInitArgs {
    /// Path to the spec JSON file to write.
    #[arg(long)]
    pub out: PathBuf,
    #[command(flatten)]
    pub geometry: ChessboardGeometryArgs,
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
    #[command(flatten)]
    pub geometry: CharucoGeometryArgs,
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
    #[command(flatten)]
    pub geometry: PuzzleboardGeometryArgs,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

#[derive(Args, Debug)]
pub struct PuzzlepoleInitArgs {
    /// Path to the spec JSON file to write.
    #[arg(long)]
    pub out: PathBuf,
    #[command(flatten)]
    pub geometry: PuzzlepoleGeometryArgs,
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
    #[command(flatten)]
    pub geometry: MarkerBoardGeometryArgs,
    #[command(flatten)]
    pub page: PageArgs,
    #[command(flatten)]
    pub render: RenderArgs,
}

pub fn run(cmd: InitCommand) -> Result<(), CliError> {
    let (mut doc, out, page, render) = match cmd {
        InitCommand::Chessboard(args) => (
            chessboard_from_args(args.geometry),
            args.out,
            args.page,
            args.render,
        ),
        InitCommand::Charuco(args) => (
            charuco_from_args(args.geometry)?,
            args.out,
            args.page,
            args.render,
        ),
        InitCommand::Puzzleboard(args) => (
            puzzleboard_from_args(args.geometry),
            args.out,
            args.page,
            args.render,
        ),
        InitCommand::Puzzlepole(args) => (
            puzzlepole_from_args(args.geometry)?,
            args.out,
            args.page,
            args.render,
        ),
        InitCommand::MarkerBoard(args) => (
            marker_board_from_args(args.geometry)?,
            args.out,
            args.page,
            args.render,
        ),
    };
    doc = doc
        .with_page(build_page_spec(&page)?)
        .with_render(build_render_options(&render));
    write_document_json(&doc, out)
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
