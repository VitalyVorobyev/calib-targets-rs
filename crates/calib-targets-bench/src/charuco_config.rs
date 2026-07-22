//! Trimmed ChArUco JSON config for the bench crate's own harnesses.
//!
//! `calib-targets-charuco` used to expose a similar `CharucoDetectConfig`
//! type from its public API, but example/CLI file-configs are tooling, not
//! part of a published crate's semver surface (see
//! `docs/migrations/0.11.0.md`, API revision S5). This is this crate's own
//! trimmed copy, carrying only the fields `charuco_stage_timing` reads from
//! the checked-in `testdata/charuco_detect_config*.json` fixtures.

use calib_targets::aruco::ArucoScanConfig;
use calib_targets::charuco::{CharucoBoardError, CharucoBoardSpec, CharucoDetector, CharucoParams};
use calib_targets::chessboard::ChessboardParams;
use calib_targets_core::io::{self, IoError};
use serde::{Deserialize, Serialize};
use std::path::Path;

fn default_px_per_square() -> f32 {
    60.0
}

/// ChArUco board spec + detector-param overrides, loadable from the
/// checked-in `testdata/charuco_detect_config*.json` fixtures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharucoDetectConfig {
    /// The ChArUco board specification to detect.
    pub board: CharucoBoardSpec,
    /// Rectified-image resolution in pixels per board square.
    #[serde(default = "default_px_per_square")]
    pub px_per_square: f32,
    /// Optional override for the minimum marker-inlier count.
    #[serde(default)]
    pub min_marker_inliers: Option<usize>,
    /// Optional override for the underlying chessboard detector params.
    #[serde(default)]
    pub chessboard: Option<ChessboardParams>,
    /// Optional ArUco scan-config overrides.
    #[serde(default)]
    pub aruco: Option<ArucoScanConfig>,
}

impl CharucoDetectConfig {
    /// Load a JSON config from disk.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, IoError> {
        io::load_json(path)
    }

    /// Build detector parameters, applying overrides from the config.
    fn build_params(&self) -> CharucoParams {
        let mut params = CharucoParams::for_board(self.board);
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
    pub fn build_detector(&self) -> Result<CharucoDetector, CharucoBoardError> {
        CharucoDetector::new(self.build_params())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn testdata(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata")
            .join(name)
    }

    /// Every checked-in ChArUco config this harness consumes deserializes
    /// and produces a valid detector. Guards the serde-compat contract.
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
