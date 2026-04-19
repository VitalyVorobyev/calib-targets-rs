//! Multi-component detection test.
//!
//! Builds two disconnected 3×3 chessboard pieces (same physical board,
//! same cell size, separated by a gap larger than the attach-search
//! window) and asserts that `Detector::detect_all` returns both
//! components as independent `Detection`s.
//!
//! This exercises the ChArUco use case where markers break the chessboard
//! into disconnected pieces — the per-component `Detection` carries its
//! own locally-rebased `(i, j)` labels; ChArUco's marker decoding is
//! responsible for aligning them to a global frame.
//!
//! Multi-physical-board scenes are explicitly out of scope.

use calib_targets_core::{AxisEstimate, Corner};
use calib_targets_chessboard::{Detector, DetectorParams};
use nalgebra::Point2;
use std::collections::HashSet;

fn corner(x: f32, y: f32, parity: usize) -> Corner {
    let (a0, a1) = if parity == 0 {
        (0.0_f32, std::f32::consts::FRAC_PI_2)
    } else {
        (std::f32::consts::FRAC_PI_2, 0.0_f32)
    };
    Corner {
        position: Point2::new(x, y),
        orientation_cluster: None,
        axes: [
            AxisEstimate {
                angle: a0,
                sigma: 0.02,
            },
            AxisEstimate {
                angle: a1,
                sigma: 0.02,
            },
        ],
        contrast: 30.0,
        fit_rms: 1.0,
        strength: 1.0,
    }
}

fn build_3x3(x0: f32, y0: f32, spacing: f32) -> Vec<Corner> {
    let mut out = Vec::new();
    for j in 0..3 {
        for i in 0..3 {
            let x = x0 + i as f32 * spacing;
            let y = y0 + j as f32 * spacing;
            let parity = ((i + j) % 2) as usize;
            out.push(corner(x, y, parity));
        }
    }
    out
}

#[test]
fn detects_two_components_of_the_same_board() {
    let spacing = 20.0_f32;
    let mut corners = build_3x3(50.0, 50.0, spacing);
    let left_count = corners.len();
    // Second piece is > 3× spacing away on the x axis so the attach-search
    // window (default 0.35 × cell_size ≈ 7 px) cannot bridge the gap.
    corners.extend(build_3x3(200.0, 50.0, spacing));

    let detector = Detector::new(DetectorParams::default());
    let detections = detector.detect_all(&corners);

    assert_eq!(
        detections.len(),
        2,
        "expected 2 disconnected components, got {}",
        detections.len()
    );

    let mut seen: HashSet<usize> = HashSet::new();
    for (k, det) in detections.iter().enumerate() {
        assert_eq!(
            det.target.corners.len(),
            9,
            "component {k}: expected 9 corners, got {}",
            det.target.corners.len()
        );
        assert_eq!(
            det.strong_indices.len(),
            9,
            "component {k}: expected 9 strong_indices, got {}",
            det.strong_indices.len()
        );
        // Each component's corners must be disjoint from the other.
        for &idx in &det.strong_indices {
            assert!(
                seen.insert(idx),
                "component {k}: input index {idx} appears in multiple components"
            );
        }
    }

    // The grouping should partition by piece: one Detection contains all
    // input indices < left_count and the other contains the rest.
    let sets: Vec<HashSet<usize>> = detections
        .iter()
        .map(|d| d.strong_indices.iter().copied().collect())
        .collect();
    let left_piece: HashSet<usize> = (0..left_count).collect();
    let right_piece: HashSet<usize> = (left_count..corners.len()).collect();
    let matches_split = (sets[0] == left_piece && sets[1] == right_piece)
        || (sets[0] == right_piece && sets[1] == left_piece);
    assert!(
        matches_split,
        "components did not partition along the physical split; sets[0]={:?} sets[1]={:?}",
        sets[0], sets[1]
    );
}

#[test]
fn single_component_scene_still_returns_one() {
    let corners = build_3x3(50.0, 50.0, 20.0);
    let detector = Detector::new(DetectorParams::default());
    let detections = detector.detect_all(&corners);
    assert_eq!(detections.len(), 1);
    assert_eq!(detections[0].target.corners.len(), 9);
    assert_eq!(detections[0].strong_indices.len(), 9);
}
