use std::{
    env, fs,
    path::{Path, PathBuf},
};

use calib_targets_aruco::{
    builtins, scan_decode_markers, ArucoScanConfig, MarkerDetection, Matcher, ScanDecodeConfig,
};
use calib_targets_chessboard::{
    rectify_mesh_from_grid, ChessboardDetectionResult, ChessboardDetector, ChessboardParams,
    GridGraphParams, RectifiedMeshView,
};
use calib_targets_core::{Corner, GrayImageView, TargetDetection};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::{save_buffer, ImageReader};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct ExampleConfig {
    image_path: String,
    #[serde(default)]
    mesh_rectified_path: Option<String>,
    #[serde(default)]
    report_path: Option<String>,
    #[serde(default = "default_px_per_square")]
    px_per_square: f32,
    #[serde(default = "default_aruco_dictionary")]
    aruco_dictionary: String,
    #[serde(default)]
    aruco_max_hamming: Option<u8>,
    #[serde(default)]
    aruco: Option<ArucoScanConfig>,
    chessboard: ChessboardParams,
    graph: GridGraphParams,
}

fn default_px_per_square() -> f32 {
    40.0
}

fn default_aruco_dictionary() -> String {
    "DICT_4X4_1000".to_string()
}

#[derive(Debug, Serialize)]
struct RectifyMeshReport {
    image_path: String,
    config_path: String,
    num_raw_corners: usize,
    detection: Option<TargetDetection>,
    mesh_rectified_path: Option<String>,
    markers: Option<Vec<MarkerDetection>>,
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

    let detector =
        ChessboardDetector::new(cfg.chessboard.clone()).with_grid_search(cfg.graph.clone());
    let detection = detector.detect_from_corners(&corners);
    let src_view = make_view(&img);

    let mut report = RectifyMeshReport {
        image_path: cfg.image_path.clone(),
        config_path: config_path.to_string_lossy().into_owned(),
        num_raw_corners: raw_corners.len(),
        detection: None,
        mesh_rectified_path: None,
        markers: None,
    };

    if let Some(det_res) = detection {
        let ChessboardDetectionResult {
            detection, inliers, ..
        } = det_res;

        if let Ok(rectified) =
            rectify_mesh_from_grid(&src_view, &detection.corners, &inliers, cfg.px_per_square)
        {
            let mesh_path = cfg
                .mesh_rectified_path
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("tmpdata/mesh_rectified.png"));
            save_mesh(&mesh_path, &rectified)?;
            report.mesh_rectified_path = Some(mesh_path.to_string_lossy().into_owned());

            if let Some(markers) = decode_markers(&rectified, &cfg) {
                if !markers.is_empty() {
                    report.markers = Some(markers);
                }
            }
        }

        report.detection = Some(detection);
    }

    let report_path = cfg
        .report_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tmpdata/mesh_report.json"));

    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&report_path, json)?;
    println!("wrote report JSON to {}", report_path.display());

    Ok(())
}

fn parse_config_path() -> PathBuf {
    env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tmpdata/rectify_config.json"))
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

fn make_view(img: &image::GrayImage) -> GrayImageView<'_> {
    GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    }
}

fn adapt_chess_corner(c: &CornerDescriptor) -> Corner {
    Corner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
        strength: c.response,
    }
}

fn save_mesh(path: &Path, rectified: &RectifiedMeshView) -> Result<(), image::ImageError> {
    save_buffer(
        path,
        &rectified.rect.data,
        rectified.rect.width as u32,
        rectified.rect.height as u32,
        image::ColorType::L8,
    )
}

fn decode_markers(
    rectified: &RectifiedMeshView,
    cfg: &ExampleConfig,
) -> Option<Vec<MarkerDetection>> {
    let dict = builtins::builtin_dictionary(&cfg.aruco_dictionary)?;
    let max_hamming = cfg
        .aruco
        .as_ref()
        .and_then(|cfg| cfg.max_hamming)
        .or(cfg.aruco_max_hamming)
        .unwrap_or(dict.max_correction_bits.min(2));
    let matcher = Matcher::new(dict, max_hamming);

    let mut scan_cfg = ScanDecodeConfig::default();
    if let Some(aruco) = cfg.aruco.as_ref() {
        aruco.apply_to_scan(&mut scan_cfg);
    }

    let rect_view = GrayImageView {
        width: rectified.rect.width,
        height: rectified.rect.height,
        data: &rectified.rect.data,
    };

    Some(scan_decode_markers(
        &rect_view,
        rectified.cells_x,
        rectified.cells_y,
        rectified.px_per_square,
        &scan_cfg,
        &matcher,
    ))
}
