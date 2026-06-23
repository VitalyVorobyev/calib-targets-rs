//! JSON configuration and report helpers for ChArUco detection.

use crate::{
    CharucoBoard, CharucoBoardError, CharucoBoardSpec, CharucoDetectError, CharucoDetectionResult,
    CharucoDetector, CharucoParams, MarkerLayout,
};
use calib_targets_aruco::{builtins, ArucoScanConfig, Dictionary, MarkerDetection};
use calib_targets_chessboard::ChessCorner;
use calib_targets_chessboard::DetectorParams;
use calib_targets_core::io::{self, IoError};
use calib_targets_core::{DetectorConfig, GridAlignment, TargetDetection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Error type for ChArUco JSON config / report I/O.
pub type CharucoIoError = IoError;

/// Error from building a detector config into a validated board / params.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum CharucoConfigError {
    /// The board specification in the config failed validation.
    #[error(transparent)]
    Board(#[from] CharucoBoardError),
}

/// Errors from loading a board specification via [`load_board_spec_any`].
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum BoardSpecLoadError {
    /// The underlying file read failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The board JSON could not be parsed.
    #[error("parse error: {0}")]
    Parse(#[from] serde_json::Error),
    /// The board JSON is missing a required field (named by the payload).
    #[error("board JSON is missing required field '{0}'")]
    MissingField(&'static str),
    /// The named dictionary is not a known built-in dictionary.
    #[error("unknown dictionary '{0}' (tried '{0}' and 'DICT_{0}')")]
    UnknownDictionary(String),
}

/// Resolve a dictionary name tolerating both the prefixed (`DICT_4X4_1000`)
/// and un-prefixed (`4X4_1000`, `APRILTAG_36h10`) spellings used in
/// different tooling JSON files.
pub fn resolve_dictionary(name: &str) -> Option<Dictionary> {
    if let Some(d) = builtins::builtin_dictionary(name) {
        return Some(d);
    }
    let prefixed = format!("DICT_{name}");
    builtins::builtin_dictionary(&prefixed)
}

#[derive(Debug, Deserialize)]
struct RawBoardSpec {
    ncols: u32,
    nrows: u32,
    cellsize_mm: f32,
    marker_scale: f32,
    dict: String,
    #[serde(default)]
    layout: Option<MarkerLayout>,
}

impl RawBoardSpec {
    fn into_spec(self) -> Result<CharucoBoardSpec, BoardSpecLoadError> {
        let dict = resolve_dictionary(&self.dict)
            .ok_or_else(|| BoardSpecLoadError::UnknownDictionary(self.dict.clone()))?;
        Ok(CharucoBoardSpec {
            rows: self.nrows,
            cols: self.ncols,
            cell_size: self.cellsize_mm,
            marker_size_rel: self.marker_scale,
            dictionary: dict,
            marker_layout: self.layout.unwrap_or_default(),
        })
    }
}

/// Load a [`CharucoBoardSpec`] from JSON accepting either the flat
/// `board.json` layout (`{"ncols": ..., "dict": "..."}`) or the nested
/// `config.json` layout (`{"target": {"ncols": ..., "dict": "..."}}`).
///
/// Field names follow the printing-tool convention:
/// `ncols`, `nrows`, `cellsize_mm`, `marker_scale`, `dict`, optional `layout`.
pub fn load_board_spec_any(path: impl AsRef<Path>) -> Result<CharucoBoardSpec, BoardSpecLoadError> {
    let raw = std::fs::read_to_string(path.as_ref())?;
    let value: serde_json::Value = serde_json::from_str(&raw)?;

    let board_value = if let Some(inner) = value.get("target") {
        inner.clone()
    } else {
        value
    };

    let spec: RawBoardSpec = serde_json::from_value(board_value)?;
    spec.into_spec()
}

fn default_px_per_square() -> f32 {
    60.0
}

/// Configuration for the ChArUco detection example.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharucoDetectConfig {
    /// Path to the input image to run detection on.
    pub image_path: String,
    /// The ChArUco board specification to detect.
    pub board: CharucoBoardSpec,
    /// Optional path for the detection report.
    #[serde(default)]
    pub output_path: Option<String>,
    /// ChESS corner-detector configuration; consumed by the upstream
    /// corner-detection step, not the chessboard stage.
    #[serde(default)]
    pub chess: DetectorConfig,
    /// Optional path to write a global-homography rectified image.
    #[serde(default)]
    pub rectified_path: Option<String>,
    /// Optional path to write a per-cell-mesh rectified image.
    #[serde(default)]
    pub mesh_rectified_path: Option<String>,
    /// Rectified-image resolution in pixels per board square.
    #[serde(default = "default_px_per_square")]
    pub px_per_square: f32,
    /// Optional override for the minimum marker-inlier count.
    #[serde(default)]
    pub min_marker_inliers: Option<usize>,
    /// Optional override for the underlying chessboard detector params.
    #[serde(default)]
    pub chessboard: Option<DetectorParams>,
    /// Optional ArUco scan-config overrides.
    #[serde(default)]
    pub aruco: Option<ArucoScanConfig>,
}

impl CharucoDetectConfig {
    /// Build a config for an input image and board spec; all paths, overrides,
    /// and tuning knobs default to unset / their default values.
    pub fn new(image_path: impl Into<String>, board: CharucoBoardSpec) -> Self {
        Self {
            image_path: image_path.into(),
            board,
            output_path: None,
            chess: DetectorConfig::default(),
            rectified_path: None,
            mesh_rectified_path: None,
            px_per_square: default_px_per_square(),
            min_marker_inliers: None,
            chessboard: None,
            aruco: None,
        }
    }

    /// Load a JSON config from disk.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, CharucoIoError> {
        io::load_json(path)
    }

    /// Write this config to disk as pretty JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), CharucoIoError> {
        io::write_json(self, path)
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
    ///
    /// Note: the chessboard detector (`DetectorParams`) does not include a
    /// nested ChESS detector config — `cfg.chess` is consumed upstream by
    /// the corner-detection step (`calib_targets::detect_corners`), not by
    /// the chessboard stage itself.
    pub fn build_params(&self) -> CharucoParams {
        let mut params = CharucoParams::for_board(&self.board);
        params.px_per_square = self.px_per_square;
        if let Some(min_marker_inliers) = self.min_marker_inliers {
            params.min_marker_inliers = min_marker_inliers;
        }
        if let Some(chessboard) = self.chessboard.clone() {
            params.chessboard = chessboard;
        }
        if let Some(aruco) = self.aruco.as_ref() {
            // `ArucoScanConfig.max_hamming` is intentionally not mapped: the
            // board-level matcher (the sole matcher) uses soft-bit scoring and
            // a margin gate, with no Hamming cap. Only the scan overrides apply.
            aruco.apply_to_scan(&mut params.scan);
        }
        params
    }

    /// Build a detector from this config.
    pub fn build_detector(&self) -> Result<CharucoDetector, CharucoConfigError> {
        let params = self.build_params();
        Ok(CharucoDetector::new(params)?)
    }
}

/// Detection report for serialization.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharucoDetectReport {
    /// Path of the image detection was run on.
    pub image_path: String,
    /// Path of the JSON config that produced this report.
    pub config_path: String,
    /// The board specification detection was run against.
    pub board: CharucoBoardSpec,
    /// Number of raw ChESS corners detected before board detection.
    pub num_raw_corners: usize,
    /// The raw ChESS corners detected in the image.
    pub raw_corners: Vec<ChessCorner>,
    /// The detected ChArUco board, when detection succeeded.
    #[serde(default)]
    pub detection: Option<TargetDetection>,
    /// The decoded inlier markers, when detection succeeded.
    #[serde(default)]
    pub markers: Option<Vec<MarkerDetection>>,
    /// Grid alignment to the board, when it could be resolved.
    #[serde(default)]
    pub alignment: Option<GridAlignment>,
    /// Human-readable error message, when detection failed.
    #[serde(default)]
    pub error: Option<String>,
}

impl CharucoDetectReport {
    /// Build a base report from the input config and raw corners.
    pub fn new(
        cfg: &CharucoDetectConfig,
        config_path: &Path,
        raw_corners: Vec<ChessCorner>,
    ) -> Self {
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
        self.detection = Some(res.target_detection());
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
        io::load_json(path)
    }

    /// Write this report to disk as pretty JSON.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), CharucoIoError> {
        io::write_json(self, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn testdata(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata")
            .join(name)
    }

    /// Every checked-in ChArUco config deserializes and produces a valid
    /// detector. Guards the serde-compat contract.
    #[test]
    fn checked_in_configs_deserialize() {
        for name in [
            "charuco_detect_config.json",
            "charuco_detect_config_small.json",
        ] {
            let path = testdata(name);
            let cfg = CharucoDetectConfig::load_json(&path)
                .unwrap_or_else(|e| panic!("load {name}: {e}"));
            cfg.build_detector()
                .unwrap_or_else(|e| panic!("{name}: build_detector failed: {e}"));
        }
    }
}
