use std::{env, fs, path::PathBuf, time::Instant};

use calib_targets_aruco::builtins;
use calib_targets_charuco::{
    CharucoBoard, CharucoBoardSpec, CharucoDetectError, CharucoDetectionResult, CharucoDetector,
    CharucoDetectorParams, MarkerLayout,
};
use calib_targets_chessboard::{ChessboardParams, GridGraphParams};
use calib_targets_core::{Corner as TargetCorner, GrayImageView, TargetDetection};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::{ImageBuffer, ImageReader, Luma};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
struct BoardConfig {
    rows: u32,
    cols: u32,
    cell_size: f32,
    marker_size_rel: f32,
    dictionary: String,
    #[serde(default)]
    marker_layout: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct ArucoConfig {
    #[serde(default)]
    max_hamming: Option<u8>,
    #[serde(default)]
    border_bits: Option<usize>,
    #[serde(default)]
    inset_frac: Option<f32>,
    #[serde(default)]
    marker_size_rel: Option<f32>,
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
    rectified_path: Option<String>,
    #[serde(default)]
    mesh_rectified_path: Option<String>,
    #[serde(default = "default_px_per_square")]
    px_per_square: f32,
    #[serde(default)]
    min_marker_inliers: Option<usize>,
    #[serde(default)]
    chessboard: Option<ChessboardParams>,
    #[serde(default)]
    graph: Option<GridGraphParams>,
    #[serde(default)]
    aruco: Option<ArucoConfig>,
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
    grid_cell: [i32; 2],
    center_rect: [f32; 2],
    center_img: Option<[f32; 2]>,
    rotation: u8,
    hamming: u8,
    score: f32,
    border_score: f32,
    inverted: bool,
    corners_rect: [[f32; 2]; 4],
    corners_img: Option<[[f32; 2]; 4]>,
}

#[derive(Debug, Serialize, Clone)]
struct RectifiedOut {
    path: Option<String>,
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
struct RawCornerOut {
    x: f32,
    y: f32,
    strength: f32,
}

#[derive(Debug, Serialize, Clone, Default)]
struct TimingsMs {
    load_image: u64,
    detect_corners: u64,
    adapt_corners: u64,
    detect_charuco: u64,
    total: u64,
}

#[derive(Debug, Serialize)]
struct ExampleReport {
    image_path: String,
    config_path: String,
    board: BoardConfig,
    num_raw_corners: usize,
    raw_corners: Vec<RawCornerOut>,
    chessboard: Option<OutputDetection>,
    charuco: Option<OutputDetection>,
    markers: Option<Vec<OutputMarker>>,
    rectified: Option<RectifiedOut>,
    alignment: Option<AlignmentOut>,
    error: Option<String>,
    timings_ms: TimingsMs,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let mut chess_cfg = ChessConfig::single_scale();
    chess_cfg.params.threshold_rel = 0.2;
    chess_cfg.params.nms_radius = 2;

    let t_corners = Instant::now();
    let raw_corners: Vec<CornerDescriptor> = find_chess_corners_image(&img, &chess_cfg);
    let detect_corners_ms = t_corners.elapsed().as_millis() as u64;

    let t_adapt = Instant::now();
    let target_corners: Vec<TargetCorner> = raw_corners.iter().map(adapt_chess_corner).collect();
    let adapt_corners_ms = t_adapt.elapsed().as_millis() as u64;

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
    }
    if let Some(graph) = cfg.graph.clone() {
        params.graph = graph;
    }
    if let Some(aruco) = cfg.aruco.clone() {
        if let Some(max_hamming) = aruco.max_hamming {
            params.max_hamming = max_hamming;
        }
        if let Some(border_bits) = aruco.border_bits {
            params.scan.border_bits = border_bits;
        }
        if let Some(inset_frac) = aruco.inset_frac {
            params.scan.inset_frac = inset_frac;
        }
        if let Some(marker_size_rel) = aruco.marker_size_rel {
            params.scan.marker_size_rel = marker_size_rel;
        }
        if let Some(min_border_score) = aruco.min_border_score {
            params.scan.min_border_score = min_border_score;
        }
        if let Some(dedup_by_id) = aruco.dedup_by_id {
            params.scan.dedup_by_id = dedup_by_id;
        }
    }

    let detector = CharucoDetector::new(board, params);

    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let mut report = ExampleReport {
        image_path: cfg.image_path.clone(),
        config_path: config_path.to_string_lossy().into_owned(),
        board: cfg.board.clone(),
        num_raw_corners: raw_corners.len(),
        raw_corners: raw_corners
            .iter()
            .map(|c| RawCornerOut {
                x: c.x,
                y: c.y,
                strength: c.response,
            })
            .collect(),
        chessboard: None,
        charuco: None,
        markers: None,
        rectified: None,
        alignment: None,
        error: None,
        timings_ms: TimingsMs {
            load_image: load_image_ms,
            detect_corners: detect_corners_ms,
            adapt_corners: adapt_corners_ms,
            detect_charuco: 0,
            total: 0,
        },
    };

    let t_detect = Instant::now();
    match detector.detect(&src_view, &target_corners) {
        Ok(res) => {
            report.timings_ms.detect_charuco = t_detect.elapsed().as_millis() as u64;
            fill_report_from_detection(&mut report, &cfg, res, &detector);
        }
        Err(err) => {
            report.timings_ms.detect_charuco = t_detect.elapsed().as_millis() as u64;
            report.error = Some(format_detect_error(err));
        }
    }

    report.timings_ms.total = t_total.elapsed().as_millis() as u64;

    let output_path = cfg
        .output_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("charuco_detect_report.json"));

    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&output_path, json)?;
    println!("wrote report JSON to {}", output_path.display());

    Ok(())
}

fn fill_report_from_detection(
    report: &mut ExampleReport,
    cfg: &ExampleConfig,
    res: CharucoDetectionResult,
    detector: &CharucoDetector,
) {
    report.chessboard = Some(map_detection(res.chessboard.clone(), None));
    report.charuco = Some(map_detection(
        res.detection.clone(),
        Some(detector.board()),
    ));

    let rect_path = cfg
        .rectified_path
        .as_ref()
        .or_else(|| cfg.mesh_rectified_path.as_ref())
        .map(PathBuf::from);

    let rectified_out = RectifiedOut {
        path: rect_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
        width: res.rectified.rect.width,
        height: res.rectified.rect.height,
        px_per_square: res.rectified.px_per_square,
        min_i: res.rectified.min_i,
        min_j: res.rectified.min_j,
        cells_x: res.rectified.cells_x,
        cells_y: res.rectified.cells_y,
        valid_cells: res.rectified.valid_cells,
    };

    if let Some(path) = rect_path {
        if let Err(err) = save_rectified(&path, &res.rectified) {
            eprintln!("failed to save rectified image to {}: {err}", path.display());
        }
    }

    let markers = res
        .markers
        .iter()
        .map(|m| map_marker(&res.rectified, m))
        .collect::<Vec<_>>();

    report.rectified = Some(rectified_out);
    report.markers = Some(markers);
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
    report.error = None;
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

fn format_detect_error(err: CharucoDetectError) -> String {
    match err {
        CharucoDetectError::ChessboardNotDetected => "chessboard not detected".to_string(),
        CharucoDetectError::MeshWarp(e) => format!("mesh warp failed: {e}"),
        CharucoDetectError::NoMarkers => "no markers decoded".to_string(),
        CharucoDetectError::AlignmentFailed { inliers } => {
            format!("alignment failed (inliers={inliers})")
        }
    }
}

fn map_detection(det: TargetDetection, board: Option<&CharucoBoard>) -> OutputDetection {
    let corners = det
        .corners
        .into_iter()
        .map(|c| {
            let object_xy = c
                .id
                .and_then(|id| board.and_then(|b| b.charuco_object_xy(id)))
                .map(|p| [p.x, p.y]);
            OutputCorner {
                x: c.position.x,
                y: c.position.y,
                grid: c.grid.map(|g| [g.i, g.j]),
                id: c.id,
                confidence: c.confidence,
                object_xy,
            }
        })
        .collect();

    OutputDetection {
        kind: match det.kind {
            calib_targets_core::TargetKind::Chessboard => "chessboard",
            calib_targets_core::TargetKind::Charuco => "charuco",
            calib_targets_core::TargetKind::CheckerboardMarker => "checkerboard_marker",
        }
        .to_string(),
        corners,
    }
}

fn map_marker(rect: &calib_targets_chessboard::RectifiedMeshView, m: &calib_targets_aruco::MarkerDetection) -> OutputMarker {
    let corners_rect = m.corners_rect.map(|p| [p.x, p.y]);

    let corners_img = if m.sx >= 0 && m.sy >= 0 {
        rect.cell_corners_img(m.sx as usize, m.sy as usize)
            .map(|pts| pts.map(|p| [p.x, p.y]))
    } else {
        None
    };

    let center_rect = [
        0.25 * (corners_rect[0][0] + corners_rect[1][0] + corners_rect[2][0] + corners_rect[3][0]),
        0.25 * (corners_rect[0][1] + corners_rect[1][1] + corners_rect[2][1] + corners_rect[3][1]),
    ];
    let center_img = corners_img.as_ref().map(|c| {
        [
            0.25 * (c[0][0] + c[1][0] + c[2][0] + c[3][0]),
            0.25 * (c[0][1] + c[1][1] + c[2][1] + c[3][1]),
        ]
    });

    OutputMarker {
        id: m.id,
        cell: [m.sx, m.sy],
        grid_cell: [rect.min_i + m.sx, rect.min_j + m.sy],
        center_rect,
        center_img,
        rotation: m.rotation,
        hamming: m.hamming,
        score: m.score,
        border_score: m.border_score,
        inverted: m.inverted,
        corners_rect,
        corners_img,
    }
}

fn save_rectified(path: &PathBuf, rect: &calib_targets_chessboard::RectifiedMeshView) -> Result<(), image::ImageError> {
    let img_buf = ImageBuffer::<Luma<u8>, _>::from_raw(
        rect.rect.width as u32,
        rect.rect.height as u32,
        rect.rect.data.clone(),
    )
    .expect("failed to build rectified output image");

    img_buf.save(path)?;
    println!("wrote rectified image to {}", path.display());
    Ok(())
}
