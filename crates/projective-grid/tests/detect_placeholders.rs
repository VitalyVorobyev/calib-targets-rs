use nalgebra::Point2;
use projective_grid::{
    detect_grid, Coord, CoordinateHypothesis, DetectionParams, DetectionRequest, Evidence,
    EvidenceKind, GridError, GridTask, LatticeKind, LocalAxis, OrientedFeature, PointFeature,
};

fn point(idx: usize) -> PointFeature {
    PointFeature::new(idx, Point2::new(idx as f32, 0.0))
}

fn assert_unsupported(request: DetectionRequest<'_>, evidence: EvidenceKind) {
    // Capture `lattice` before the request is consumed by `detect_grid`.
    let lattice = request.lattice;
    let err = detect_grid(request).unwrap_err();
    assert_eq!(
        err,
        GridError::UnsupportedCombination {
            task: GridTask::Detection,
            lattice,
            evidence,
        }
    );
}

#[test]
fn square_position_detection_is_typed_unsupported() {
    let features = [point(0), point(1), point(2), point(3)];
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Positions(&features),
        None,
        DetectionParams::default(),
    );
    assert_unsupported(request, EvidenceKind::Positions);
}

// `square_oriented_detection_is_typed_unsupported` was removed in Phase C of
// the `projective-grid` rewrite: `(LatticeKind::Square,
// Evidence::Oriented2)` now runs the seed-and-grow port and returns a real
// labelled grid. The success path is covered by
// `tests/detect_square_oriented2.rs`.

#[test]
fn hex_position_detection_is_typed_unsupported() {
    let features = [point(0), point(1), point(2), point(3)];
    let request = DetectionRequest::new(
        LatticeKind::Hex,
        Evidence::Positions(&features),
        None,
        DetectionParams::default(),
    );
    assert_unsupported(request, EvidenceKind::Positions);
}

#[test]
fn hex_oriented_detection_is_typed_unsupported() {
    let axis = LocalAxis::new(0.0_f32, None);
    let features = [
        OrientedFeature::<3>::new(
            point(0),
            [axis, LocalAxis::new(1.0, None), LocalAxis::new(2.0, None)],
        ),
        OrientedFeature::<3>::new(
            point(1),
            [axis, LocalAxis::new(1.0, None), LocalAxis::new(2.0, None)],
        ),
    ];
    let request = DetectionRequest::new(
        LatticeKind::Hex,
        Evidence::Oriented3(&features),
        None,
        DetectionParams::default(),
    );
    assert_unsupported(request, EvidenceKind::Oriented3);
}

#[test]
fn coordinate_hypothesis_detection_is_typed_unsupported() {
    let features = [point(0), point(1), point(2), point(3)];
    let hypotheses = [
        CoordinateHypothesis::new(0, Coord::new(0, 0), None),
        CoordinateHypothesis::new(1, Coord::new(1, 0), None),
    ];
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::CoordinateHypotheses {
            features: &features,
            hypotheses: &hypotheses,
        },
        None,
        DetectionParams::default(),
    );
    assert_unsupported(request, EvidenceKind::CoordinateHypotheses);
}
