//! JSON configuration & report I/O for PuzzleBoard.

use std::path::{Path, PathBuf};

use calib_targets_core::ChessConfig;
use serde::{Deserialize, Serialize};

use crate::detector::PuzzleBoardDetectionResult;
use crate::params::PuzzleBoardParams;

/// Errors from PuzzleBoard JSON I/O.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum PuzzleBoardIoError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Top-level detector config read from JSON.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PuzzleBoardDetectConfig {
    pub image_path: PathBuf,
    #[serde(default)]
    pub output_path: Option<PathBuf>,
    #[serde(default)]
    pub chess_config: Option<ChessConfig>,
    pub detector: PuzzleBoardParams,
}

/// End-to-end report for one detection run.
#[derive(Clone, Debug, Serialize)]
pub struct PuzzleBoardDetectReport {
    pub image_path: PathBuf,
    pub result: PuzzleBoardDetectionResult,
}

impl PuzzleBoardDetectConfig {
    /// Deserialise from a JSON string.
    pub fn from_json_str(s: &str) -> Result<Self, PuzzleBoardIoError> {
        Ok(serde_json::from_str(s)?)
    }

    /// Deserialise from any `Read` source.
    pub fn from_reader(r: impl std::io::Read) -> Result<Self, PuzzleBoardIoError> {
        Ok(serde_json::from_reader(r)?)
    }

    /// Load a JSON config from disk.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, PuzzleBoardIoError> {
        let file = std::fs::File::open(path)?;
        Ok(serde_json::from_reader(std::io::BufReader::new(file))?)
    }

    /// Serialise to a pretty-printed JSON string.
    pub fn to_json_string_pretty(&self) -> Result<String, PuzzleBoardIoError> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Write this config to disk as pretty-printed JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), PuzzleBoardIoError> {
        let file = std::fs::File::create(path)?;
        Ok(serde_json::to_writer_pretty(
            std::io::BufWriter::new(file),
            self,
        )?)
    }
}
