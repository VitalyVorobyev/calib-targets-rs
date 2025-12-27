use std::{env, fs, path::PathBuf};

use calib_targets_core::{Corner as TargetCorner, GrayImageView};
use calib_targets_marker::{MarkerBoardDetectionResult, MarkerBoardDetector, MarkerBoardParams};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

#[cfg(not(feature = "tracing"))]
use std::str::FromStr;

#[cfg(not(feature = "tracing"))]
use log::{info, warn, LevelFilter};

#[cfg(feature = "tracing")]
use tracing::{info, warn};

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;
#[cfg(not(feature = "tracing"))]
use calib_targets_core::init_with_level;

#[derive(Debug, Deserialize)]
struct ExampleConfig {
    image_path: String,
    #[serde(default)]
    output_path: Option<String>,
    marker: MarkerBoardParams,
}

#[derive(Debug, Serialize)]
struct ExampleReport {
    image_path: String,
    config_path: String,
    num_raw_corners: usize,
    detection: Option<MarkerBoardDetectionResult>,
    error: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "tracing"))]
    let log_level = LevelFilter::from_str("info").unwrap_or(LevelFilter::Info);
    #[cfg(not(feature = "tracing"))]
    init_with_level(log_level)?;
    #[cfg(not(feature = "tracing"))]
    info!("Logger initialized");

    #[cfg(feature = "tracing")]
    init_tracing(false);

    run()
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "info"))]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = parse_config_path();
    let cfg = load_config(&config_path)?;

    let image_path = PathBuf::from(&cfg.image_path);
    let img = load_image(&image_path)?;

    let chess_cfg = make_chess_config();
    let raw_corners = detect_raw_corners(&img, &chess_cfg);
    info!("raw ChESS corners: {}", raw_corners.len());

    let target_corners = adapt_corners(&raw_corners);

    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let detector = MarkerBoardDetector::new(cfg.marker.clone());
    let detection = detector.detect_from_image_and_corners(&src_view, &target_corners);
    if detection.is_none() {
        warn!("marker board not detected");
    }

    let report = ExampleReport {
        image_path: cfg.image_path.clone(),
        config_path: config_path.to_string_lossy().into_owned(),
        num_raw_corners: raw_corners.len(),
        detection: detection.clone(),
        error: detection
            .is_none()
            .then(|| "marker board not detected".to_string()),
    };

    write_report(cfg.output_path.as_deref(), report)
}

fn parse_config_path() -> PathBuf {
    env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/marker_detect_config.json"))
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip(path)))]
fn load_config(path: &PathBuf) -> Result<ExampleConfig, Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "info", skip(image_path))
)]
fn load_image(image_path: &PathBuf) -> Result<image::GrayImage, Box<dyn std::error::Error>> {
    Ok(ImageReader::open(image_path)?.decode()?.to_luma8())
}

fn make_chess_config() -> ChessConfig {
    let mut chess_cfg = ChessConfig::single_scale();
    chess_cfg.params.threshold_rel = 0.2;
    chess_cfg.params.nms_radius = 2;
    chess_cfg
}

#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "info", skip(img, chess_cfg))
)]
fn detect_raw_corners(img: &image::GrayImage, chess_cfg: &ChessConfig) -> Vec<CornerDescriptor> {
    find_chess_corners_image(img, chess_cfg)
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip(raw)))]
fn adapt_corners(raw: &[CornerDescriptor]) -> Vec<TargetCorner> {
    raw.iter().map(adapt_chess_corner).collect()
}

fn adapt_chess_corner(c: &CornerDescriptor) -> TargetCorner {
    TargetCorner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
        strength: c.response,
    }
}

fn write_report(
    path: Option<&str>,
    report: ExampleReport,
) -> Result<(), Box<dyn std::error::Error>> {
    let out_path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tmpdata/marker_detect_report.json"));
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out_path, json)?;
    println!("wrote report JSON to {}", out_path.display());
    Ok(())
}
