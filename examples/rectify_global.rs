use std::{env, fs, path::PathBuf};

use calib_targets_chessboard::{
    rectify_from_chessboard_result, ChessboardDetector, ChessboardParams, GridGraphParams,
};
use calib_targets_core::{Corner as TargetCorner, GrayImageView, LabeledCorner, TargetKind};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::{save_buffer, ImageBuffer, ImageReader, Luma};
use log::LevelFilter;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct ExampleConfig {
    image_path: String,
    #[serde(default)]
    rectified_path: Option<String>,
    #[serde(default)]
    report_path: Option<String>,
    #[serde(default = "default_px_per_square")]
    px_per_square: f32,
    #[serde(default = "default_margin_squares")]
    margin_squares: f32,
    chessboard: ChessboardParams,
    graph: GridGraphParams,
}

fn default_px_per_square() -> f32 {
    40.0
}

fn default_margin_squares() -> f32 {
    0.5
}

#[derive(Debug, Serialize, Clone)]
struct OutputCorner {
    x: f32,
    y: f32,
    grid: Option<[i32; 2]>,
    id: Option<u32>,
    confidence: f32,
}

#[derive(Debug, Serialize, Clone)]
struct OutputDetection {
    kind: String,
    corners: Vec<OutputCorner>,
}

#[derive(Debug, Serialize, Clone)]
struct RectifiedInfo {
    width: usize,
    height: usize,
    px_per_square: f32,
    min_i: i32,
    max_i: i32,
    min_j: i32,
    max_j: i32,
    h_img_from_rect: [[f64; 3]; 3],
    h_rect_from_img: [[f64; 3]; 3],
}

#[derive(Debug, Serialize)]
struct ExampleReport {
    image_path: String,
    config_path: String,
    num_raw_corners: usize,
    detection: Option<OutputDetection>,
    inliers: Vec<usize>,
    orientations: Option<[f32; 2]>,
    rectified: Option<RectifiedInfo>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();

    let args: Vec<String> = env::args().collect();
    let config_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tmpdata/rectify_config.json"));

    let cfg: ExampleConfig = {
        let raw = fs::read_to_string(&config_path)?;
        serde_json::from_str(&raw)?
    };

    let image_path = PathBuf::from(&cfg.image_path);
    let img = ImageReader::open(&image_path)?.decode()?.to_luma8();

    // Run ChESS corner detector from the `chess-corners` crate.
    let mut chess_cfg = ChessConfig::single_scale();
    chess_cfg.params.threshold_rel = 0.2;
    chess_cfg.params.nms_radius = 2;
    let raw_corners: Vec<CornerDescriptor> = find_chess_corners_image(&img, &chess_cfg);
    println!("found {} raw ChESS corners", raw_corners.len());

    // Adapt ChESS corners to calib-targets core `Corner` type.
    let target_corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    // Configure the chessboard detector.
    let detector = ChessboardDetector::new(cfg.chessboard).with_grid_search(cfg.graph);
    let detection = detector.detect_from_corners(&target_corners);

    // Prepare GrayImageView for rectification.
    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let mut rectified_png_written = false;
    let mut rectified_info = None;
    let mut detection_json = None;
    let mut inliers = Vec::new();
    let mut orientations = None;

    if let Some(det_res) = detection {
        orientations = det_res.orientations;
        inliers = det_res.inliers.clone();
        detection_json = Some(map_detection(det_res.detection.clone()));

        if let Ok(rectified) = rectify_from_chessboard_result(
            &src_view,
            &det_res.detection.corners,
            &det_res.inliers,
            cfg.px_per_square,
            cfg.margin_squares,
        ) {
            let rect_img = ImageBuffer::<Luma<u8>, _>::from_raw(
                rectified.rect.width as u32,
                rectified.rect.height as u32,
                rectified.rect.data,
            )
            .expect("failed to build output image");

            let rectified_path = cfg
                .rectified_path
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("tmpdata/rectified.png"));

            save_buffer(
                &rectified_path,
                rect_img.as_raw(),
                rect_img.width(),
                rect_img.height(),
                image::ColorType::L8,
            )?;
            println!("wrote rectified image to {}", rectified_path.display());
            rectified_png_written = true;

            rectified_info = Some(RectifiedInfo {
                width: rect_img.width() as usize,
                height: rect_img.height() as usize,
                px_per_square: rectified.px_per_square,
                min_i: rectified.min_i,
                max_i: rectified.max_i,
                min_j: rectified.min_j,
                max_j: rectified.max_j,
                h_img_from_rect: rectified.h_img_from_rect.to_array(),
                h_rect_from_img: rectified.h_rect_from_img.to_array(),
            });
        }
    }

    let detection_present = detection_json.is_some();

    let report = ExampleReport {
        image_path: cfg.image_path.clone(),
        config_path: config_path.to_string_lossy().into_owned(),
        num_raw_corners: raw_corners.len(),
        detection: detection_json.clone(),
        inliers,
        orientations,
        rectified: rectified_info,
    };

    let report_path = cfg
        .report_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tmpdata/rectify_report.json"));

    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&report_path, json)?;
    println!("wrote report JSON to {}", report_path.display());

    if !detection_present {
        eprintln!("no chessboard detected; rectification not performed");
    } else if !rectified_png_written {
        eprintln!("chessboard detected but rectification failed");
    }

    Ok(())
}

fn adapt_chess_corner(c: &CornerDescriptor) -> TargetCorner {
    TargetCorner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
        strength: c.response,
        phase: c.phase,
    }
}

fn map_detection(det: calib_targets_core::TargetDetection) -> OutputDetection {
    OutputDetection {
        kind: match det.kind {
            TargetKind::Chessboard => "chessboard",
            TargetKind::Charuco => "charuco",
            TargetKind::CheckerboardMarker => "checkerboard_marker",
        }
        .to_string(),
        corners: det.corners.into_iter().map(map_corner).collect(),
    }
}

fn map_corner(c: LabeledCorner) -> OutputCorner {
    OutputCorner {
        x: c.position.x,
        y: c.position.y,
        grid: c.grid.map(|g| [g.i, g.j]),
        id: c.id,
        confidence: c.confidence,
    }
}

static LOGGER: SimpleLogger = SimpleLogger;

struct SimpleLogger;

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= LevelFilter::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "[{}] {}: {}",
                record.level(),
                record.target(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

fn init_logger() {
    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(LevelFilter::Info));
}
