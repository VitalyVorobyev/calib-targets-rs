//! CLI error type shared across subcommands.

use calib_targets_print::PrintableTargetError;

#[derive(thiserror::Error, Debug)]
pub enum CliError {
    #[error(transparent)]
    Printable(#[from] PrintableTargetError),
    #[error("invalid page configuration: {0}")]
    InvalidPage(String),
    #[error("unknown dictionary {0}; run `list-dictionaries` to inspect built-ins")]
    UnknownDictionary(String),
    #[error("invalid --circle '{0}', expected i,j,polarity")]
    InvalidCircle(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
