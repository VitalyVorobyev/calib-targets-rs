use std::{collections::HashMap, env, fs, path::PathBuf};

use calib_targets_chessboard::{
    ChessboardDetectionResult, ChessboardDetector, ChessboardParams, GridGraphParams,
};
use calib_targets_core::{Corner as TargetCorner, GrayImageView, GridCoords};
use calib_targets_marker::circle_score::{CircleCandidate, CircleScoreParams};
use calib_targets_marker::detect::{detect_circles_via_square_warp, top_k_by_polarity};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Deserialize, Serialize, Clone)]
struct BoardConfig {
    /// Inner corners (rows/cols) of the checkerboard.
    rows: u32,
    cols: u32,
    /// Expected circle centers in grid **cell** coordinates (i, j).
    circle_positions: [[i32; 2]; 3],
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
struct CircleScoreConfig {
    patch_size: Option<usize>,
    diameter_frac: Option<f32>,
    ring_thickness_frac: Option<f32>,
    ring_radius_mul: Option<f32>,
    min_contrast: Option<f32>,
    samples: Option<usize>,
    center_search_px: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct ExampleConfig {
    image_path: String,
    #[serde(default)]
    output_path: Option<String>,
    board: BoardConfig,
    #[serde(default)]
    chessboard: Option<ChessboardParams>,
    #[serde(default)]
    graph: Option<GridGraphParams>,
    #[serde(default)]
    circle_score: Option<CircleScoreConfig>,
    /// Optional ROI in cell coords to restrict circle search: [i0, j0, i1, j1]
    #[serde(default)]
    roi_cells: Option<[i32; 4]>,
}

#[derive(Debug, Serialize, Clone)]
struct OutputCorner {
    x: f32,
    y: f32,
    grid: Option<[i32; 2]>,
    confidence: f32,
}

#[derive(Debug, Serialize, Clone)]
struct ChessboardOut {
    corners: Vec<OutputCorner>,
    inliers: Vec<usize>,
}

#[derive(Debug, Serialize, Clone)]
struct CircleCandidateOut {
    center_img: [f32; 2],
    center_grid: [f32; 2],
    polarity: String,
    score: f32,
    contrast: f32,
}

#[derive(Debug, Serialize, Clone)]
struct CircleMatchOut {
    expected_cell: [i32; 2],
    matched_index: Option<usize>,
    center_img: Option<[f32; 2]>,
    polarity: Option<String>,
    distance_cells: Option<f32>,
}

#[derive(Debug, Serialize)]
struct ExampleReport {
    image_path: String,
    config_path: String,
    board: BoardConfig,
    num_raw_corners: usize,
    chessboard: Option<ChessboardOut>,
    circle_candidates: Vec<CircleCandidateOut>,
    matches: Vec<CircleMatchOut>,
    all_circles_found: bool,
    error: Option<String>,
}

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
    let config_path = parse_config_path();
    let cfg = load_config(&config_path)?;

    let image_path = PathBuf::from(&cfg.image_path);
    let img = load_image(&image_path)?;

    let chess_cfg = make_chess_config();
    let raw_corners = detect_raw_corners(&img, &chess_cfg);
    info!("raw ChESS corners: {}", raw_corners.len());

    let target_corners = adapt_corners(&raw_corners);

    let mut report = ExampleReport {
        image_path: cfg.image_path.clone(),
        config_path: config_path.to_string_lossy().into_owned(),
        board: cfg.board.clone(),
        num_raw_corners: raw_corners.len(),
        chessboard: None,
        circle_candidates: Vec::new(),
        matches: Vec::new(),
        all_circles_found: false,
        error: None,
    };

    let (chess_params, graph_params) = build_chessboard_params(&cfg);
    info!("chessboard params: {:?}", chess_params);
    info!("grid graph params: {:?}", graph_params);

    let detector = ChessboardDetector::new(chess_params.clone())
        .with_grid_search(graph_params.clone());
    let detection = detect_chessboard(&detector, &target_corners);

    let Some(det_res) = detection else {
        warn!("chessboard not detected");
        log_chessboard_diagnostics(&chess_cfg, &chess_params, &graph_params, &target_corners);
        report.error = Some("chessboard not detected".to_string());
        return write_report(cfg.output_path.as_deref(), report);
    };

    log_chessboard_summary(&det_res);

    let corner_map = build_corner_map(&det_res);

    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    let circle_params = build_circle_params(&cfg);
    info!("circle score params: {:?}", circle_params);
    if let Some(roi_cells) = cfg.roi_cells {
        info!("circle ROI cells: {:?}", roi_cells);
    }

    let roi = cfg
        .roi_cells
        .map(|[i0, j0, i1, j1]| (i0, j0, i1, j1));
    let mut candidates = detect_circles(&src_view, &corner_map, &circle_params, roi);
    info!("circle candidates before filtering: {}", candidates.len());

    // Keep strongest per polarity to reduce noise (3 expected markers)
    let (white, black) = top_k_by_polarity(candidates, 3, 3);
    info!("top candidates: white={}, black={}", white.len(), black.len());
    candidates = [white, black].concat();

    let expected_cells = cfg.board.circle_positions;
    let matches = match_expected_circles(&expected_cells, &candidates);

    report.chessboard = Some(map_chessboard(det_res.detection, det_res.inliers));
    report.circle_candidates = candidates
        .iter()
        .map(|c| CircleCandidateOut {
            center_img: [c.center_img.x, c.center_img.y],
            center_grid: [c.center_grid.0, c.center_grid.1],
            polarity: format!("{:?}", c.polarity).to_lowercase(),
            score: c.score,
            contrast: c.contrast,
        })
        .collect();
    report.all_circles_found = matches.iter().all(|m| m.matched_index.is_some());
    report.matches = matches;

    write_report(cfg.output_path.as_deref(), report)
}

fn parse_config_path() -> PathBuf {
    env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/marker_detect_config.json"))
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip(path)))]
fn load_config(path: &PathBuf) -> Result<ExampleConfig, Box<dyn std::error::Error>> {
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip(image_path)))]
fn load_image(image_path: &PathBuf) -> Result<image::GrayImage, Box<dyn std::error::Error>> {
    Ok(ImageReader::open(image_path)?.decode()?.to_luma8())
}

fn make_chess_config() -> ChessConfig {
    let mut chess_cfg = ChessConfig::single_scale();
    chess_cfg.params.threshold_rel = 0.2;
    chess_cfg.params.nms_radius = 2;
    chess_cfg
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip(img, chess_cfg)))]
fn detect_raw_corners(img: &image::GrayImage, chess_cfg: &ChessConfig) -> Vec<CornerDescriptor> {
    find_chess_corners_image(img, chess_cfg)
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip(raw)))]
fn adapt_corners(raw: &[CornerDescriptor]) -> Vec<TargetCorner> {
    raw.iter().map(adapt_chess_corner).collect()
}

#[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip(cfg)))]
fn build_chessboard_params(cfg: &ExampleConfig) -> (ChessboardParams, GridGraphParams) {
    let mut chess_params = cfg.chessboard.clone().unwrap_or_else(|| ChessboardParams {
        expected_rows: Some(cfg.board.rows),
        expected_cols: Some(cfg.board.cols),
        ..ChessboardParams::default()
    });
    if chess_params.expected_rows.is_none() {
        chess_params.expected_rows = Some(cfg.board.rows);
    }
    if chess_params.expected_cols.is_none() {
        chess_params.expected_cols = Some(cfg.board.cols);
    }
    let graph_params = cfg.graph.clone().unwrap_or_default();
    (chess_params, graph_params)
}

#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "info", skip(detector, corners), fields(num_corners = corners.len()))
)]
fn detect_chessboard(
    detector: &ChessboardDetector,
    corners: &[TargetCorner],
) -> Option<ChessboardDetectionResult> {
    detector.detect_from_corners(corners)
}

fn log_chessboard_diagnostics(
    chess_cfg: &ChessConfig,
    chess_params: &ChessboardParams,
    graph_params: &GridGraphParams,
    corners: &[TargetCorner],
) {
    warn!(
        "chess config: threshold_rel={}, nms_radius={}",
        chess_cfg.params.threshold_rel, chess_cfg.params.nms_radius
    );
    warn!("chessboard params: {:?}", chess_params);
    warn!("grid graph params: {:?}", graph_params);

    if corners.is_empty() {
        warn!("no adapted corners available for diagnostics");
        return;
    }

    let (min_strength, mean_strength, max_strength) = strength_stats(corners);
    let strong_count = corners
        .iter()
        .filter(|c| c.strength >= chess_params.min_corner_strength)
        .count();

    warn!(
        "corner strength stats: min={:.2}, mean={:.2}, max={:.2}",
        min_strength, mean_strength, max_strength
    );
    warn!(
        "strong corners (>= {:.2}): {} / {}",
        chess_params.min_corner_strength,
        strong_count,
        corners.len()
    );
    warn!(
        "min_corners requirement: {} (have {})",
        chess_params.min_corners,
        strong_count
    );

    let (min_x, max_x, min_y, max_y) = bounds_xy(corners);
    warn!(
        "corner bounds: x=[{:.1}, {:.1}], y=[{:.1}, {:.1}]",
        min_x, max_x, min_y, max_y
    );

    if let Some(spacing) = estimate_spacing(corners) {
        warn!("estimated corner spacing: {:.1} px", spacing);
        if spacing < graph_params.min_spacing_pix || spacing > graph_params.max_spacing_pix {
            warn!(
                "spacing outside graph limits: min_spacing_pix={}, max_spacing_pix={}",
                graph_params.min_spacing_pix, graph_params.max_spacing_pix
            );
        }
    }
}

fn log_chessboard_summary(det_res: &ChessboardDetectionResult) {
    info!(
        "chessboard detected: corners={}, inliers={}",
        det_res.detection.corners.len(),
        det_res.inliers.len()
    );

    let grids: Vec<GridCoords> = det_res
        .detection
        .corners
        .iter()
        .filter_map(|c| c.grid)
        .collect();
    if grids.is_empty() {
        warn!("detected chessboard has no grid coords");
        return;
    }

    let (min_i, max_i, min_j, max_j) = bounds_grid(&grids);
    let width = (max_i - min_i + 1).max(0) as usize;
    let height = (max_j - min_j + 1).max(0) as usize;
    let grid_area = width * height;
    let completeness = if grid_area > 0 {
        det_res.detection.corners.len() as f32 / grid_area as f32
    } else {
        0.0
    };

    info!(
        "grid bounds: i=[{}, {}], j=[{}, {}], completeness={:.3}",
        min_i, max_i, min_j, max_j, completeness
    );
}

fn strength_stats(corners: &[TargetCorner]) -> (f32, f32, f32) {
    let mut min_strength = f32::INFINITY;
    let mut max_strength = f32::NEG_INFINITY;
    let mut sum = 0.0f32;
    for c in corners {
        min_strength = min_strength.min(c.strength);
        max_strength = max_strength.max(c.strength);
        sum += c.strength;
    }
    let mean = sum / corners.len() as f32;
    (min_strength, mean, max_strength)
}

fn estimate_spacing(corners: &[TargetCorner]) -> Option<f32> {
    if corners.len() < 2 {
        return None;
    }
    let mut distances = Vec::with_capacity(corners.len());
    for (idx, c) in corners.iter().enumerate() {
        let mut best = f32::INFINITY;
        for (j, other) in corners.iter().enumerate() {
            if idx == j {
                continue;
            }
            let dx = c.position.x - other.position.x;
            let dy = c.position.y - other.position.y;
            let d = (dx * dx + dy * dy).sqrt();
            if d < best {
                best = d;
            }
        }
        if best.is_finite() {
            distances.push(best);
        }
    }

    if distances.is_empty() {
        return None;
    }
    distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(distances[distances.len() / 2])
}

fn bounds_xy(corners: &[TargetCorner]) -> (f32, f32, f32, f32) {
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for c in corners {
        min_x = min_x.min(c.position.x);
        max_x = max_x.max(c.position.x);
        min_y = min_y.min(c.position.y);
        max_y = max_y.max(c.position.y);
    }

    (min_x, max_x, min_y, max_y)
}

fn bounds_grid(grids: &[GridCoords]) -> (i32, i32, i32, i32) {
    let mut min_i = i32::MAX;
    let mut max_i = i32::MIN;
    let mut min_j = i32::MAX;
    let mut max_j = i32::MIN;

    for g in grids {
        min_i = min_i.min(g.i);
        max_i = max_i.max(g.i);
        min_j = min_j.min(g.j);
        max_j = max_j.max(g.j);
    }

    (min_i, max_i, min_j, max_j)
}

fn build_circle_params(cfg: &ExampleConfig) -> CircleScoreParams {
    let mut circle_params = CircleScoreParams::default();
    if let Some(cfg_cs) = &cfg.circle_score {
        if let Some(v) = cfg_cs.patch_size {
            circle_params.patch_size = v;
        }
        if let Some(v) = cfg_cs.diameter_frac {
            circle_params.diameter_frac = v;
        }
        if let Some(v) = cfg_cs.ring_thickness_frac {
            circle_params.ring_thickness_frac = v;
        }
        if let Some(v) = cfg_cs.ring_radius_mul {
            circle_params.ring_radius_mul = v;
        }
        if let Some(v) = cfg_cs.min_contrast {
            circle_params.min_contrast = v;
        }
        if let Some(v) = cfg_cs.samples {
            circle_params.samples = v;
        }
        if let Some(v) = cfg_cs.center_search_px {
            circle_params.center_search_px = v;
        }
    }
    circle_params
}

#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "info", skip(image, corner_map, circle_params, roi))
)]
fn detect_circles(
    image: &GrayImageView<'_>,
    corner_map: &HashMap<GridCoords, Point2<f32>>,
    circle_params: &CircleScoreParams,
    roi: Option<(i32, i32, i32, i32)>,
) -> Vec<CircleCandidate> {
    detect_circles_via_square_warp(image, corner_map, circle_params, roi)
}

fn build_corner_map(det_res: &ChessboardDetectionResult) -> HashMap<GridCoords, Point2<f32>> {
    det_res
        .detection
        .corners
        .iter()
        .filter_map(|c| Some((c.grid?, c.position)))
        .collect()
}

#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "info", skip(expected, candidates), fields(candidates = candidates.len()))
)]
fn match_expected_circles(
    expected: &[[i32; 2]; 3],
    candidates: &[CircleCandidate],
) -> Vec<CircleMatchOut> {
    let mut remaining: Vec<(usize, &CircleCandidate)> = candidates.iter().enumerate().collect();
    let mut out = Vec::new();

    for &cell in expected {
        let target_center = (cell[0] as f32 + 0.5, cell[1] as f32 + 0.5);
        let mut best: Option<(usize, f32)> = None;
        for (idx, cand) in &remaining {
            let dx = cand.center_grid.0 - target_center.0;
            let dy = cand.center_grid.1 - target_center.1;
            let dist = (dx * dx + dy * dy).sqrt();
            if best.map(|b| dist < b.1).unwrap_or(true) {
                best = Some((*idx, dist));
            }
        }

        if let Some((best_idx, dist)) = best {
            remaining.retain(|(idx, _)| *idx != best_idx);
            let cand = &candidates[best_idx];
            out.push(CircleMatchOut {
                expected_cell: cell,
                matched_index: Some(best_idx),
                center_img: Some([cand.center_img.x, cand.center_img.y]),
                polarity: Some(format!("{:?}", cand.polarity).to_lowercase()),
                distance_cells: Some(dist),
            });
        } else {
            out.push(CircleMatchOut {
                expected_cell: cell,
                matched_index: None,
                center_img: None,
                polarity: None,
                distance_cells: None,
            });
        }
    }

    out
}

fn map_chessboard(det: calib_targets_core::TargetDetection, inliers: Vec<usize>) -> ChessboardOut {
    ChessboardOut {
        corners: det
            .corners
            .into_iter()
            .map(|c| OutputCorner {
                x: c.position.x,
                y: c.position.y,
                grid: c.grid.map(|g| [g.i, g.j]),
                confidence: c.confidence,
            })
            .collect(),
        inliers,
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

fn write_report(
    path: Option<&str>,
    report: ExampleReport,
) -> Result<(), Box<dyn std::error::Error>> {
    let out_path = path
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tmpdata/marker_detect_report.json"));
    if let Some(parent) = out_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&out_path, json)?;
    println!("wrote report JSON to {}", out_path.display());
    Ok(())
}
