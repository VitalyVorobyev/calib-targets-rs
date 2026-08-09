use nalgebra::Point2;
use projective_grid::{
    detect_grid, DetectionRequest, Evidence, GridDimensions, GridError, LatticeKind, LocalAxis,
    OrientedFeature, PointFeature,
};

fn grid(rows: usize, cols: usize) -> Vec<OrientedFeature<2>> {
    let mut features = Vec::with_capacity(rows * cols);
    for v in 0..rows {
        for u in 0..cols {
            let source_index = v * cols + u;
            features.push(OrientedFeature::new(
                PointFeature::new(
                    source_index,
                    Point2::new(30.0 + 17.0 * u as f32, 40.0 + 19.0 * v as f32),
                ),
                [
                    LocalAxis::new(0.0, Some(0.02)),
                    LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(0.02)),
                ],
            ));
        }
    }
    features
}

fn request(features: &[OrientedFeature<2>]) -> DetectionRequest<'_> {
    DetectionRequest::new(LatticeKind::Square, Evidence::Oriented2(features))
}

#[test]
fn non_finite_position_is_a_typed_error_not_a_panic() {
    let mut features = grid(3, 3);
    features[4].point.position.x = f32::NAN;
    assert!(matches!(
        detect_grid(request(&features)),
        Err(GridError::InconsistentInput(_))
    ));
}

#[test]
fn invalid_axis_metadata_is_rejected_before_geometry() {
    let mut features = grid(3, 3);
    features[0].axes[0].angle_rad = f32::INFINITY;
    assert!(matches!(
        detect_grid(request(&features)),
        Err(GridError::InconsistentInput(_))
    ));

    let mut features = grid(3, 3);
    features[0].axes[0].sigma_rad = Some(-0.01);
    assert!(matches!(
        detect_grid(request(&features)),
        Err(GridError::InconsistentInput(_))
    ));
}

#[test]
fn duplicate_source_index_is_rejected() {
    let mut features = grid(3, 3);
    features[1].point.source_index = features[0].point.source_index;
    assert!(matches!(
        detect_grid(request(&features)),
        Err(GridError::InconsistentInput(_))
    ));
}

#[test]
fn dimensions_count_feature_positions_and_bound_the_span() {
    let features = grid(5, 5);
    let detection = detect_grid(request(&features).with_dimensions(GridDimensions::new(5, 5)))
        .expect("5x5 feature-position bounds admit a 5x5 corner grid");
    assert_eq!(
        detection.grid().dimensions(),
        Some(GridDimensions::new(5, 5))
    );

    assert!(matches!(
        detect_grid(request(&features).with_dimensions(GridDimensions::new(4, 5))),
        Err(GridError::DegenerateGeometry)
    ));
}

#[test]
fn fit_and_residuals_are_bitwise_stable_across_fresh_maps() {
    let mut features = grid(7, 8);
    for (index, feature) in features.iter_mut().enumerate() {
        let phase = index as f32 * 0.37;
        feature.point.position.x += phase.sin() * 0.15;
        feature.point.position.y += phase.cos() * 0.11;
    }

    let reference = detect_grid(request(&features)).expect("reference detection");
    let reference_matrix = reference.fit().model_to_image.matrix().map(f32::to_bits);
    let reference_residuals: Vec<u32> = reference
        .grid()
        .entries()
        .iter()
        .map(|entry| entry.residual_px.expect("fitted residual").to_bits())
        .collect();

    for _ in 0..32 {
        let actual = detect_grid(request(&features)).expect("repeated detection");
        assert_eq!(
            actual.fit().model_to_image.matrix().map(f32::to_bits),
            reference_matrix
        );
        assert_eq!(
            actual
                .grid()
                .entries()
                .iter()
                .map(|entry| entry.residual_px.expect("residual").to_bits())
                .collect::<Vec<_>>(),
            reference_residuals
        );
    }
}
