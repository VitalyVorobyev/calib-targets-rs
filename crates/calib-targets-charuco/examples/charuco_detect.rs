use std::{
    env,
    path::{Path, PathBuf},
};

use calib_targets_aruco::{ArucoScanConfig, MarkerDetection};
use calib_targets_charuco::{
    CharucoBoardSpec, CharucoDetectError, CharucoDetection, CharucoDetector, CharucoParams,
};
use calib_targets_chessboard::ChessCorner as Corner;
use calib_targets_chessboard::ChessboardParams;
use calib_targets_core::io::{self, IoError};
use calib_targets_core::{GrayImageView, GridAlignment, TargetDetection};
use chess_corners::{CornerDescriptor, Detector as ChessDetector, DetectorConfig};
use image::ImageReader;
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

#[cfg(not(feature = "tracing"))]
use log::{debug, info, warn};
#[cfg(feature = "tracing")]
use tracing::{debug, info, warn};

fn default_px_per_square() -> f32 {
    60.0
}

/// Config for this example, loaded from a checked-in JSON fixture
/// (`testdata/charuco_detect_config*.json`). This struct is example-local:
/// the library used to expose a similar type from `calib_targets_charuco`,
/// but example/CLI file-configs are tooling, not part of the crate's public
/// API — see `docs/migrations/0.11.0.md` (API revision S5).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CharucoDetectConfig {
    /// Path to the input image to run detection on.
    image_path: String,
    /// The ChArUco board specification to detect.
    board: CharucoBoardSpec,
    /// Optional path for the detection report.
    #[serde(default)]
    output_path: Option<String>,
    /// Optional path to write a global-homography rectified image.
    #[serde(default)]
    rectified_path: Option<String>,
    /// Optional path to write a per-cell-mesh rectified image.
    #[serde(default)]
    mesh_rectified_path: Option<String>,
    /// Rectified-image resolution in pixels per board square.
    #[serde(default = "default_px_per_square")]
    px_per_square: f32,
    /// Optional override for the minimum marker-inlier count.
    #[serde(default)]
    min_marker_inliers: Option<usize>,
    /// Optional override for the underlying chessboard detector params.
    #[serde(default)]
    chessboard: Option<ChessboardParams>,
    /// Optional ArUco scan-config overrides.
    #[serde(default)]
    aruco: Option<ArucoScanConfig>,
}

impl CharucoDetectConfig {
    /// Load a JSON config from disk.
    fn load_json(path: impl AsRef<Path>) -> Result<Self, IoError> {
        io::load_json(path)
    }

    /// Resolve the output report path.
    fn output_path(&self) -> PathBuf {
        self.output_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("charuco_detect_report.json"))
    }

    /// Build detector parameters, applying overrides from the config.
    fn build_params(&self) -> CharucoParams {
        let mut params = CharucoParams::for_board(self.board);
        params.px_per_square = self.px_per_square;
        if let Some(min_marker_inliers) = self.min_marker_inliers {
            params.min_marker_inliers = min_marker_inliers;
        }
        if let Some(chessboard) = self.chessboard.clone() {
            params.chessboard = chessboard;
        }
        if let Some(aruco) = self.aruco.as_ref() {
            // `ArucoScanConfig.max_hamming` is intentionally not mapped: the
            // board-level matcher (the sole matcher) uses soft-bit scoring and
            // a margin gate, with no Hamming cap. Only the scan overrides apply.
            aruco.apply_to_scan(&mut params.scan);
        }
        params
    }
}

/// Detection report for serialization. Example-local for the same reason as
/// [`CharucoDetectConfig`].
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CharucoDetectReport {
    /// Path of the image detection was run on.
    image_path: String,
    /// Path of the JSON config that produced this report.
    config_path: String,
    /// The board specification detection was run against.
    board: CharucoBoardSpec,
    /// Number of raw ChESS corners detected before board detection.
    num_raw_corners: usize,
    /// The raw ChESS corners detected in the image.
    raw_corners: Vec<Corner>,
    /// The detected ChArUco board, when detection succeeded.
    #[serde(default)]
    detection: Option<TargetDetection>,
    /// The decoded inlier markers, when detection succeeded.
    #[serde(default)]
    markers: Option<Vec<MarkerDetection>>,
    /// Grid alignment to the board, when it could be resolved.
    #[serde(default)]
    alignment: Option<GridAlignment>,
    /// Human-readable error message, when detection failed.
    #[serde(default)]
    error: Option<String>,
}

impl CharucoDetectReport {
    /// Build a base report from the input config and raw corners.
    fn new(cfg: &CharucoDetectConfig, config_path: &Path, raw_corners: Vec<Corner>) -> Self {
        Self {
            image_path: cfg.image_path.clone(),
            config_path: config_path.to_string_lossy().into_owned(),
            board: cfg.board,
            num_raw_corners: raw_corners.len(),
            raw_corners,
            detection: None,
            markers: None,
            alignment: None,
            error: None,
        }
    }

    /// Populate report fields from a successful detection.
    fn set_detection(&mut self, res: CharucoDetection) {
        self.detection = Some(res.target_detection());
        self.markers = Some(res.markers);
        self.alignment = Some(res.alignment);
        self.error = None;
    }

    /// Record a detection error.
    fn set_error(&mut self, err: CharucoDetectError) {
        self.error = Some(err.to_string());
    }

    /// Write this report to disk as pretty JSON.
    fn write_json(&self, path: impl AsRef<Path>) -> Result<(), IoError> {
        io::write_json(self, path)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // No logger/subscriber is installed here: neither `env_logger` nor
    // `tracing-subscriber` is a dependency of this crate's examples, so the
    // `log`/`tracing` macros below are no-ops unless the caller installs
    // their own logger. Detection output does not depend on logging.
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
    let chess_cfg = // chess-corners 1.0 made `threshold` a single absolute floor on the raw
    // ChESS response (relative thresholding is Radon-only now). 15.0 is the
    // workspace production default — see
    // `calib_targets::detect::default_chess_config`. This crate's examples do
    // not depend on the facade, so the value is restated rather than imported.
    DetectorConfig::chess()
        .with_threshold(15.0)
        .with_detection(|d| d.nms_radius = 2);
    debug!(
        "Running ChESS corner scan with threshold={:?}",
        chess_cfg.threshold
    );
    let mut detector = ChessDetector::new(chess_cfg).expect("build ChESS detector");
    detector.detect(img).expect("ChESS detection")
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
    Corner::new(
        Point2::new(c.x, c.y),
        // `axes` is `None` only when the upstream orientation fit is
        // skipped; these fixtures always fit it.
        c.axes
            .map(|a| {
                [
                    calib_targets_core::AxisEstimate {
                        angle: a[0].angle,
                        sigma: a[0].sigma,
                    },
                    calib_targets_core::AxisEstimate {
                        angle: a[1].angle,
                        sigma: a[1].sigma,
                    },
                ]
            })
            .expect("orientation fit enabled"),
        c.response,
    )
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
        cfg.board.dictionary.name(),
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
        "Detector params: px_per_square={:.1}, min_marker_inliers={}",
        params.px_per_square, params.min_marker_inliers
    );
    let chessboard_tuning = params.chessboard.effective_tuning();
    debug!(
        "Chessboard params: min_corner_strength={:.3}, max_axis_sigma_rad={:.3}, cluster_tol_deg={:.1}, attach_search_rel={:.2}, max_components={}",
        params.chessboard.min_corner_strength,
        chessboard_tuning.topological.max_axis_sigma_rad,
        chessboard_tuning.cluster_tol_deg,
        chessboard_tuning.attach_search_rel,
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

fn log_detection_success(res: &calib_targets_charuco::CharucoDetection) {
    info!(
        "Detection succeeded: {} ChArUco corners, {} markers, alignment {:?} + {:?}",
        res.corners.len(),
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
