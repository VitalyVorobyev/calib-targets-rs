//! Contract check for adapting marker-board outputs into `projective-grid`.
//!
//! The marker-board crate owns board-specific circle IDs and optional
//! board-canonical corner IDs. The generic grid crate should only receive
//! image positions plus square-lattice coordinate hypotheses.

use calib_targets_core::{grid_coords_to_next, GridCoords};
use calib_targets_marker::{MarkerBoardCorner, MarkerBoardDetectionResult};
use nalgebra::Point2;
use projective_grid::{
    check_consistency, ConsistencyParams, ConsistencyRequest, CoordinateHypothesis, LatticeKind,
    PointFeature,
};

fn image_point(i: i32, j: i32) -> Point2<f32> {
    let i = i as f32;
    let j = j as f32;
    Point2::new(80.0 + 21.0 * i + 3.0 * j, 45.0 + 2.0 * i + 18.0 * j)
}

fn synthetic_marker_result() -> MarkerBoardDetectionResult {
    let mut corners = Vec::new();
    for j in 0..4 {
        for i in 0..5 {
            let grid = GridCoords { i, j };
            let id = (1_000 - (j * 5 + i)) as u32;
            let target = Point2::new(i as f32 * 12.0, j as f32 * 12.0);
            corners.push(
                MarkerBoardCorner::new(image_point(i, j), grid, 1.0)
                    .with_id(id)
                    .with_target_position(target),
            );
        }
    }
    corners.reverse();
    MarkerBoardDetectionResult::new(corners, None)
}

#[test]
fn marker_board_corners_pass_check_consistency_without_ids() {
    let result = synthetic_marker_result();
    assert!(result.corners.iter().all(|corner| corner.id.is_some()));

    let features: Vec<PointFeature> = result
        .corners
        .iter()
        .enumerate()
        .map(|(source_index, corner)| PointFeature::new(source_index, corner.position))
        .collect();
    let hypotheses: Vec<CoordinateHypothesis> = result
        .corners
        .iter()
        .enumerate()
        .map(|(source_index, corner)| {
            CoordinateHypothesis::unweighted(source_index, grid_coords_to_next(corner.grid))
        })
        .collect();

    let request = ConsistencyRequest::new(
        LatticeKind::Square,
        &features,
        &hypotheses,
        None,
        ConsistencyParams::new(0.05),
    );
    let report = check_consistency(request).expect("marker-board consistency check");

    assert!(report.passed, "rejected={:?}", report.solution.rejected);
    assert!(report.solution.rejected.is_empty());
    let fit = report.solution.fit.as_ref().expect("fit");
    assert_eq!(fit.residuals.count, result.corners.len());
    assert!(
        fit.residuals.max_px < 0.01,
        "max residual {} px",
        fit.residuals.max_px
    );
}
