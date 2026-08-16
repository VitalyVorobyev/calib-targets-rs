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
//! Every named fixture gates **zero wrong `(q, r)` labels**; this is regression
//! evidence, not a universal production guarantee. Hex labels are defined only up to the 12 D6
//! automorphisms composed with a lattice translation, so the consistency check
//! mods out that automorphism (see `assert_labels_consistent_with_truth`).
//! Recall floors are measured-minus-margin so tuning drift stays green while a
//! real regression trips.

use std::collections::HashMap;

use nalgebra::{Matrix3, Point2, Vector3};
use projective_grid::expert::lattice::D6_TRANSFORMS;
use projective_grid::{
    detect_grid, Coord, DetectionRequest, Evidence, LatticeKind, LocalAxis, OrientedFeature,
    PointFeature,
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

fn hex_oriented1_patch(
    radius: i32,
    spacing: f32,
    origin: f32,
    h: &Matrix3<f32>,
    axis_noise_rad: f32,
) -> (Vec<OrientedFeature<1>>, HashMap<usize, (i32, i32)>) {
    let mut features = Vec::new();
    let mut truth = HashMap::new();
    let mut rng = Lcg::new(0xA11CE551);
    for (index, (q, r)) in hex_coords(radius).into_iter().enumerate() {
        let model = hex_model(q, r);
        let here_model = Vector3::new(model.x * spacing + origin, model.y * spacing + origin, 1.0);
        let next_model = Vector3::new(
            (model.x + 1.0) * spacing + origin,
            model.y * spacing + origin,
            1.0,
        );
        let here_h = h * here_model;
        let next_h = h * next_model;
        let here = Point2::new(here_h.x / here_h.z, here_h.y / here_h.z);
        let next = Point2::new(next_h.x / next_h.z, next_h.y / next_h.z);
        let seam_shift = if index % 2 == 0 {
            std::f32::consts::PI
        } else {
            0.0
        };
        let angle = (next.y - here.y).atan2(next.x - here.x)
            + seam_shift
            + axis_noise_rad * rng.next_centered();
        features.push(OrientedFeature::<1>::new(
            PointFeature::new(index, here),
            [LocalAxis::new(angle, Some(axis_noise_rad.max(0.01)))],
        ));
        truth.insert(index, (q, r));
    }
    (features, truth)
}

fn request(features: &[PointFeature]) -> DetectionRequest<'_> {
    DetectionRequest::new(LatticeKind::Hex, Evidence::Positions(features))
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

fn entries_with_truth(sol: &projective_grid::GridDetection) -> Vec<(usize, Coord)> {
    sol.grid()
        .entries()
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
        sol.grid().entries().len() >= (n * 3) / 5,
        "recovered only {}/{n} hex nodes on a perfect patch",
        sol.grid().entries().len()
    );
    assert_labels_consistent_with_truth(&entries_with_truth(&sol), &truth, "perfect hex");
    assert_eq!(sol.grid().lattice(), LatticeKind::Hex);
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
        sol.grid().entries().len() >= 24,
        "recovered only {} hex nodes under perspective",
        sol.grid().entries().len()
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
        sol.grid().entries().len() >= 24,
        "recovered only {} hex nodes under noise",
        sol.grid().entries().len()
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
        sol.grid().entries().len() >= 15,
        "recovered only {} hex nodes with dropouts",
        sol.grid().entries().len()
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
    let grid_entries = entries_with_truth(&sol);
    assert!(
        grid_entries.iter().all(|(src, _)| truth.contains_key(src)),
        "off-lattice clutter received a grid label"
    );
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
    let req = DetectionRequest::new(LatticeKind::Hex, Evidence::Oriented3(&feats));
    let sol = detect_grid(req).expect("hex Oriented3 native");
    assert!(sol.grid().entries().len() >= 12);
    assert_labels_consistent_with_truth(&entries_with_truth(&sol), &truth, "native Oriented3 hex");
}

#[test]
fn hex_oriented1_preserves_trusted_family_under_perspective() {
    let h = Matrix3::new(
        0.98, 0.14, 0.0, //
        -0.04, 1.03, 0.0, //
        0.000_55, 0.000_35, 1.0,
    );
    let (features, truth) = hex_oriented1_patch(4, 28.0, 200.0, &h, 0.0);
    let request = DetectionRequest::new(LatticeKind::Hex, Evidence::Oriented1(&features));
    let solution = detect_grid(request).expect("hex Oriented1 under perspective");
    assert!(
        solution.grid().entries().len() >= 24,
        "recovered only {} Oriented1 nodes",
        solution.grid().entries().len()
    );
    assert_labels_consistent_with_truth(
        &entries_with_truth(&solution),
        &truth,
        "perspective Oriented1 hex",
    );
}

#[test]
fn hex_oriented1_noise_dropout_shuffle_is_deterministic() {
    let h = Matrix3::new(
        1.02, 0.08, 0.0, //
        0.03, 0.97, 0.0, //
        0.000_4, 0.000_3, 1.0,
    );
    let (all, truth_all) = hex_oriented1_patch(4, 30.0, 200.0, &h, 0.04);
    let mut features: Vec<_> = all
        .into_iter()
        .enumerate()
        .filter(|(index, _)| index % 9 != 4)
        .map(|(_, feature)| feature)
        .collect();
    let mut position_noise = Lcg::new(0x5107_1015);
    for feature in &mut features {
        feature.point.position.x += 0.5 * position_noise.next_centered();
        feature.point.position.y += 0.5 * position_noise.next_centered();
    }
    features.reverse();
    let truth: HashMap<usize, (i32, i32)> = features
        .iter()
        .map(|feature| {
            (
                feature.point.source_index,
                truth_all[&feature.point.source_index],
            )
        })
        .collect();
    let detect = || {
        detect_grid(DetectionRequest::new(
            LatticeKind::Hex,
            Evidence::Oriented1(&features),
        ))
        .expect("noisy shuffled Oriented1 hex")
    };
    let first = detect();
    assert!(first.grid().entries().len() >= 18);
    assert_labels_consistent_with_truth(
        &entries_with_truth(&first),
        &truth,
        "noisy shuffled Oriented1 hex",
    );
    assert_eq!(entries_with_truth(&first), entries_with_truth(&detect()));
}

#[test]
fn hex_oriented1_rejects_near_and_far_off_lattice_clutter() {
    let (mut features, truth) = hex_oriented1_patch(4, 30.0, 200.0, &Matrix3::identity(), 0.0);
    let first_clutter = features.len();
    for (dx, dy) in [(7.0, 5.0), (-11.0, 8.0), (340.0, -280.0)] {
        let source_index = features.len();
        features.push(OrientedFeature::<1>::new(
            PointFeature::new(source_index, Point2::new(200.0 + dx, 200.0 + dy)),
            [LocalAxis::new(0.0, Some(0.02))],
        ));
    }
    let solution = detect_grid(DetectionRequest::new(
        LatticeKind::Hex,
        Evidence::Oriented1(&features),
    ))
    .expect("Oriented1 hex with clutter");
    let entries = entries_with_truth(&solution);
    assert!(
        entries.iter().all(|(source, _)| *source < first_clutter),
        "off-lattice Oriented1 clutter received a label"
    );
    assert_labels_consistent_with_truth(&entries, &truth, "Oriented1 clutter hex");
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
            sol.grid().entries().len() >= 12,
            "{deg}°: recovered only {}",
            sol.grid().entries().len()
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
    let signature = |sol: &projective_grid::GridDetection| -> Vec<(usize, i32, i32)> {
        let mut sig: Vec<(usize, i32, i32)> = sol
            .grid()
            .entries()
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

/// Negative: `(Hex, Oriented2)` remains unsupported because two supplied
/// directions do not identify which physical family is missing.
#[test]
fn hex_oriented2_is_unsupported() {
    let pts: Vec<PointFeature> = hex_coords(2)
        .into_iter()
        .enumerate()
        .map(|(idx, (q, r))| {
            let m = hex_model(q, r);
            PointFeature::new(idx, Point2::new(m.x * 30.0 + 100.0, m.y * 30.0 + 100.0))
        })
        .collect();
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
    let req2 = DetectionRequest::new(LatticeKind::Hex, Evidence::Oriented2(&o2));
    assert!(matches!(
        detect_grid(req2),
        Err(projective_grid::GridError::UnsupportedCombination { .. })
    ));
}

/// A rectangular, axis-aligned hex patch with **no perspective term** must keep
/// its recall under sub-pixel centre noise.
///
/// Regression guard for
/// [#78](https://github.com/VitalyVorobyev/calib-targets-rs/issues/78), which
/// reported this configuration collapsing from 30 labelled cells at zero noise
/// to 12 at 0.1 px — noise ~0.3 % of the neighbour spacing. Every other hex
/// fixture in this file carries a perspective term and a hex-*disc* shape, so
/// none of them covered the axis-aligned rectangular case.
///
/// Noise is scaled with the spacing so the sweep tests the same *relative*
/// perturbation at every scale, which is what the issue reported as
/// scale-independent.
#[test]
fn axis_aligned_hex_rows_survive_subpixel_noise() {
    /// Rows alternating `wide` and `wide - 1` cells, offset by half a spacing:
    /// a triangular lattice whose nearest-neighbour distance is `spacing`.
    fn hex_rows(
        rows: i32,
        wide: i32,
        spacing: f32,
        noise: f32,
    ) -> (Vec<PointFeature>, HashMap<usize, (i32, i32)>) {
        let dy = spacing * 3.0_f32.sqrt() * 0.5;
        let mut rng = Lcg::new(0x1234_5678);
        let mut features = Vec::new();
        let mut truth = HashMap::new();
        for row in 0..rows {
            let odd = row % 2 == 1;
            let count = if odd { wide - 1 } else { wide };
            let x0 = 200.0 + if odd { spacing * 0.5 } else { 0.0 };
            for col in 0..count {
                let x = x0 + col as f32 * spacing + noise * rng.next_centered();
                let y = 200.0 + row as f32 * dy + noise * rng.next_centered();
                let index = features.len();
                features.push(PointFeature::new(index, Point2::new(x, y)));
                // Axial coords for this row-offset triangular lattice: the
                // model basis is `(q + r/2, (sqrt3/2) r)`, so `q = col -
                // floor(row / 2)` and `r = row`.
                truth.insert(index, (col - (row - (row & 1)) / 2, row));
            }
        }
        (features, truth)
    }

    for &spacing in &[14.0f32, 36.0, 140.0] {
        for &relative_noise in &[0.0f32, 0.05 / 36.0, 0.1 / 36.0, 0.3 / 36.0] {
            let (features, truth) = hex_rows(7, 7, spacing, relative_noise * spacing);
            let n = features.len();
            let solution = detect_grid(DetectionRequest::new(
                LatticeKind::Hex,
                Evidence::Positions(&features),
            ))
            .unwrap_or_else(|e| {
                panic!("spacing {spacing} noise {relative_noise:.4}·s: detect failed: {e}")
            });
            let labelled = solution.grid().entries().len();
            assert!(
                labelled * 4 >= n * 3,
                "spacing {spacing}, noise {:.3} px: labelled only {labelled}/{n} — the \
                 axis-aligned hex path lost recall under sub-pixel noise",
                relative_noise * spacing,
            );
            // Recall is only half the contract: the labels must also be right.
            assert_labels_consistent_with_truth(
                &entries_with_truth(&solution),
                &truth,
                &format!("axis-aligned hex, spacing {spacing}, noise {relative_noise:.4}·s"),
            );
        }
    }
}
