use std::{env, fs, path::PathBuf, time::Instant};

use calib_targets_aruco::{builtins, scan_decode_markers, Matcher, ScanDecodeConfig};
use calib_targets_chessboard::{
    rectify_mesh_from_grid, ChessboardDetector, ChessboardParams, GridGraphParams,
    RectifiedMeshView,
};
use calib_targets_core::GrayImageView;
use calib_targets_core::{Corner as TargetCorner, LabeledCorner, TargetKind};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::{save_buffer, ImageBuffer, ImageReader, Luma};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};
#[cfg(feature = "tracing")]
use tracing::{info, info_span};
#[cfg(feature = "tracing")]
use tracing_log::LogTracer;
#[cfg(feature = "tracing")]
use tracing_subscriber::EnvFilter;

#[derive(Debug, Deserialize)]
struct ExampleConfig {
    image_path: String,
    #[serde(default)]
    mesh_rectified_path: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    rectified_path: Option<String>, // kept for compatibility with existing configs
    #[serde(default)]
    report_path: Option<String>,
    #[serde(default = "default_px_per_square")]
    px_per_square: f32,
    #[serde(default)]
    #[allow(dead_code)]
    margin_squares: Option<f32>, // ignored for mesh warp; present for compatibility
    #[serde(default = "default_aruco_dictionary")]
    aruco_dictionary: String,
    #[serde(default)]
    aruco_max_hamming: Option<u8>,
    chessboard: ChessboardParams,
    graph: GridGraphParams,
}

fn default_px_per_square() -> f32 {
    40.0
}

fn default_aruco_dictionary() -> String {
    "DICT_4X4_1000".to_string()
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
struct OutputMarker {
    id: u32,
    cell: [i32; 2],
    grid_cell: [i32; 2],
    rotation: u8,
    hamming: u8,
    score: f32,
    border_score: f32,
    inverted: bool,
    corners_rect: [[f32; 2]; 4],
    corners_img: Option<[[f32; 2]; 4]>,
}

#[derive(Debug, Serialize, Clone)]
struct MarkerScanOut {
    dictionary: String,
    max_hamming: u8,
    markers: Vec<OutputMarker>,
}

#[derive(Debug, Serialize, Clone)]
struct MeshRectifiedOut {
    path: String,
    width: usize,
    height: usize,
    px_per_square: f32,
    min_i: i32,
    min_j: i32,
    cells_x: usize,
    cells_y: usize,
    valid_cells: usize,
}

#[derive(Debug, Serialize, Clone)]
struct TimingsMs {
    load_image: u64,
    detect_corners: u64,
    adapt_corners: u64,
    detect_board: u64,
    mesh_rectify: Option<u64>,
    save_mesh: Option<u64>,
    scan_markers: Option<u64>,
    total: u64,
}

#[derive(Debug, Serialize)]
struct ExampleReport {
    image_path: String,
    config_path: String,
    num_raw_corners: usize,
    detection: Option<OutputDetection>,
    inliers: Vec<usize>,
    orientations: Option<[f32; 2]>,
    mesh_rectified: Option<MeshRectifiedOut>,
    markers: Option<MarkerScanOut>,
    timings_ms: TimingsMs,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let args: Vec<String> = env::args().collect();
    let config_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tmpdata/rectify_config.json"));

    let cfg: ExampleConfig = {
        let raw = fs::read_to_string(&config_path)?;
        serde_json::from_str(&raw)?
    };

    let t_total = Instant::now();

    let image_path = PathBuf::from(&cfg.image_path);
    let t_img = Instant::now();
    let img = ImageReader::open(&image_path)?.decode()?.to_luma8();
    let load_image_ms = t_img.elapsed().as_millis() as u64;

    // Run ChESS corner detector from the `chess-corners` crate.
    let mut chess_cfg = ChessConfig::single_scale();
    chess_cfg.params.threshold_rel = 0.2;
    chess_cfg.params.nms_radius = 2;

    let span_corners = make_span("chess_corners");
    let (raw_corners, detect_corners_ms): (Vec<CornerDescriptor>, u64) = {
        let _g = span_corners.enter();
        let t0 = Instant::now();
        let corners = find_chess_corners_image(&img, &chess_cfg);
        let detect_corners_ms = t0.elapsed().as_millis() as u64;
        #[cfg(feature = "tracing")]
        info!(
            duration_ms = detect_corners_ms,
            count = corners.len(),
            "found ChESS corners"
        );
        #[cfg(not(feature = "tracing"))]
        log::info!(
            "found ChESS corners duration_ms={} count={}",
            detect_corners_ms,
            corners.len()
        );
        (corners, detect_corners_ms)
    };

    let t_adapt = Instant::now();
    let target_corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();
    let adapt_ms = t_adapt.elapsed().as_millis() as u64;
    #[cfg(feature = "tracing")]
    info!(
        duration_ms = adapt_ms,
        "adapted corners to calib-targets core type"
    );
    #[cfg(not(feature = "tracing"))]
    log::info!(
        "adapted corners to calib-targets core type duration_ms={}",
        adapt_ms
    );

    // Configure the chessboard detector.
    let detector = ChessboardDetector::new(cfg.chessboard).with_grid_search(cfg.graph);
    let span_detect = make_span("chessboard_detect");
    let (detection_res, detect_board_ms) = {
        let _g = span_detect.enter();
        let t0 = Instant::now();
        let det = detector.detect_from_corners(&target_corners);
        let detect_board_ms = t0.elapsed().as_millis() as u64;
        #[cfg(feature = "tracing")]
        info!(
            duration_ms = detect_board_ms,
            detected = det.is_some(),
            "chessboard detection finished"
        );
        #[cfg(not(feature = "tracing"))]
        log::info!(
            "chessboard detection finished duration_ms={} detected={}",
            detect_board_ms,
            det.is_some()
        );
        (det, detect_board_ms)
    };

    // Prepare GrayImageView for rectification.
    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let mut mesh_rectified = None;
    let mut mesh_rectify_ms = None;
    let mut mesh_save_ms = None;
    let mut scan_markers_ms = None;
    let mut detection_json = None;
    let mut markers_json = None;
    let mut inliers = Vec::new();
    let mut orientations = None;

    if let Some(det_res) = detection_res {
        orientations = det_res.orientations;
        inliers = det_res.inliers.clone();
        detection_json = Some(map_detection(det_res.detection.clone()));

        let t_mesh = Instant::now();
        match rectify_mesh_from_grid(
            &src_view,
            &det_res.detection.corners,
            &det_res.inliers,
            cfg.px_per_square,
        ) {
            Ok(rectified) => {
                let rect_ms = t_mesh.elapsed().as_millis() as u64;
                mesh_rectify_ms = Some(rect_ms);
                #[cfg(feature = "tracing")]
                info!(
                    duration_ms = rect_ms,
                    width = rectified.rect.width,
                    height = rectified.rect.height,
                    valid_cells = rectified.valid_cells,
                    "mesh rectification succeeded"
                );
                #[cfg(not(feature = "tracing"))]
                log::info!(
                    "mesh rectification succeeded duration_ms={} width={} height={} valid_cells={}",
                    rect_ms,
                    rectified.rect.width,
                    rectified.rect.height,
                    rectified.valid_cells
                );
                let t_save = Instant::now();
                let mesh_path = cfg
                    .mesh_rectified_path
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("tmpdata/mesh_rectified.png"));

                save_mesh_view(&mesh_path, &rectified)?;
                mesh_save_ms = Some(t_save.elapsed().as_millis() as u64);

                // Decode ArUco markers on the rectified grid.
                let span_markers = make_span("aruco_decode");
                let (markers, scan_ms) = {
                    let _g = span_markers.enter();
                    let t0 = Instant::now();

                    match builtins::builtin_dictionary(&cfg.aruco_dictionary) {
                        None => {
                            #[cfg(feature = "tracing")]
                            info!(
                                dictionary = %cfg.aruco_dictionary,
                                "unknown dictionary; skipping marker decoding"
                            );
                            #[cfg(not(feature = "tracing"))]
                            log::info!(
                                "unknown dictionary {}; skipping marker decoding",
                                cfg.aruco_dictionary
                            );
                            (Vec::new(), 0u64)
                        }
                        Some(dict) => {
                            if dict.codes.is_empty() {
                                (Vec::new(), 0u64)
                            } else {
                                let max_hamming = cfg
                                    .aruco_max_hamming
                                    .unwrap_or(dict.max_correction_bits.min(2));
                                let matcher = Matcher::new(dict, max_hamming);
                                let scan_cfg = ScanDecodeConfig::default();

                                let rect_view = GrayImageView {
                                    width: rectified.rect.width,
                                    height: rectified.rect.height,
                                    data: &rectified.rect.data,
                                };

                                let dets = scan_decode_markers(
                                    &rect_view,
                                    rectified.cells_x,
                                    rectified.cells_y,
                                    rectified.px_per_square,
                                    &scan_cfg,
                                    &matcher,
                                );

                                let scan_ms = t0.elapsed().as_millis() as u64;
                                #[cfg(feature = "tracing")]
                                info!(
                                    duration_ms = scan_ms,
                                    dictionary = %dict.name,
                                    max_hamming = max_hamming,
                                    count = dets.len(),
                                    "decoded markers on rectified grid"
                                );
                                #[cfg(not(feature = "tracing"))]
                                log::info!(
                                    "decoded markers on rectified grid duration_ms={} dictionary={} max_hamming={} count={}",
                                    scan_ms,
                                    dict.name,
                                    max_hamming,
                                    dets.len()
                                );

                                (dets, scan_ms)
                            }
                        }
                    }
                };
                if scan_ms > 0 {
                    scan_markers_ms = Some(scan_ms);
                }

                if !markers.is_empty() {
                    let dict_name = cfg.aruco_dictionary.clone();
                    let dict = builtins::builtin_dictionary(&dict_name)
                        .expect("dictionary was already validated");
                    let max_hamming = cfg
                        .aruco_max_hamming
                        .unwrap_or(dict.max_correction_bits.min(2));
                    markers_json = Some(MarkerScanOut {
                        dictionary: dict_name,
                        max_hamming,
                        markers: markers
                            .into_iter()
                            .map(|m| map_marker(&rectified, m))
                            .collect(),
                    });
                }

                mesh_rectified = Some(map_mesh_rectified(mesh_path, rectified));
            }
            Err(err) => {
                #[cfg(feature = "tracing")]
                info!(error = %err, "mesh rectification failed");
                #[cfg(not(feature = "tracing"))]
                log::info!("mesh rectification failed: {}", err);
            }
        }
    }

    let total_ms = t_total.elapsed().as_millis() as u64;

    let report = ExampleReport {
        image_path: cfg.image_path.clone(),
        config_path: config_path.to_string_lossy().into_owned(),
        num_raw_corners: raw_corners.len(),
        detection: detection_json,
        inliers,
        orientations,
        mesh_rectified,
        markers: markers_json,
        timings_ms: TimingsMs {
            load_image: load_image_ms,
            detect_corners: detect_corners_ms,
            adapt_corners: adapt_ms,
            detect_board: detect_board_ms,
            mesh_rectify: mesh_rectify_ms,
            save_mesh: mesh_save_ms,
            scan_markers: scan_markers_ms,
            total: total_ms,
        },
    };

    let report_path = cfg
        .report_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tmpdata/mesh_report.json"));

    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&report_path, json)?;
    log::info!("wrote mesh report JSON to {}", report_path.display());

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
        confidence: c.score,
    }
}

fn map_mesh_rectified(path: PathBuf, rect: RectifiedMeshView) -> MeshRectifiedOut {
    MeshRectifiedOut {
        path: path.to_string_lossy().into_owned(),
        width: rect.rect.width,
        height: rect.rect.height,
        px_per_square: rect.px_per_square,
        min_i: rect.min_i,
        min_j: rect.min_j,
        cells_x: rect.cells_x,
        cells_y: rect.cells_y,
        valid_cells: rect.valid_cells,
    }
}

fn map_marker(rect: &RectifiedMeshView, m: calib_targets_aruco::MarkerDetection) -> OutputMarker {
    let corners_rect = m.corners_rect.map(|p| [p.x, p.y]);

    let ci = m.gc.gx.max(0) as usize;
    let cj = m.gc.gy.max(0) as usize;
    let corners_img = rect
        .cell_corners_img(ci, cj)
        .map(|pts| pts.map(|p| [p.x, p.y]));

    OutputMarker {
        id: m.id,
        cell: [m.gc.gx, m.gc.gy],
        grid_cell: [rect.min_i + m.gc.gx, rect.min_j + m.gc.gy],
        rotation: m.rotation,
        hamming: m.hamming,
        score: m.score,
        border_score: m.border_score,
        inverted: m.inverted,
        corners_rect,
        corners_img,
    }
}

fn save_mesh_view(path: &PathBuf, rect: &RectifiedMeshView) -> Result<(), image::ImageError> {
    let img_buf = ImageBuffer::<Luma<u8>, _>::from_raw(
        rect.rect.width as u32,
        rect.rect.height as u32,
        rect.rect.data.clone(),
    )
    .expect("failed to build mesh output image");

    save_buffer(
        path,
        img_buf.as_raw(),
        img_buf.width(),
        img_buf.height(),
        image::ColorType::L8,
    )?;
    log::info!("wrote mesh-rectified image to {}", path.display());
    Ok(())
}
