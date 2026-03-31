//! JSON configuration and report helpers for marker board detection.

use crate::{MarkerBoardDetectionResult, MarkerBoardDetector, MarkerBoardParams};
use calib_targets_core::{ChessConfig, Corner, GridAlignment, TargetDetection};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(thiserror::Error, Debug)]
pub enum MarkerBoardIoError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

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
        let raw = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Write this config to disk as pretty JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), MarkerBoardIoError> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Resolve the output report path.
    pub fn output_path(&self) -> PathBuf {
        self.output_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("marker_board_detect_report.json"))
    }

    /// Build a detector from this config.
    pub fn build_detector(&self) -> MarkerBoardDetector {
        MarkerBoardDetector::new(self.marker.clone())
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
        let raw = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Write this report to disk as pretty JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), MarkerBoardIoError> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}
