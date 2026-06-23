//! End-to-end integration tests for `detect_grid` with
//! `(LatticeKind::Square, Evidence::Positions)` — the orientation-free path.
//!
//! These exercise the position-only entry: per-corner local grid directions are
//! synthesized from neighbour geometry, then the topological square assembler
//! runs with the geometry-only recovery schedule enabled (the `Auto`-on
//! synthesized-axis path). Inputs are synthetic and target-agnostic, including a
//! genuine perspective warp (the two grid directions are non-orthogonal and
//! vary across the image).
//!
//! The precision contract is the headline assertion: **zero wrong `(i, j)`
//! labels**. A perfect synthetic grid must also be *fully* recovered.

use std::collections::{HashMap, HashSet};

use nalgebra::{Matrix3, Point2, Vector3};
use projective_grid::{
    detect_grid, Coord, DetectionParams, DetectionRequest, Evidence, LatticeKind, PointFeature,
};

/// Build position-only features for an axis-aligned `rows × cols` grid.
fn grid_positions(rows: i32, cols: i32, s: f32, origin: f32) -> Vec<PointFeature> {
    let mut out = Vec::with_capacity((rows * cols) as usize);
    let mut idx = 0usize;
    for j in 0..rows {
        for i in 0..cols {
            out.push(PointFeature::new(
                idx,
                Point2::new(i as f32 * s + origin, j as f32 * s + origin),
            ));
            idx += 1;
        }
    }
    out
}

/// Project `(rows × cols)` grid points through a homography. Returns the
/// features plus the ground-truth `source_index → (i, j)` map.
fn perspective_grid(
    rows: i32,
    cols: i32,
    s: f32,
    origin: f32,
    h: &Matrix3<f32>,
) -> (Vec<PointFeature>, HashMap<usize, (i32, i32)>) {
    let mut feats = Vec::with_capacity((rows * cols) as usize);
    let mut truth = HashMap::new();
    let mut idx = 0usize;
    for j in 0..rows {
        for i in 0..cols {
            let g = Vector3::new(i as f32 * s + origin, j as f32 * s + origin, 1.0);
            let p = h * g;
            feats.push(PointFeature::new(idx, Point2::new(p.x / p.z, p.y / p.z)));
            truth.insert(idx, (i, j));
            idx += 1;
        }
    }
    (feats, truth)
}

fn request(features: &[PointFeature]) -> DetectionRequest<'_> {
    DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Positions(features),
        None,
        DetectionParams::default(),
    )
}

/// A detection's labels are *precision-correct* against ground truth iff there
/// exists a single lattice automorphism (one of the 8 D4 maps composed with a
/// translation) taking detected `(i, j)` to truth `(i, j)` for every labelled
/// corner. We don't know the orientation the detector picked, so we recover the
/// affine integer map from one labelled edge and verify it holds for all.
fn assert_labels_consistent_with_truth(
    entries: &[(usize, Coord)],
    truth: &HashMap<usize, (i32, i32)>,
    ctx: &str,
) {
    assert!(
        entries.len() >= 4,
        "{ctx}: too few labelled corners ({})",
        entries.len()
    );
    // Map detected coord -> truth coord for each labelled source.
    let pairs: Vec<((i32, i32), (i32, i32))> = entries
        .iter()
        .map(|(src, c)| ((c.u, c.v), truth[src]))
        .collect();

    // Fit truth = M * det + t over integers, where M is one of the 8 D4
    // matrices. Try each; require an exact fit for ALL pairs under one (M, t).
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
        let (du0, dv0) = pairs[0].0;
        let (tu0, tv0) = pairs[0].1;
        let (mu0, mv0) = apply(m, (du0, dv0));
        let t = (tu0 - mu0, tv0 - mv0);
        pairs.iter().all(|(d, truth_c)| {
            let (mu, mv) = apply(m, *d);
            (mu + t.0, mv + t.1) == *truth_c
        })
    });
    assert!(
        found,
        "{ctx}: labels are NOT a consistent lattice map of ground truth — \
         a wrong (i,j) label slipped in (precision contract violation)"
    );
}

fn entries_with_truth(sol: &projective_grid::GridSolution) -> Vec<(usize, Coord)> {
    sol.grid
        .entries
        .iter()
        .map(|e| (e.source_index, e.coord))
        .collect()
}

#[test]
fn perfect_grid_topological_fully_recovered_zero_wrong() {
    let feats = grid_positions(7, 7, 24.0, 60.0);
    let truth: HashMap<usize, (i32, i32)> = feats
        .iter()
        .enumerate()
        .map(|(idx, _)| (idx, ((idx as i32) % 7, (idx as i32) / 7)))
        .collect();
    let sol = detect_grid(request(&feats)).expect("topological on perfect 7x7 position grid");
    // Topological recovers the full quad-mesh interior; with the recovery
    // schedule on the synthesized-axis path the boundary fills in too. The
    // contract is zero wrong labels, not 100% recall.
    assert!(
        sol.grid.entries.len() >= 36,
        "topological recovered only {}/49",
        sol.grid.entries.len()
    );
    assert_labels_consistent_with_truth(&entries_with_truth(&sol), &truth, "perfect Topological");
}

/// Determinism (enabled): the orientation-free (`Evidence::Positions`)
/// topological path must produce byte-identical output across repeated runs on
/// the same input. The topological walk, the local component merge, and the
/// recovery schedule's extension / fill / drop filters all break
/// HashMap-iteration / kiddo `within_unsorted` ties by stable corner index /
/// sorted coords, so the run-to-run swing (formerly ~4 vs ~22 of 64 from
/// identical input) is gone.
#[test]
fn perspective_grid_positions_is_deterministic() {
    // Real perspective term: grid directions are non-orthogonal and drift.
    let h = Matrix3::new(
        1.0, 0.18, 0.0, //
        0.0, 1.0, 0.0, //
        0.0011, 0.0007, 1.0,
    );
    let (feats, _truth) = perspective_grid(8, 8, 28.0, 50.0, &h);

    let signature = |sol: &projective_grid::GridSolution| -> Vec<(usize, i32, i32)> {
        let mut sig: Vec<(usize, i32, i32)> = sol
            .grid
            .entries
            .iter()
            .map(|e| (e.source_index, e.coord.u, e.coord.v))
            .collect();
        sig.sort_unstable();
        sig
    };

    let first = signature(&detect_grid(request(&feats)).expect("topological on perspective grid"));
    for run in 1..10 {
        let again =
            signature(&detect_grid(request(&feats)).expect("topological on perspective grid"));
        assert_eq!(
            first, again,
            "positions topological output differs on run {run} (non-determinism)"
        );
    }
}

#[test]
fn perspective_grid_topological_zero_wrong_labels() {
    let h = Matrix3::new(
        1.0, 0.18, 0.0, //
        0.0, 1.0, 0.0, //
        0.0011, 0.0007, 1.0,
    );
    let (feats, truth) = perspective_grid(8, 8, 28.0, 50.0, &h);
    let sol = detect_grid(request(&feats)).expect("topological on perspective grid");
    assert!(
        sol.grid.entries.len() >= 48,
        "recovered only {}/64 under perspective",
        sol.grid.entries.len()
    );
    assert_labels_consistent_with_truth(
        &entries_with_truth(&sol),
        &truth,
        "perspective Topological",
    );
}

#[test]
fn outliers_do_not_corrupt_labels() {
    // A clean 6x6 grid plus a handful of off-grid noise points. The grid must
    // still be recovered with zero wrong labels; noise points may be dropped.
    // Grid spans (60, 60)..(185, 185) at pitch 25. Outliers sit well outside
    // it (>1 pitch from any grid corner) — realistic sparse spurious
    // detections, not points crowding the lattice.
    let mut feats = grid_positions(6, 6, 25.0, 60.0);
    let truth: HashMap<usize, (i32, i32)> = (0..36)
        .map(|idx| (idx, ((idx as i32) % 6, (idx as i32) / 6)))
        .collect();
    let base = feats.len();
    for (k, (x, y)) in [(20.0, 20.0), (230.0, 22.0), (18.0, 235.0), (235.0, 232.0)]
        .into_iter()
        .enumerate()
    {
        feats.push(PointFeature::new(base + k, Point2::new(x, y)));
    }
    let sol = detect_grid(request(&feats)).expect("topological with outliers");
    // Keep only labelled corners that belong to the true grid for the
    // consistency check; the contract is "no wrong label on a grid corner".
    let grid_entries: Vec<(usize, Coord)> = entries_with_truth(&sol)
        .into_iter()
        .filter(|(src, _)| truth.contains_key(src))
        .collect();
    assert!(
        grid_entries.len() >= 30,
        "recovered only {}/36 grid corners with outliers present",
        grid_entries.len()
    );
    assert_labels_consistent_with_truth(&grid_entries, &truth, "outliers Topological");
}

/// Sanity: the synthesized-axis position-only path recovers a clean grid fully.
#[test]
fn position_only_matches_oriented_on_clean_grid() {
    let feats = grid_positions(6, 6, 25.0, 60.0);
    let pos_sol = detect_grid(request(&feats)).expect("position-only detect");
    let pos_labels: HashSet<usize> = pos_sol
        .grid
        .entries
        .iter()
        .map(|e| e.source_index)
        .collect();
    assert_eq!(
        pos_labels.len(),
        36,
        "position-only path recovered {}/36 on a clean grid",
        pos_labels.len()
    );
}
