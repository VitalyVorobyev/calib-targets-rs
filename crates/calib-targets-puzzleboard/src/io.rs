//! JSON configuration & report I/O for PuzzleBoard.

use std::path::PathBuf;

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
    pub fn from_json_str(s: &str) -> Result<Self, PuzzleBoardIoError> {
        Ok(serde_json::from_str(s)?)
    }
    pub fn from_reader(r: impl std::io::Read) -> Result<Self, PuzzleBoardIoError> {
        Ok(serde_json::from_reader(r)?)
    }
    pub fn to_json_string_pretty(&self) -> Result<String, PuzzleBoardIoError> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
