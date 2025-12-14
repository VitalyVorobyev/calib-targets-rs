use std::{env, fs, path::PathBuf, time::Instant};

use calib_targets_chessboard::{
    ChessboardDetectionResult, ChessboardDetector, ChessboardParams, GridGraphParams,
};
use calib_targets_core::{Corner as TargetCorner, LabeledCorner, TargetDetection, TargetKind};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};
#[cfg(feature = "tracing")]
use tracing::{info, info_span};
#[cfg(feature = "tracing")]
use tracing_log::LogTracer;
#[cfg(feature = "tracing")]
use tracing_subscriber::EnvFilter;

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
    #[serde(default)]
    debug_outputs: DebugOutputsConfig,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct DebugOutputsConfig {
    orientation_histogram: bool,
    grid_graph: bool,
    board_orientation: bool,
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
struct RawCornerOut {
    x: f32,
    y: f32,
    strength: f32,
    /// Strength normalized to [0, 1] by the max response in the image.
    confidence: f32,
}

#[derive(Debug, Serialize)]
struct OrientationBinOut {
    angle_rad: f32,
    angle_deg: f32,
    value: f32,
}

#[derive(Debug, Serialize)]
struct OrientationHistogramOut {
    bins: Vec<OrientationBinOut>,
}

#[derive(Debug, Serialize)]
struct OrientationSummaryOut {
    centers_rad: [f32; 2],
    centers_deg: [f32; 2],
}

#[derive(Debug, Serialize)]
struct GraphNeighborOut {
    index: usize,
    direction: String,
    distance: f32,
}

#[derive(Debug, Serialize)]
struct GraphNodeOut {
    index: usize,
    x: f32,
    y: f32,
    neighbors: Vec<GraphNeighborOut>,
}

#[derive(Debug, Serialize)]
struct GridGraphOut {
    nodes: Vec<GraphNodeOut>,
}

#[derive(Debug, Serialize)]
struct DetectionReport {
    detection: OutputDetection,
    inliers: Vec<usize>,
    orientations: Option<OrientationSummaryOut>,
    orientation_histogram: Option<OrientationHistogramOut>,
    grid_graph: Option<GridGraphOut>,
}

#[derive(Debug, Serialize)]
struct ExampleOutput {
    image_path: String,
    config_path: String,
    num_raw_corners: usize,
    raw_corners: Vec<RawCornerOut>,
    detections: Vec<DetectionReport>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

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
    let span_corners = make_span("chess_corners");
    let raw_corners: Vec<CornerDescriptor> = {
        let _g = span_corners.enter();
        let t0 = Instant::now();
        let corners = find_chess_corners_image(&img, &chess_cfg);
        let dt = t0.elapsed().as_millis() as u64;
        #[cfg(feature = "tracing")]
        info!(
            duration_ms = dt,
            count = corners.len(),
            "found ChESS corners"
        );
        #[cfg(not(feature = "tracing"))]
        log::info!(
            "found ChESS corners duration_ms={} count={}",
            dt,
            corners.len()
        );
        corners
    };

    // Adapt ChESS corners to calib-targets core `Corner` type.
    let target_corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();

    // Configure the chessboard detector.
    let detector = ChessboardDetector::new(cfg.chessboard).with_grid_search(cfg.graph);
    let span_detect = make_span("chessboard_detect");
    let detection = {
        let _g = span_detect.enter();
        let t0 = Instant::now();
        let det = detector.detect_from_corners(&target_corners);
        let dt = t0.elapsed().as_millis() as u64;
        #[cfg(feature = "tracing")]
        info!(
            duration_ms = dt,
            detected = det.is_some(),
            "chessboard detection finished"
        );
        #[cfg(not(feature = "tracing"))]
        log::info!(
            "chessboard detection finished duration_ms={} detected={}",
            dt,
            det.is_some()
        );
        det
    };

    let max_response = raw_corners
        .iter()
        .map(|c| c.response)
        .fold(0.0_f32, f32::max);
    let response_scale = if max_response > 0.0 {
        1.0 / max_response
    } else {
        0.0
    };
    let raw_corners_out: Vec<RawCornerOut> = raw_corners
        .iter()
        .map(|c| RawCornerOut {
            x: c.x,
            y: c.y,
            strength: c.response,
            confidence: (c.response * response_scale).clamp(0.0, 1.0),
        })
        .collect();

    let output = ExampleOutput {
        image_path: cfg.image_path.clone(),
        config_path: config_path.to_string_lossy().into_owned(),
        num_raw_corners: raw_corners.len(),
        raw_corners: raw_corners_out,
        detections: detection
            .into_iter()
            .map(|res| map_detection_report(res, &cfg.debug_outputs))
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

fn init_tracing() {
    // Ignore errors if a logger/subscriber was already installed (e.g. when
    // running multiple examples in the same process).
    #[cfg(feature = "tracing")]
    {
        let _ = LogTracer::init();
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
        let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
    }
}

#[cfg(feature = "tracing")]
fn make_span(name: &str) -> tracing::Span {
    info_span!(name)
}

#[cfg(not(feature = "tracing"))]
fn make_span(_name: &str) -> NoopSpan {
    NoopSpan
}

#[cfg(not(feature = "tracing"))]
struct NoopSpan;
#[cfg(not(feature = "tracing"))]
struct NoopGuard;
#[cfg(not(feature = "tracing"))]
impl NoopSpan {
    fn enter(&self) -> NoopGuard {
        NoopGuard
    }
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

fn map_detection_report(
    res: ChessboardDetectionResult,
    debug_cfg: &DebugOutputsConfig,
) -> DetectionReport {
    let orientations = res.orientations.and_then(|c| {
        if debug_cfg.board_orientation {
            Some(OrientationSummaryOut {
                centers_rad: c,
                centers_deg: [c[0].to_degrees(), c[1].to_degrees()],
            })
        } else {
            None
        }
    });

    let orientation_histogram = if debug_cfg.orientation_histogram {
        res.debug.orientation_histogram.as_ref().map(|hist| {
            let bins = hist
                .bin_centers
                .iter()
                .zip(hist.values.iter())
                .map(|(angle_rad, value)| OrientationBinOut {
                    angle_rad: *angle_rad,
                    angle_deg: angle_rad.to_degrees(),
                    value: *value,
                })
                .collect();
            OrientationHistogramOut { bins }
        })
    } else {
        None
    };

    let grid_graph = if debug_cfg.grid_graph {
        res.debug.graph.as_ref().map(|g| {
            let nodes = g
                .nodes
                .iter()
                .enumerate()
                .map(|(idx, node)| {
                    let neighbors = node
                        .neighbors
                        .iter()
                        .map(|n| GraphNeighborOut {
                            index: n.index,
                            direction: n.direction.to_string(),
                            distance: n.distance,
                        })
                        .collect();
                    GraphNodeOut {
                        index: idx,
                        x: node.position[0],
                        y: node.position[1],
                        neighbors,
                    }
                })
                .collect();
            GridGraphOut { nodes }
        })
    } else {
        None
    };

    DetectionReport {
        detection: map_detection(res.detection),
        inliers: res.inliers,
        orientations,
        orientation_histogram,
        grid_graph,
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
