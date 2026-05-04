//! Regression gates for the synthetic topological-grid recovery set.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use calib_targets::detect::{default_chess_config, detect_corners, ChessConfig};
use calib_targets_chessboard::{
    trace_topological, Detection, Detector, DetectorParams, GraphBuildAlgorithm,
};
use image::imageops::FilterType;
use image::{GrayImage, ImageReader};
use serde::Deserialize;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn manifest_path() -> PathBuf {
    workspace_root().join("testdata/02-topo-grid/regression_manifest.json")
}

#[derive(Debug, Deserialize)]
struct Manifest {
    images: Vec<ImageCase>,
}

#[derive(Debug, Deserialize)]
struct ImageCase {
    path: String,
    #[serde(default)]
    topological: Option<Gate>,
    #[serde(default)]
    chessboard_v2: Option<Gate>,
    #[serde(default)]
    low_res: Option<LowResGate>,
    #[serde(default)]
    diagnostic_topological: Option<DiagnosticGate>,
}

#[derive(Debug, Deserialize)]
struct Gate {
    #[serde(default)]
    diagnostic_only: bool,
    #[serde(default)]
    min_labelled: usize,
    #[serde(default)]
    max_holes: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct LowResGate {
    algorithm: String,
    #[serde(default = "default_upscale")]
    upscale: f32,
    chess: LowResChessConfig,
    min_labelled: usize,
    #[serde(default)]
    max_holes: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct LowResChessConfig {
    #[serde(default)]
    pre_blur_sigma_px: f32,
    #[serde(default = "default_threshold")]
    threshold_value: f32,
}

#[derive(Debug, Deserialize)]
struct DiagnosticGate {
    min_labeled_corners: usize,
    #[serde(default)]
    axis_align_tol_deg: Option<f32>,
    #[serde(default)]
    diagonal_angle_tol_deg: Option<f32>,
    min_trace_components: usize,
    min_total_labelled: usize,
}

fn default_upscale() -> f32 {
    1.0
}

fn default_threshold() -> f32 {
    0.2
}

fn load_manifest() -> Manifest {
    let path = manifest_path();
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn load_image(path: &Path) -> GrayImage {
    ImageReader::open(path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()))
        .decode()
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
        .to_luma8()
}

fn maybe_upscale(img: &GrayImage, scale: f32) -> GrayImage {
    if (scale - 1.0).abs() < f32::EPSILON {
        return img.clone();
    }
    assert!(scale.is_finite() && scale > 0.0, "invalid upscale {scale}");
    let width = ((img.width() as f32) * scale).round().max(1.0) as u32;
    let height = ((img.height() as f32) * scale).round().max(1.0) as u32;
    image::imageops::resize(img, width, height, FilterType::Triangle)
}

fn params_for(algorithm: GraphBuildAlgorithm) -> DetectorParams {
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = algorithm;
    params
}

fn run_detector(
    img: &GrayImage,
    chess_cfg: &ChessConfig,
    algorithm: GraphBuildAlgorithm,
) -> Option<Detection> {
    let corners = detect_corners(img, chess_cfg);
    Detector::new(params_for(algorithm)).detect(&corners)
}

fn label_stats(detection: &Detection, context: &str) -> (usize, usize) {
    let mut labelled: HashMap<(i32, i32), (f32, f32)> = HashMap::new();
    for corner in &detection.target.corners {
        assert!(
            corner.position.x.is_finite() && corner.position.y.is_finite(),
            "{context}: non-finite corner position"
        );
        let grid = corner
            .grid
            .expect("chessboard detections carry grid coords");
        assert!(
            labelled
                .insert((grid.i, grid.j), (corner.position.x, corner.position.y))
                .is_none(),
            "{context}: duplicate grid label ({}, {})",
            grid.i,
            grid.j
        );
    }

    let min_i = labelled.keys().map(|(i, _)| *i).min().unwrap_or(0);
    let min_j = labelled.keys().map(|(_, j)| *j).min().unwrap_or(0);
    let max_i = labelled.keys().map(|(i, _)| *i).max().unwrap_or(0);
    let max_j = labelled.keys().map(|(_, j)| *j).max().unwrap_or(0);
    assert_eq!((min_i, min_j), (0, 0), "{context}: labels not rebased");

    if labelled.len() >= 4 {
        let mut sum_dxi = 0.0f64;
        let mut sum_dyj = 0.0f64;
        let mut n_i = 0usize;
        let mut n_j = 0usize;
        for (&(i, j), &(x, y)) in &labelled {
            if let Some(&(xn, _)) = labelled.get(&(i + 1, j)) {
                sum_dxi += (xn - x) as f64;
                n_i += 1;
            }
            if let Some(&(_, yn)) = labelled.get(&(i, j + 1)) {
                sum_dyj += (yn - y) as f64;
                n_j += 1;
            }
        }
        if n_i > 0 && n_j > 0 {
            assert!(
                sum_dxi / n_i as f64 > 0.0,
                "{context}: +i does not point right"
            );
            assert!(
                sum_dyj / n_j as f64 > 0.0,
                "{context}: +j does not point down"
            );
        }
    }

    let coords: HashSet<(i32, i32)> = labelled.keys().copied().collect();
    let mut holes = 0usize;
    for j in min_j..=max_j {
        for i in min_i..=max_i {
            if !coords.contains(&(i, j)) {
                holes += 1;
            }
        }
    }
    (labelled.len(), holes)
}

fn assert_gate(case: &ImageCase, name: &str, gate: &Gate, detection: Option<Detection>) {
    if gate.diagnostic_only {
        return;
    }
    let context = format!("{} {name}", case.path);
    let detection = detection.unwrap_or_else(|| panic!("{context}: detector returned None"));
    let (labelled, holes) = label_stats(&detection, &context);
    assert!(
        labelled >= gate.min_labelled,
        "{context}: labelled={labelled} < {}",
        gate.min_labelled
    );
    if let Some(max_holes) = gate.max_holes {
        assert!(holes <= max_holes, "{context}: holes={holes} > {max_holes}");
    }
}

fn algorithm_from_name(name: &str) -> GraphBuildAlgorithm {
    match name {
        "topological" => GraphBuildAlgorithm::Topological,
        "chessboard_v2" => GraphBuildAlgorithm::ChessboardV2,
        other => panic!("unknown graph_build_algorithm {other:?}"),
    }
}

#[test]
fn topo_grid_manifest_gates_hold() {
    let manifest = load_manifest();
    let root = workspace_root();
    assert!(!manifest.images.is_empty(), "manifest has no images");

    for case in &manifest.images {
        let path = root.join(&case.path);
        let img = load_image(&path);
        let default_cfg = default_chess_config();
        let default_corners = detect_corners(&img, &default_cfg);

        if let Some(gate) = &case.topological {
            let detection = Detector::new(params_for(GraphBuildAlgorithm::Topological))
                .detect(&default_corners);
            assert_gate(case, "topological", gate, detection);
        }
        if let Some(gate) = &case.chessboard_v2 {
            let detection = Detector::new(params_for(GraphBuildAlgorithm::ChessboardV2))
                .detect(&default_corners);
            assert_gate(case, "chessboard_v2", gate, detection);
        }
        if let Some(gate) = &case.low_res {
            let fed = maybe_upscale(&img, gate.upscale);
            let mut cfg = default_chess_config();
            cfg.threshold_value = gate.chess.threshold_value;
            cfg.pre_blur_sigma_px = gate.chess.pre_blur_sigma_px;
            let detection = run_detector(&fed, &cfg, algorithm_from_name(&gate.algorithm));
            let context = format!("{} low_res", case.path);
            let detection =
                detection.unwrap_or_else(|| panic!("{context}: detector returned None"));
            let (labelled, holes) = label_stats(&detection, &context);
            assert!(
                labelled >= gate.min_labelled,
                "{context}: labelled={labelled} < {}",
                gate.min_labelled
            );
            if let Some(max_holes) = gate.max_holes {
                assert!(holes <= max_holes, "{context}: holes={holes} > {max_holes}");
            }
        }
        if let Some(gate) = &case.diagnostic_topological {
            let mut params = params_for(GraphBuildAlgorithm::Topological);
            params.min_labeled_corners = gate.min_labeled_corners;
            if let Some(deg) = gate.axis_align_tol_deg {
                params.topological.axis_align_tol_rad = deg.to_radians();
            }
            if let Some(deg) = gate.diagonal_angle_tol_deg {
                params.topological.diagonal_angle_tol_rad = deg.to_radians();
            }
            let trace = trace_topological(&default_corners, &params)
                .unwrap_or_else(|e| panic!("{} diagnostic trace: {e}", case.path));
            let total_labelled: usize = trace.components.iter().map(|c| c.labels.len()).sum();
            assert!(
                trace.components.len() >= gate.min_trace_components,
                "{} diagnostic trace components={} < {}",
                case.path,
                trace.components.len(),
                gate.min_trace_components
            );
            assert!(
                total_labelled >= gate.min_total_labelled,
                "{} diagnostic trace labelled={total_labelled} < {}",
                case.path,
                gate.min_total_labelled
            );
        }
    }
}
