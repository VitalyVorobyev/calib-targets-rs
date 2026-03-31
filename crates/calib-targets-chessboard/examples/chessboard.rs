use std::{env, fs, path::PathBuf};

use calib_targets_chessboard::{ChessboardDetectConfig, ChessboardDetectReport};
use calib_targets_core::Corner;
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use nalgebra::Point2;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/chessboard_config.json"));

    let cfg = ChessboardDetectConfig::load_json(&config_path)?;
    let img = ImageReader::open(&cfg.image_path)?.decode()?.to_luma8();

    let raw_corners = detect_raw_corners(&img);
    let corners = adapt_corners(&raw_corners);

    let mut report = ChessboardDetectReport::new(&cfg, &config_path, corners.clone());

    let detector = cfg.build_detector();
    match detector.detect_from_corners(&corners) {
        Some(res) => report.set_detection(res),
        None => report.error = Some("no board detected".into()),
    }

    let output_path = cfg.output_path();
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    report.write_json(&output_path)?;
    println!("wrote detection JSON to {}", output_path.display());

    Ok(())
}

fn detect_raw_corners(img: &image::GrayImage) -> Vec<CornerDescriptor> {
    let mut chess_cfg = ChessConfig::single_scale();
    chess_cfg.threshold_mode = chess_corners::ThresholdMode::Relative;
    chess_cfg.threshold_value = 0.2;
    chess_cfg.nms_radius = 2;
    find_chess_corners_image(img, &chess_cfg)
}

fn adapt_corners(raw: &[CornerDescriptor]) -> Vec<Corner> {
    raw.iter()
        .map(|c| Corner {
            position: Point2::new(c.x, c.y),
            orientation: c.orientation,
            orientation_cluster: None,
            strength: c.response,
        })
        .collect()
}
