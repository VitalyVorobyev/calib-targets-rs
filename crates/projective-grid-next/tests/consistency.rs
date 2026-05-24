use nalgebra::Point2;
use projective_grid_next::{
    check_consistency, ConsistencyParams, ConsistencyRequest, Coord, CoordinateHypothesis,
    GridError, LatticeKind, PointFeature,
};

fn feature(idx: usize, x: f64, y: f64) -> PointFeature<f64> {
    PointFeature::new(idx, Point2::new(x, y))
}

fn hypothesis(idx: usize, u: i32, v: i32) -> CoordinateHypothesis<f64> {
    CoordinateHypothesis::new(idx, Coord::new(u, v), None)
}

fn square_features() -> (Vec<PointFeature<f64>>, Vec<CoordinateHypothesis<f64>>) {
    let mut features = Vec::new();
    let mut hypotheses = Vec::new();
    let mut idx = 0;
    for v in 0..3 {
        for u in 0..3 {
            features.push(feature(
                idx,
                10.0 + 20.0 * f64::from(u),
                -5.0 + 30.0 * f64::from(v),
            ));
            hypotheses.push(hypothesis(idx, u, v));
            idx += 1;
        }
    }
    (features, hypotheses)
}

#[test]
fn clean_square_coordinate_hypotheses_pass() {
    let (features, hypotheses) = square_features();
    let request = ConsistencyRequest::new(
        LatticeKind::Square,
        &features,
        &hypotheses,
        None,
        ConsistencyParams::new(0.01),
    );
    let report = check_consistency(request).unwrap();
    assert!(report.passed);
    assert_eq!(report.solution.grid.entries.len(), 9);
    assert_eq!(
        report.solution.grid.bbox,
        Some((Coord::new(0, 0), Coord::new(2, 2)))
    );
    assert!(report.solution.fit.unwrap().residuals.max_px < 1e-9);
}

#[test]
fn clean_hex_coordinate_hypotheses_pass() {
    let coords = [
        Coord::new(0, 0),
        Coord::new(1, 0),
        Coord::new(0, 1),
        Coord::new(1, 1),
        Coord::new(2, 0),
        Coord::new(0, 2),
    ];
    let mut features = Vec::new();
    let mut hypotheses = Vec::new();
    for (idx, coord) in coords.into_iter().enumerate() {
        let model = LatticeKind::Hex.model_point::<f64>(coord);
        features.push(feature(idx, 3.0 + 40.0 * model.x, 7.0 + 40.0 * model.y));
        hypotheses.push(CoordinateHypothesis::new(idx, coord, None));
    }

    let report = check_consistency(ConsistencyRequest::new(
        LatticeKind::Hex,
        &features,
        &hypotheses,
        None,
        ConsistencyParams::new(0.01),
    ))
    .unwrap();
    assert!(report.passed);
    assert!(report.solution.rejected.is_empty());
    assert!(report.solution.fit.unwrap().residuals.max_px < 1e-8);
}

#[test]
fn shuffled_feature_order_uses_source_indices() {
    let (mut features, hypotheses) = square_features();
    features.reverse();

    let report = check_consistency(ConsistencyRequest::new(
        LatticeKind::Square,
        &features,
        &hypotheses,
        None,
        ConsistencyParams::new(0.01),
    ))
    .unwrap();
    assert!(report.passed);
}

#[test]
fn wrong_coordinate_hypothesis_is_rejected_by_residual() {
    let (features, mut hypotheses) = square_features();
    hypotheses[4].coord = Coord::new(10, 10);

    let report = check_consistency(ConsistencyRequest::new(
        LatticeKind::Square,
        &features,
        &hypotheses,
        None,
        ConsistencyParams::new(1.0),
    ))
    .unwrap();
    assert!(!report.passed);
    assert!(!report.solution.rejected.is_empty());
}

#[test]
fn duplicate_coordinate_conflict_is_inconsistent_input() {
    let (features, mut hypotheses) = square_features();
    hypotheses[1].coord = hypotheses[0].coord;

    let err = check_consistency(ConsistencyRequest::new(
        LatticeKind::Square,
        &features,
        &hypotheses,
        None,
        ConsistencyParams::default(),
    ))
    .unwrap_err();
    assert!(matches!(err, GridError::InconsistentInput(_)));
}

#[test]
fn duplicate_source_conflict_is_inconsistent_input() {
    let (features, mut hypotheses) = square_features();
    hypotheses[1].source_index = hypotheses[0].source_index;

    let err = check_consistency(ConsistencyRequest::new(
        LatticeKind::Square,
        &features,
        &hypotheses,
        None,
        ConsistencyParams::default(),
    ))
    .unwrap_err();
    assert!(matches!(err, GridError::InconsistentInput(_)));
}

#[test]
fn too_few_hypotheses_are_insufficient_evidence() {
    let features = vec![
        feature(0, 0.0, 0.0),
        feature(1, 1.0, 0.0),
        feature(2, 0.0, 1.0),
    ];
    let hypotheses = vec![
        hypothesis(0, 0, 0),
        hypothesis(1, 1, 0),
        hypothesis(2, 0, 1),
    ];

    let err = check_consistency(ConsistencyRequest::new(
        LatticeKind::Square,
        &features,
        &hypotheses,
        None,
        ConsistencyParams::default(),
    ))
    .unwrap_err();
    assert_eq!(err, GridError::InsufficientEvidence);
}
