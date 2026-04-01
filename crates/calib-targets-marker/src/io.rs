//! JSON configuration and report helpers for marker board detection.

use crate::{MarkerBoardDetectionResult, MarkerBoardDetector, MarkerBoardParams};
use calib_targets_core::io::{self, IoError};
use calib_targets_core::{ChessConfig, Corner, GridAlignment, TargetDetection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub type MarkerBoardIoError = IoError;

/// Configuration for marker board detection, loadable from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkerBoardDetectConfig {
    pub image_path: String,
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub chess: ChessConfig,
    pub marker: MarkerBoardParams,
}

impl MarkerBoardDetectConfig {
    /// Load a JSON config from disk.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, MarkerBoardIoError> {
        io::load_json(path)
    }

    /// Write this config to disk as pretty JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), MarkerBoardIoError> {
        io::write_json(self, path)
    }

    /// Resolve the output report path.
    pub fn output_path(&self) -> PathBuf {
        self.output_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("marker_board_detect_report.json"))
    }

    /// Build a detector from this config.
    ///
    /// If a top-level `chess` field is present, it overrides `marker.chessboard.chess`.
    pub fn build_detector(&self) -> MarkerBoardDetector {
        let mut params = self.marker.clone();
        params.chessboard.chess = self.chess.clone();
        MarkerBoardDetector::new(params)
    }
}

/// Detection report for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkerBoardDetectReport {
    pub image_path: String,
    pub config_path: String,
    pub num_raw_corners: usize,
    pub raw_corners: Vec<Corner>,
    #[serde(default)]
    pub detection: Option<TargetDetection>,
    #[serde(default)]
    pub alignment: Option<GridAlignment>,
    #[serde(default)]
    pub error: Option<String>,
}

impl MarkerBoardDetectReport {
    /// Build a base report from the input config and raw corners.
    pub fn new(
        cfg: &MarkerBoardDetectConfig,
        config_path: &Path,
        raw_corners: Vec<Corner>,
    ) -> Self {
        Self {
            image_path: cfg.image_path.clone(),
            config_path: config_path.to_string_lossy().into_owned(),
            num_raw_corners: raw_corners.len(),
            raw_corners,
            detection: None,
            alignment: None,
            error: None,
        }
    }

    /// Populate report fields from a successful detection.
    pub fn set_detection(&mut self, res: MarkerBoardDetectionResult) {
        self.detection = Some(res.detection);
        self.alignment = res.alignment;
        self.error = None;
    }

    /// Load a report from JSON on disk.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, MarkerBoardIoError> {
        io::load_json(path)
    }

    /// Write this report to disk as pretty JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), MarkerBoardIoError> {
        io::write_json(self, path)
    }
}
