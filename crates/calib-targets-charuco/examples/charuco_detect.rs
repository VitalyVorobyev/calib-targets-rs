use std::{
    env,
    path::{Path, PathBuf},
};

#[cfg(not(feature = "tracing"))]
use std::str::FromStr;

use calib_targets_charuco::{
    CharucoDetectConfig, CharucoDetectError, CharucoDetectReport, CharucoDetector, CharucoParams,
};
use calib_targets_core::{Corner, GrayImageView};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use nalgebra::Point2;

#[cfg(not(feature = "tracing"))]
use log::{debug, info, warn, LevelFilter};
#[cfg(feature = "tracing")]
use tracing::{debug, info, warn};

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;
#[cfg(not(feature = "tracing"))]
use calib_targets_core::init_with_level;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "tracing"))]
    let log_level = LevelFilter::from_str("debug").unwrap_or(LevelFilter::Info);
    #[cfg(not(feature = "tracing"))]
    init_with_level(log_level)?;
    #[cfg(not(feature = "tracing"))]
    info!("Logger initialized");

    #[cfg(feature = "tracing")]
    init_tracing(false);

    let config_path = parse_config_path();
    info!("Loading ChArUco config from {}", config_path.display());
    let cfg = CharucoDetectConfig::load_json(&config_path)?;
    log_config(&cfg, &config_path);

    let img = load_image(Path::new(&cfg.image_path))?;
    info!("Loaded grayscale image {}x{}", img.width(), img.height());

    let params = cfg.build_params();
    log_detector_params(&params);

    let raw_corners = detect_raw_corners(&img);
    let target_corners = adapt_corners(&raw_corners);
    log_corner_stats(&target_corners, &params);

    let detector = CharucoDetector::new(params.clone())?;
    let src_view = make_view(&img);

    let detect_result = detector.detect(&src_view, &target_corners);

    let mut report = CharucoDetectReport::new(&cfg, &config_path, target_corners);
    match detect_result {
        Ok(res) => {
            log_detection_success(&res);
            report.set_detection(res);
        }
        Err(err) => {
            log_detection_failure(&err);
            report.set_error(err);
        }
    }

    let output_path = cfg.output_path();
    report.write_json(&output_path)?;
    println!("wrote report JSON to {}", output_path.display());

    Ok(())
}

fn parse_config_path() -> PathBuf {
    env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/charuco_detect_config.json"))
}

fn load_image(path: &Path) -> Result<image::GrayImage, Box<dyn std::error::Error>> {
    Ok(ImageReader::open(path)?.decode()?.to_luma8())
}

fn detect_raw_corners(img: &image::GrayImage) -> Vec<CornerDescriptor> {
    let mut chess_cfg = ChessConfig::single_scale();
    chess_cfg.threshold_mode = chess_corners::ThresholdMode::Relative;
    chess_cfg.threshold_value = 0.2;
    chess_cfg.nms_radius = 2;
    debug!(
        "Running ChESS corner scan with threshold={:.3} ({:?}), nms_radius={}",
        chess_cfg.threshold_value, chess_cfg.threshold_mode, chess_cfg.nms_radius
    );
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
        orientation_cluster: None,
        axes: [
            calib_targets_core::AxisEstimate {
                angle: c.axes[0].angle,
                sigma: c.axes[0].sigma,
            },
            calib_targets_core::AxisEstimate {
                angle: c.axes[1].angle,
                sigma: c.axes[1].sigma,
            },
        ],
        contrast: c.contrast,
        fit_rms: c.fit_rms,
        strength: c.response,
    }
}

fn log_config(cfg: &CharucoDetectConfig, config_path: &Path) {
    info!(
        "Config summary: image={} report={} config={}",
        cfg.image_path,
        cfg.output_path().display(),
        config_path.display()
    );
    info!(
        "Board summary: {} cols x {} rows squares, dictionary={}, marker_layout={:?}, marker_size_rel={:.3}",
        cfg.board.cols,
        cfg.board.rows,
        cfg.board.dictionary.name,
        cfg.board.marker_layout,
        cfg.board.marker_size_rel
    );
    debug!(
        "Optional outputs: rectified={} mesh_rectified={}",
        cfg.rectified_path.as_deref().unwrap_or("-"),
        cfg.mesh_rectified_path.as_deref().unwrap_or("-")
    );
}

fn log_detector_params(params: &CharucoParams) {
    debug!(
        "Detector params: px_per_square={:.1}, min_marker_inliers={}, max_hamming={}",
        params.px_per_square, params.min_marker_inliers, params.max_hamming
    );
    debug!(
        "Chessboard params: min_corner_strength={:.3}, max_fit_rms_ratio={:.3}, cluster_tol_deg={:.1}, seed_edge_tol={:.2}, attach_search_rel={:.2}, max_components={}",
        params.chessboard.min_corner_strength,
        params.chessboard.max_fit_rms_ratio,
        params.chessboard.cluster_tol_deg,
        params.chessboard.seed_edge_tol,
        params.chessboard.attach_search_rel,
        params.chessboard.max_components,
    );
    debug!(
        "Marker scan params: marker_size_rel={:.3}, inset_frac={:.3}, border_bits={}, min_border_score={:.3}, dedup_by_id={}",
        params.scan.marker_size_rel,
        params.scan.inset_frac,
        params.scan.border_bits,
        params.scan.min_border_score,
        params.scan.dedup_by_id
    );
}

fn log_corner_stats(corners: &[Corner], _params: &CharucoParams) {
    if corners.is_empty() {
        warn!("ChESS scan returned no raw corners");
        return;
    }

    let mut xs = Vec::with_capacity(corners.len());
    let mut ys = Vec::with_capacity(corners.len());
    let mut strengths = Vec::with_capacity(corners.len());
    let mut nearest_neighbor = Vec::with_capacity(corners.len());

    for (idx, corner) in corners.iter().enumerate() {
        xs.push(corner.position.x);
        ys.push(corner.position.y);
        strengths.push(corner.strength);

        let mut best_distance = f32::INFINITY;
        for (other_idx, other) in corners.iter().enumerate() {
            if idx == other_idx {
                continue;
            }
            let distance = (other.position - corner.position).norm();
            best_distance = best_distance.min(distance);
        }
        if best_distance.is_finite() {
            nearest_neighbor.push(best_distance);
        }
    }

    strengths.sort_by(|a, b| a.total_cmp(b));
    nearest_neighbor.sort_by(|a, b| a.total_cmp(b));

    debug!(
        "Raw corner stats: count={}, bbox=({:.1}, {:.1})..({:.1}, {:.1}), strength[min/p50/p90/max]={:.2}/{:.2}/{:.2}/{:.2}",
        corners.len(),
        xs.iter().copied().fold(f32::INFINITY, f32::min),
        ys.iter().copied().fold(f32::INFINITY, f32::min),
        xs.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        ys.iter().copied().fold(f32::NEG_INFINITY, f32::max),
        percentile(&strengths, 0.0),
        percentile(&strengths, 0.5),
        percentile(&strengths, 0.9),
        percentile(&strengths, 1.0)
    );
    debug!(
        "Nearest-neighbor distances[min/p10/p50/p90/max]={:.1}/{:.1}/{:.1}/{:.1}/{:.1} (chessboard discovers cell size from the seed; no fixed spacing window)",
        percentile(&nearest_neighbor, 0.0),
        percentile(&nearest_neighbor, 0.1),
        percentile(&nearest_neighbor, 0.5),
        percentile(&nearest_neighbor, 0.9),
        percentile(&nearest_neighbor, 1.0),
    );
}

fn log_detection_success(res: &calib_targets_charuco::CharucoDetectionResult) {
    info!(
        "Detection succeeded: {} ChArUco corners, {} markers, alignment {:?} + {:?}",
        res.detection.corners.len(),
        res.markers.len(),
        res.alignment.transform,
        res.alignment.translation
    );
}

fn log_detection_failure(err: &CharucoDetectError) {
    warn!("Detection failed: {err}");
    if matches!(err, CharucoDetectError::ChessboardNotDetected) {
        warn!("ChArUco detection stopped before marker decoding because the chessboard stage produced no board candidate");
    }
}

fn percentile(sorted: &[f32], q: f32) -> f32 {
    if sorted.is_empty() {
        return f32::NAN;
    }

    let q = q.clamp(0.0, 1.0);
    let idx = ((sorted.len() - 1) as f32 * q).round() as usize;
    sorted[idx]
}
