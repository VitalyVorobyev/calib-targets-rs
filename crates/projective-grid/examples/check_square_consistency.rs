use projective_grid::{
    check_consistency, ConsistencyParams, ConsistencyRequest, Coord, CoordinateHypothesis,
    LatticeKind, PointFeature,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut features = Vec::new();
    let mut hypotheses = Vec::new();
    for v in 0..3 {
        for u in 0..4 {
            let source_index = features.len();
            features.push(PointFeature::new(
                source_index,
                nalgebra::Point2::new(100.0 + u as f32 * 16.0, 80.0 + v as f32 * 16.0),
            ));
            hypotheses.push(CoordinateHypothesis::new(
                source_index,
                Coord::new(u, v),
                Some(1.0),
            ));
        }
    }

    let request = ConsistencyRequest::new(
        LatticeKind::Square,
        &features,
        &hypotheses,
        None,
        ConsistencyParams::default(),
    );
    let report = check_consistency(request)?;
    println!("passed: {}", report.passed);
    Ok(())
}
