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
    #[error(
        "{circumference_squares} pieces around the circumference is not a \
         PuzzlePole period that closes seamlessly; supported circumferences are \
         listed by `calib_targets::puzzleboard::SUPPORTED_PERIODS`"
    )]
    UnsupportedPuzzlePolePeriod {
        /// The requested circumference, in pieces.
        circumference_squares: u32,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
