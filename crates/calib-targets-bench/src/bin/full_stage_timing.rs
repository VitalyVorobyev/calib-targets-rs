//! Full-detector per-stage timing for the PUBLIC performance report.
//!
//! Unlike `topo_stage_timing` (which times only the chessboard grid builder on
//! a directory of images), this binary runs the **complete** detector for a
//! fixed set of four PUBLIC `testdata/` images and reports the three
//! report-facing stages — `corner_detection`, `grid_build`, and `decode` —
//! plus the counts the report cards render (raw ChESS corners, labelled
//! corners, and marker count for ChArUco cards).
//!
//! The four images are hard-coded on purpose: the output feeds the committed
//! `.github/pages/performance/data.json`, which is published, so every input
//! must stay public. Two cards are ChArUco (`small.png`, `large.png`), one is a
//! plain chessboard (`mid.png`), and one is a PuzzleBoard (`example2.png` — a
//! heavily radially-distorted board whose edge-dot pattern decodes against the
//! 501×501 master since PR #61's distortion-aware sampling closed Gap 18).
//!
//! ## Stage decomposition
//!
//! Corners are detected once with the same ChESS configuration the regression
//! tests use, and the detected corners are passed into the detectors (so the
//! detector wall time never re-runs corner detection):
//!
//! - `corner_detection` — ChESS corner detection (`detect_corners`).
//! - `grid_build` — the chessboard grid build. For ChArUco this is *exactly*
//!   the `ChessDetector::new(params.chessboard).detect_all(corners)` call the
//!   ChArUco pipeline runs internally before decoding, so the isolated
//!   measurement is representative.
//! - `decode` — for ChArUco, marker sampling + decode + board alignment; for the
//!   PuzzleBoard, the edge-dot sample + 501×501 master decode across the full
//!   `detect_puzzleboard_best` sweep (every config, as a user pays for it).
//!   Derived as `full_detect − grid_build`. `null` for plain chessboard cards
//!   (no decode stage).
//!
//! Honours `REPEATS` / `WARMUP` env vars (set by `scripts/gen-perf-data.sh`).

use std::error::Error;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use calib_targets::aruco::builtins;
use calib_targets::charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
use calib_targets::chessboard::{
    ChessCorner, Detector as ChessboardDetector, DetectorParams as ChessboardParams,
};
use calib_targets::core::{DetectorConfig, GrayImageView};
use calib_targets::detect::detect_corners;
use calib_targets::puzzleboard::{
    PuzzleBoardDetectionResult, PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec,
};
use chess_corners::Threshold;
use clap::Parser;
use image::ImageReader;
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(
    name = "full_stage_timing",
    about = "Full-detector per-stage timing for the four public performance-report images"
)]
struct Args {
    /// Output JSON report path.
    #[arg(long)]
    out: PathBuf,
    /// Timed repeats per image (env `REPEATS` overrides the default).
    #[arg(long, env = "REPEATS", default_value_t = 30)]
    repeats: usize,
    /// Warmup repeats per image (env `WARMUP` overrides the default).
    #[arg(long, env = "WARMUP", default_value_t = 5)]
    warmup: usize,
}

/// The detector a card exercises. Chessboard cards have no decode stage.
enum Kind {
    /// Plain chessboard: `min_corner_strength` is the marker-free floor.
    Chessboard { min_corner_strength: f32 },
    /// ChArUco: a full board spec + the detector knobs the regression mirrors.
    Charuco(CharucoSpec),
    /// PuzzleBoard: self-identifying chessboard decoded against the 501×501
    /// master via the edge-dot pattern. Timed through the full multi-config
    /// sweep (`detect_puzzleboard_best`) — the decode bar is the whole sweep,
    /// not a single config.
    Puzzleboard(PuzzleboardSpec),
}

/// The PuzzleBoard spec for one card. `PuzzleBoardSpec::new(rows, cols, cell)`.
struct PuzzleboardSpec {
    rows: u32,
    cols: u32,
    cell_size: f32,
}

/// The ChArUco board + detector parameters for one card, mirroring the
/// corresponding `calib-targets-charuco` regression test exactly.
struct CharucoSpec {
    rows: u32,
    cols: u32,
    cell_size: f32,
    marker_size_rel: f32,
    dictionary: &'static str,
    px_per_square: f32,
    min_marker_inliers: usize,
}

/// One report card: a public image plus the detector that produces it.
struct Card {
    /// `testdata/` path, relative to the workspace root.
    file: &'static str,
    kind: Kind,
}

/// The four public report images. Hard-coded — the output is published.
fn cards() -> Vec<Card> {
    vec![
        // small.png — ChArUco (DICT_4X4_250). Mirrors
        // `detects_charuco_on_small_png` / `board_matcher_detects_small_png`.
        Card {
            file: "testdata/small.png",
            kind: Kind::Charuco(CharucoSpec {
                rows: 22,
                cols: 22,
                cell_size: 5.2,
                marker_size_rel: 0.75,
                dictionary: "DICT_4X4_250",
                px_per_square: 60.0,
                min_marker_inliers: 12,
            }),
        },
        // mid.png — plain chessboard (no markers). Mirrors
        // `detects_plain_chessboard_on_mid_png` (min_corner_strength = 0.5).
        Card {
            file: "testdata/mid.png",
            kind: Kind::Chessboard {
                min_corner_strength: 0.5,
            },
        },
        // large.png — ChArUco (DICT_4X4_1000). Mirrors
        // `board_matcher_detects_large_png` / `testdata/charuco_detect_config.json`.
        Card {
            file: "testdata/large.png",
            kind: Kind::Charuco(CharucoSpec {
                rows: 22,
                cols: 22,
                cell_size: 1.0,
                marker_size_rel: 0.75,
                dictionary: "DICT_4X4_1000",
                px_per_square: 60.0,
                min_marker_inliers: 64,
            }),
        },
        // example2.png — a heavily radially-distorted PuzzleBoard. The grid
        // detects cleanly and, since PR #61's distortion-aware edge sampling
        // (Gap 18 resolved), the edge-dot pattern now decodes against the
        // 501×501 master, so it is rendered as a full PuzzleBoard card. Spec
        // mirrors the `interop_authors` author set: `PuzzleBoardSpec::new(20,
        // 20, 5.0)`.
        Card {
            file: "testdata/example2.png",
            kind: Kind::Puzzleboard(PuzzleboardSpec {
                rows: 20,
                cols: 20,
                cell_size: 5.0,
            }),
        },
    ]
}

/// The ChESS configuration the regression tests use: a relative response
/// threshold and a tight NMS radius. Reused for every card so the bench's raw
/// corner counts match the regression suite's.
fn chess_config() -> DetectorConfig {
    DetectorConfig::chess()
        .with_threshold(Threshold::Relative(0.2))
        .with_chess(|c| c.nms_radius = 2)
}

#[derive(Clone, Copy, Debug, Default, Serialize)]
struct Stat {
    p50_ms: f64,
    mean_ms: f64,
}

fn p50(mut values: Vec<f64>) -> Stat {
    if values.is_empty() {
        return Stat::default();
    }
    let mean_ms = values.iter().sum::<f64>() / values.len() as f64;
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((values.len() - 1) as f64 * 0.5).round() as usize;
    Stat {
        p50_ms: values[idx.min(values.len() - 1)],
        mean_ms,
    }
}

#[derive(Debug, Serialize)]
struct ImageReport {
    image: String,
    kind: &'static str,
    width: u32,
    height: u32,
    raw_corners: usize,
    labelled: usize,
    /// Decoded marker count (ChArUco only); `None` for chessboard cards.
    markers: Option<usize>,
    corner_detection: Stat,
    grid_build: Stat,
    /// Marker decode (ChArUco only); `None` for chessboard cards.
    decode: Option<Stat>,
}

#[derive(Debug, Serialize)]
struct Metadata {
    git_sha: Option<String>,
    rustc: Option<String>,
    cpu: Option<String>,
    profile: &'static str,
    repeats: usize,
    warmup: usize,
    timing_source: &'static str,
}

#[derive(Debug, Serialize)]
struct Report {
    metadata: Metadata,
    images: Vec<ImageReport>,
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn cpu_name() -> Option<String> {
    command_output("sysctl", &["-n", "machdep.cpu.brand_string"]).or_else(|| {
        command_output(
            "sh",
            &["-c", "lscpu | sed -n 's/^Model name:[[:space:]]*//p'"],
        )
    })
}

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// Labelled-corner count of the best (largest) chessboard component, or 0.
fn best_component_corners(detections: &[calib_targets::chessboard::ChessboardDetection]) -> usize {
    detections
        .iter()
        .map(|d| d.corners.len())
        .max()
        .unwrap_or(0)
}

fn measure_chessboard(
    corners: &[ChessCorner],
    min_corner_strength: f32,
    repeats: usize,
    warmup: usize,
) -> Result<(usize, Stat), Box<dyn Error>> {
    let mut params = ChessboardParams::default();
    params.min_corner_strength = min_corner_strength;
    let detector = ChessboardDetector::new(params)?;

    for _ in 0..warmup {
        let _ = detector.detect_all(corners);
    }
    let mut grid = Vec::with_capacity(repeats);
    let mut labelled = 0;
    for _ in 0..repeats {
        let start = Instant::now();
        let detections = detector.detect_all(corners);
        grid.push(elapsed_ms(start));
        labelled = best_component_corners(&detections);
    }
    Ok((labelled, p50(grid)))
}

struct CharucoMeasurement {
    labelled: usize,
    markers: usize,
    grid_build: Stat,
    decode: Stat,
}

fn measure_charuco(
    view: &GrayImageView<'_>,
    corners: &[ChessCorner],
    spec: &CharucoSpec,
    repeats: usize,
    warmup: usize,
) -> Result<CharucoMeasurement, Box<dyn Error>> {
    let dict = builtins::builtin_dictionary(spec.dictionary)
        .ok_or_else(|| format!("unknown builtin dictionary {}", spec.dictionary))?;
    let board = CharucoBoardSpec::new(
        spec.rows,
        spec.cols,
        spec.cell_size,
        spec.marker_size_rel,
        dict,
    )
    .with_marker_layout(MarkerLayout::OpenCvCharuco);

    let mut params = CharucoParams::for_board(&board);
    params.px_per_square = spec.px_per_square;
    params.min_marker_inliers = spec.min_marker_inliers;

    // `grid_build` measures exactly the grid stage the ChArUco pipeline runs
    // internally: `ChessDetector::new(params.chessboard).detect_all(corners)`.
    let chess_detector = ChessboardDetector::new(params.chessboard.clone())?;
    let detector = CharucoDetector::new(params)?;

    for _ in 0..warmup {
        let _ = chess_detector.detect_all(corners);
        let _ = detector.detect(view, corners);
    }

    let mut grid = Vec::with_capacity(repeats);
    let mut full = Vec::with_capacity(repeats);
    let mut grid_labelled = 0;
    let mut labelled = 0;
    let mut markers = 0;
    for _ in 0..repeats {
        let g_start = Instant::now();
        let detections = chess_detector.detect_all(corners);
        grid.push(elapsed_ms(g_start));
        grid_labelled = best_component_corners(&detections);

        let f_start = Instant::now();
        let res = detector.detect(view, corners);
        full.push(elapsed_ms(f_start));
        if let Ok(res) = res {
            labelled = res.corners.len();
            markers = res.markers.len();
        }
    }

    let grid_build = p50(grid);
    let full_stat = p50(full);
    // decode = full_detect − grid_build (corner detection is not inside
    // `detect`; corners are precomputed and passed in). Clamp at 0 to guard
    // against measurement noise in the rare case grid ≳ full.
    let decode = Stat {
        p50_ms: (full_stat.p50_ms - grid_build.p50_ms).max(0.0),
        mean_ms: (full_stat.mean_ms - grid_build.mean_ms).max(0.0),
    };

    // The labelled count reported on a ChArUco card is the marker-ID'd corner
    // set the detector returns. Fall back to the grid component if the decode
    // produced none (should not happen for the two configured boards).
    if labelled == 0 {
        labelled = grid_labelled;
    }

    Ok(CharucoMeasurement {
        labelled,
        markers,
        grid_build,
        decode,
    })
}

struct PuzzleboardMeasurement {
    labelled: usize,
    bit_error_rate: f32,
    grid_build: Stat,
    decode: Stat,
}

fn measure_puzzleboard(
    view: &GrayImageView<'_>,
    corners: &[ChessCorner],
    spec: &PuzzleboardSpec,
    repeats: usize,
    warmup: usize,
) -> Result<PuzzleboardMeasurement, Box<dyn Error>> {
    let board = PuzzleBoardSpec::new(spec.rows, spec.cols, spec.cell_size)?;
    let sweep = PuzzleBoardParams::sweep_for_board(&board);
    let detectors = sweep
        .iter()
        .cloned()
        .map(PuzzleBoardDetector::new)
        .collect::<Result<Vec<_>, _>>()?;

    // `grid_build` measures one chessboard grid pass — the per-config grid stage
    // the sweep repeats internally — with the first config's chessboard params.
    let chess_detector = ChessboardDetector::new(sweep[0].chessboard.clone())?;

    for _ in 0..warmup {
        let _ = chess_detector.detect_all(corners);
        let _ = best_sweep_decode(&detectors, view, corners);
    }

    let mut grid = Vec::with_capacity(repeats);
    let mut full = Vec::with_capacity(repeats);
    let mut labelled = 0;
    let mut bit_error_rate = 1.0f32;
    let mut decoded = false;
    for _ in 0..repeats {
        let g_start = Instant::now();
        let _ = chess_detector.detect_all(corners);
        grid.push(elapsed_ms(g_start));

        // Time the full multi-config sweep exactly as `detect_puzzleboard_best`
        // runs it: every config in order, keeping the best decode. On frames
        // that need the 40% pass the earlier configs are real work, so this is
        // the honest end-to-end PuzzleBoard latency, not just the winner.
        let f_start = Instant::now();
        let best = best_sweep_decode(&detectors, view, corners);
        full.push(elapsed_ms(f_start));
        if let Some(res) = best {
            decoded = true;
            labelled = res.corners.len();
            bit_error_rate = res.decode.bit_error_rate;
        }
    }

    if !decoded {
        return Err(format!(
            "{}x{} PuzzleBoard: no sweep config decoded — refusing to publish a \
             grid-only card as a PuzzleBoard (Gap 18 regression?)",
            spec.rows, spec.cols
        )
        .into());
    }

    let grid_build = p50(grid);
    let full_stat = p50(full);
    // decode = full_sweep − one grid_build. The sweep's per-config grid builds
    // and every config's edge-dot decode land in this bar; the card note flags
    // it as the full multi-config sweep cost.
    let decode = Stat {
        p50_ms: (full_stat.p50_ms - grid_build.p50_ms).max(0.0),
        mean_ms: (full_stat.mean_ms - grid_build.mean_ms).max(0.0),
    };

    Ok(PuzzleboardMeasurement {
        labelled,
        bit_error_rate,
        grid_build,
        decode,
    })
}

/// Run every sweep config (corners precomputed) and return the best decode,
/// matching `detect_puzzleboard_best`'s ranking (most corners, then mean
/// confidence). `None` if no config decodes.
fn best_sweep_decode(
    detectors: &[PuzzleBoardDetector],
    view: &GrayImageView<'_>,
    corners: &[ChessCorner],
) -> Option<PuzzleBoardDetectionResult> {
    let mut best: Option<PuzzleBoardDetectionResult> = None;
    for detector in detectors {
        if let Ok(r) = detector.detect(view, corners) {
            let better = best.as_ref().is_none_or(|b| {
                (r.corners.len(), r.decode.mean_confidence)
                    > (b.corners.len(), b.decode.mean_confidence)
            });
            if better {
                best = Some(r);
            }
        }
    }
    best
}

fn measure_card(
    card: &Card,
    chess_cfg: &DetectorConfig,
    repeats: usize,
    warmup: usize,
) -> Result<ImageReport, Box<dyn Error>> {
    let img = ImageReader::open(card.file)?.decode()?.to_luma8();
    let view = GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    };

    // ---- corner detection (shared by both kinds) ----
    for _ in 0..warmup {
        let _ = detect_corners(&img, chess_cfg);
    }
    let mut cd = Vec::with_capacity(repeats);
    let mut corners: Vec<ChessCorner> = Vec::new();
    for _ in 0..repeats {
        let start = Instant::now();
        corners = detect_corners(&img, chess_cfg);
        cd.push(elapsed_ms(start));
    }
    let raw_corners = corners.len();
    let corner_detection = p50(cd);

    match &card.kind {
        Kind::Chessboard {
            min_corner_strength,
        } => {
            let (labelled, grid_build) =
                measure_chessboard(&corners, *min_corner_strength, repeats, warmup)?;
            Ok(ImageReport {
                image: card.file.to_owned(),
                kind: "Chessboard",
                width: img.width(),
                height: img.height(),
                raw_corners,
                labelled,
                markers: None,
                corner_detection,
                grid_build,
                decode: None,
            })
        }
        Kind::Charuco(spec) => {
            let m = measure_charuco(&view, &corners, spec, repeats, warmup)?;
            Ok(ImageReport {
                image: card.file.to_owned(),
                kind: "ChArUco",
                width: img.width(),
                height: img.height(),
                raw_corners,
                labelled: m.labelled,
                markers: Some(m.markers),
                corner_detection,
                grid_build: m.grid_build,
                decode: Some(m.decode),
            })
        }
        Kind::Puzzleboard(spec) => {
            let m = measure_puzzleboard(&view, &corners, spec, repeats, warmup)?;
            // Surface the decode quality on stderr (not part of the published
            // JSON schema) so a regenerate can confirm the board still decodes.
            eprintln!(
                "  {} → PuzzleBoard decode: {} corners, BER {:.3}",
                card.file, m.labelled, m.bit_error_rate
            );
            Ok(ImageReport {
                image: card.file.to_owned(),
                kind: "PuzzleBoard",
                width: img.width(),
                height: img.height(),
                raw_corners,
                labelled: m.labelled,
                markers: None,
                corner_detection,
                grid_build: m.grid_build,
                decode: Some(m.decode),
            })
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let chess_cfg = chess_config();

    let mut images = Vec::new();
    for card in cards() {
        eprintln!("measuring {} ...", card.file);
        images.push(measure_card(&card, &chess_cfg, args.repeats, args.warmup)?);
    }

    let report = Report {
        metadata: Metadata {
            git_sha: command_output("git", &["rev-parse", "--short", "HEAD"]),
            rustc: command_output("rustc", &["--version"]),
            cpu: cpu_name(),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
            repeats: args.repeats,
            warmup: args.warmup,
            timing_source: "wall_clock",
        },
        images,
    };

    if let Some(parent) = args.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&args.out, serde_json::to_string_pretty(&report)?)?;
    println!("wrote {}", args.out.display());
    Ok(())
}
