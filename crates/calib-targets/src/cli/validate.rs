//! `validate --spec ...` — load a spec JSON and report the target kind on success.

use calib_targets_print::PrintableTargetDocument;
use std::path::PathBuf;

use super::error::CliError;

pub fn run(spec: PathBuf) -> Result<(), CliError> {
    let doc = PrintableTargetDocument::load_json(&spec)?;
    println!("valid {}", doc.target.kind_name());
    Ok(())
}
