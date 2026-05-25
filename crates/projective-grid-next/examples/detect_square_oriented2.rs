use projective_grid_next::{
    detect_grid, DetectionParams, DetectionRequest, Evidence, LatticeKind, LocalAxis,
    OrientedFeature, PointFeature, SquareAlgorithm,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut features = Vec::new();
    for v in 0..5 {
        for u in 0..5 {
            let x = 40.0 + u as f32 * 20.0 + v as f32 * 1.5;
            let y = 30.0 + v as f32 * 20.0;
            let source_index = features.len();
            features.push(OrientedFeature::new(
                PointFeature::new(source_index, nalgebra::Point2::new(x, y)),
                [
                    LocalAxis::new(0.0, Some(0.02)),
                    LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(0.02)),
                ],
            ));
        }
    }

    let params = DetectionParams::default().with_algorithm(SquareAlgorithm::SeedAndGrow);
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        params,
    );
    let solution = detect_grid(request)?;
    println!("labelled corners: {}", solution.grid.entries.len());
    Ok(())
}
