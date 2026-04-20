//! `generate --spec ... --out-stem ...` — render a validated spec into a bundle.

use calib_targets_print::{write_target_bundle, PrintableTargetDocument};
use std::path::PathBuf;

use super::error::CliError;

pub fn run(spec: PathBuf, out_stem: PathBuf) -> Result<(), CliError> {
    let doc = PrintableTargetDocument::load_json(&spec)?;
    let written = write_target_bundle(&doc, out_stem)?;
    println!("{}", written.json_path.display());
    println!("{}", written.svg_path.display());
    println!("{}", written.png_path.display());
    Ok(())
}
