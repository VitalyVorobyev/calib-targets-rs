use std::collections::HashMap;

use nalgebra::{Matrix3, Point2};
use projective_grid::expert::geometry::estimate_homography;
use projective_grid::expert::lattice::{
    predict_grid_position, GridTransform, D4_TRANSFORMS, D6_TRANSFORMS,
};
use projective_grid::{
    Coord, CoordinateHypothesis, GridDimensions, LatticeKind, LocalAxis, OrientedFeature,
    PointFeature,
};

#[test]
fn feature_evidence_types_are_constructible() {
    let point = PointFeature::new(12, Point2::new(10.0_f32, 20.0));
    let axis0 = LocalAxis::new(0.0, Some(0.05));
    let axis1 = LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(0.05));
    let axis2 = LocalAxis::new(std::f32::consts::PI / 3.0, None);

    let one = OrientedFeature::<1>::new(point, [axis0]);
    let two = OrientedFeature::<2>::new(point, [axis0, axis1]);
    let three = OrientedFeature::<3>::new(point, [axis0, axis1, axis2]);
    let hypothesis = CoordinateHypothesis::new(12, Coord::new(3, 4), Some(0.8));

    assert_eq!(one.axes.len(), 1);
    assert_eq!(two.axes.len(), 2);
    assert_eq!(three.axes.len(), 3);
    assert_eq!(hypothesis.source_index, point.source_index);
    assert_eq!(hypothesis.coord, Coord::new(3, 4));
}

#[test]
fn dimensions_and_model_mapping_are_explicit() {
    let dims = GridDimensions::new(9, 6);
    assert_eq!(dims.width, 9);
    assert_eq!(dims.height, 6);

    let square = LatticeKind::Square.model_point(Coord::new(2, 5));
    assert_eq!(square, Point2::new(2.0, 5.0));

    let hex = LatticeKind::Hex.model_point(Coord::new(1, 2));
    assert!((hex.x - 2.0).abs() < 1e-5);
    assert!((hex.y - 3.0_f32.sqrt()).abs() < 1e-5);
}

#[test]
fn symmetry_tables_stay_lattice_tagged() {
    assert_eq!(D4_TRANSFORMS.len(), 8);
    assert_eq!(D6_TRANSFORMS.len(), 12);
    assert!(D4_TRANSFORMS
        .iter()
        .all(|t| t.lattice() == LatticeKind::Square));
    assert!(D6_TRANSFORMS
        .iter()
        .all(|t| t.lattice() == LatticeKind::Hex));
}

#[test]
fn detector_builder_primitives_stay_on_expert_surface() {
    let transform = GridTransform::identity(LatticeKind::Square).with_translation([4, -2]);
    assert_eq!(transform.apply(Coord::new(3, 5)), Coord::new(7, 3));

    let mut labelled = HashMap::new();
    labelled.insert(Coord::new(-1, 0), Point2::new(-2.0_f32, 1.0));
    labelled.insert(Coord::new(1, 0), Point2::new(2.0_f32, 1.0));
    let predicted = predict_grid_position(&labelled, Coord::new(0, 0), LatticeKind::Square)
        .expect("opposite pair predicts the center");
    assert_eq!(predicted.position, Point2::new(0.0, 1.0));

    let source = [
        Point2::new(0.0_f32, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    ];
    let expected = Matrix3::new(2.0, 0.0, 3.0, 0.0, 2.0, 5.0, 0.0, 0.0, 1.0);
    let destination = source.map(|point| {
        let homogeneous = expected * point.to_homogeneous();
        Point2::new(homogeneous.x / homogeneous.z, homogeneous.y / homogeneous.z)
    });
    let estimated = estimate_homography(&source, &destination).expect("four-point homography");
    assert!((estimated.apply(Point2::new(0.5, 0.5)) - Point2::new(4.0, 6.0)).norm() < 1e-5);
}
