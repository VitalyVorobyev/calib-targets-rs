use std::{
    env,
    path::{Path, PathBuf},
    time::Instant
};

#[cfg(not(feature = "tracing"))]
use std::str::FromStr;

use calib_targets_charuco::{CharucoDetectConfig, CharucoDetectReport, TimingsMs};
use calib_targets_core::{Corner, GrayImageView};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use image::ImageReader;
use nalgebra::Point2;

#[cfg(not(feature = "tracing"))]
use log::{info, LevelFilter};

#[cfg(feature = "tracing")]
use calib_targets_core::init_tracing;
#[cfg(not(feature = "tracing"))]
use calib_targets_core::init_with_level;


fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(feature = "tracing"))]
    let log_level = LevelFilter::from_str("info").unwrap_or(LevelFilter::Info);
    #[cfg(not(feature = "tracing"))]
    init_with_level(log_level)?;
    #[cfg(not(feature = "tracing"))]
    info!("Logger initialized");

    #[cfg(feature = "tracing")]
    init_tracing(false);

    let config_path = parse_config_path();
    let cfg = CharucoDetectConfig::load_json(&config_path)?;
    let t_total = Instant::now();

    let (img, load_image_ms) = timed_result(|| load_image(Path::new(&cfg.image_path)))?;
    let (raw_corners, detect_corners_ms) = timed_value(|| detect_raw_corners(&img));
    let (target_corners, adapt_corners_ms) = timed_value(|| adapt_corners(&raw_corners));

    let detector = cfg.build_detector()?;
    let src_view = make_view(&img);

    let (detect_result, detect_charuco_ms) =
        timed_value(|| detector.detect(&src_view, &target_corners));

    let timings = TimingsMs {
        load_image: load_image_ms,
        detect_corners: detect_corners_ms,
        adapt_corners: adapt_corners_ms,
        detect_charuco: detect_charuco_ms,
        total: 0,
    };

    let mut report = CharucoDetectReport::new(&cfg, &config_path, target_corners, timings);
    match detect_result {
        Ok(res) => {
            report.set_detection(res);
        }
        Err(err) => report.set_error(err),
    }

    report.timings_ms.total = t_total.elapsed().as_millis() as u64;

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
    chess_cfg.params.threshold_rel = 0.2;
    chess_cfg.params.nms_radius = 2;
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

fn timed_result<T, E, F: FnOnce() -> Result<T, E>>(f: F) -> Result<(T, u64), E> {
    let start = Instant::now();
    let value = f()?;
    let elapsed = start.elapsed().as_millis() as u64;
    Ok((value, elapsed))
}

fn timed_value<T, F: FnOnce() -> T>(f: F) -> (T, u64) {
    let start = Instant::now();
    let value = f();
    let elapsed = start.elapsed().as_millis() as u64;
    (value, elapsed)
}

fn adapt_chess_corner(c: &CornerDescriptor) -> Corner {
    Corner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
        strength: c.response,
    }
}
