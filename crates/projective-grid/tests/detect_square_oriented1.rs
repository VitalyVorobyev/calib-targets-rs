//! End-to-end integration tests for `detect_grid` with
//! `(LatticeKind::Square, Evidence::Oriented1)` — the single-axis path.
//!
//! Each feature supplies one trusted local grid direction; the orthogonal
//! direction is synthesized from neighbour geometry, and the resulting
//! oriented-2 features run the chosen square strategy.
//!
//! The headline assertion is the precision contract: **zero wrong `(i, j)`
//! labels** — the recovered labelling is always a consistent lattice
//! automorphism of ground truth, so a single-axis input never produces a
//! mislabelled corner.
//!
//! Recovery *recall* of the synthesized path is lower than feeding true
//! two-axis evidence under strong perspective (the synthesized second axis is
//! weaker at boundaries where the nearest-neighbour set pulls in a diagonal).
//! That is the same gap the deferred positions recall test documents; full
//! orientation-free recall parity is the Phase 3 goal. These tests therefore
//! assert a recall *floor* plus the zero-wrong-label contract, not identical
//! recall counts.

use std::collections::HashMap;

use nalgebra::{Matrix3, Point2, Vector3};
use projective_grid::{
    detect_grid, Coord, DetectionParams, DetectionRequest, Evidence, LatticeKind, LocalAxis,
    OrientedFeature, PointFeature, SquareAlgorithm,
};

/// Ground-truth `source_index → (i, j)` map.
type Truth = HashMap<usize, (i32, i32)>;

/// Project a `rows × cols` grid through a homography. Returns the projected
/// positions plus the ground-truth `source_index → (i, j)` map.
fn perspective_grid(
    rows: i32,
    cols: i32,
    s: f32,
    origin: f32,
    h: &Matrix3<f32>,
) -> (Vec<Point2<f32>>, Truth) {
    let mut pts = Vec::new();
    let mut truth = HashMap::new();
    let mut idx = 0usize;
    for j in 0..rows {
        for i in 0..cols {
            let g = Vector3::new(i as f32 * s + origin, j as f32 * s + origin, 1.0);
            let p = h * g;
            pts.push(Point2::new(p.x / p.z, p.y / p.z));
            truth.insert(idx, (i, j));
            idx += 1;
        }
    }
    (pts, truth)
}

fn fold_pi(theta: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let mut t = theta % pi;
    if t < 0.0 {
        t += pi;
    }
    t
}

/// True local `+u` direction at flat index `flat` (toward the `(i+1, j)`
/// neighbour, or `(i-1, j)` at the right boundary), folded to `[0, π)`.
fn local_u_axis(pts: &[Point2<f32>], cols: usize, i: usize, j: usize) -> f32 {
    let here = pts[j * cols + i];
    let nb = if i + 1 < cols {
        pts[j * cols + i + 1]
    } else {
        pts[j * cols + i - 1]
    };
    fold_pi((nb.y - here.y).atan2(nb.x - here.x))
}

/// Build Oriented1 features (supply the true +u axis) and the matching
/// Oriented2 features (supply both true axes) for the same point cloud.
fn build_features(
    pts: &[Point2<f32>],
    cols: usize,
    rows: usize,
) -> (Vec<OrientedFeature<1>>, Vec<OrientedFeature<2>>) {
    let mut o1 = Vec::with_capacity(pts.len());
    let mut o2 = Vec::with_capacity(pts.len());
    for j in 0..rows {
        for i in 0..cols {
            let flat = j * cols + i;
            let point = PointFeature::new(flat, pts[flat]);
            let u = local_u_axis(pts, cols, i, j);
            // v axis toward (i, j+1) neighbour, or (i, j-1) at the bottom.
            let here = pts[flat];
            let vnb = if j + 1 < rows {
                pts[(j + 1) * cols + i]
            } else {
                pts[(j - 1) * cols + i]
            };
            let v = fold_pi((vnb.y - here.y).atan2(vnb.x - here.x));
            o1.push(OrientedFeature::<1>::new(point, [LocalAxis::new(u, None)]));
            o2.push(OrientedFeature::<2>::new(
                point,
                [LocalAxis::new(u, None), LocalAxis::new(v, None)],
            ));
        }
    }
    (o1, o2)
}

/// Verify labels are a consistent lattice automorphism of ground truth (no
/// wrong `(i, j)` slipped in). Mirrors the positions-path checker.
fn assert_labels_consistent_with_truth(entries: &[(usize, Coord)], truth: &Truth, ctx: &str) {
    assert!(entries.len() >= 4, "{ctx}: too few labelled corners");
    let pairs: Vec<((i32, i32), (i32, i32))> = entries
        .iter()
        .map(|(src, c)| ((c.u, c.v), truth[src]))
        .collect();
    const D4: [[[i32; 2]; 2]; 8] = [
        [[1, 0], [0, 1]],
        [[0, -1], [1, 0]],
        [[-1, 0], [0, -1]],
        [[0, 1], [-1, 0]],
        [[-1, 0], [0, 1]],
        [[1, 0], [0, -1]],
        [[0, 1], [1, 0]],
        [[0, -1], [-1, 0]],
    ];
    let apply = |m: &[[i32; 2]; 2], (u, v): (i32, i32)| {
        (m[0][0] * u + m[0][1] * v, m[1][0] * u + m[1][1] * v)
    };
    let found = D4.iter().any(|m| {
        let (mu0, mv0) = apply(m, pairs[0].0);
        let t = (pairs[0].1 .0 - mu0, pairs[0].1 .1 - mv0);
        pairs.iter().all(|(d, tc)| {
            let (mu, mv) = apply(m, *d);
            (mu + t.0, mv + t.1) == *tc
        })
    });
    assert!(
        found,
        "{ctx}: labels are NOT a consistent lattice map of ground truth"
    );
}

fn entries(sol: &projective_grid::GridSolution) -> Vec<(usize, Coord)> {
    sol.grid
        .entries
        .iter()
        .map(|e| (e.source_index, e.coord))
        .collect()
}

fn h_perspective() -> Matrix3<f32> {
    Matrix3::new(
        1.0, 0.16, 0.0, //
        0.05, 1.0, 0.0, //
        0.0011, 0.0007, 1.0,
    )
}

fn detect_o1(feats: &[OrientedFeature<1>], algo: SquareAlgorithm) -> projective_grid::GridSolution {
    detect_grid(DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented1(feats),
        None,
        DetectionParams::default().with_algorithm(algo),
    ))
    .expect("oriented1 detect")
}

fn detect_o2(feats: &[OrientedFeature<2>], algo: SquareAlgorithm) -> projective_grid::GridSolution {
    detect_grid(DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(feats),
        None,
        DetectionParams::default().with_algorithm(algo),
    ))
    .expect("oriented2 detect")
}

#[test]
fn oriented1_topological_parity_and_zero_wrong() {
    let (pts, truth) = perspective_grid(8, 8, 28.0, 50.0, &h_perspective());
    let (o1, o2) = build_features(&pts, 8, 8);

    let sol1 = detect_o1(&o1, SquareAlgorithm::Topological);
    let sol2 = detect_o2(&o2, SquareAlgorithm::Topological);

    assert_labels_consistent_with_truth(&entries(&sol1), &truth, "oriented1 topological");
    assert_labels_consistent_with_truth(&entries(&sol2), &truth, "oriented2 topological");

    // Recall floor: the single-axis topological path recovers the large
    // majority of the grid. The two-axis path is the upper bound; the
    // synthesized second axis is weaker at boundaries (Phase 3 closes the
    // remaining gap). Zero wrong labels is the hard contract (asserted above).
    let n1 = sol1.grid.entries.len();
    let n2 = sol2.grid.entries.len();
    assert!(n2 >= n1, "oriented2 should be the upper bound: {n2} < {n1}");
    // Phase 3 wired the synthesized-axis topological path through the post-merge
    // recovery schedule; it now recovers the full grid (measured 64/64). Floor
    // set to measured-minus-margin (60).
    assert!(
        n1 >= 60,
        "oriented1 topological recovered only {n1}/64 (Phase-3 recall floor 60)"
    );
}

#[test]
fn oriented1_seed_and_grow_parity_and_zero_wrong() {
    let (pts, truth) = perspective_grid(8, 8, 28.0, 50.0, &h_perspective());
    let (o1, o2) = build_features(&pts, 8, 8);

    let sol1 = detect_o1(&o1, SquareAlgorithm::SeedAndGrow);
    let sol2 = detect_o2(&o2, SquareAlgorithm::SeedAndGrow);

    assert_labels_consistent_with_truth(&entries(&sol1), &truth, "oriented1 seed-and-grow");
    assert_labels_consistent_with_truth(&entries(&sol2), &truth, "oriented2 seed-and-grow");

    let n1 = sol1.grid.entries.len();
    let n2 = sol2.grid.entries.len();
    assert!(n2 >= n1, "oriented2 should be the upper bound: {n2} < {n1}");
    // Phase 3: the seed-and-grow synthesized-axis path now runs the
    // `PositionsAttachPolicy` + recovery schedule, closing the foreshortening
    // gap — it recovers the full grid (measured 64/64). Floor set to
    // measured-minus-margin (60).
    assert!(
        n1 >= 60,
        "oriented1 seed-and-grow recovered only {n1}/64 (Phase-3 recall floor 60)"
    );
}
