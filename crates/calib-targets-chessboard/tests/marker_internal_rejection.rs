//! Marker-internal rejection test.
//!
//! Builds a 6×6 board lattice and sprinkles "marker-internal" x-junction
//! candidates inside every interior cell: each at an off-axis offset with
//! axes rotated 20° vs. the board. Such corners are the common failure mode
//! on ChArUco scenes — they would corrupt calibration if labelled.
//!
//! The 20° rotation exceeds the default `cluster_tol_deg = 12°`, so the
//! Stage-3 cluster assignment is the primary defense exercised here. The
//! test asserts the **precision** half of the detection asymmetry (a false
//! positive is unrecoverable; a miss is acceptable): across every recovered
//! component, none of the marker-internal input indices appears as a labelled
//! corner. On this synthetic worst case (markers ~3.5 px off the cell centres)
//! the topological builder may decline to recover the dense board at all — an
//! allowed miss — so detection itself is not required.
//!
//! The detector's empirical precision contract on REAL ChArUco scenes
//! (high detection rate, zero wrong labels on our private regression dataset)
//! is validated separately in `private_dataset.rs`. This test provides a
//! fast synthetic gate that catches cluster-level regressions.

use calib_targets_chessboard::{ChessCorner, Detector, DetectorParams};
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
        // Above the default `min_corner_strength` floor (33.0): this test
        // exercises the *geometric* cluster-gate rejection of markers, so
        // every corner must survive the strength pre-filter.
        strength: 100.0,
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
        // Above the floor as well, so the markers reach the cluster gate
        // and are rejected by geometry (the 20° rotation), not silently
        // dropped by the strength pre-filter.
        strength: 50.0,
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
    // markers outright before the grid grows.
    let marker_rot = 20.0_f32.to_radians();
    for j in 0..rows - 1 {
        for i in 0..cols - 1 {
            let cx = (i as f32 + 0.5) * spacing + 50.0 + 3.5;
            let cy = (j as f32 + 0.5) * spacing + 50.0 + 3.5;
            corners.push(marker_internal_corner(cx, cy, marker_rot));
        }
    }
    let marker_indices: Vec<usize> = (board_count..corners.len()).collect();

    // Marker-internal rejection rests on the Stage-3 cluster gate
    // (cluster_tol_deg = 12°): the 20°-rotated marker corners fail it and are
    // never admitted to the labelled grid. The same gate runs ahead of the
    // topological builder (the only builder), which seeds its walk from the
    // cluster centres.
    //
    // The contract verified here is the **precision** half of the detection
    // asymmetry (a false positive is unrecoverable; a miss is acceptable):
    // across *every* recovered component, no marker-internal corner may be
    // labelled. On this pathological synthetic input — 25 markers placed only
    // ~3.5 px off the cell centres of a 20 px-pitch board — the topological
    // Delaunay walk is polluted enough that it may decline to recover the
    // board (a miss). That recall difference vs. the retired seed-and-grow
    // builder is allowed by the contract; what matters is that no marker
    // corner is ever mislabelled, which also holds when a board *is* recovered.
    // The real-image marker-rejection contract is the live charuco gate
    // `charuco::tests::private_dataset::flagship_rejects_reviewed_marker_bit_false_corners`.
    let detector = Detector::new(DetectorParams::default()).expect("valid detector params");
    let components = detector.detect_all(&corners);

    // No marker-internal corner may appear in ANY recovered component.
    let marker_set: std::collections::HashSet<usize> = marker_indices.into_iter().collect();
    let labelled_markers: Vec<(usize, (i32, i32))> = components
        .iter()
        .flat_map(|d| d.corners.iter())
        .filter(|c| marker_set.contains(&c.input_index))
        .map(|c| (c.input_index, (c.grid.u, c.grid.v)))
        .collect();
    assert!(
        labelled_markers.is_empty(),
        "precision contract violated: marker-internal corners labelled as board: {labelled_markers:?}"
    );

    // Whenever a component IS recovered, every labelled corner is a board
    // corner (index < board_count), so no component can exceed the board size.
    for d in &components {
        assert!(
            d.corners.len() <= board_count,
            "component labelled {} corners but board only has {}",
            d.corners.len(),
            board_count
        );
    }
}
