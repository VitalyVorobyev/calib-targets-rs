//! Marker-internal rejection test.
//!
//! Builds a 6×6 board lattice and sprinkles "marker-internal" x-junction
//! candidates inside every interior cell: each at an off-axis offset with
//! axes rotated 20° vs. the board. Such corners are the common failure mode
//! on ChArUco scenes — they would corrupt calibration if labelled.
//!
//! The 20° rotation exceeds the default `cluster_tol_deg = 12°`, so the
//! Stage-3 cluster assignment is the primary defense exercised here. The
//! test asserts:
//!
//! 1. A detection is produced (the 6×6 board is recovered).
//! 2. None of the marker-internal input indices appears as a labelled corner.
//!
//! v2's empirical precision contract on REAL ChArUco scenes (119/120 on the
//! 3536119669 dataset with 0 wrong labels) is validated separately in
//! `dataset_3536119669.rs`. This test provides a fast synthetic gate that
//! catches cluster-level regressions.

use calib_targets_chessboard::{Detector, DetectorParams};
use calib_targets_core::{AxisEstimate, Corner};
use nalgebra::Point2;

fn board_corner(x: f32, y: f32, parity: usize) -> Corner {
    // Parity-0: axes[0] = 0 (horizontal), axes[1] = π/2 (vertical).
    // Parity-1: axes flipped so the dark-sector CCW sweep invariant holds.
    let axes = if parity == 0 {
        [
            AxisEstimate {
                angle: 0.0,
                sigma: 0.02,
            },
            AxisEstimate {
                angle: std::f32::consts::FRAC_PI_2,
                sigma: 0.02,
            },
        ]
    } else {
        [
            AxisEstimate {
                angle: std::f32::consts::FRAC_PI_2,
                sigma: 0.02,
            },
            AxisEstimate {
                angle: std::f32::consts::PI,
                sigma: 0.02,
            },
        ]
    };
    Corner {
        position: Point2::new(x, y),
        orientation_cluster: None,
        axes,
        contrast: 30.0,
        fit_rms: 3.0,
        strength: 1.0,
    }
}

fn marker_internal_corner(x: f32, y: f32, angle_rad: f32) -> Corner {
    let axes = [
        AxisEstimate {
            angle: angle_rad,
            sigma: 0.02,
        },
        AxisEstimate {
            angle: angle_rad + std::f32::consts::FRAC_PI_2,
            sigma: 0.02,
        },
    ];
    Corner {
        position: Point2::new(x, y),
        orientation_cluster: None,
        axes,
        contrast: 20.0,
        fit_rms: 5.0,
        strength: 0.5,
    }
}

#[test]
fn marker_internal_corners_never_labelled() {
    let spacing = 20.0_f32;
    let rows = 6;
    let cols = 6;

    let mut corners = Vec::new();

    // 36 board corners on a regular 6×6 lattice.
    for j in 0..rows {
        for i in 0..cols {
            let x = i as f32 * spacing + 50.0;
            let y = j as f32 * spacing + 50.0;
            let parity = ((i + j) % 2) as usize;
            corners.push(board_corner(x, y, parity));
        }
    }
    let board_count = corners.len();

    // 25 marker-internal corners: one per interior cell, offset from the
    // cell center, with axes rotated 20° off the board axes. This rotation
    // exceeds the default cluster_tol_deg = 12°, so Stage 3 rejects the
    // markers outright before seed/grow get a chance to attach them.
    let marker_rot = 20.0_f32.to_radians();
    for j in 0..rows - 1 {
        for i in 0..cols - 1 {
            let cx = (i as f32 + 0.5) * spacing + 50.0 + 3.5;
            let cy = (j as f32 + 0.5) * spacing + 50.0 + 3.5;
            corners.push(marker_internal_corner(cx, cy, marker_rot));
        }
    }
    let marker_indices: Vec<usize> = (board_count..corners.len()).collect();

    let detector = Detector::new(DetectorParams::default());
    let frame = detector.detect_debug(&corners);
    let detection = frame
        .detection
        .as_ref()
        .expect("board must still be detected despite marker-internal noise");

    // Every labelled corner should be a board corner (index < board_count).
    // We read this via the DebugFrame, which carries the input_index on
    // every CornerAug.
    let marker_set: std::collections::HashSet<usize> = marker_indices.into_iter().collect();
    let mut labelled_markers = Vec::new();
    for aug in &frame.corners {
        if marker_set.contains(&aug.input_index) {
            if let calib_targets_chessboard::CornerStage::Labeled { at, .. } = aug.stage {
                labelled_markers.push((aug.input_index, at));
            }
        }
    }
    assert!(
        labelled_markers.is_empty(),
        "precision contract violated: marker-internal corners labelled as board: {labelled_markers:?}"
    );

    // And the detection itself should have at most 36 corners (the board).
    // v2 may legitimately miss a few on the edges; it must never exceed 36
    // or land a corner at a non-board position.
    assert!(
        detection.target.corners.len() <= board_count,
        "detection labelled {} corners but board only has {}",
        detection.target.corners.len(),
        board_count
    );
    assert!(
        detection.target.corners.len() >= 16,
        "expected ≥16 board corners labelled, got {}",
        detection.target.corners.len()
    );
}
