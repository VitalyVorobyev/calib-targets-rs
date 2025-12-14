use std::{collections::HashMap, env, fs, path::PathBuf, time::Instant};

use calib_targets_chessboard::{ChessboardDetector, ChessboardParams, GridGraphParams};
use calib_targets_core::{Corner as TargetCorner, GrayImageView, GridCoords};
use calib_targets_marker::circle_score::{CircleCandidate, CircleScoreParams};
use calib_targets_marker::detect::{detect_circles_via_square_warp, top_k_by_polarity};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Serialize, Clone, Default)]
struct TimingsMs {
    load_image: u64,
    detect_corners: u64,
    adapt_corners: u64,
    chessboard_detect: u64,
    circle_detect: u64,
    total: u64,
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
    timings_ms: TimingsMs,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let config_path = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("synthetic/marker_detect_config.json"));

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
        timings_ms: TimingsMs {
            load_image: load_image_ms,
            detect_corners: detect_corners_ms,
            adapt_corners: adapt_corners_ms,
            ..TimingsMs::default()
        },
    };

    // Prepare chessboard detector params
    let mut chess_params = cfg.chessboard.unwrap_or_else(|| ChessboardParams {
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
    let graph_params = cfg.graph.unwrap_or_default();

    // Run chessboard detector
    let detector = ChessboardDetector::new(chess_params).with_grid_search(graph_params);
    let t_chess = Instant::now();
    let detection = detector.detect_from_corners(&target_corners);
    report.timings_ms.chessboard_detect = t_chess.elapsed().as_millis() as u64;

    let Some(det_res) = detection else {
        report.error = Some("chessboard not detected".to_string());
        return write_report(cfg.output_path.as_deref(), report);
    };

    // Build corner map for circle search
    let corner_map: HashMap<GridCoords, Point2<f32>> = det_res
        .detection
        .corners
        .iter()
        .filter_map(|c| Some((c.grid?, c.position)))
        .collect();

    let src_view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

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

    let roi = cfg.roi_cells.map(|[i0, j0, i1, j1]| (i0, j0, i1, j1));
    let t_circles = Instant::now();
    let mut candidates = detect_circles_via_square_warp(&src_view, &corner_map, &circle_params, roi);
    report.timings_ms.circle_detect = t_circles.elapsed().as_millis() as u64;

    // Keep strongest per polarity to reduce noise (3 expected markers)
    let (white, black) = top_k_by_polarity(candidates, 3, 3);
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
    report.timings_ms.total = t_total.elapsed().as_millis() as u64;
    report.all_circles_found = matches.iter().all(|m| m.matched_index.is_some());
    report.matches = matches;

    write_report(cfg.output_path.as_deref(), report)
}

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
        phase: c.phase,
    }
}

fn write_report(path: Option<&str>, report: ExampleReport) -> Result<(), Box<dyn std::error::Error>> {
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
