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
//! plain chessboard (`mid.png`), and one is a PuzzleBoard
//! (`author_like_oblique.png` — a public photo-realistic synthetic render from
//! the canonical 501×501 maps that decodes uniquely against them, so the card
//! exercises a real corner → grid → decode path).
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
//!   PuzzleBoard, the edge-dot sample + 501×501 master sweep + alignment from a
//!   single representative `PuzzleBoardParams::for_board` config (the same
//!   default soft full-master decode the `synthetic_decode` bench times — one
//!   config, not the multi-config `detect_puzzleboard_best` sweep, so corners
//!   stay reused). Derived as `full_detect − grid_build`. `null` for plain
//!   chessboard cards (no decode stage).
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
use calib_targets::puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use calib_targets_bench::baseline::{BaselineCorner, BaselineImage};
use calib_targets_bench::overlay::{render_report_overlay_on_gray, MarkerQuad};
use chess_corners::Threshold;
use clap::Parser;
use image::{GrayImage, ImageReader};
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
    /// If set, write a detection overlay PNG per card into this directory
    /// (`<basename>.png`). Used to refresh the committed report previews under
    /// `.github/pages/performance/img/`.
    #[arg(long)]
    overlay_dir: Option<PathBuf>,
}

/// The detector a card exercises. Chessboard cards have no decode stage.
enum Kind {
    /// Plain chessboard: `min_corner_strength` is the marker-free floor.
    Chessboard { min_corner_strength: f32 },
    /// ChArUco: a full board spec + the detector knobs the regression mirrors.
    Charuco(CharucoSpec),
    /// PuzzleBoard: board geometry for a single representative decode config.
    PuzzleBoard(PuzzleBoardSpecCard),
}

/// The PuzzleBoard board geometry for one card. The detector decodes against
/// the full 501×501 master regardless of the declared origin (the default soft
/// scorer is origin-independent); the origin is carried only so the spec
/// validates and matches the fixture's manifest.
struct PuzzleBoardSpecCard {
    rows: u32,
    cols: u32,
    cell_size: f32,
    origin_row: u32,
    origin_col: u32,
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
    /// Downscale factor for the published overlay preview (`1.0` = full size).
    /// `large.png` ships at half size to keep the committed asset small; the
    /// detector still runs on the full-resolution image.
    preview_scale: f32,
}

/// The four public report images. Hard-coded — the output is published.
fn cards() -> Vec<Card> {
    vec![
        // small.png — ChArUco (DICT_4X4_250). Mirrors
        // `detects_charuco_on_small_png` / `board_matcher_detects_small_png`.
        Card {
            file: "testdata/small.png",
            preview_scale: 1.0,
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
            preview_scale: 1.0,
            kind: Kind::Chessboard {
                min_corner_strength: 0.5,
            },
        },
        // large.png — ChArUco (DICT_4X4_1000). Mirrors
        // `board_matcher_detects_large_png` / `testdata/charuco_detect_config.json`.
        Card {
            file: "testdata/large.png",
            // 3 MP frame — ship a half-size overlay preview (the detector still
            // runs on the full-resolution image).
            preview_scale: 0.5,
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
        // author_like_oblique.png — a PUBLIC photo-realistic synthetic
        // PuzzleBoard. It is a deterministic render from the canonical 501×501
        // maps (perspective + radial distortion + blur/noise/vignette/JPEG), so
        // unlike the author example photos it decodes uniquely against the
        // current master. This card therefore exercises the full PuzzleBoard
        // pipeline — corner detection, chessboard grid build, and a real
        // edge-dot 501² master decode + alignment — with a non-empty decode
        // stage. The 20×20/origin (18,219) geometry mirrors the fixture's
        // manifest and the `synthetic_author_like` / `synthetic_decode`
        // regressions.
        Card {
            file: "testdata/puzzleboard_synthetic_author_like/author_like_oblique.png",
            preview_scale: 1.0,
            kind: Kind::PuzzleBoard(PuzzleBoardSpecCard {
                rows: 20,
                cols: 20,
                cell_size: 5.0,
                origin_row: 18,
                origin_col: 219,
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

/// Captured detection for the per-card overlay PNG: the base image, the
/// labelled grid corners, the decoded marker quads (ChArUco), and the preview
/// downscale factor.
struct OverlayData {
    base: GrayImage,
    baseline: BaselineImage,
    markers: Vec<MarkerQuad>,
    preview_scale: f32,
}

/// Labelled-corner count of the best (largest) chessboard component, or 0.
fn best_component_corners(detections: &[calib_targets::chessboard::ChessboardDetection]) -> usize {
    detections
        .iter()
        .map(|d| d.corners.len())
        .max()
        .unwrap_or(0)
}

/// Build a [`BaselineImage`] from the best (largest) chessboard component, for
/// the overlay. Mirrors `runner::run_pipeline_engine`'s corner mapping
/// (`grid.u → i`, `grid.v → j`).
fn baseline_from_chessboard(
    detections: &[calib_targets::chessboard::ChessboardDetection],
) -> BaselineImage {
    let best = detections.iter().max_by_key(|d| d.corners.len());
    let corners: Vec<BaselineCorner> = best
        .map(|d| {
            d.corners
                .iter()
                .map(|lc| BaselineCorner {
                    i: lc.grid.u,
                    j: lc.grid.v,
                    x: lc.position.x,
                    y: lc.position.y,
                    id: None,
                    score: lc.score,
                })
                .collect()
        })
        .unwrap_or_default();
    BaselineImage {
        labelled_count: corners.len(),
        cell_size_px: best.and_then(|d| d.cell_size).unwrap_or(0.0),
        corners,
    }
}

fn measure_chessboard(
    corners: &[ChessCorner],
    min_corner_strength: f32,
    repeats: usize,
    warmup: usize,
) -> Result<(usize, Stat, BaselineImage), Box<dyn Error>> {
    let mut params = ChessboardParams::default();
    params.min_corner_strength = min_corner_strength;
    let detector = ChessboardDetector::new(params)?;

    for _ in 0..warmup {
        let _ = detector.detect_all(corners);
    }
    let mut grid = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        let start = Instant::now();
        let _ = detector.detect_all(corners);
        grid.push(elapsed_ms(start));
    }
    // One more (untimed) detection to capture the best component for the overlay.
    let baseline = baseline_from_chessboard(&detector.detect_all(corners));
    Ok((baseline.labelled_count, p50(grid), baseline))
}

struct CharucoMeasurement {
    labelled: usize,
    markers: usize,
    grid_build: Stat,
    decode: Stat,
    /// Captured for the overlay: labelled corners + decoded marker quads.
    baseline: BaselineImage,
    marker_quads: Vec<MarkerQuad>,
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

    // One more (untimed) detection to capture corners + marker quads for the
    // overlay (TL, TR, BR, BL order, in input-image pixels).
    let (baseline, marker_quads) = match detector.detect(view, corners) {
        Ok(res) => {
            let corners_bl: Vec<BaselineCorner> = res
                .corners
                .iter()
                .map(|c| BaselineCorner {
                    i: c.grid.u,
                    j: c.grid.v,
                    x: c.position.x,
                    y: c.position.y,
                    id: Some(c.id),
                    score: c.score,
                })
                .collect();
            let quads: Vec<MarkerQuad> = res
                .markers
                .iter()
                .filter_map(|m| {
                    m.corners_img.map(|q| {
                        [
                            (q[0].x, q[0].y),
                            (q[1].x, q[1].y),
                            (q[2].x, q[2].y),
                            (q[3].x, q[3].y),
                        ]
                    })
                })
                .collect();
            (
                BaselineImage {
                    labelled_count: corners_bl.len(),
                    cell_size_px: 0.0,
                    corners: corners_bl,
                },
                quads,
            )
        }
        Err(_) => (
            BaselineImage {
                labelled_count: 0,
                cell_size_px: 0.0,
                corners: Vec::new(),
            },
            Vec::new(),
        ),
    };

    Ok(CharucoMeasurement {
        labelled,
        markers,
        grid_build,
        decode,
        baseline,
        marker_quads,
    })
}

struct PuzzleBoardMeasurement {
    labelled: usize,
    grid_build: Stat,
    decode: Stat,
    /// Captured for the overlay: labelled corners in absolute master coords.
    baseline: BaselineImage,
}

fn measure_puzzleboard(
    view: &GrayImageView<'_>,
    corners: &[ChessCorner],
    spec: &PuzzleBoardSpecCard,
    repeats: usize,
    warmup: usize,
) -> Result<PuzzleBoardMeasurement, Box<dyn Error>> {
    let board = PuzzleBoardSpec::with_origin(
        spec.rows,
        spec.cols,
        spec.cell_size,
        spec.origin_row,
        spec.origin_col,
    )?;
    // A single representative config (default soft full-master decode), matching
    // the `synthetic_decode` bench — not the multi-config `detect_puzzleboard_best`
    // sweep, so corner detection stays out of the timed loop.
    let params = PuzzleBoardParams::for_board(&board);

    // `grid_build` measures exactly the grid stage the PuzzleBoard pipeline runs
    // internally: `ChessDetector::new(params.chessboard).detect_all(corners)`.
    let chess_detector = ChessboardDetector::new(params.chessboard.clone())?;
    let detector = PuzzleBoardDetector::new(params)?;

    for _ in 0..warmup {
        let _ = chess_detector.detect_all(corners);
        let _ = detector.detect(view, corners);
    }

    let mut grid = Vec::with_capacity(repeats);
    let mut full = Vec::with_capacity(repeats);
    let mut labelled = 0;
    for _ in 0..repeats {
        let g_start = Instant::now();
        let _ = chess_detector.detect_all(corners);
        grid.push(elapsed_ms(g_start));

        let f_start = Instant::now();
        let res = detector.detect(view, corners);
        full.push(elapsed_ms(f_start));
        if let Ok(res) = res {
            labelled = res.corners.len();
        }
    }

    let grid_build = p50(grid);
    let full_stat = p50(full);
    // decode = full_detect − grid_build (corners are precomputed and passed in).
    // Clamp at 0 to guard against measurement noise when grid ≳ full.
    let decode = Stat {
        p50_ms: (full_stat.p50_ms - grid_build.p50_ms).max(0.0),
        mean_ms: (full_stat.mean_ms - grid_build.mean_ms).max(0.0),
    };

    // One more (untimed) detection to capture the decoded corners for the
    // overlay (absolute master coords; `grid.u → i`, `grid.v → j`).
    let baseline = match detector.detect(view, corners) {
        Ok(res) => {
            let corners_bl: Vec<BaselineCorner> = res
                .corners
                .iter()
                .map(|c| BaselineCorner {
                    i: c.grid.u,
                    j: c.grid.v,
                    x: c.position.x,
                    y: c.position.y,
                    id: Some(c.id),
                    score: c.score,
                })
                .collect();
            BaselineImage {
                labelled_count: corners_bl.len(),
                cell_size_px: 0.0,
                corners: corners_bl,
            }
        }
        Err(_) => BaselineImage {
            labelled_count: 0,
            cell_size_px: 0.0,
            corners: Vec::new(),
        },
    };

    Ok(PuzzleBoardMeasurement {
        labelled,
        grid_build,
        decode,
        baseline,
    })
}

fn measure_card(
    card: &Card,
    chess_cfg: &DetectorConfig,
    repeats: usize,
    warmup: usize,
) -> Result<(ImageReport, OverlayData), Box<dyn Error>> {
    let img = ImageReader::open(card.file)?.decode()?.to_luma8();

    // `view` borrows `img`; confine it to this block so `img` can move into the
    // returned `OverlayData` afterwards.
    let (report, baseline, markers) = {
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
                let (labelled, grid_build, baseline) =
                    measure_chessboard(&corners, *min_corner_strength, repeats, warmup)?;
                let report = ImageReport {
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
                };
                (report, baseline, Vec::new())
            }
            Kind::Charuco(spec) => {
                let m = measure_charuco(&view, &corners, spec, repeats, warmup)?;
                let report = ImageReport {
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
                };
                (report, m.baseline, m.marker_quads)
            }
            Kind::PuzzleBoard(spec) => {
                let m = measure_puzzleboard(&view, &corners, spec, repeats, warmup)?;
                let report = ImageReport {
                    image: card.file.to_owned(),
                    kind: "PuzzleBoard",
                    width: img.width(),
                    height: img.height(),
                    raw_corners,
                    labelled: m.labelled,
                    // No marker concept; decode quality (BER/edges) lives in the
                    // data.json editorial note, not a fake marker count.
                    markers: None,
                    corner_detection,
                    grid_build: m.grid_build,
                    decode: Some(m.decode),
                };
                (report, m.baseline, Vec::new())
            }
        }
    };

    let overlay = OverlayData {
        base: img,
        baseline,
        markers,
        preview_scale: card.preview_scale,
    };
    Ok((report, overlay))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let chess_cfg = chess_config();

    let mut images = Vec::new();
    for card in cards() {
        eprintln!("measuring {} ...", card.file);
        let (report, overlay) = measure_card(&card, &chess_cfg, args.repeats, args.warmup)?;

        // Refresh the committed report preview as a detection overlay (corners +
        // grid + decoded marker quads), downscaled per the card's preview_scale.
        if let Some(dir) = &args.overlay_dir {
            let stem = std::path::Path::new(&report.image)
                .file_name()
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("overlay.png"));
            let out = dir.join(stem);
            render_report_overlay_on_gray(
                &overlay.base,
                Some(&overlay.baseline),
                &overlay.markers,
                overlay.preview_scale,
                &out,
            )?;
            eprintln!("  overlay -> {}", out.display());
        }

        images.push(report);
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
