use std::{
    env,
    fs,
    path::{Path, PathBuf},
};

use calib_targets_chessboard::{ChessboardDetector, ChessboardParams, GridGraphParams};
use calib_targets_core::{Corner, TargetDetection};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

/// Configuration for the chessboard example, loaded from JSON.
#[derive(Debug, Deserialize)]
struct ExampleConfig {
    /// Path to the input chessboard image.
    image_path: String,
    /// Where to write the detection JSON.
    #[serde(default)]
    output_path: Option<String>,
    chessboard: ChessboardParams,
    graph: GridGraphParams,
}

#[derive(Debug, Serialize)]
struct ChessboardReport {
    image_path: String,
    config_path: String,
    num_raw_corners: usize,
    raw_corners: Vec<Corner>,
    detection: Option<TargetDetection>,
    inliers: Vec<usize>,
    orientations: Option<[f32; 2]>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config_path = parse_config_path();
    let cfg: ExampleConfig = {
        let raw = fs::read_to_string(&config_path)?;
        serde_json::from_str(&raw)?
    };

    let img = load_image(Path::new(&cfg.image_path))?;
    let raw_corners = detect_raw_corners(&img);
    let corners = adapt_corners(&raw_corners);

    let detector = ChessboardDetector::new(cfg.chessboard).with_grid_search(cfg.graph);
    let detection = detector.detect_from_corners(&corners);

    let (detection, inliers, orientations) = match detection {
        Some(res) => (Some(res.detection), res.inliers, res.orientations),
        None => (None, Vec::new(), None),
    };

    let report = ChessboardReport {
        image_path: cfg.image_path.clone(),
        config_path: config_path.to_string_lossy().into_owned(),
        num_raw_corners: raw_corners.len(),
        raw_corners: corners,
        detection,
        inliers,
        orientations,
    };

    let output_path = cfg
        .output_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/chessboard_detection.json"));

    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&output_path, json)?;
    println!("wrote detection JSON to {}", output_path.display());

    Ok(())
}

fn parse_config_path() -> PathBuf {
    env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/chessboard_config.json"))
}

fn load_image(path: &Path) -> Result<image::GrayImage, Box<dyn std::error::Error>> {
    Ok(ImageReader::open(path)?.decode()?.to_luma8())
}

fn detect_raw_corners(img: &image::GrayImage) -> Vec<CornerDescriptor> {
    let mut chess_cfg = ChessConfig::single_scale();
    chess_cfg.params.threshold_rel = 0.2;
    chess_cfg.params.nms_radius = 2;
    find_chess_corners_image(img, &chess_cfg)
}

fn adapt_corners(raw: &[CornerDescriptor]) -> Vec<Corner> {
    raw.iter().map(adapt_chess_corner).collect()
}

fn adapt_chess_corner(c: &CornerDescriptor) -> Corner {
    Corner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
        strength: c.response,
    }
}
