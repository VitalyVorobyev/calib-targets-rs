//! Contract check for adapting ChArUco outputs into `projective-grid-next`.
//!
//! ChArUco owns marker IDs, corner IDs, and board-space target positions.
//! The generic grid crate receives only image positions plus square-lattice
//! coordinate hypotheses.

use calib_targets_charuco::{CharucoCorner, CharucoDetectionResult};
use calib_targets_core::{grid_coords_to_next, GridAlignment, GridCoords, GRID_TRANSFORMS_D4};
use nalgebra::Point2;
use projective_grid_next::{
    check_consistency, ConsistencyParams, ConsistencyRequest, CoordinateHypothesis, LatticeKind,
    PointFeature,
};

fn image_point(i: i32, j: i32) -> Point2<f32> {
    let i = i as f32;
    let j = j as f32;
    Point2::new(120.0 + 19.0 * i - 4.0 * j, 70.0 + 1.5 * i + 22.0 * j)
}

fn synthetic_charuco_result() -> CharucoDetectionResult {
    let mut corners = Vec::new();
    for j in 0..5 {
        for i in 0..4 {
            let grid = GridCoords { i, j };
            let id = (500 - (j * 4 + i)) as u32;
            let target = Point2::new(i as f32 * 10.0, j as f32 * 10.0);
            corners.push(CharucoCorner::new(image_point(i, j), grid, id, target, 1.0));
        }
    }
    corners.rotate_left(7);
    CharucoDetectionResult::new(
        corners,
        Vec::new(),
        GridAlignment {
            transform: GRID_TRANSFORMS_D4[0],
            translation: [0, 0],
        },
    )
}

#[test]
fn charuco_corners_pass_check_consistency_without_corner_ids() {
    let result = synthetic_charuco_result();
    assert!(result.corners.iter().all(|corner| corner.id >= 481));

    let features: Vec<PointFeature<f32>> = result
        .corners
        .iter()
        .enumerate()
        .map(|(source_index, corner)| PointFeature::new(source_index, corner.position))
        .collect();
    let hypotheses: Vec<CoordinateHypothesis<f32>> = result
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
    let report = check_consistency(request).expect("charuco consistency check");

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
