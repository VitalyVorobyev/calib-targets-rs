//! Phase E.0 adapter parity tests: legacy `projective_grid` vs new
//! `projective_grid_next` on the same synthetic `ChessCorner` fixtures.
//!
//! These tests are the precondition codex's review identified for Phase E
//! consumer migration: before any chessboard / charuco / puzzleboard
//! production path swaps from the legacy crate to the new one, the two
//! pipelines must agree on the inputs every consumer is actually feeding
//! them today.
//!
//! ## What we compare and why
//!
//! Each test builds a `Vec<ChessCorner>`, then runs **both** pipelines on
//! the same inputs and compares the labelled set as a `HashSet<usize>` of
//! source / `input_index` references. The two pipelines do not produce
//! byte-identical `(i, j)` maps:
//!
//! * `calib_targets_chessboard::Detector` applies a `D4`
//!   canonicalisation that orients the grid so `+i ≈ +x` and `+j ≈ +y`
//!   in image coordinates (see `pipeline/output.rs::canonicalize_orientation`).
//! * `projective_grid_next::detect_grid_all` only rebases to `(0, 0)`
//!   per component; the orientation comes from the seed quad / topological
//!   walker and is free to be any of the four `D4` rotations.
//!
//! Comparing label sets directly would fold both the algorithmic-parity
//! signal and the canonicalisation choice into one assertion. The
//! canonicalisation difference is intentional: production callers want
//! the chessboard orientation, the lower-level crate is target-agnostic
//! and stays uncommitted. The parity signal is "which corners get
//! labelled" — that's `HashSet<source_index>`.
//!
//! For multi-component tests (cases 4, 5) we compare per-component
//! `HashSet<source_index>` partitions against the legacy
//! `projective_grid::build_grid_topological` output directly (skipping
//! the chessboard's higher-level booster / merge stages so the parity
//! check is at the image-free layer where the two crates compete).

use std::collections::{BTreeSet, HashSet};

use calib_targets_chessboard::{ChessCorner, Detector, DetectorParams, GraphBuildAlgorithm};
use calib_targets_core::{axis_estimate_to_next, AxisEstimate};
use nalgebra::Point2;
use projective_grid::{
    build_grid_topological, AxisClusterCenters, TopologicalParams as LegacyTopo,
};
use projective_grid_next::{
    detect_grid_all, DetectionParams, DetectionRequest, Evidence, LatticeKind, OrientedFeature,
    PointFeature, SquareAlgorithm, TopologicalParams as NextTopo,
};

// ---------------------------------------------------------------------------
// Fixture builders
// ---------------------------------------------------------------------------

/// Build a synthetic `(rows × cols)` `ChessCorner` grid with cell spacing
/// `s` and origin `(origin_x, origin_y)`. Parity alternates so the
/// chessboard cluster gate has a clean two-axis split.
fn build_chess_grid(
    rows: i32,
    cols: i32,
    s: f32,
    origin_x: f32,
    origin_y: f32,
) -> Vec<ChessCorner> {
    let mut out = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let parity = ((i + j) % 2) as usize;
            let (a0, a1) = if parity == 0 {
                (0.0_f32, std::f32::consts::FRAC_PI_2)
            } else {
                (std::f32::consts::FRAC_PI_2, 0.0_f32)
            };
            out.push(ChessCorner {
                position: Point2::new(origin_x + i as f32 * s, origin_y + j as f32 * s),
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
                strength: 1.0,
            });
        }
    }
    out
}

/// Convert a `[ChessCorner]` slice into the new crate's
/// `OrientedFeature<f32, 2>` shape, using the slice index as
/// `source_index` so the two pipelines share a vocabulary for
/// labelled-corner comparisons.
fn to_next_features(corners: &[ChessCorner]) -> Vec<OrientedFeature<f32, 2>> {
    corners
        .iter()
        .enumerate()
        .map(|(idx, c)| {
            OrientedFeature::new(
                PointFeature::new(idx, c.position),
                [
                    axis_estimate_to_next(c.axes[0]),
                    axis_estimate_to_next(c.axes[1]),
                ],
            )
        })
        .collect()
}

/// Convert the same corner slice into the legacy
/// `projective_grid::build_grid_topological` input shape (positions +
/// per-corner axes). Position-slice indices line up with the new-crate
/// `source_index`.
fn to_legacy_topo_inputs(corners: &[ChessCorner]) -> (Vec<Point2<f32>>, Vec<[AxisEstimate; 2]>) {
    let positions: Vec<Point2<f32>> = corners.iter().map(|c| c.position).collect();
    let axes: Vec<[AxisEstimate; 2]> = corners.iter().map(|c| c.axes).collect();
    (positions, axes)
}

// ---------------------------------------------------------------------------
// Helpers for label-set comparison
// ---------------------------------------------------------------------------

fn next_labelled_source_indices(
    solutions: &[projective_grid_next::GridSolution<f32>],
) -> HashSet<usize> {
    solutions
        .iter()
        .flat_map(|s| s.grid.entries.iter().map(|e| e.source_index))
        .collect()
}

fn chess_labelled_input_indices(
    detection: &[calib_targets_chessboard::ChessboardDetection],
) -> HashSet<usize> {
    detection
        .iter()
        .flat_map(|d| d.corners.iter().map(|c| c.input_index))
        .collect()
}

fn next_component_sets(
    solutions: &[projective_grid_next::GridSolution<f32>],
) -> Vec<BTreeSet<usize>> {
    let mut out: Vec<BTreeSet<usize>> = solutions
        .iter()
        .map(|s| s.grid.entries.iter().map(|e| e.source_index).collect())
        .collect();
    // Order by descending size, then by smallest source index for
    // determinism — same convention `detect_grid_all` uses internally.
    out.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| a.iter().next().cmp(&b.iter().next()))
    });
    out
}

fn chess_detection_sets(
    detections: &[calib_targets_chessboard::ChessboardDetection],
) -> Vec<BTreeSet<usize>> {
    let mut out: Vec<BTreeSet<usize>> = detections
        .iter()
        .map(|d| d.corners.iter().map(|c| c.input_index).collect())
        .collect();
    out.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| a.iter().next().cmp(&b.iter().next()))
    });
    out
}

fn legacy_topo_component_sets(grid: &projective_grid::TopologicalGrid) -> Vec<BTreeSet<usize>> {
    let mut out: Vec<BTreeSet<usize>> = grid
        .components
        .iter()
        .map(|c| c.labelled.values().copied().collect())
        .collect();
    out.sort_by(|a, b| {
        b.len()
            .cmp(&a.len())
            .then_with(|| a.iter().next().cmp(&b.iter().next()))
    });
    out
}

// ---------------------------------------------------------------------------
// Case 1: seed-and-grow on a clean 8×8 grid
// ---------------------------------------------------------------------------

#[test]
fn seed_and_grow_clean_grid_matches_legacy() {
    // Single 8×8 board, no noise. Default chessboard params run
    // `GraphBuildAlgorithm::ChessboardV2` (seed-and-grow).
    let corners = build_chess_grid(8, 8, 20.0, 50.0, 50.0);
    let features = to_next_features(&corners);

    let chess_detector = Detector::new(DetectorParams::default());
    let chess_detections = chess_detector.detect_all(&corners);
    assert_eq!(
        chess_detections.len(),
        1,
        "expected single component on clean 8×8"
    );
    let chess_set = chess_labelled_input_indices(&chess_detections);

    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::<f32>::default(), // SquareAlgorithm::SeedAndGrow by default
    );
    let report = detect_grid_all(request).expect("next-crate seed-and-grow on clean 8×8");
    assert_eq!(
        report.solutions.len(),
        1,
        "seed-and-grow returns one component"
    );
    let next_set = next_labelled_source_indices(&report.solutions);

    assert_eq!(
        chess_set.len(),
        next_set.len(),
        "labelled count mismatch: legacy {}, next {}",
        chess_set.len(),
        next_set.len()
    );
    assert_eq!(
        chess_set, next_set,
        "labelled input/source index sets differ"
    );
    // Every grid corner labelled.
    assert_eq!(chess_set.len(), 64, "expected 64/64 corners labelled");
}

// ---------------------------------------------------------------------------
// Case 2: topological on a clean 8×8 grid (no cluster gate)
// ---------------------------------------------------------------------------

#[test]
fn topological_clean_grid_matches_legacy() {
    let corners = build_chess_grid(8, 8, 20.0, 50.0, 50.0);
    let features = to_next_features(&corners);
    let (positions, axes) = to_legacy_topo_inputs(&corners);

    let legacy = build_grid_topological(&positions, &axes, &LegacyTopo::default())
        .expect("legacy topological on clean 8×8");
    let legacy_sets = legacy_topo_component_sets(&legacy);

    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::<f32>::default().with_algorithm(SquareAlgorithm::Topological),
    );
    let report = detect_grid_all(request).expect("next topological on clean 8×8");
    let next_sets = next_component_sets(&report.solutions);

    assert_eq!(
        next_sets.len(),
        legacy_sets.len(),
        "component count: legacy {}, next {}",
        legacy_sets.len(),
        next_sets.len()
    );
    assert_eq!(
        next_sets, legacy_sets,
        "per-component source index sets differ"
    );
}

// ---------------------------------------------------------------------------
// Case 3: topological with the cluster gate on a noisy grid
// ---------------------------------------------------------------------------

#[test]
fn topological_cluster_gated_noisier_matches_legacy() {
    // 8×8 axis-aligned grid + 4 noise corners far from the grid bounding
    // box with axes ≈ 45° (i.e. clearly off the [0, π/2] cluster). The
    // gate at the legacy default tolerance (16°) must filter the
    // noisers before Delaunay in both pipelines.
    let mut corners = build_chess_grid(8, 8, 20.0, 50.0, 50.0);
    let extra: [(f32, f32); 4] = [
        (-100.0, -100.0),
        (500.0, -100.0),
        (-100.0, 500.0),
        (500.0, 500.0),
    ];
    for (x, y) in extra.iter() {
        let off_axis = std::f32::consts::FRAC_PI_4;
        corners.push(ChessCorner {
            position: Point2::new(*x, *y),
            axes: [
                AxisEstimate {
                    angle: off_axis,
                    sigma: 0.05,
                },
                AxisEstimate {
                    angle: off_axis + std::f32::consts::FRAC_PI_2,
                    sigma: 0.05,
                },
            ],
            contrast: 20.0,
            fit_rms: 1.0,
            strength: 1.0,
        });
    }
    let noise_ids: HashSet<usize> = (corners.len() - 4..corners.len()).collect();

    let features = to_next_features(&corners);
    let (positions, axes) = to_legacy_topo_inputs(&corners);

    // Legacy default-on cluster gate with the same centers + 16° tol the
    // chessboard topological adapter wires in today. `LegacyTopo` is
    // `#[non_exhaustive]` so we mutate a default-constructed value.
    let mut legacy_params = LegacyTopo::default();
    legacy_params.axis_cluster_centers =
        Some(AxisClusterCenters::new(0.0, std::f32::consts::FRAC_PI_2));
    let legacy = build_grid_topological(&positions, &axes, &legacy_params)
        .expect("legacy topological with gate");
    let legacy_sets = legacy_topo_component_sets(&legacy);

    let next_params = DetectionParams::<f32>::default()
        .with_algorithm(SquareAlgorithm::Topological)
        .with_topological(
            NextTopo::<f32>::default()
                .with_axis_cluster_centers([0.0, std::f32::consts::FRAC_PI_2]),
        );
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        next_params,
    );
    let report = detect_grid_all(request).expect("next topological with gate");
    let next_sets = next_component_sets(&report.solutions);

    assert_eq!(
        next_sets.len(),
        legacy_sets.len(),
        "component count: legacy {}, next {}",
        legacy_sets.len(),
        next_sets.len()
    );
    assert_eq!(
        next_sets, legacy_sets,
        "per-component source index sets differ"
    );

    // The cluster gate must scrub the noisers from every labelled
    // component in both pipelines.
    for set in &next_sets {
        for nid in &noise_ids {
            assert!(
                !set.contains(nid),
                "noise corner {nid} leaked into labelled component {set:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Case 4: multi-component chessboard matches legacy detect_all
// ---------------------------------------------------------------------------

#[test]
fn multi_component_chessboard_matches_legacy_detect_all() {
    // Two 4×4 boards with a horizontal gap larger than 3× cell size so
    // the attach-search window cannot bridge them. Both pipelines must
    // return two components of size 16 each.
    let spacing = 20.0_f32;
    let mut corners = build_chess_grid(4, 4, spacing, 50.0, 50.0);
    let left_count = corners.len();
    corners.extend(build_chess_grid(4, 4, spacing, 300.0, 50.0));

    let features = to_next_features(&corners);
    let (positions, axes) = to_legacy_topo_inputs(&corners);

    // Legacy topological direct (no chessboard post-stages): expected to
    // return two disconnected quad-mesh components.
    let legacy = build_grid_topological(&positions, &axes, &LegacyTopo::default())
        .expect("legacy topological on two boards");
    let legacy_sets = legacy_topo_component_sets(&legacy);

    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::<f32>::default().with_algorithm(SquareAlgorithm::Topological),
    );
    let report = detect_grid_all(request).expect("next topological on two boards");
    let next_sets = next_component_sets(&report.solutions);

    assert_eq!(
        next_sets.len(),
        2,
        "next: expected 2 components, got {}",
        next_sets.len()
    );
    assert_eq!(
        legacy_sets.len(),
        2,
        "legacy: expected 2 components, got {}",
        legacy_sets.len()
    );
    assert_eq!(
        next_sets, legacy_sets,
        "per-component source index sets differ"
    );

    let left_set: BTreeSet<usize> = (0..left_count).collect();
    let right_set: BTreeSet<usize> = (left_count..corners.len()).collect();
    let next_unordered: HashSet<BTreeSet<usize>> = next_sets.iter().cloned().collect();
    assert!(next_unordered.contains(&left_set), "left piece missing");
    assert!(next_unordered.contains(&right_set), "right piece missing");

    // Cross-check against the chessboard's higher-level `detect_all` on
    // the topological algorithm — same physical pieces should appear
    // (post-canonicalisation, post-recovery), again at the source-index
    // set level. This validates that the chessboard's per-component
    // recovery stages preserve the partition the topological crate
    // produces.
    let mut topo_params = DetectorParams::default();
    topo_params.graph_build_algorithm = GraphBuildAlgorithm::Topological;
    let chess_detector = Detector::new(topo_params);
    let chess_detections = chess_detector.detect_all(&corners);
    let chess_sets = chess_detection_sets(&chess_detections);
    assert_eq!(
        chess_sets.len(),
        2,
        "chessboard detect_all (topological): expected 2 components, got {}",
        chess_sets.len()
    );
    assert_eq!(
        chess_sets, next_sets,
        "chessboard detect_all (topological) partition differs from next-crate"
    );
}

// ---------------------------------------------------------------------------
// Case 5: puzzleboard component ranking — by labelled count descending
// ---------------------------------------------------------------------------

#[test]
fn puzzleboard_component_ranking_matches_legacy() {
    // Three disjoint boards of strictly different sizes (3×3, 4×4, 5×5).
    // The puzzleboard decoder's `search_all_components` ranking is by
    // labelled-count descending; the partition shapes must match between
    // legacy and next at that ranking.
    let spacing = 20.0_f32;
    let mut corners = build_chess_grid(3, 3, spacing, 50.0, 50.0);
    corners.extend(build_chess_grid(5, 5, spacing, 250.0, 50.0));
    corners.extend(build_chess_grid(4, 4, spacing, 50.0, 250.0));

    let features = to_next_features(&corners);
    let (positions, axes) = to_legacy_topo_inputs(&corners);

    let legacy = build_grid_topological(&positions, &axes, &LegacyTopo::default())
        .expect("legacy topological on three boards");
    let legacy_sets = legacy_topo_component_sets(&legacy);

    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::<f32>::default().with_algorithm(SquareAlgorithm::Topological),
    );
    let report = detect_grid_all(request).expect("next topological on three boards");
    let next_sets = next_component_sets(&report.solutions);

    assert_eq!(
        next_sets.len(),
        legacy_sets.len(),
        "component count: legacy {}, next {}",
        legacy_sets.len(),
        next_sets.len()
    );
    assert_eq!(
        next_sets, legacy_sets,
        "per-component source index sets differ"
    );

    // The ranking is by labelled count descending; with 3×3, 4×4, 5×5
    // (= 9, 16, 25 corners) the order must be 25, 16, 9 in *both*
    // pipelines.
    let next_sizes: Vec<usize> = next_sets.iter().map(|s| s.len()).collect();
    assert_eq!(next_sizes, vec![25, 16, 9], "ranking by size descending");
}

// ---------------------------------------------------------------------------
// Case 6: ChArUco pins to seed-and-grow — next path is faithful
// ---------------------------------------------------------------------------

#[test]
fn charuco_pin_to_seed_and_grow() {
    // A small chessboard-like fixture (5×5) which the ChArUco pipeline's
    // unconditional override drives through seed-and-grow regardless of
    // caller selection. The parity check verifies the next-crate
    // seed-and-grow path agrees with the chessboard's chessboard-v2
    // entry on this image-free fixture.
    let corners = build_chess_grid(5, 5, 20.0, 50.0, 50.0);
    let features = to_next_features(&corners);

    let chess_detector = Detector::new(DetectorParams::default()); // ChessboardV2 default
    let chess_detections = chess_detector.detect_all(&corners);
    assert_eq!(chess_detections.len(), 1, "single component on clean 5×5");
    let chess_set = chess_labelled_input_indices(&chess_detections);

    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::<f32>::default(), // SquareAlgorithm::SeedAndGrow default
    );
    let report = detect_grid_all(request).expect("next seed-and-grow on charuco-pin fixture");
    assert_eq!(report.solutions.len(), 1, "seed-and-grow single component");
    let next_set = next_labelled_source_indices(&report.solutions);

    assert_eq!(
        chess_set, next_set,
        "ChArUco-pin seed-and-grow parity broken"
    );
    assert_eq!(chess_set.len(), 25, "expected all 25 corners labelled");
}
