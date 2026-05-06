//! Sweep the absolute ChESS-response threshold across the public regression
//! testdata images to pick a sensible default for `default_chess_config()`.
//!
//! Run from the repo root:
//!
//! ```bash
//! cargo run --release -p calib-targets --example threshold_sweep
//! ```

use std::path::Path;

use calib_targets::chessboard::{Detector, DetectorParams, GraphBuildAlgorithm};
use calib_targets::detect::{default_chess_config, detect_corners, ThresholdMode};
use image::imageops::FilterType;

const IMAGES: &[&str] = &[
    "testdata/mid.png",
    "testdata/large.png",
    "testdata/small0.png",
    "testdata/small1.png",
    "testdata/small2.png",
    "testdata/small3.png",
    "testdata/small4.png",
    "testdata/small5.png",
    "testdata/puzzleboard_reference/example0.png",
    "testdata/puzzleboard_reference/example1.png",
    "testdata/puzzleboard_reference/example3.png",
    "testdata/puzzleboard_reference/example8.png",
    "testdata/02-topo-grid/GeminiChess1.png",
    "testdata/02-topo-grid/GeminiChess2.png",
    "testdata/02-topo-grid/GeminiChess3.png",
    "testdata/02-topo-grid/gptchess1.png",
];

const THRESHOLDS: &[f32] = &[0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0, 50.0];

fn main() {
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR")
        .map(|s| Path::new(&s).join("../..").canonicalize().unwrap())
        .unwrap_or_else(|_| std::env::current_dir().unwrap());

    print!("{:<58}", "image");
    for t in THRESHOLDS {
        print!(" {:>4.0}/h  ", t);
    }
    println!(" {:>8}", "raw@0");

    for rel in IMAGES {
        let path = workspace_root.join(rel);
        let img = match image::open(&path) {
            Ok(i) => i.to_luma8(),
            Err(e) => {
                eprintln!("skip {}: {e}", rel);
                continue;
            }
        };
        let img = if img.width() < 640 {
            let scale = 2.0;
            let w = (img.width() as f32 * scale) as u32;
            let h = (img.height() as f32 * scale) as u32;
            image::imageops::resize(&img, w, h, FilterType::Triangle)
        } else {
            img
        };

        let mut cfg0 = default_chess_config();
        cfg0.threshold_mode = ThresholdMode::Absolute;
        cfg0.threshold_value = 0.0;
        let raw_at_zero = detect_corners(&img, &cfg0, 0.0).len();

        let algo = std::env::var("ALGO").unwrap_or_else(|_| "chessboard_v2".to_string());
        let algorithm = match algo.as_str() {
            "topological" => GraphBuildAlgorithm::Topological,
            _ => GraphBuildAlgorithm::ChessboardV2,
        };
        print!("{:<58}", rel);
        for &t in THRESHOLDS {
            let mut cfg = default_chess_config();
            cfg.threshold_mode = ThresholdMode::Absolute;
            cfg.threshold_value = t;
            let corners = detect_corners(&img, &cfg, 0.0);
            let mut params = DetectorParams::default();
            params.graph_build_algorithm = algorithm;
            let detector = Detector::new(params);
            let detection = detector.detect(&corners);
            let (labelled, holes) = match &detection {
                Some(d) => {
                    let mut min_i = i32::MAX;
                    let mut max_i = i32::MIN;
                    let mut min_j = i32::MAX;
                    let mut max_j = i32::MIN;
                    let mut coords = std::collections::HashSet::new();
                    for c in &d.target.corners {
                        if let Some(g) = c.grid {
                            min_i = min_i.min(g.i);
                            max_i = max_i.max(g.i);
                            min_j = min_j.min(g.j);
                            max_j = max_j.max(g.j);
                            coords.insert((g.i, g.j));
                        }
                    }
                    let mut h = 0usize;
                    if !coords.is_empty() {
                        for j in min_j..=max_j {
                            for i in min_i..=max_i {
                                if !coords.contains(&(i, j)) {
                                    h += 1;
                                }
                            }
                        }
                    }
                    (d.target.corners.len(), h)
                }
                None => (0, 0),
            };
            print!(" {:>4}/{:<3}", labelled, holes);
        }
        println!(" {:>8}", raw_at_zero);
    }
}
