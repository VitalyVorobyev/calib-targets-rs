//! End-to-end smoke test for the [`GraphBuildAlgorithm`] dispatch.
//!
//! Builds a clean 6×6 synthetic grid and runs the detector once with
//! each algorithm. Verifies:
//!
//! 1. The dispatch wiring routes to the correct path (no panics).
//! 2. Both pipelines emit a labelled detection.
//! 3. The labelled grid is non-negative and well-formed
//!    (workspace invariant on grid coordinates).
//!
//! Recall comparison between the two pipelines is intentionally *not*
//! gated here — the topological pipeline ships with looser recall on
//! noisy real-world images and is iterated separately. This test only
//! covers the dispatch contract.

use calib_targets_chessboard::{ChessCorner, Detector, DetectorParams, GraphBuildAlgorithm};
use calib_targets_core::AxisEstimate;
use nalgebra::Point2;
use std::f32::consts::FRAC_PI_2;

fn synthetic_grid(rows: usize, cols: usize, step: f32) -> Vec<ChessCorner> {
    let mut out = Vec::with_capacity(rows * cols);
    for j in 0..rows {
        for i in 0..cols {
            // Alternate axis-slot assignment by parity, matching the
            // chessboard `cluster.label_of` contract used by the
            // seed-and-grow validator.
            let parity = (i + j) % 2;
            let (a0, a1) = if parity == 0 {
                (0.0_f32, FRAC_PI_2)
            } else {
                (FRAC_PI_2, 0.0_f32)
            };
            out.push(ChessCorner {
                position: Point2::new(i as f32 * step, j as f32 * step),
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
                strength: 100.0,
            });
        }
    }
    out
}

fn run_with(
    algorithm: GraphBuildAlgorithm,
    corners: &[ChessCorner],
) -> Vec<calib_targets_chessboard::ChessboardDetection> {
    let mut params = DetectorParams::default();
    params.graph_build_algorithm = algorithm;
    let det = Detector::new(params);
    det.detect_all(corners)
}

fn assert_labels_non_negative(detections: &[calib_targets_chessboard::ChessboardDetection]) {
    for d in detections {
        for c in &d.corners {
            assert!(c.grid.i >= 0, "grid.i < 0 violates workspace invariant");
            assert!(c.grid.j >= 0, "grid.j < 0 violates workspace invariant");
        }
    }
}

#[test]
fn dispatch_routes_to_topological_pipeline() {
    let corners = synthetic_grid(6, 6, 12.0);
    let detections = run_with(GraphBuildAlgorithm::Topological, &corners);
    assert!(
        !detections.is_empty(),
        "topological dispatch returned no detection on a clean 6x6 grid"
    );
    let total: usize = detections.iter().map(|d| d.corners.len()).sum();
    assert!(
        total >= 16,
        "topological dispatch labelled too few corners ({total} < 16)",
    );
    assert_labels_non_negative(&detections);
}

#[test]
fn dispatch_routes_to_seed_and_grow_pipeline() {
    let corners = synthetic_grid(6, 6, 12.0);
    let detections = run_with(GraphBuildAlgorithm::SeedAndGrow, &corners);
    assert!(
        !detections.is_empty(),
        "seed-and-grow dispatch returned no detection on a clean 6x6 grid"
    );
    let total: usize = detections.iter().map(|d| d.corners.len()).sum();
    assert!(
        total >= 30,
        "seed-and-grow should label most corners on a clean grid (got {total})",
    );
    assert_labels_non_negative(&detections);
}

#[test]
fn default_dispatch_matches_seed_and_grow() {
    // The current default is SeedAndGrow; flipping it to Topological
    // is gated on closing the recall gap on the public testdata
    // regression set.
    assert_eq!(
        DetectorParams::default().graph_build_algorithm,
        GraphBuildAlgorithm::SeedAndGrow,
    );
}
