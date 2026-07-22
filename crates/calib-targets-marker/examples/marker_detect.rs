use std::path::Path;
use std::{env, fs, path::PathBuf};

use calib_targets_chessboard::{ChessCorner as TargetCorner, ChessboardParamsError};
use calib_targets_core::io::{self, IoError};
use calib_targets_core::{GrayImageView, GridAlignment, TargetDetection};
use calib_targets_marker::{MarkerBoardDetection, MarkerBoardDetector, MarkerBoardParams};
use chess_corners::{CornerDescriptor, Detector as ChessDetector, DetectorConfig};
use image::ImageReader;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

#[cfg(not(feature = "tracing"))]
use log::{info, warn};

#[cfg(feature = "tracing")]
use tracing::{info, warn};

/// Configuration for marker board detection, loaded from a checked-in JSON
/// fixture (`testdata/marker_detect_config.json`). This struct is
/// example-local: the library used to expose a similar type from
/// `calib_targets_marker`, but example/CLI file-configs are tooling, not
/// part of the crate's public API — see `docs/migrations/0.11.0.md` (API
/// revision S5).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MarkerBoardDetectConfig {
    /// Path to the input image to run detection on.
    image_path: String,
    /// Optional path for the detection report; defaults applied by
    /// [`MarkerBoardDetectConfig::output_path`] when unset.
    #[serde(default)]
    output_path: Option<String>,
    /// Marker-board detector parameters (layout + tuning).
    marker: MarkerBoardParams,
}

impl MarkerBoardDetectConfig {
    /// Load a JSON config from disk.
    fn load_json(path: impl AsRef<Path>) -> Result<Self, IoError> {
        io::load_json(path)
    }

    /// Resolve the output report path.
    fn output_path(&self) -> PathBuf {
        self.output_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("marker_board_detect_report.json"))
    }

    /// Build a detector from this config.
    fn build_detector(&self) -> Result<MarkerBoardDetector, ChessboardParamsError> {
        MarkerBoardDetector::new(self.marker.clone())
    }
}

/// Detection report for serialization. Example-local for the same reason as
/// [`MarkerBoardDetectConfig`].
#[derive(Debug, Clone, Serialize, Deserialize)]
struct MarkerBoardDetectReport {
    /// Path of the image detection was run on.
    image_path: String,
    /// Path of the JSON config that produced this report.
    config_path: String,
    /// Number of raw ChESS corners detected before board detection.
    num_raw_corners: usize,
    /// The raw ChESS corners detected in the image.
    raw_corners: Vec<TargetCorner>,
    /// The detected board, when detection succeeded.
    #[serde(default)]
    detection: Option<TargetDetection>,
    /// Grid alignment to the known board layout, when it could be resolved.
    #[serde(default)]
    alignment: Option<GridAlignment>,
    /// Human-readable error message, when detection failed.
    #[serde(default)]
    error: Option<String>,
}

impl MarkerBoardDetectReport {
    /// Build a base report from the input config and raw corners.
    fn new(
        cfg: &MarkerBoardDetectConfig,
        config_path: &Path,
        raw_corners: Vec<TargetCorner>,
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
    fn set_detection(&mut self, res: MarkerBoardDetection) {
        self.detection = Some(res.target_detection());
        self.alignment = res.alignment;
        self.error = None;
    }

    /// Write this report to disk as pretty JSON.
    fn write_json(&self, path: impl AsRef<Path>) -> Result<(), IoError> {
        io::write_json(self, path)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // No logger/subscriber is installed here: neither `env_logger` nor
    // `tracing-subscriber` is a dependency of this crate's examples, so the
    // `log`/`tracing` macros below are no-ops unless the caller installs
    // their own logger. Detection output does not depend on logging.
    run()
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "info"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/marker_detect_config.json"));

    let cfg = MarkerBoardDetectConfig::load_json(&config_path)?;
    let img = ImageReader::open(&cfg.image_path)?.decode()?.to_luma8();

    let chess_cfg = make_chess_config();
    let mut chess_detector = ChessDetector::new(chess_cfg)?;
    let raw_corners = chess_detector.detect(&img)?;
    info!("raw ChESS corners: {}", raw_corners.len());

    let corners = adapt_corners(&raw_corners);

    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let mut report = MarkerBoardDetectReport::new(&cfg, &config_path, corners.clone());

    let detector = cfg.build_detector()?;
    match detector.detect(&src_view, &corners) {
        Some(res) => report.set_detection(res),
        None => {
            warn!("marker board not detected");
            report.error = Some("marker board not detected".into());
        }
    }

    let output_path = cfg.output_path();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    report.write_json(&output_path)?;
    println!("wrote report JSON to {}", output_path.display());

    Ok(())
}

fn make_chess_config() -> DetectorConfig {
    // chess-corners 1.0 made `threshold` a single absolute floor on the raw
    // ChESS response (relative thresholding is Radon-only now). 15.0 is the
    // workspace production default — see
    // `calib_targets::detect::default_chess_config`. This crate's examples do
    // not depend on the facade, so the value is restated rather than imported.
    DetectorConfig::chess()
        .with_threshold(15.0)
        .with_detection(|d| d.nms_radius = 2)
}

fn adapt_corners(raw: &[CornerDescriptor]) -> Vec<TargetCorner> {
    raw.iter()
        .map(|c| {
            TargetCorner::new(
                Point2::new(c.x, c.y),
                // `axes` is `None` only when the upstream orientation fit is
                // skipped; these fixtures always fit it.
                c.axes
                    .map(|a| {
                        [
                            calib_targets_core::AxisEstimate {
                                angle: a[0].angle,
                                sigma: a[0].sigma,
                            },
                            calib_targets_core::AxisEstimate {
                                angle: a[1].angle,
                                sigma: a[1].sigma,
                            },
                        ]
                    })
                    .expect("orientation fit enabled"),
                c.response,
            )
        })
        .collect()
}
