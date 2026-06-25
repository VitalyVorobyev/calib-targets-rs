//! Synthetic author-like PuzzleBoard photo regression.
//!
//! The images under `testdata/puzzleboard_synthetic_author_like/` are
//! deterministic renders from the committed 501×501 PuzzleBoard maps, then
//! warped through perspective, radial distortion, blur, noise, vignetting, and
//! JPEG-like compression. Unlike the upstream example photos, these fixtures
//! have unambiguous ground-truth master coordinates.

use calib_targets::detect;
use calib_targets::puzzleboard::PuzzleBoardDetectionResult;
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use calib_targets_core::GRID_TRANSFORMS_D4;
use image::{GrayImage, ImageReader, Rgb, RgbImage};
use serde::Deserialize;
use std::path::{Path, PathBuf};

const PIXEL_MATCH_TOL: f32 = 4.0;
const MIN_MATCHED_CORNERS: usize = 24;
const MAX_BER: f32 = 0.18;

#[derive(Debug, Deserialize)]
struct Manifest {
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    name: String,
    image: String,
    rows: u32,
    cols: u32,
    origin_row: u32,
    origin_col: u32,
    corners: Vec<TruthCorner>,
}

#[derive(Debug, Deserialize)]
struct TruthCorner {
    master_row: i32,
    master_col: i32,
    pixel_x: f32,
    pixel_y: f32,
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("testdata")
        .join("puzzleboard_synthetic_author_like")
}

fn load_manifest(dir: &Path) -> Manifest {
    let text = std::fs::read_to_string(dir.join("manifest.json")).expect("read manifest");
    serde_json::from_str(&text).expect("parse manifest")
}

fn d4_consistent(pairs: &[(i32, i32, i32, i32)]) -> Option<(usize, i32, i32)> {
    for (idx, t) in GRID_TRANSFORMS_D4.iter().enumerate() {
        let deltas: Vec<(i32, i32)> = pairs
            .iter()
            .map(|&(our_row, our_col, truth_row, truth_col)| {
                let mapped_row = t.a * our_row + t.b * our_col;
                let mapped_col = t.c * our_row + t.d * our_col;
                (
                    (truth_row - mapped_row).rem_euclid(501),
                    (truth_col - mapped_col).rem_euclid(501),
                )
            })
            .collect();
        if deltas.iter().all(|d| *d == deltas[0]) {
            return Some((idx, deltas[0].0, deltas[0].1));
        }
    }
    None
}

fn draw_disc(img: &mut RgbImage, x: f32, y: f32, radius: i32, color: Rgb<u8>) {
    let cx = x.round() as i32;
    let cy = y.round() as i32;
    let r2 = radius * radius;
    for yy in (cy - radius)..=(cy + radius) {
        for xx in (cx - radius)..=(cx + radius) {
            let dx = xx - cx;
            let dy = yy - cy;
            if dx * dx + dy * dy <= r2
                && xx >= 0
                && yy >= 0
                && (xx as u32) < img.width()
                && (yy as u32) < img.height()
            {
                img.put_pixel(xx as u32, yy as u32, color);
            }
        }
    }
}

fn draw_ring(img: &mut RgbImage, x: f32, y: f32, radius: i32, color: Rgb<u8>) {
    let cx = x.round() as i32;
    let cy = y.round() as i32;
    let inner = (radius - 2).max(1);
    let outer2 = radius * radius;
    let inner2 = inner * inner;
    for yy in (cy - radius)..=(cy + radius) {
        for xx in (cx - radius)..=(cx + radius) {
            let dx = xx - cx;
            let dy = yy - cy;
            let d2 = dx * dx + dy * dy;
            if d2 <= outer2
                && d2 >= inner2
                && xx >= 0
                && yy >= 0
                && (xx as u32) < img.width()
                && (yy as u32) < img.height()
            {
                img.put_pixel(xx as u32, yy as u32, color);
            }
        }
    }
}

fn write_overlay(
    out_dir: &Path,
    scenario: &Scenario,
    image: &GrayImage,
    result: &PuzzleBoardDetectionResult,
) {
    std::fs::create_dir_all(out_dir).expect("create overlay dir");
    let mut rgb = RgbImage::from_fn(image.width(), image.height(), |x, y| {
        let v = image.get_pixel(x, y).0[0];
        Rgb([v, v, v])
    });

    // Red = generated ground-truth corner locations.
    for truth in &scenario.corners {
        draw_ring(&mut rgb, truth.pixel_x, truth.pixel_y, 4, Rgb([255, 0, 0]));
    }
    // Green = detected PuzzleBoard-labelled corners.
    for corner in &result.corners {
        draw_disc(
            &mut rgb,
            corner.position.x,
            corner.position.y,
            2,
            Rgb([0, 230, 0]),
        );
    }
    rgb.save(out_dir.join(format!("{}_detected_vs_truth.png", scenario.name)))
        .expect("save overlay");
}

#[test]
fn synthetic_author_like_photos_decode_against_canonical_map() {
    let dir = fixture_dir();
    let manifest = load_manifest(&dir);
    let overlay_dir = std::env::var_os("CALIB_PUZZLE_SYNTHETIC_OVERLAY_DIR").map(PathBuf::from);
    for scenario in &manifest.scenarios {
        let img = ImageReader::open(dir.join(&scenario.image))
            .expect("open fixture image")
            .decode()
            .expect("decode fixture image")
            .to_luma8();

        let board = PuzzleBoardSpec::with_origin(
            scenario.rows,
            scenario.cols,
            5.0,
            scenario.origin_row,
            scenario.origin_col,
        )
        .expect("board spec");
        let sweep = PuzzleBoardParams::sweep_for_board(&board);
        let result = detect::detect_puzzleboard_best(&img, &sweep)
            .unwrap_or_else(|err| panic!("{} failed to decode: {err}", scenario.name));
        if let Some(out_dir) = overlay_dir.as_deref() {
            write_overlay(out_dir, scenario, &img, &result);
        }

        assert!(
            result.decode.bit_error_rate <= MAX_BER,
            "{} BER {:.3} exceeds {:.3}; matched {}/{} edges",
            scenario.name,
            result.decode.bit_error_rate,
            MAX_BER,
            result.decode.edges_matched,
            result.decode.edges_observed
        );

        let mut pairs = Vec::new();
        for detected in &result.corners {
            let mut best: Option<(&TruthCorner, f32)> = None;
            for truth in &scenario.corners {
                let dx = detected.position.x - truth.pixel_x;
                let dy = detected.position.y - truth.pixel_y;
                let dist = (dx * dx + dy * dy).sqrt();
                if best.is_none_or(|(_, best_dist)| dist < best_dist) {
                    best = Some((truth, dist));
                }
            }
            let Some((truth, dist)) = best else {
                continue;
            };
            if dist <= PIXEL_MATCH_TOL {
                pairs.push((
                    detected.grid.v,
                    detected.grid.u,
                    truth.master_row,
                    truth.master_col,
                ));
            }
        }

        assert!(
            pairs.len() >= MIN_MATCHED_CORNERS,
            "{} matched only {} detected corners to truth within {PIXEL_MATCH_TOL}px",
            scenario.name,
            pairs.len()
        );

        let relation = d4_consistent(&pairs);
        assert!(
            relation.is_some(),
            "{} detected master IDs are not explainable by one D4+translation relation over {} matched corners",
            scenario.name,
            pairs.len()
        );

        println!(
            "{}: decoded {} corners, matched {} to truth, BER={:.3}, D4+offset={:?}",
            scenario.name,
            result.corners.len(),
            pairs.len(),
            result.decode.bit_error_rate,
            relation
        );
    }
}
