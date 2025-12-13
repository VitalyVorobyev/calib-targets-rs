use std::{env, fs, path::PathBuf};

use calib_targets_chessboard::{ChessboardDetector, ChessboardParams, GridGraphParams};
use calib_targets_core::{Corner as TargetCorner, LabeledCorner, TargetDetection, TargetKind};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use log::LevelFilter;
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
struct OutputCorner {
    x: f32,
    y: f32,
    /// Optional grid coordinates (i, j) on the board.
    grid: Option<[i32; 2]>,
    /// Optional logical ID (unused for plain chessboard).
    id: Option<u32>,
    /// Detection confidence in [0, 1].
    confidence: f32,
}

#[derive(Debug, Serialize)]
struct OutputDetection {
    kind: String,
    corners: Vec<OutputCorner>,
}

#[derive(Debug, Serialize)]
struct ExampleOutput {
    image_path: String,
    config_path: String,
    num_raw_corners: usize,
    detections: Vec<OutputDetection>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();

    let args: Vec<String> = env::args().collect();
    let config_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/chessboard_config.json"));

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

    let output = ExampleOutput {
        image_path: cfg.image_path.clone(),
        config_path: config_path.to_string_lossy().into_owned(),
        num_raw_corners: raw_corners.len(),
        detections: detection
            .into_iter()
            .map(|res| map_detection(res.detection))
            .collect(),
    };

    let output_path = cfg
        .output_path
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/chessboard_detection.json"));

    let json = serde_json::to_string_pretty(&output)?;
    fs::write(&output_path, json)?;
    println!("wrote detection JSON to {}", output_path.display());

    Ok(())
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

fn adapt_chess_corner(c: &CornerDescriptor) -> TargetCorner {
    TargetCorner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
        strength: c.response,
        phase: c.phase,
    }
}

fn map_detection(det: TargetDetection) -> OutputDetection {
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
