//! JSON configuration and report helpers for chessboard detection.

use crate::{ChessboardDetectionResult, ChessboardDetector, ChessboardParams};
use calib_targets_core::io::{self, IoError};
use calib_targets_core::{ChessConfig, Corner, TargetDetection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub type ChessboardIoError = IoError;

/// Configuration for chessboard detection, loadable from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChessboardDetectConfig {
    pub image_path: String,
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub chess: ChessConfig,
    #[serde(default)]
    pub chessboard: ChessboardParams,
}

impl ChessboardDetectConfig {
    /// Load a JSON config from disk.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, ChessboardIoError> {
        io::load_json(path)
    }

    /// Write this config to disk as pretty JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), ChessboardIoError> {
        io::write_json(self, path)
    }

    /// Resolve the output report path.
    pub fn output_path(&self) -> PathBuf {
        self.output_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("chessboard_detect_report.json"))
    }

    /// Build a detector from this config.
    ///
    /// If a top-level `chess` field is present, it overrides `chessboard.chess`.
    pub fn build_detector(&self) -> ChessboardDetector {
        let mut params = self.chessboard.clone();
        params.chess = self.chess.clone();
        ChessboardDetector::new(params)
    }
}

/// Detection report for serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChessboardDetectReport {
    pub image_path: String,
    pub config_path: String,
    pub num_raw_corners: usize,
    pub raw_corners: Vec<Corner>,
    #[serde(default)]
    pub detection: Option<TargetDetection>,
    #[serde(default)]
    pub inliers: Vec<usize>,
    #[serde(default)]
    pub orientations: Option<[f32; 2]>,
    #[serde(default)]
    pub error: Option<String>,
}

impl ChessboardDetectReport {
    /// Build a base report from the input config and raw corners.
    pub fn new(cfg: &ChessboardDetectConfig, config_path: &Path, raw_corners: Vec<Corner>) -> Self {
        Self {
            image_path: cfg.image_path.clone(),
            config_path: config_path.to_string_lossy().into_owned(),
            num_raw_corners: raw_corners.len(),
            raw_corners,
            detection: None,
            inliers: Vec::new(),
            orientations: None,
            error: None,
        }
    }

    /// Populate report fields from a successful detection.
    pub fn set_detection(&mut self, res: ChessboardDetectionResult) {
        self.detection = Some(res.detection);
        self.inliers = res.inliers;
        self.orientations = res.orientations;
        self.error = None;
    }

    /// Load a report from JSON on disk.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, ChessboardIoError> {
        io::load_json(path)
    }

    /// Write this report to disk as pretty JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), ChessboardIoError> {
        io::write_json(self, path)
    }
}
