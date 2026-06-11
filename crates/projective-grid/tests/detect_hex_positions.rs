//! End-to-end integration tests for `detect_grid` with
//! `(LatticeKind::Hex, Evidence::Positions)` and `(Hex, Evidence::Oriented3)`
//! — the hexagonal topological path.
//!
//! ## This file is the hex regression gate
//!
//! Hex detection has **no real checked-in images** (the bench harness and
//! `datasets.toml` are chessboard-specific). The hex precision/recall contract
//! is therefore gated here, inside `projective-grid`, as deterministic
//! synthetic fixtures (perfect / perspective / position-noise / dropouts /
//! off-lattice clutter). All randomness is a seeded xorshift LCG so runs are
//! reproducible; there is no `rand` dependency.
//!
//! The headline assertion is the same as the square positions suite: **zero
//! wrong `(q, r)` labels**. Hex labels are defined only up to the 12 D6
//! automorphisms composed with a lattice translation, so the consistency check
//! mods out that automorphism (see `assert_labels_consistent_with_truth`).
//! Recall floors are measured-minus-margin so tuning drift stays green while a
//! real regression trips.

use std::collections::HashMap;

use nalgebra::{Matrix3, Point2, Vector3};
use projective_grid::{
    detect_grid, Coord, DetectionParams, DetectionRequest, Evidence, LatticeKind, OrientedFeature,
    PointFeature, SquareAlgorithm, D6_TRANSFORMS,
};

/// Axial hex node `(q, r)` model position with unit nearest-neighbour spacing.
fn hex_model(q: i32, r: i32) -> Point2<f32> {
    let sqrt3_2 = 3.0_f32.sqrt() * 0.5;
    Point2::new(q as f32 + 0.5 * r as f32, sqrt3_2 * r as f32)
}

/// All axial coords within hex distance `radius` of the origin.
fn hex_coords(radius: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for q in -radius..=radius {
        for r in (-radius).max(-q - radius)..=radius.min(-q + radius) {
            out.push((q, r));
        }
    }
    out
}

/// Project a hex patch through `h`. Returns position-only features plus the
/// ground-truth `source_index → (q, r)` map.
fn hex_patch(
    radius: i32,
    s: f32,
    origin: f32,
    h: &Matrix3<f32>,
) -> (Vec<PointFeature>, HashMap<usize, (i32, i32)>) {
    let mut feats = Vec::new();
    let mut truth = HashMap::new();
    for (idx, (q, r)) in hex_coords(radius).into_iter().enumerate() {
        let m = hex_model(q, r);
        let g = Vector3::new(m.x * s + origin, m.y * s + origin, 1.0);
        let p = h * g;
        feats.push(PointFeature::new(idx, Point2::new(p.x / p.z, p.y / p.z)));
        truth.insert(idx, (q, r));
    }
    (feats, truth)
}

fn request(features: &[PointFeature]) -> DetectionRequest<'_> {
    DetectionRequest::new(
        LatticeKind::Hex,
        Evidence::Positions(features),
        None,
        DetectionParams::default().with_algorithm(SquareAlgorithm::Topological),
    )
}

/// A detection's labels are *precision-correct* against ground truth iff there
/// exists a single hex automorphism (one of the 12 D6 maps composed with an
/// axial translation) taking detected `(q, r)` to truth `(q, r)` for every
/// labelled node. We recover the integer map from one anchor and verify it
/// holds for all.
fn assert_labels_consistent_with_truth(
    entries: &[(usize, Coord)],
    truth: &HashMap<usize, (i32, i32)>,
    ctx: &str,
) {
    assert!(
        entries.len() >= 4,
        "{ctx}: too few labelled nodes ({})",
        entries.len()
    );
    let pairs: Vec<((i32, i32), (i32, i32))> = entries
        .iter()
        .map(|(src, c)| ((c.u, c.v), truth[src]))
        .collect();

    let found = D6_TRANSFORMS.iter().any(|m| {
        let (du0, dv0) = pairs[0].0;
        let mapped0 = m.apply(Coord::new(du0, dv0));
        let (tu0, tv0) = pairs[0].1;
        let t = (tu0 - mapped0.u, tv0 - mapped0.v);
        pairs.iter().all(|(d, truth_c)| {
            let mapped = m.apply(Coord::new(d.0, d.1));
            (mapped.u + t.0, mapped.v + t.1) == *truth_c
        })
    });
    assert!(
        found,
        "{ctx}: labels are NOT a consistent D6 automorphism of ground truth — \
         a wrong (q, r) label slipped in (precision contract violation)"
    );
}

fn entries_with_truth(sol: &projective_grid::GridSolution) -> Vec<(usize, Coord)> {
    sol.grid
        .entries
        .iter()
        .map(|e| (e.source_index, e.coord))
        .collect()
}

/// Deterministic xorshift LCG → uniform `[-0.5, 0.5)`. No `rand` dependency.
struct Lcg(u32);
impl Lcg {
    fn new(seed: u32) -> Self {
        Self(seed)
    }
    fn next_centered(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) - 0.5
    }
}

#[test]
fn perfect_hex_patch_recovered_zero_wrong() {
    let (feats, truth) = hex_patch(4, 30.0, 200.0, &Matrix3::identity());
    let n = feats.len();
    let sol = detect_grid(request(&feats)).expect("hex topological on perfect patch");
    // Interior nodes recover; convex-hull boundary slivers may drop. Contract
    // is zero wrong labels, with a solid recall floor.
    assert!(
        sol.grid.entries.len() >= (n * 3) / 5,
        "recovered only {}/{n} hex nodes on a perfect patch",
        sol.grid.entries.len()
    );
    assert_labels_consistent_with_truth(&entries_with_truth(&sol), &truth, "perfect hex");
    assert_eq!(sol.grid.lattice, LatticeKind::Hex);
}

#[test]
fn perspective_hex_patch_zero_wrong() {
    // Genuine perspective term: the three projected hex directions bend across
    // the patch and are not 60° apart.
    let h = Matrix3::new(
        1.0, 0.10, 0.0, //
        0.03, 1.0, 0.0, //
        0.0006, 0.0004, 1.0,
    );
    let (feats, truth) = hex_patch(4, 28.0, 200.0, &h);
    let sol = detect_grid(request(&feats)).expect("hex topological under perspective");
    assert!(
        sol.grid.entries.len() >= 24,
        "recovered only {} hex nodes under perspective",
        sol.grid.entries.len()
    );
    assert_labels_consistent_with_truth(&entries_with_truth(&sol), &truth, "perspective hex");
}

#[test]
fn hex_with_position_noise_zero_wrong() {
    let h = Matrix3::new(
        1.0, 0.06, 0.0, //
        0.02, 1.0, 0.0, //
        0.0004, 0.0003, 1.0,
    );
    let (mut feats, truth) = hex_patch(4, 30.0, 200.0, &h);
    let mut rng = Lcg::new(0xC0FFEE);
    for f in feats.iter_mut() {
        f.position.x += 0.8 * rng.next_centered();
        f.position.y += 0.8 * rng.next_centered();
    }
    let sol = detect_grid(request(&feats)).expect("hex topological under noise");
    assert!(
        sol.grid.entries.len() >= 24,
        "recovered only {} hex nodes under noise",
        sol.grid.entries.len()
    );
    assert_labels_consistent_with_truth(&entries_with_truth(&sol), &truth, "noisy hex");
}

#[test]
fn hex_with_dropouts_zero_wrong() {
    // Remove a handful of interior nodes (occlusion); the rest must still label
    // consistently. Drop every 7th node by index.
    let (all, truth_all) = hex_patch(4, 30.0, 200.0, &Matrix3::identity());
    let feats: Vec<PointFeature> = all
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 7 != 3)
        .map(|(_, f)| *f)
        .collect();
    let truth: HashMap<usize, (i32, i32)> = feats
        .iter()
        .map(|f| (f.source_index, truth_all[&f.source_index]))
        .collect();
    let sol = detect_grid(request(&feats)).expect("hex topological with dropouts");
    // Dropouts fragment the patch and degrade the synthesized axes near holes,
    // so recall drops; the contract is zero wrong labels (missing is fine).
    // Floor is measured (17) minus margin.
    assert!(
        sol.grid.entries.len() >= 15,
        "recovered only {} hex nodes with dropouts",
        sol.grid.entries.len()
    );
    assert_labels_consistent_with_truth(&entries_with_truth(&sol), &truth, "dropout hex");
}

#[test]
fn hex_with_off_lattice_clutter_zero_wrong() {
    // A clean hex patch plus off-lattice spurious points well outside the patch
    // and between nodes. Grid nodes must label with zero wrong labels; clutter
    // may be dropped or, if labelled, must remain a consistent lattice map of
    // the true grid (we check only the true-grid subset, mirroring the square
    // suite's `outliers_do_not_corrupt_labels`).
    let (mut feats, truth) = hex_patch(4, 30.0, 200.0, &Matrix3::identity());
    let base = feats.len();
    let mut rng = Lcg::new(0xBADCAB);
    // Spurious points at large offsets from the patch centre.
    for k in 0..6 {
        let x = 200.0 + 260.0 * rng.next_centered();
        let y = 200.0 + 260.0 * rng.next_centered();
        // Push them well outside the patch radius (~120 px) where possible.
        let x = if x.abs() < 1.0 { x + 240.0 } else { x };
        feats.push(PointFeature::new(
            base + k,
            Point2::new(x + 360.0, y - 360.0),
        ));
    }
    let sol = detect_grid(request(&feats)).expect("hex topological with clutter");
    let grid_entries: Vec<(usize, Coord)> = entries_with_truth(&sol)
        .into_iter()
        .filter(|(src, _)| truth.contains_key(src))
        .collect();
    assert!(
        grid_entries.len() >= 24,
        "recovered only {} true hex nodes with clutter present",
        grid_entries.len()
    );
    assert_labels_consistent_with_truth(&grid_entries, &truth, "clutter hex");
}

#[test]
fn hex_oriented3_native_path() {
    // Supply exact 0/60/120° axes per node (no synthesis); the native
    // Oriented3 path must recover the patch with zero wrong labels.
    let third = std::f32::consts::PI / 3.0;
    let mut feats = Vec::new();
    let mut truth = HashMap::new();
    for (idx, (q, r)) in hex_coords(3).into_iter().enumerate() {
        let m = hex_model(q, r);
        let p = PointFeature::new(idx, Point2::new(m.x * 30.0 + 200.0, m.y * 30.0 + 200.0));
        let axes = [
            projective_grid::LocalAxis::new(0.0, Some(0.02)),
            projective_grid::LocalAxis::new(third, Some(0.02)),
            projective_grid::LocalAxis::new(2.0 * third, Some(0.02)),
        ];
        feats.push(OrientedFeature::<3>::new(p, axes));
        truth.insert(idx, (q, r));
    }
    let req = DetectionRequest::new(
        LatticeKind::Hex,
        Evidence::Oriented3(&feats),
        None,
        DetectionParams::default().with_algorithm(SquareAlgorithm::Topological),
    );
    let sol = detect_grid(req).expect("hex Oriented3 native");
    assert!(sol.grid.entries.len() >= 12);
    assert_labels_consistent_with_truth(&entries_with_truth(&sol), &truth, "native Oriented3 hex");
}

/// D6-symmetry property test: under random in-plane rotations the recovered
/// labelling stays a consistent D6 automorphism of ground truth (zero wrong).
#[test]
fn hex_d6_symmetry_property_under_rotation() {
    for deg in [0.0_f32, 17.0, 33.0, 61.0, 95.0, 142.0] {
        let theta = deg.to_radians();
        let (c, s) = (theta.cos(), theta.sin());
        let rot = Matrix3::new(c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0);
        let (feats, truth) = hex_patch(3, 30.0, 250.0, &rot);
        let sol =
            detect_grid(request(&feats)).unwrap_or_else(|e| panic!("hex detect at {deg}°: {e:?}"));
        assert!(
            sol.grid.entries.len() >= 12,
            "{deg}°: recovered only {}",
            sol.grid.entries.len()
        );
        assert_labels_consistent_with_truth(
            &entries_with_truth(&sol),
            &truth,
            &format!("rotation {deg}°"),
        );
    }
}

/// Determinism: 10 identical runs on a hex fixture must produce byte-identical
/// label sets (HashMap-iteration ties are broken by sorted coords / index).
#[test]
fn hex_detection_is_deterministic() {
    let h = Matrix3::new(
        1.0, 0.08, 0.0, //
        0.02, 1.0, 0.0, //
        0.0005, 0.0003, 1.0,
    );
    let (feats, _truth) = hex_patch(4, 28.0, 200.0, &h);
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
    let first = signature(&detect_grid(request(&feats)).expect("hex detect run 0"));
    for run in 1..10 {
        let again = signature(&detect_grid(request(&feats)).expect("hex detect run n"));
        assert_eq!(first, again, "hex detection differs on run {run}");
    }
}

/// Negative: hex on the seed-and-grow selector is a typed UnsupportedCombination.
#[test]
fn hex_seed_and_grow_is_unsupported() {
    let (feats, _) = hex_patch(2, 30.0, 100.0, &Matrix3::identity());
    let req = DetectionRequest::new(
        LatticeKind::Hex,
        Evidence::Positions(&feats),
        None,
        DetectionParams::default().with_algorithm(SquareAlgorithm::SeedAndGrow),
    );
    let err = detect_grid(req).expect_err("hex + seed-and-grow must be unsupported");
    match err {
        projective_grid::GridError::UnsupportedCombination { lattice, .. } => {
            assert_eq!(lattice, LatticeKind::Hex);
        }
        other => panic!("expected UnsupportedCombination, got {other:?}"),
    }
}

/// Negative: `(Hex, Oriented1)` and `(Hex, Oriented2)` are unsupported (hex
/// needs three axis families).
#[test]
fn hex_oriented1_and_oriented2_unsupported() {
    let pts: Vec<PointFeature> = hex_coords(2)
        .into_iter()
        .enumerate()
        .map(|(idx, (q, r))| {
            let m = hex_model(q, r);
            PointFeature::new(idx, Point2::new(m.x * 30.0 + 100.0, m.y * 30.0 + 100.0))
        })
        .collect();
    let o1: Vec<OrientedFeature<1>> = pts
        .iter()
        .map(|p| OrientedFeature::<1>::new(*p, [projective_grid::LocalAxis::new(0.0, None)]))
        .collect();
    let req1 = DetectionRequest::new(
        LatticeKind::Hex,
        Evidence::Oriented1(&o1),
        None,
        DetectionParams::default().with_algorithm(SquareAlgorithm::Topological),
    );
    assert!(matches!(
        detect_grid(req1),
        Err(projective_grid::GridError::UnsupportedCombination { .. })
    ));

    let o2: Vec<OrientedFeature<2>> = pts
        .iter()
        .map(|p| {
            OrientedFeature::<2>::new(
                *p,
                [
                    projective_grid::LocalAxis::new(0.0, None),
                    projective_grid::LocalAxis::new(1.0, None),
                ],
            )
        })
        .collect();
    let req2 = DetectionRequest::new(
        LatticeKind::Hex,
        Evidence::Oriented2(&o2),
        None,
        DetectionParams::default().with_algorithm(SquareAlgorithm::Topological),
    );
    assert!(matches!(
        detect_grid(req2),
        Err(projective_grid::GridError::UnsupportedCombination { .. })
    ));
}
