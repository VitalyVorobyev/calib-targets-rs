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
//! The detector's empirical precision contract on REAL ChArUco scenes
//! (high detection rate, zero wrong labels on our private regression dataset)
//! is validated separately in `private_dataset.rs`. This test provides a
//! fast synthetic gate that catches cluster-level regressions.

use calib_targets_chessboard::{ChessCorner, Detector, DetectorParams, GraphBuildAlgorithm};
use calib_targets_core::AxisEstimate;
use nalgebra::Point2;

fn board_corner(x: f32, y: f32, parity: usize) -> ChessCorner {
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
    ChessCorner {
        position: Point2::new(x, y),
        axes,
        contrast: 30.0,
        fit_rms: 3.0,
        strength: 1.0,
    }
}

fn marker_internal_corner(x: f32, y: f32, angle_rad: f32) -> ChessCorner {
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
    ChessCorner {
        position: Point2::new(x, y),
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

    // Marker-internal rejection is a seed-and-grow guarantee — the Stage-3
    // cluster gate (cluster_tol_deg = 12°) is the primary defense, and ChArUco
    // pins seed-and-grow for exactly this reason. The topological builder (now
    // the default) is intentionally NOT hardened against marker-internal
    // corners, so this precision contract is exercised on the seed-and-grow
    // path that actually runs on marker scenes.
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = GraphBuildAlgorithm::SeedAndGrow;
    let detector = Detector::new(params).expect("valid detector params");
    let detection = detector
        .detect(&corners)
        .expect("board must still be detected despite marker-internal noise");

    // Every labelled corner should be a board corner (index < board_count).
    // The stable detection carries `input_index` on every labelled corner,
    // so the precision check reads straight off the lean `detect()` path —
    // no diagnostics needed.
    let marker_set: std::collections::HashSet<usize> = marker_indices.into_iter().collect();
    let labelled_markers: Vec<(usize, (i32, i32))> = detection
        .corners
        .iter()
        .filter(|c| marker_set.contains(&c.input_index))
        .map(|c| (c.input_index, (c.grid.i, c.grid.j)))
        .collect();
    assert!(
        labelled_markers.is_empty(),
        "precision contract violated: marker-internal corners labelled as board: {labelled_markers:?}"
    );

    // And the detection itself should have at most 36 corners (the board).
    // The detector may legitimately miss a few on the edges; it must
    // never exceed 36
    // or land a corner at a non-board position.
    assert!(
        detection.corners.len() <= board_count,
        "detection labelled {} corners but board only has {}",
        detection.corners.len(),
        board_count
    );
    assert!(
        detection.corners.len() >= 16,
        "expected ≥16 board corners labelled, got {}",
        detection.corners.len()
    );
}
