//! `list-dictionaries` — print the built-in ArUco dictionary names, one per line.

use calib_targets_aruco::builtins::BUILTIN_DICTIONARY_NAMES;

use super::error::CliError;

pub fn run() -> Result<(), CliError> {
    for name in BUILTIN_DICTIONARY_NAMES {
        println!("{name}");
    }
    Ok(())
}
