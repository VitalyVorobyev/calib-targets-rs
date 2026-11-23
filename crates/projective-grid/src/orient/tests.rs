use super::*;
use nalgebra::Matrix3;
use std::collections::HashMap;

fn grid_features(rows: i32, cols: i32, s: f32) -> Vec<PointFeature> {
    let mut out = Vec::new();
    let mut idx = 0usize;
    for j in 0..rows {
        for i in 0..cols {
            out.push(PointFeature::new(
                idx,
                Point2::new(i as f32 * s + 40.0, j as f32 * s + 40.0),
            ));
            idx += 1;
        }
    }
    out
}

/// Both synthesized axes must align (undirected) with the two expected
/// directions — in either slot order, and with NO orthogonality assumption.
fn assert_axes_match(axes: [LocalAxis; 2], exp_a: f32, exp_b: f32, tol_deg: f32) {
    let tol = tol_deg.to_radians();
    // axes[0] matches one expected, axes[1] the other (some assignment).
    let direct = dist_pi(axes[0].angle_rad, exp_a).max(dist_pi(axes[1].angle_rad, exp_b));
    let swapped = dist_pi(axes[0].angle_rad, exp_b).max(dist_pi(axes[1].angle_rad, exp_a));
    let err = direct.min(swapped);
    assert!(
        err < tol,
        "axes {:?},{:?} don't match expected {exp_a},{exp_b} (err {err})",
        axes[0].angle_rad,
        axes[1].angle_rad
    );
}

#[test]
fn axis_aligned_grid_recovers_horizontal_vertical() {
    let feats = grid_features(6, 6, 25.0);
    let oriented = synthesize_oriented2(&feats);
    // Interior corner (2, 2) -> flat index 2*6 + 2 = 14.
    assert_axes_match(oriented[14].axes, 0.0, std::f32::consts::FRAC_PI_2, 4.0);
}

#[test]
fn rotated_grid_tracks_orientation() {
    for deg in [10.0_f32, 30.0, 47.0, 80.0] {
        let theta = deg.to_radians();
        let (c, s) = (theta.cos(), theta.sin());
        let feats: Vec<PointFeature> = grid_features(6, 6, 25.0)
            .iter()
            .map(|f| {
                let (x, y) = (f.position.x, f.position.y);
                PointFeature::new(f.source_index, Point2::new(c * x - s * y, s * x + c * y))
            })
            .collect();
        let oriented = synthesize_oriented2(&feats);
        // Pure rotation keeps axes orthogonal: expected theta and theta+90.
        assert_axes_match(
            oriented[14].axes,
            fold_pi(theta),
            fold_pi(theta + std::f32::consts::FRAC_PI_2),
            6.0,
        );
    }
}

#[test]
fn perspective_grid_axes_are_non_orthogonal_and_correct() {
    // Project a canonical grid through a homography with a real
    // perspective term, then check each interior corner's recovered axes
    // match the LOCAL projected grid directions — which are NOT 90° apart.
    let h = Matrix3::new(
        1.0, 0.20, 0.0, //
        0.0, 1.0, 0.0, //
        0.0015, 0.0009, 1.0,
    );
    let project = |gx: f32, gy: f32| -> Point2<f32> {
        let v = h * nalgebra::Vector3::new(gx, gy, 1.0);
        Point2::new(v.x / v.z, v.y / v.z)
    };

    let rows = 9;
    let cols = 9;
    let s = 30.0_f32;
    let mut feats = Vec::new();
    let mut idx = 0usize;
    for j in 0..rows {
        for i in 0..cols {
            let p = project(i as f32 * s + 40.0, j as f32 * s + 40.0);
            feats.push(PointFeature::new(idx, p));
            idx += 1;
        }
    }
    let oriented = synthesize_oriented2(&feats);

    let mut saw_non_orthogonal = false;
    // Check several interior corners.
    for j in 2..rows - 2 {
        for i in 2..cols - 2 {
            let flat = (j * cols + i) as usize;
            let here = feats[flat].position;
            let pu = feats[(j * cols + (i + 1)) as usize].position;
            let pv = feats[((j + 1) * cols + i) as usize].position;
            let exp_u = fold_pi((pu.y - here.y).atan2(pu.x - here.x));
            let exp_v = fold_pi((pv.y - here.y).atan2(pv.x - here.x));
            assert_axes_match(oriented[flat].axes, exp_u, exp_v, 6.0);
            if dist_pi(exp_u, exp_v) < 80.0_f32.to_radians() {
                saw_non_orthogonal = true;
            }
        }
    }
    // The whole point: the perspective view really does bend the inter-axis
    // angle away from 90° somewhere in the interior.
    assert!(
        saw_non_orthogonal,
        "test homography too weak to exercise non-orthogonal axes"
    );
}

#[test]
fn preserves_source_index_and_position() {
    let feats = grid_features(3, 3, 20.0);
    let oriented = synthesize_oriented2(&feats);
    for (f, o) in feats.iter().zip(&oriented) {
        assert_eq!(o.point.source_index, f.source_index);
        assert_eq!(o.point.position, f.position);
    }
}

#[test]
fn handles_degenerate_inputs() {
    assert!(synthesize_oriented2(&[]).is_empty());
    let one = vec![PointFeature::new(0, Point2::new(1.0, 2.0))];
    let got = synthesize_oriented2(&one);
    assert_eq!(got.len(), 1);
    assert!(got[0].axes[0].angle_rad.is_finite() && got[0].axes[1].angle_rad.is_finite());
}

// --- Oriented1 → Oriented2 synthesis -------------------------------

/// Project a `rows × cols` grid through `h`; return the features and the
/// ground-truth per-feature LOCAL `+u` axis (the direction to the `(i+1,j)`
/// neighbour, folded to `[0, π)`), so each Oriented1 input carries one true
/// axis. Interior corners only carry a defined neighbour for both axes.
fn perspective_grid_with_u_axis(
    rows: i32,
    cols: i32,
    s: f32,
    h: &Matrix3<f32>,
) -> (Vec<Point2<f32>>, Vec<(i32, i32)>) {
    let mut pts = Vec::new();
    let mut ij = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            let v = h * nalgebra::Vector3::new(i as f32 * s + 40.0, j as f32 * s + 40.0, 1.0);
            pts.push(Point2::new(v.x / v.z, v.y / v.z));
            ij.push((i, j));
        }
    }
    (pts, ij)
}

#[test]
fn oriented1_anchors_supplied_axis_and_recovers_second() {
    // Random-ish homography with a genuine perspective term.
    let h = Matrix3::new(
        1.0, 0.16, 0.0, //
        0.05, 1.0, 0.0, //
        0.0012, 0.0008, 1.0,
    );
    let (rows, cols, s) = (9, 9, 30.0_f32);
    let (pts, ij) = perspective_grid_with_u_axis(rows, cols, s, &h);
    let cols_us = cols as usize;

    // Build Oriented1 features: supply the TRUE local +u direction per
    // corner, plus a small angular noise. Position-only otherwise.
    let mut rng = 0x9E3779B9u32;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        (rng as f32 / u32::MAX as f32) - 0.5
    };
    let o1: Vec<OrientedFeature<1>> = ij
        .iter()
        .enumerate()
        .map(|(flat, &(i, j))| {
            // Use the +u neighbour where it exists, else the -u neighbour.
            let here = pts[flat];
            let u_nb = if i + 1 < cols {
                pts[(j as usize) * cols_us + (i as usize + 1)]
            } else {
                pts[(j as usize) * cols_us + (i as usize - 1)]
            };
            let true_u = fold_pi((u_nb.y - here.y).atan2(u_nb.x - here.x));
            let noisy = true_u + 3.0_f32.to_radians() * next();
            OrientedFeature::<1>::new(PointFeature::new(flat, here), [LocalAxis::new(noisy, None)])
        })
        .collect();

    let o2 = synthesize_oriented2_from_oriented1(&o1);
    assert_eq!(o2.len(), o1.len());

    // For interior corners, the recovered axes must match the LOCAL
    // projected grid directions (both +u and +v), and the supplied axis
    // must survive as one of the two (within the injected noise band).
    for j in 1..rows - 1 {
        for i in 1..cols - 1 {
            let flat = (j as usize) * cols_us + i as usize;
            let here = pts[flat];
            let pu = pts[(j as usize) * cols_us + (i as usize + 1)];
            let pv = pts[((j + 1) as usize) * cols_us + i as usize];
            let exp_u = fold_pi((pu.y - here.y).atan2(pu.x - here.x));
            let exp_v = fold_pi((pv.y - here.y).atan2(pv.x - here.x));
            assert_axes_match(o2[flat].axes, exp_u, exp_v, 6.0);
            // The supplied (noisy) axis is preserved (anchored): one of the
            // two output axes equals exp_u within the noise band.
            let d0 = dist_pi(o2[flat].axes[0].angle_rad, exp_u);
            let d1 = dist_pi(o2[flat].axes[1].angle_rad, exp_u);
            assert!(
                d0.min(d1) < 4.0_f32.to_radians(),
                "supplied +u axis not anchored at ({i},{j})"
            );
        }
    }
}

#[test]
fn oriented1_matches_oriented2_path_on_clean_grid() {
    // On a clean axis-aligned grid the from-Oriented1 synthesis should
    // produce the same two directions as the from-Positions synthesis:
    // both recover (0, π/2) at every interior corner.
    let feats = grid_features(7, 7, 25.0);
    let o1: Vec<OrientedFeature<1>> = feats
        .iter()
        .map(|f| OrientedFeature::<1>::new(*f, [LocalAxis::new(0.0, None)]))
        .collect();
    let from_o1 = synthesize_oriented2_from_oriented1(&o1);
    let from_pos = synthesize_oriented2(&feats);
    // Interior corner (3,3) -> flat 3*7+3 = 24.
    assert_axes_match(from_o1[24].axes, 0.0, std::f32::consts::FRAC_PI_2, 4.0);
    assert_axes_match(from_pos[24].axes, 0.0, std::f32::consts::FRAC_PI_2, 4.0);
}

#[test]
fn oriented1_handles_degenerate_inputs() {
    assert!(synthesize_oriented2_from_oriented1(&[]).is_empty());
    let one = vec![OrientedFeature::<1>::new(
        PointFeature::new(0, Point2::new(1.0, 2.0)),
        [LocalAxis::new(0.3, None)],
    )];
    let got = synthesize_oriented2_from_oriented1(&one);
    assert_eq!(got.len(), 1);
    // Supplied axis preserved as one of the two.
    let d = dist_pi(got[0].axes[0].angle_rad, fold_pi(0.3))
        .min(dist_pi(got[0].axes[1].angle_rad, fold_pi(0.3)));
    assert!(d < 1e-4);
}

#[test]
fn oriented1_preserves_source_index_and_position() {
    let feats = grid_features(3, 3, 20.0);
    let o1: Vec<OrientedFeature<1>> = feats
        .iter()
        .map(|f| OrientedFeature::<1>::new(*f, [LocalAxis::new(0.1, None)]))
        .collect();
    let got = synthesize_oriented2_from_oriented1(&o1);
    for (f, o) in feats.iter().zip(&got) {
        assert_eq!(o.point.source_index, f.source_index);
        assert_eq!(o.point.position, f.position);
    }
}

// --- Positions → Oriented3 (hex) synthesis -------------------------

/// Axial hex node `(q, r)` model position with unit nearest-neighbour
/// spacing: `(q + r/2, sqrt(3)/2 · r)`.
fn hex_model(q: i32, r: i32) -> Point2<f32> {
    let sqrt3_2 = 3.0_f32.sqrt() * 0.5;
    Point2::new(q as f32 + 0.5 * r as f32, sqrt3_2 * r as f32)
}

/// Build a roughly-circular patch of hex nodes (axial radius `radius`),
/// projected through `h`. Returns the point features plus their `(q, r)`.
fn hex_features(radius: i32, s: f32, h: &Matrix3<f32>) -> (Vec<PointFeature>, Vec<(i32, i32)>) {
    let mut feats = Vec::new();
    let mut qr = Vec::new();
    let mut idx = 0usize;
    for q in -radius..=radius {
        for r in (-radius).max(-q - radius)..=radius.min(-q + radius) {
            let m = hex_model(q, r);
            let v = h * nalgebra::Vector3::new(m.x * s + 200.0, m.y * s + 200.0, 1.0);
            feats.push(PointFeature::new(idx, Point2::new(v.x / v.z, v.y / v.z)));
            qr.push((q, r));
            idx += 1;
        }
    }
    (feats, qr)
}

/// Check the three synthesized axes match the three expected directions in
/// some assignment (undirected, no inter-axis-angle constraint).
fn assert_axes3_match(axes: [LocalAxis; 3], exp: [f32; 3], tol_deg: f32) {
    let tol = tol_deg.to_radians();
    let got = [axes[0].angle_rad, axes[1].angle_rad, axes[2].angle_rad];
    // Optimal (min-max) assignment over all 3! permutations of expected.
    const PERMS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let best_max = PERMS
        .iter()
        .map(|p| {
            (0..3)
                .map(|k| dist_pi(got[k], exp[p[k]]))
                .fold(0.0_f32, f32::max)
        })
        .fold(f32::INFINITY, f32::min);
    assert!(
        best_max < tol,
        "axes {got:?} don't match expected {exp:?} (max err {best_max})"
    );
}

#[test]
fn hex_axis_aligned_recovers_three_directions() {
    // Identity homography: the three undirected hex directions are 0°, 60°,
    // 120° regardless of node — recovered at every interior node.
    let h = Matrix3::identity();
    let (feats, qr) = hex_features(3, 26.0, &h);
    let third = std::f32::consts::PI / 3.0;
    // Pick a deep-interior node (the centre (0,0)).
    let centre = qr.iter().position(|&c| c == (0, 0)).unwrap();
    assert_axes3_match(feats_axes3(&feats)[centre], [0.0, third, 2.0 * third], 5.0);
}

#[test]
fn hex_perspective_axes_track_local_directions() {
    // Genuine perspective term: the three projected hex directions bend
    // across the patch. The recovered axis along a family is the undirected
    // mean of the `+` and `−` chords; under perspective those two are
    // antipodal through the node (collinearity) so the fold-mod-π mean
    // recovers the chord line. We therefore compare each recovered axis to
    // the line through the node's two opposite neighbours along that axis.
    let h = Matrix3::new(
        1.0, 0.12, 0.0, //
        0.03, 1.0, 0.0, //
        0.0006, 0.0004, 1.0,
    );
    let (feats, qr) = hex_features(4, 24.0, &h);
    let index: HashMap<(i32, i32), usize> = qr.iter().enumerate().map(|(i, &c)| (c, i)).collect();
    let oriented = synthesize_oriented3(&feats);

    let mut checked = 0usize;
    for (flat, &(q, r)) in qr.iter().enumerate() {
        // Need BOTH opposite neighbours along each of the three axes so the
        // expected direction is the through-line (matching the mod-π mean).
        let line_dir = |a: (i32, i32), b: (i32, i32)| -> Option<f32> {
            let pa = feats[*index.get(&a)?].position;
            let pb = feats[*index.get(&b)?].position;
            Some(fold_pi((pb.y - pa.y).atan2(pb.x - pa.x)))
        };
        let (Some(dq), Some(dr), Some(ds)) = (
            line_dir((q - 1, r), (q + 1, r)),
            line_dir((q, r - 1), (q, r + 1)),
            line_dir((q + 1, r - 1), (q - 1, r + 1)),
        ) else {
            continue;
        };
        assert_axes3_match(oriented[flat].axes, [dq, dr, ds], 8.0);
        checked += 1;
    }
    assert!(checked >= 4, "too few interior nodes checked: {checked}");
}

#[test]
fn hex_axes_survive_position_noise() {
    // Add small isotropic position noise; the synthesized axes should stay
    // within a wider band of the local projected directions.
    let h = Matrix3::new(
        1.0, 0.10, 0.0, //
        0.03, 1.0, 0.0, //
        0.0005, 0.0004, 1.0,
    );
    let (mut feats, qr) = hex_features(4, 24.0, &h);
    let index: HashMap<(i32, i32), usize> = qr.iter().enumerate().map(|(i, &c)| (c, i)).collect();
    // Deterministic xorshift noise, ±0.6 px.
    let mut rng = 0x1234_5678u32;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        (rng as f32 / u32::MAX as f32) - 0.5
    };
    for f in feats.iter_mut() {
        f.position.x += 1.2 * next();
        f.position.y += 1.2 * next();
    }
    let oriented = synthesize_oriented3(&feats);
    let mut checked = 0usize;
    for (flat, &(q, r)) in qr.iter().enumerate() {
        let line_dir = |a: (i32, i32), b: (i32, i32)| -> Option<f32> {
            let pa = feats[*index.get(&a)?].position;
            let pb = feats[*index.get(&b)?].position;
            Some(fold_pi((pb.y - pa.y).atan2(pb.x - pa.x)))
        };
        let (Some(dq), Some(dr), Some(ds)) = (
            line_dir((q - 1, r), (q + 1, r)),
            line_dir((q, r - 1), (q, r + 1)),
            line_dir((q + 1, r - 1), (q - 1, r + 1)),
        ) else {
            continue;
        };
        assert_axes3_match(oriented[flat].axes, [dq, dr, ds], 12.0);
        checked += 1;
    }
    assert!(checked >= 4, "too few interior nodes checked: {checked}");
}

#[test]
fn hex_preserves_source_index_and_position() {
    let h = Matrix3::identity();
    let (feats, _) = hex_features(2, 20.0, &h);
    let oriented = synthesize_oriented3(&feats);
    assert_eq!(oriented.len(), feats.len());
    for (f, o) in feats.iter().zip(&oriented) {
        assert_eq!(o.point.source_index, f.source_index);
        assert_eq!(o.point.position, f.position);
    }
}

#[test]
fn hex_handles_degenerate_inputs() {
    assert!(synthesize_oriented3(&[]).is_empty());
    let one = vec![PointFeature::new(0, Point2::new(1.0, 2.0))];
    let got = synthesize_oriented3(&one);
    assert_eq!(got.len(), 1);
    assert!(got[0].axes.iter().all(|a| a.angle_rad.is_finite()));
}

#[test]
fn hex_oriented1_preserves_measured_observation_exactly() {
    let (points, _) = hex_features(3, 24.0, &Matrix3::identity());
    let features: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let angle = if index % 2 == 0 {
                std::f32::consts::PI + 0.013
            } else {
                0.013
            };
            OrientedFeature::<1>::new(*point, [LocalAxis::new(angle, Some(0.037))])
        })
        .collect();
    let synthesized = synthesize_oriented3_from_oriented1(&features);
    for (input, output) in features.iter().zip(&synthesized) {
        assert_eq!(
            output.axes[0].angle_rad.to_bits(),
            input.axes[0].angle_rad.to_bits()
        );
        assert_eq!(
            output.axes[0].sigma_rad.map(f32::to_bits),
            input.axes[0].sigma_rad.map(f32::to_bits)
        );
    }
}

#[test]
fn hex_oriented1_does_not_trust_uncorroborated_synthetic_families() {
    let feature = OrientedFeature::<1>::new(
        PointFeature::new(7, Point2::new(1.0, 2.0)),
        [LocalAxis::new(0.2, Some(0.03))],
    );
    let synthesized = synthesize_oriented3_from_oriented1(&[feature]);
    assert_eq!(synthesized[0].axes[0].sigma_rad, Some(0.03));
    assert!(synthesized[0].axes[1..]
        .iter()
        .all(|axis| axis.sigma_rad == Some(std::f32::consts::PI)));
}

/// Helper: run `synthesize_oriented3` and collect just the axis arrays.
fn feats_axes3(feats: &[PointFeature]) -> Vec<[LocalAxis; 3]> {
    synthesize_oriented3(feats)
        .into_iter()
        .map(|o| o.axes)
        .collect()
}
