use calib_targets::detect;
use calib_targets::puzzleboard::{PuzzleBoardDetection, PuzzleBoardParams, PuzzleBoardSpec};
use image::ImageReader;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Config for this example, loaded from a checked-in JSON fixture
/// (`testdata/puzzleboard_detect_config.json`). This struct is
/// example-local: the library used to expose a similar type from
/// `calib_targets_puzzleboard`, but example/CLI file-configs are tooling,
/// not part of the crate's public API — see `docs/migrations/0.11.0.md`
/// (API revision S5).
#[derive(Clone, Debug, Deserialize)]
struct PuzzleBoardDetectConfig {
    /// Path to the input image to run detection on.
    image_path: PathBuf,
    /// Optional path for the detection report.
    #[serde(default)]
    output_path: Option<PathBuf>,
    /// PuzzleBoard detector parameters. The ChESS corner front-end is
    /// configured through this struct's own `chess` field, like every other
    /// detector knob.
    detector: PuzzleBoardParams,
}

impl PuzzleBoardDetectConfig {
    /// Deserialise from any `Read` source.
    fn from_reader(r: impl std::io::Read) -> Result<Self, serde_json::Error> {
        serde_json::from_reader(r)
    }
}

/// End-to-end report for one detection run. Example-local for the same
/// reason as [`PuzzleBoardDetectConfig`].
#[derive(Clone, Debug, Serialize)]
struct PuzzleBoardDetectReport {
    /// Path of the image detection was run on.
    image_path: PathBuf,
    /// The detection result.
    result: PuzzleBoardDetection,
}

impl PuzzleBoardDetectReport {
    /// Build a report pairing an input image path with its detection result.
    fn new(image_path: impl Into<PathBuf>, result: PuzzleBoardDetection) -> Self {
        Self {
            image_path: image_path.into(),
            result,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing_subscriber();

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_puzzleboard <image_path|config.json>");
        return Ok(());
    };

    if path.ends_with(".json") {
        return run_config(Path::new(&path));
    }

    // Default 12×12 PuzzleBoard anchored at master origin (0, 0).
    let spec = PuzzleBoardSpec::new(12, 12, 1.0)?;
    let params = PuzzleBoardParams::for_board(spec);
    run_image(Path::new(&path), &params)?;

    Ok(())
}

fn run_config(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::fs::File::open(path)?;
    let cfg = PuzzleBoardDetectConfig::from_reader(file)?;
    let result = run_image(&cfg.image_path, &cfg.detector)?;
    if let Some(output_path) = cfg.output_path {
        let report = PuzzleBoardDetectReport::new(cfg.image_path, result);
        let file = std::fs::File::create(output_path)?;
        serde_json::to_writer_pretty(file, &report)?;
    }

    Ok(())
}

fn run_image(
    path: &Path,
    params: &PuzzleBoardParams,
) -> Result<calib_targets::puzzleboard::PuzzleBoardDetection, Box<dyn std::error::Error>> {
    let img = ImageReader::open(path)?.decode()?.to_luma8();
    let result = detect::detect_puzzleboard(&img, params)?;
    report(&result);
    Ok(result)
}

fn report(result: &calib_targets::puzzleboard::PuzzleBoardDetection) {
    println!(
        "detected {} labelled corners (mean confidence = {:.3}, bit-error rate = {:.3})",
        result.corners.len(),
        result.decode.mean_confidence,
        result.decode.bit_error_rate,
    );
    println!(
        "master origin for local (0, 0): ({}, {})",
        result.decode.master_origin_row, result.decode.master_origin_col,
    );
}

/// Install a minimal `tracing` subscriber reading `RUST_LOG` (default
/// `info`). Examples no longer depend on the removed
/// `calib_targets_core::init_tracing` helper.
#[cfg(feature = "tracing")]
fn init_tracing_subscriber() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}
