use nalgebra::Point2;
use projective_grid_next::{
    Coord, CoordinateHypothesis, GridDimensions, LatticeKind, LocalAxis, OrientedFeature,
    PointFeature, D4_TRANSFORMS, D6_TRANSFORMS,
};

#[test]
fn feature_evidence_types_are_constructible() {
    let point = PointFeature::new(12, Point2::new(10.0_f64, 20.0));
    let axis0 = LocalAxis::new(0.0, Some(0.05));
    let axis1 = LocalAxis::new(std::f64::consts::FRAC_PI_2, Some(0.05));
    let axis2 = LocalAxis::new(std::f64::consts::PI / 3.0, None);

    let one = OrientedFeature::<_, 1>::new(point, [axis0]);
    let two = OrientedFeature::<_, 2>::new(point, [axis0, axis1]);
    let three = OrientedFeature::<_, 3>::new(point, [axis0, axis1, axis2]);
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

    let square = LatticeKind::Square.model_point::<f64>(Coord::new(2, 5));
    assert_eq!(square, Point2::new(2.0, 5.0));

    let hex = LatticeKind::Hex.model_point::<f64>(Coord::new(1, 2));
    assert!((hex.x - 2.0).abs() < 1e-12);
    assert!((hex.y - 3.0_f64.sqrt()).abs() < 1e-12);
}

#[test]
fn symmetry_tables_stay_lattice_tagged() {
    assert_eq!(D4_TRANSFORMS.len(), 8);
    assert_eq!(D6_TRANSFORMS.len(), 12);
    assert!(D4_TRANSFORMS
        .iter()
        .all(|t| t.source_kind == LatticeKind::Square));
    assert!(D6_TRANSFORMS
        .iter()
        .all(|t| t.source_kind == LatticeKind::Hex));
}
