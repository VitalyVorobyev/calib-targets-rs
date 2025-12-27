//! JSON configuration and report helpers for ChArUco detection.

use crate::{
    CharucoBoard, CharucoBoardError, CharucoBoardSpec, CharucoDetectError, CharucoDetectionResult,
    CharucoDetector, CharucoDetectorParams,
};
use calib_targets_aruco::{ArucoScanConfig, MarkerDetection};
use calib_targets_chessboard::{ChessboardParams, GridGraphParams};
use calib_targets_core::{Corner, GridAlignment, TargetDetection};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(thiserror::Error, Debug)]
pub enum CharucoIoError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum CharucoConfigError {
    #[error(transparent)]
    Board(#[from] CharucoBoardError),
}

fn default_px_per_square() -> f32 {
    60.0
}

/// Configuration for the ChArUco detection example.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharucoDetectConfig {
    pub image_path: String,
    pub board: CharucoBoardSpec,
    #[serde(default)]
    pub output_path: Option<String>,
    #[serde(default)]
    pub rectified_path: Option<String>,
    #[serde(default)]
    pub mesh_rectified_path: Option<String>,
    #[serde(default = "default_px_per_square")]
    pub px_per_square: f32,
    #[serde(default)]
    pub min_marker_inliers: Option<usize>,
    #[serde(default)]
    pub chessboard: Option<ChessboardParams>,
    #[serde(default)]
    pub graph: Option<GridGraphParams>,
    #[serde(default)]
    pub aruco: Option<ArucoScanConfig>,
}

impl CharucoDetectConfig {
    /// Load a JSON config from disk.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, CharucoIoError> {
        let raw = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Write this config to disk as pretty JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), CharucoIoError> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }

    /// Resolve the output report path.
    pub fn output_path(&self) -> PathBuf {
        self.output_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("charuco_detect_report.json"))
    }

    /// Build a validated ChArUco board from the config.
    pub fn build_board(&self) -> Result<CharucoBoard, CharucoConfigError> {
        Ok(CharucoBoard::new(self.board)?)
    }

    /// Build detector parameters, applying overrides from the config.
    pub fn build_params(&self, board: &CharucoBoardSpec) -> CharucoDetectorParams {
        let mut params = CharucoDetectorParams::for_board(board);
        params.px_per_square = self.px_per_square;
        if let Some(min_marker_inliers) = self.min_marker_inliers {
            params.min_marker_inliers = min_marker_inliers;
        }
        if let Some(chessboard) = self.chessboard.clone() {
            params.chessboard = chessboard;
        }
        if let Some(graph) = self.graph.clone() {
            params.graph = graph;
        }
        if let Some(aruco) = self.aruco.as_ref() {
            if let Some(max_hamming) = aruco.max_hamming {
                params.max_hamming = max_hamming;
            }
            aruco.apply_to_scan(&mut params.scan);
        }
        params
    }

    /// Build a detector from this config.
    pub fn build_detector(&self) -> Result<CharucoDetector, CharucoConfigError> {
        let params = self.build_params(&self.board);
        Ok(CharucoDetector::new(self.board, params)?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharucoDetectReport {
    pub image_path: String,
    pub config_path: String,
    pub board: CharucoBoardSpec,
    pub num_raw_corners: usize,
    pub raw_corners: Vec<Corner>,
    #[serde(default)]
    pub detection: Option<TargetDetection>,
    #[serde(default)]
    pub markers: Option<Vec<MarkerDetection>>,
    #[serde(default)]
    pub alignment: Option<GridAlignment>,
    #[serde(default)]
    pub error: Option<String>,
}

impl CharucoDetectReport {
    /// Build a base report from the input config and raw corners.
    pub fn new(cfg: &CharucoDetectConfig, config_path: &Path, raw_corners: Vec<Corner>) -> Self {
        Self {
            image_path: cfg.image_path.clone(),
            config_path: config_path.to_string_lossy().into_owned(),
            board: cfg.board,
            num_raw_corners: raw_corners.len(),
            raw_corners,
            detection: None,
            markers: None,
            alignment: None,
            error: None,
        }
    }

    /// Populate report fields from a successful detection.
    pub fn set_detection(&mut self, res: CharucoDetectionResult) {
        self.detection = Some(res.detection);
        self.markers = Some(res.markers);
        self.alignment = Some(res.alignment);
        self.error = None;
    }

    /// Record a detection error.
    pub fn set_error(&mut self, err: CharucoDetectError) {
        self.error = Some(err.to_string());
    }

    /// Load a report from JSON on disk.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, CharucoIoError> {
        let raw = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Write this report to disk as pretty JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), CharucoIoError> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}
