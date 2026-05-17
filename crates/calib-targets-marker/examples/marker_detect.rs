use std::{env, fs, path::PathBuf};

use calib_targets_chessboard::ChessCorner as TargetCorner;
use calib_targets_core::GrayImageView;
use calib_targets_marker::{MarkerBoardDetectConfig, MarkerBoardDetectReport};
use chess_corners::{CornerDescriptor, Detector as ChessDetector, DetectorConfig, Threshold};
use image::ImageReader;
use nalgebra::Point2;

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

    let detector = cfg.build_detector();
    match detector.detect_from_image_and_corners(&src_view, &corners) {
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
    DetectorConfig::chess()
        .with_threshold(Threshold::Relative(0.2))
        .with_chess(|c| c.nms_radius = 2)
}

fn adapt_corners(raw: &[CornerDescriptor]) -> Vec<TargetCorner> {
    raw.iter()
        .map(|c| TargetCorner {
            position: Point2::new(c.x, c.y),
            axes: [
                calib_targets_core::AxisEstimate {
                    angle: c.axes[0].angle,
                    sigma: c.axes[0].sigma,
                },
                calib_targets_core::AxisEstimate {
                    angle: c.axes[1].angle,
                    sigma: c.axes[1].sigma,
                },
            ],
            contrast: c.contrast,
            fit_rms: c.fit_rms,
            strength: c.response,
        })
        .collect()
}
