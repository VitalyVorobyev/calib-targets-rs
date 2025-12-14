use std::{env, fs, path::PathBuf, time::Instant};

use calib_targets_aruco::{builtins, ScanDecodeConfig};
use calib_targets_charuco::{
    CharucoBoard, CharucoBoardSpec, CharucoDetectError, CharucoDetectionResult, CharucoDetector,
    CharucoDetectorParams, MarkerLayout,
};
use calib_targets_chessboard::{ChessboardParams, GridGraphParams};
use calib_targets_core::{Corner as TargetCorner, GrayImageView, LabeledCorner, TargetKind};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::{save_buffer, ImageBuffer, ImageReader, Luma};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};
use tracing::{info, info_span};
use tracing_log::LogTracer;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Deserialize, Serialize)]
struct BoardConfig {
    rows: u32,
    cols: u32,
    cell_size: f32,
    marker_size_rel: f32,
    dictionary: String,
    #[serde(default)]
    marker_layout: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ArucoConfig {
    #[serde(default)]
    max_hamming: Option<u8>,
    #[serde(default)]
    border_bits: Option<usize>,
    #[serde(default)]
    inset_frac: Option<f32>,
    #[serde(default)]
    min_border_score: Option<f32>,
    #[serde(default)]
    dedup_by_id: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ExampleConfig {
    image_path: String,
    board: BoardConfig,
    #[serde(default)]
    output_path: Option<String>,
    #[serde(default)]
    mesh_rectified_path: Option<String>,
    #[serde(default = "default_px_per_square")]
    px_per_square: f32,
    #[serde(default)]
    min_marker_inliers: Option<usize>,
    #[serde(default)]
    aruco: Option<ArucoConfig>,
    #[serde(default)]
    chessboard: Option<ChessboardParams>,
    #[serde(default)]
    graph: Option<GridGraphParams>,
}

fn default_px_per_square() -> f32 {
    60.0
}

#[derive(Debug, Serialize, Clone)]
struct OutputCorner {
    x: f32,
    y: f32,
    grid: Option<[i32; 2]>,
    id: Option<u32>,
    confidence: f32,
    object_xy: Option<[f32; 2]>,
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
    expected_board_cell: Option<[i32; 2]>,
    mapped_board_cell: [i32; 2],
    rotation: u8,
    hamming: u8,
    score: f32,
    border_score: f32,
    inverted: bool,
    corners_rect: [[f32; 2]; 4],
    corners_img: Option<[[f32; 2]; 4]>,
    inlier: bool,
}

#[derive(Debug, Serialize, Clone)]
struct RectifiedOut {
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
struct AlignmentOut {
    transform: [i32; 4],
    translation: [i32; 2],
    marker_inliers: usize,
}

#[derive(Debug, Serialize, Clone)]
struct TimingsMs {
    load_image: u64,
    detect_corners: u64,
    adapt_corners: u64,
    detect_charuco: u64,
    save_mesh: Option<u64>,
    total: u64,
}

#[derive(Debug, Serialize)]
struct ExampleReport {
    image_path: String,
    config_path: String,
    board: BoardConfig,
    num_raw_corners: usize,
    chessboard: Option<OutputDetection>,
    charuco: Option<OutputDetection>,
    rectified: Option<RectifiedOut>,
    markers: Option<Vec<OutputMarker>>,
    alignment: Option<AlignmentOut>,
    error: Option<String>,
    timings_ms: TimingsMs,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let args: Vec<String> = env::args().collect();
    let config_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/charuco_detect_config.json"));

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

    let span_corners = info_span!("chess_corners");
    let (raw_corners, detect_corners_ms): (Vec<CornerDescriptor>, u64) = {
        let _g = span_corners.enter();
        let t0 = Instant::now();
        let corners = find_chess_corners_image(&img, &chess_cfg);
        let dt = t0.elapsed().as_millis() as u64;
        info!(
            duration_ms = dt,
            count = corners.len(),
            "found ChESS corners"
        );
        (corners, dt)
    };

    let t_adapt = Instant::now();
    let target_corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();
    let adapt_ms = t_adapt.elapsed().as_millis() as u64;

    // Prepare GrayImageView for the detector.
    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let (board, detector) = build_detector(&cfg)?;

    let span_detect = info_span!("charuco_detect");
    let (result, detect_charuco_ms) = {
        let _g = span_detect.enter();
        let t0 = Instant::now();
        let res = detector.detect(&src_view, &target_corners);
        (res, t0.elapsed().as_millis() as u64)
    };

    let mut report = ExampleReport {
        image_path: cfg.image_path.clone(),
        config_path: config_path.to_string_lossy().into_owned(),
        board: cfg.board,
        num_raw_corners: raw_corners.len(),
        chessboard: None,
        charuco: None,
        rectified: None,
        markers: None,
        alignment: None,
        error: None,
        timings_ms: TimingsMs {
            load_image: load_image_ms,
            detect_corners: detect_corners_ms,
            adapt_corners: adapt_ms,
            detect_charuco: detect_charuco_ms,
            save_mesh: None,
            total: 0,
        },
    };

    match result {
        Ok(res) => {
            report.chessboard = Some(map_detection(&res.chessboard, None));
            report.charuco = Some(map_detection(&res.detection, Some(&board)));

            let rectified_path = cfg
                .mesh_rectified_path
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("testdata/charuco_detect_rectified.png"));

            let t_save = Instant::now();
            save_mesh_view(&rectified_path, &res.rectified)?;
            report.timings_ms.save_mesh = Some(t_save.elapsed().as_millis() as u64);

            report.rectified = Some(RectifiedOut {
                path: rectified_path.to_string_lossy().into_owned(),
                width: res.rectified.rect.width,
                height: res.rectified.rect.height,
                px_per_square: res.rectified.px_per_square,
                min_i: res.rectified.min_i,
                min_j: res.rectified.min_j,
                cells_x: res.rectified.cells_x,
                cells_y: res.rectified.cells_y,
                valid_cells: res.rectified.valid_cells,
            });

            report.alignment = Some(AlignmentOut {
                transform: [
                    res.alignment.transform.a,
                    res.alignment.transform.b,
                    res.alignment.transform.c,
                    res.alignment.transform.d,
                ],
                translation: res.alignment.translation,
                marker_inliers: res.alignment.marker_inliers.len(),
            });

            report.markers = Some(map_markers(&board, &res));
        }
        Err(err) => {
            report.error = Some(format_detect_error(err));
        }
    }

    report.timings_ms.total = t_total.elapsed().as_millis() as u64;

    let output_path = cfg
        .output_path
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/charuco_detect_report.json"));

    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&output_path, json)?;
    info!("wrote report JSON to {}", output_path.display());

    Ok(())
}

fn init_tracing() {
    let _ = LogTracer::init();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

fn build_detector(
    cfg: &ExampleConfig,
) -> Result<(CharucoBoard, CharucoDetector), Box<dyn std::error::Error>> {
    let dict = builtins::builtin_dictionary(&cfg.board.dictionary)
        .ok_or_else(|| format!("unknown dictionary {}", cfg.board.dictionary))?;

    let layout = match cfg.board.marker_layout.as_deref() {
        None | Some("opencv_charuco") => MarkerLayout::OpenCvCharuco,
        Some(other) => return Err(format!("unknown marker_layout {other}").into()),
    };

    let board = CharucoBoard::new(CharucoBoardSpec {
        rows: cfg.board.rows,
        cols: cfg.board.cols,
        cell_size: cfg.board.cell_size,
        marker_size_rel: cfg.board.marker_size_rel,
        dictionary: dict,
        marker_layout: layout,
    })?;

    let mut params = CharucoDetectorParams::for_board(&board);
    params.px_per_square = cfg.px_per_square;
    if let Some(min_marker_inliers) = cfg.min_marker_inliers {
        params.min_marker_inliers = min_marker_inliers;
    }

    if let Some(chessboard) = cfg.chessboard.clone() {
        params.chessboard = chessboard;
        if params.chessboard.expected_rows.is_none() {
            params.chessboard.expected_rows = Some(board.expected_inner_rows());
        }
        if params.chessboard.expected_cols.is_none() {
            params.chessboard.expected_cols = Some(board.expected_inner_cols());
        }
    }
    if let Some(graph) = cfg.graph.clone() {
        params.graph = graph;
    }

    if let Some(aruco) = cfg.aruco.as_ref() {
        if let Some(max_hamming) = aruco.max_hamming {
            params.max_hamming = max_hamming;
        }

        let mut scan = ScanDecodeConfig {
            marker_size_rel: cfg.board.marker_size_rel,
            ..ScanDecodeConfig::default()
        };
        if let Some(border_bits) = aruco.border_bits {
            scan.border_bits = border_bits;
        }
        if let Some(inset_frac) = aruco.inset_frac {
            scan.inset_frac = inset_frac;
        }
        if let Some(min_border_score) = aruco.min_border_score {
            scan.min_border_score = min_border_score;
        }
        if let Some(dedup_by_id) = aruco.dedup_by_id {
            scan.dedup_by_id = dedup_by_id;
        }
        params.scan = scan;
    }

    Ok((board.clone(), CharucoDetector::new(board, params)))
}

fn format_detect_error(err: CharucoDetectError) -> String {
    match err {
        CharucoDetectError::ChessboardNotDetected => "chessboard not detected".to_string(),
        CharucoDetectError::MeshWarp(e) => format!("mesh warp failed: {e}"),
        CharucoDetectError::NoMarkers => "no markers decoded".to_string(),
        CharucoDetectError::AlignmentFailed { inliers } => {
            format!("marker alignment failed (inliers={inliers})")
        }
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

fn map_detection(
    det: &calib_targets_core::TargetDetection,
    board: Option<&CharucoBoard>,
) -> OutputDetection {
    OutputDetection {
        kind: match det.kind {
            TargetKind::Chessboard => "chessboard",
            TargetKind::Charuco => "charuco",
            TargetKind::CheckerboardMarker => "checkerboard_marker",
        }
        .to_string(),
        corners: det.corners.iter().map(|c| map_corner(c, board)).collect(),
    }
}

fn map_corner(c: &LabeledCorner, board: Option<&CharucoBoard>) -> OutputCorner {
    let object_xy = board
        .and_then(|b| c.id.and_then(|id| b.charuco_object_xy(id)))
        .map(|p| [p.x, p.y]);

    OutputCorner {
        x: c.position.x,
        y: c.position.y,
        grid: c.grid.map(|g| [g.i, g.j]),
        id: c.id,
        confidence: c.confidence,
        object_xy,
    }
}

fn map_markers(board: &CharucoBoard, res: &CharucoDetectionResult) -> Vec<OutputMarker> {
    let mut out = Vec::with_capacity(res.markers.len());

    let inlier_set: std::collections::HashSet<usize> =
        res.alignment.marker_inliers.iter().copied().collect();

    for (idx, m) in res.markers.iter().enumerate() {
        let corners_rect = m.corners_rect.map(|p| [p.x, p.y]);

        let ci = m.sx.max(0) as usize;
        let cj = m.sy.max(0) as usize;
        let corners_img = res
            .rectified
            .cell_corners_img(ci, cj)
            .map(|pts| pts.map(|p| [p.x, p.y]));

        let expected_board_cell = board.marker_position(m.id);
        let mapped_board_cell = res.alignment.map(m.sx, m.sy);

        let inlier = inlier_set.contains(&idx);

        out.push(OutputMarker {
            id: m.id,
            cell: [m.sx, m.sy],
            expected_board_cell,
            mapped_board_cell,
            rotation: m.rotation,
            hamming: m.hamming,
            score: m.score,
            border_score: m.border_score,
            inverted: m.inverted,
            corners_rect,
            corners_img,
            inlier,
        });
    }

    out
}

fn save_mesh_view(
    path: &PathBuf,
    rect: &calib_targets_charuco::RectifiedMeshView,
) -> Result<(), image::ImageError> {
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
    info!("wrote mesh-rectified image to {}", path.display());
    Ok(())
}
