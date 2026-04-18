//! Focused chessboard graph probe for one 3536119669 snap.
//!
//! This is intentionally diagnostic: it sweeps graph-building parameters on a
//! single 720x540 snap and prints the best partial-grid candidates.

use std::env;
use std::path::{Path, PathBuf};

use calib_targets::chessboard::{
    score_frame, ChessboardDetector, ChessboardParams, GridGraphParams,
};
use calib_targets::detect::{
    detect_corners, ChessConfig, DescriptorMode, DetectorMode, ThresholdMode, UpscaleConfig,
};
use image::{GenericImageView, GrayImage};

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;
const EXPECTED_ROWS: u32 = 21;
const EXPECTED_COLS: u32 = 21;

#[derive(Clone)]
struct ProbeChessConfig {
    name: &'static str,
    chess: ChessConfig,
}

#[derive(Clone, Debug)]
struct Candidate {
    config_name: &'static str,
    raw_corners: usize,
    use_orientation_clustering: bool,
    min_spacing_pix: f32,
    max_spacing_pix: f32,
    k_neighbors: usize,
    orientation_tolerance_deg: f32,
    corner_count: usize,
    extent_i: u32,
    extent_j: u32,
    residual_median_px: Option<f32>,
    residual_p95_px: Option<f32>,
}

impl Candidate {
    fn is_visible_subset(&self) -> bool {
        self.corner_count >= 30 && self.extent_i >= 6 && self.extent_j >= 4
    }

    fn p95_sort_key(&self) -> f32 {
        self.residual_p95_px.unwrap_or(f32::INFINITY)
    }

    fn median_sort_key(&self) -> f32 {
        self.residual_median_px.unwrap_or(f32::INFINITY)
    }
}

fn testdata_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/3536119669")
}

fn target_path(target_index: u32) -> PathBuf {
    testdata_dir().join(format!("target_{target_index}.png"))
}

fn load_snap(target_index: u32, snap_index: u32) -> GrayImage {
    let path = target_path(target_index);
    let full = image::open(&path)
        .unwrap_or_else(|err| panic!("failed to open {}: {err}", path.display()))
        .to_luma8();
    let x = snap_index * SNAP_WIDTH;
    full.view(x, 0, SNAP_WIDTH, SNAP_HEIGHT).to_image()
}

fn relative_chess_config(threshold: f32) -> ChessConfig {
    ChessConfig {
        threshold_mode: ThresholdMode::Relative,
        threshold_value: threshold,
        nms_radius: 2,
        ..ChessConfig::single_scale()
    }
}

fn chess_configs() -> Vec<ProbeChessConfig> {
    let mut configs = Vec::new();
    for (name, threshold) in [
        ("native_rel020", 0.20),
        ("native_rel015", 0.15),
        ("native_rel010", 0.10),
        ("native_rel008", 0.08),
        ("native_rel006", 0.06),
    ] {
        configs.push(ProbeChessConfig {
            name,
            chess: relative_chess_config(threshold),
        });
    }

    for (name, threshold) in [
        ("up2_rel020", 0.20),
        ("up2_rel015", 0.15),
        ("up2_rel010", 0.10),
        ("up2_rel008", 0.08),
        ("up2_rel006", 0.06),
    ] {
        let mut chess = relative_chess_config(threshold);
        chess.upscale = UpscaleConfig::fixed(2);
        configs.push(ProbeChessConfig { name, chess });
    }

    let mut broad = relative_chess_config(0.20);
    broad.upscale = UpscaleConfig::fixed(2);
    broad.detector_mode = DetectorMode::Broad;
    broad.descriptor_mode = DescriptorMode::FollowDetector;
    configs.push(ProbeChessConfig {
        name: "up2_broad_rel020",
        chess: broad,
    });

    let mut up3 = relative_chess_config(0.20);
    up3.upscale = UpscaleConfig::fixed(3);
    configs.push(ProbeChessConfig {
        name: "up3_rel020",
        chess: up3,
    });

    configs
}

fn parse_arg(args: &[String], flag: &str, default: u32) -> u32 {
    args.windows(2)
        .find_map(|w| (w[0] == flag).then(|| w[1].parse().ok()).flatten())
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let target_index = parse_arg(&args, "--target", 17);
    let snap_index = parse_arg(&args, "--snap", 0);
    let limit = parse_arg(&args, "--limit", 40) as usize;
    let img = load_snap(target_index, snap_index);

    let mut candidates = Vec::new();
    for config in chess_configs() {
        let corners = detect_corners(&img, &config.chess);
        let raw_corners = corners.len();

        for use_orientation_clustering in [true, false] {
            for min_spacing_pix in [4.0, 6.0, 8.0, 10.0, 12.0] {
                for max_spacing_pix in [24.0, 32.0, 40.0, 50.0, 60.0, 72.0, 90.0] {
                    if max_spacing_pix <= min_spacing_pix {
                        continue;
                    }
                    for k_neighbors in [4, 6, 8, 12, 16, 24, 32] {
                        for orientation_tolerance_deg in [10.0, 15.0, 20.0, 22.5, 25.0, 30.0] {
                            let params = ChessboardParams {
                                expected_rows: None,
                                expected_cols: None,
                                min_corners: 4,
                                use_orientation_clustering,
                                graph: GridGraphParams {
                                    min_spacing_pix,
                                    max_spacing_pix,
                                    k_neighbors,
                                    orientation_tolerance_deg,
                                    ..GridGraphParams::default()
                                },
                                ..ChessboardParams::default()
                            };
                            let detector = ChessboardDetector::new(params);
                            for result in detector.detect_all_from_corners(&corners) {
                                let metrics =
                                    score_frame(&result.detection, EXPECTED_ROWS, EXPECTED_COLS);
                                candidates.push(Candidate {
                                    config_name: config.name,
                                    raw_corners,
                                    use_orientation_clustering,
                                    min_spacing_pix,
                                    max_spacing_pix,
                                    k_neighbors,
                                    orientation_tolerance_deg,
                                    corner_count: metrics.corner_count,
                                    extent_i: metrics.extent_i,
                                    extent_j: metrics.extent_j,
                                    residual_median_px: metrics.residual_median_px,
                                    residual_p95_px: metrics.residual_p95_px,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    candidates.sort_by(|a, b| {
        b.is_visible_subset()
            .cmp(&a.is_visible_subset())
            .then_with(|| a.p95_sort_key().total_cmp(&b.p95_sort_key()))
            .then_with(|| a.median_sort_key().total_cmp(&b.median_sort_key()))
            .then_with(|| b.corner_count.cmp(&a.corner_count))
    });

    println!(
        "target={target_index} snap={snap_index} candidates={}",
        candidates.len()
    );
    println!("rank\tconfig\traw\tcluster\tmin\tmax\tk\ttol\tcorners\textent\tmed\tp95");
    for (rank, c) in candidates
        .iter()
        .filter(|c| c.is_visible_subset())
        .take(limit)
        .enumerate()
    {
        println!(
            "{}\t{}\t{}\t{}\t{:.1}\t{:.1}\t{}\t{:.1}\t{}\t{}x{}\t{}\t{}",
            rank + 1,
            c.config_name,
            c.raw_corners,
            c.use_orientation_clustering,
            c.min_spacing_pix,
            c.max_spacing_pix,
            c.k_neighbors,
            c.orientation_tolerance_deg,
            c.corner_count,
            c.extent_i,
            c.extent_j,
            c.residual_median_px
                .map(|v| format!("{v:.3}"))
                .unwrap_or_else(|| "-".to_string()),
            c.residual_p95_px
                .map(|v| format!("{v:.3}"))
                .unwrap_or_else(|| "-".to_string()),
        );
    }
}
