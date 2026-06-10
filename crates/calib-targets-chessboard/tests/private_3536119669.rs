//! Precision regression for the private `3536119669` dataset.
//!
//! The first three stitched target images contain ChArUco marker regions
//! that can generate plausible but wrong seed-and-grow labels. The table
//! below records the reviewed false coordinates. A passing detector either
//! refuses the frame or emits a grid without those labels.

use std::collections::HashSet;
use std::path::PathBuf;

use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_chessboard::{Detector, DetectorParams, GraphBuildAlgorithm};
use image::GenericImageView;

const SNAP_WIDTH: u32 = 720;
const SNAP_HEIGHT: u32 = 540;
const REVIEW_TARGETS: u32 = 3;
const SNAPS_PER_TARGET: u32 = 6;

type Coord = (i32, i32);
type FalseCoordCase = (u32, u32, &'static [Coord]);

const FALSE_COORDS: &[FalseCoordCase] = &[
    (0, 1, &[(3, 6)]),
    (0, 4, &[(0, 5), (1, 5), (8, 7)]),
    (
        0,
        5,
        &[
            (2, 7),
            (3, 7),
            (2, 8),
            (3, 8),
            (4, 6),
            (5, 6),
            (6, 6),
            (5, 7),
            (6, 7),
            (7, 7),
        ],
    ),
    (1, 0, &[(5, 12), (10, 6), (12, 5)]),
    (1, 1, &[(1, 7)]),
    (1, 4, &[(0, 5), (0, 6), (6, 7)]),
    (
        1,
        5,
        &[
            (2, 8),
            (3, 8),
            (4, 7),
            (5, 7),
            (5, 8),
            (6, 7),
            (6, 8),
            (7, 7),
            (8, 6),
            (9, 6),
            (8, 7),
            (9, 7),
            (12, 5),
            (11, 6),
        ],
    ),
    (2, 0, &[(6, 10), (8, 9)]),
    (2, 1, &[(6, 7), (7, 7), (8, 7), (9, 8)]),
    (2, 3, &[(6, 11)]),
    (2, 4, &[(3, 6), (4, 6), (3, 7), (4, 7), (5, 7), (6, 7)]),
    (
        2,
        5,
        &[
            (2, 8),
            (3, 7),
            (3, 8),
            (4, 8),
            (6, 6),
            (5, 7),
            (6, 7),
            (7, 6),
            (7, 7),
            (8, 6),
            (8, 7),
            (9, 7),
            (10, 5),
            (11, 5),
            (11, 6),
        ],
    ),
];

fn dataset_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("CALIB_CHESSBOARD_3536119669_DATASET") {
        return PathBuf::from(custom);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../privatedata/3536119669")
}

fn target_path(idx: u32) -> PathBuf {
    dataset_dir().join(format!("target_{idx}.png"))
}

fn load_snap(target_idx: u32, snap_idx: u32) -> image::GrayImage {
    let path = target_path(target_idx);
    let img = image::open(&path)
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()))
        .to_luma8();
    let x0 = snap_idx * SNAP_WIDTH;
    img.view(x0, 0, SNAP_WIDTH, SNAP_HEIGHT).to_image()
}

/// Precision contract for the **seed-and-grow** builder on these ChArUco-style
/// frames: no reviewed false `(i, j)` label may survive.
///
/// Only seed-and-grow is gated here, on purpose. These are marker images, and
/// the ChArUco detector pins seed-and-grow precisely because the topological
/// builder admits marker-internal corners (its per-cell axis test cannot reject
/// an x-junction whose axes happen to align with the grid) — running the
/// topological builder on this set reproduces known wrong labels. Topological
/// is the default for *plain* chessboard / puzzle detection, where precision
/// holds; marker scenes must go through the ChArUco detector. See
/// `docs/algorithmic_gaps.md` Gap 10 and the `GraphBuildAlgorithm` docs.
fn assert_rejects_false_labels(algorithm: GraphBuildAlgorithm) {
    let dir = dataset_dir();
    if !dir.exists() {
        eprintln!(
            "[skipped] 3536119669 false-label regression ({algorithm:?}): dataset missing at {}",
            dir.display()
        );
        return;
    }

    let chess_cfg = default_chess_config();
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = algorithm;
    let detector = Detector::new(params);

    let mut detected = 0usize;
    for target_idx in 0..REVIEW_TARGETS {
        for snap_idx in 0..SNAPS_PER_TARGET {
            let false_coords = FALSE_COORDS
                .iter()
                .find_map(|&(t, s, coords)| (t == target_idx && s == snap_idx).then_some(coords))
                .unwrap_or(&[]);
            let snap = load_snap(target_idx, snap_idx);
            let corners = detect_corners(&snap, &chess_cfg);
            let Some(detection) = detector.detect(&corners) else {
                continue;
            };
            detected += 1;
            let labels: HashSet<(i32, i32)> = detection
                .corners
                .iter()
                .map(|corner| (corner.grid.i, corner.grid.j))
                .collect();
            for &coord in false_coords {
                assert!(
                    !labels.contains(&coord),
                    "{algorithm:?} target_{target_idx} snap {snap_idx}: false label {coord:?} survived"
                );
            }
        }
    }

    eprintln!(
        "3536119669 reviewed frames with detections after final gate ({algorithm:?}): {detected}/{}",
        REVIEW_TARGETS * SNAPS_PER_TARGET
    );
}

#[test]
fn seed_and_grow_rejects_reviewed_3536119669_false_labels() {
    assert_rejects_false_labels(GraphBuildAlgorithm::SeedAndGrow);
}

// NOTE: deliberately no `topological_*` variant. Topological is not
// precision-safe on ChArUco-style marker frames (it labels marker-internal
// corners) and is intentionally not gated against them — marker scenes use the
// ChArUco detector, which pins seed-and-grow.
