//! `calib-targets` CLI entry point.
//!
//! All logic lives in [`calib_targets::cli`]; this binary just dispatches.

use std::process::ExitCode;

fn main() -> ExitCode {
    match calib_targets::cli::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}
