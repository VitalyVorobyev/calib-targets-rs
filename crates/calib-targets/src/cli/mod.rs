//! CLI dispatcher for the `calib-targets` binary.
//!
//! The CLI is gated behind the `cli` feature (enabled by default). The binary
//! entry point (`src/bin/calib_targets.rs`) just parses arguments and calls
//! [`run`]; all subcommand logic lives in child modules.

mod args;
mod dictionaries;
mod error;
mod gen;
mod generate;
mod init;
mod validate;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

pub use error::CliError;

#[derive(Parser, Debug)]
#[command(name = "calib-targets")]
#[command(about = "CLI for printable calibration target generation")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Render a validated printable spec into .json, .svg, and .png outputs.
    Generate {
        /// Path to the input printable spec JSON file.
        #[arg(long)]
        spec: PathBuf,
        /// Output path stem; the CLI writes `stem.json`, `stem.svg`, and `stem.png`.
        #[arg(long)]
        out_stem: PathBuf,
    },
    /// Validate a printable spec without writing any output files.
    Validate {
        /// Path to the printable spec JSON file to validate.
        #[arg(long)]
        spec: PathBuf,
    },
    /// List the built-in dictionary names available for ChArUco initialization.
    ListDictionaries,
    /// Initialize a printable spec JSON file for one target family.
    Init {
        #[command(subcommand)]
        target: init::InitCommand,
    },
    /// Render a printable bundle in one step, without writing a spec JSON file first.
    Gen {
        #[command(subcommand)]
        target: gen::GenCommand,
    },
}

/// Parse CLI arguments from `std::env::args_os()` and dispatch to the chosen subcommand.
pub fn run() -> Result<(), CliError> {
    run_from(Cli::parse())
}

fn run_from(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Generate { spec, out_stem } => generate::run(spec, out_stem),
        Command::Validate { spec } => validate::run(spec),
        Command::ListDictionaries => dictionaries::run(),
        Command::Init { target } => init::run(target),
        Command::Gen { target } => gen::run(target),
    }
}
