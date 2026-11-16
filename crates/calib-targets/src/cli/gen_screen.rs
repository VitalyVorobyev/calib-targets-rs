use std::{fs, path::PathBuf};

use calib_targets_print::{render_screen_target, stem_paths, ScreenTargetOptions};
use clap::{Args, Subcommand};

use super::error::CliError;
use super::gen::{
    charuco_from_args, chessboard_from_args, puzzleboard_from_args, CharucoGeometryArgs,
    ChessboardGeometryArgs, PuzzleboardGeometryArgs,
};

#[derive(Subcommand, Debug)]
pub enum GenScreenCommand {
    /// Display a chessboard with pixel-aligned checker edges.
    Chessboard {
        #[command(flatten)]
        geometry: ChessboardGeometryArgs,
        #[command(flatten)]
        output: ScreenArgs,
    },
    /// Display a ChArUco board with pixel-aligned checker and marker edges.
    Charuco {
        #[command(flatten)]
        geometry: CharucoGeometryArgs,
        #[command(flatten)]
        output: ScreenArgs,
    },
    /// Display a PuzzleBoard with pixel-aligned checker edges and grayscale dots.
    Puzzleboard {
        #[command(flatten)]
        geometry: PuzzleboardGeometryArgs,
        #[command(flatten)]
        output: ScreenArgs,
    },
}

#[derive(Args, Debug)]
pub struct ScreenArgs {
    /// Output stem for the native-size PNG and calibration JSON.
    #[arg(long)]
    out_stem: PathBuf,
    /// Native screen width in the intended display orientation.
    #[arg(long)]
    width_px: u32,
    /// Native screen height in the intended display orientation.
    #[arg(long)]
    height_px: u32,
    /// Physical side length of one screen pixel in millimeters.
    #[arg(long)]
    pixel_pitch_mm: f64,
    /// Minimum white border around the board in display pixels.
    #[arg(long, default_value_t = 1)]
    margin_px: u32,
}

pub(super) fn run(command: GenScreenCommand) -> Result<(), CliError> {
    let (doc, output) = match command {
        GenScreenCommand::Chessboard { geometry, output } => {
            (chessboard_from_args(geometry), output)
        }
        GenScreenCommand::Charuco { geometry, output } => (charuco_from_args(geometry)?, output),
        GenScreenCommand::Puzzleboard { geometry, output } => {
            (puzzleboard_from_args(geometry), output)
        }
    };
    let screen = ScreenTargetOptions::new(output.width_px, output.height_px, output.pixel_pitch_mm)
        .with_margin_px(output.margin_px);
    let generated = render_screen_target(&doc.target, &screen)?;
    let json = generated.metadata.to_json_pretty()?;
    let paths = stem_paths(&output.out_stem);
    if let Some(parent) = paths.png.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&paths.png, generated.png_bytes)?;
    fs::write(&paths.json, json)?;
    let actual = f64::from(generated.metadata.square_size_px) * screen.pixel_pitch_mm;
    let requested = generated.metadata.requested_square_size_mm;
    println!("{}", paths.png.display());
    println!("{}", paths.json.display());
    println!(
        "Square size {requested:.6} mm requested, {actual:.6} mm realized ({} px, {:+.6} mm)",
        generated.metadata.square_size_px,
        actual - requested
    );
    Ok(())
}
