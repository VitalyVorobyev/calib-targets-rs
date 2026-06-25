//! Criterion bench: PuzzleBoard decode on PUBLIC photo-realistic synthetic
//! fixtures rendered from the canonical 501×501 maps.
//!
//! Public counterpart to the `dataset_decode` bench (which needs the private
//! regression set). The fixtures under
//! `testdata/puzzleboard_synthetic_author_like/` are deterministic renders
//! from `map_a.bin` / `map_b.bin`, warped through perspective + radial
//! distortion + camera effects (blur, vignette, noise, JPEG). They exercise the
//! full corner → chessboard → decode path on realistic photos that decode
//! against the *current canonical map* — without any private data. The upstream
//! author example photos cannot serve here: they were rendered from a different
//! 501×501 map and do not localize (see
//! `crates/calib-targets-puzzleboard/docs/SYNTHETIC_AUTHOR_LIKE.md`).
//!
//! Same three groups as `dataset_decode`, so the two are directly comparable:
//!
//!   - `puzzleboard/synthetic/corners`    — ChESS corner detection only.
//!   - `puzzleboard/synthetic/chessboard` — chessboard graph build on cached corners.
//!   - `puzzleboard/synthetic/decode`     — `PuzzleBoardDetector::detect` on cached
//!     corners (default `Full` search: the 501² × 8 master sweep).
//!
//! Run with:
//! ```text
//! cargo bench -p calib-targets-puzzleboard --bench synthetic_decode
//! ```

use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners, gray_view};
use calib_targets_chessboard::ChessCorner as Corner;
use calib_targets_chessboard::{Detector as ChessDetector, DetectorParams};
use calib_targets_puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use image::GrayImage;
use serde::Deserialize;

/// Square size in board units; the synthetic fixtures use 5.0 (matches the
/// `synthetic_author_like` regression test).
const BOARD_CELL: f32 = 5.0;

#[derive(Deserialize)]
struct Manifest {
    scenarios: Vec<Scenario>,
}

#[derive(Deserialize)]
struct Scenario {
    name: String,
    image: String,
    rows: u32,
    cols: u32,
    origin_row: u32,
    origin_col: u32,
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/puzzleboard_synthetic_author_like")
}

fn load_manifest() -> Option<Manifest> {
    let text = std::fs::read_to_string(fixture_dir().join("manifest.json")).ok()?;
    serde_json::from_str(&text).ok()
}

fn load_image(name: &str) -> Option<GrayImage> {
    Some(image::open(fixture_dir().join(name)).ok()?.to_luma8())
}

fn bench_all(c: &mut Criterion) {
    let cfg = default_chess_config();
    let chess_params = DetectorParams::default();

    let Some(manifest) = load_manifest() else {
        eprintln!(
            "synthetic_decode: manifest not found — skipping (dir={:?}).",
            fixture_dir()
        );
        return;
    };

    // Pre-load each fixture: image, cached corners, and a per-board params set.
    let fixtures: Vec<(String, GrayImage, Vec<Corner>, PuzzleBoardParams)> = manifest
        .scenarios
        .iter()
        .filter_map(|s| {
            let img = load_image(&s.image)?;
            let corners = detect_corners(&img, &cfg);
            let spec = PuzzleBoardSpec::with_origin(
                s.rows,
                s.cols,
                BOARD_CELL,
                s.origin_row,
                s.origin_col,
            )
            .ok()?;
            let params = PuzzleBoardParams::for_board(&spec);
            Some((s.name.clone(), img, corners, params))
        })
        .collect();

    if fixtures.is_empty() {
        eprintln!("synthetic_decode: no fixtures loaded — skipping.");
        return;
    }

    let mut corners_group = c.benchmark_group("puzzleboard/synthetic/corners");
    for (label, img, _, _) in &fixtures {
        corners_group.bench_with_input(BenchmarkId::from_parameter(label), img, |b, img| {
            b.iter(|| {
                let out = detect_corners(img, &cfg);
                criterion::black_box(out)
            });
        });
    }
    corners_group.finish();

    let mut chess_group = c.benchmark_group("puzzleboard/synthetic/chessboard");
    for (label, _, corners, _) in &fixtures {
        chess_group.bench_with_input(BenchmarkId::from_parameter(label), corners, |b, corners| {
            let detector = ChessDetector::new(chess_params.clone()).expect("valid detector params");
            b.iter(|| {
                let detection = detector.detect(corners);
                criterion::black_box(detection)
            });
        });
    }
    chess_group.finish();

    let mut decode_group = c.benchmark_group("puzzleboard/synthetic/decode");
    for (label, img, corners, params) in &fixtures {
        decode_group.bench_with_input(
            BenchmarkId::from_parameter(label),
            &(img, corners, params),
            |b, (img, corners, params)| {
                let detector =
                    PuzzleBoardDetector::new((*params).clone()).expect("puzzle detector");
                let view = gray_view(img);
                b.iter(|| {
                    let out = detector.detect(&view, corners);
                    criterion::black_box(out)
                });
            },
        );
    }
    decode_group.finish();
}

criterion_group!(benches, bench_all);
criterion_main!(benches);
