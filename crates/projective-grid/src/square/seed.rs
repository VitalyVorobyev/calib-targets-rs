//! Seed-finder data types + pure-geometry helpers.
//!
//! The chessboard detector's seed search in
//! `calib_targets_chessboard::seed` is still pattern-specific (it
//! relies on chessboard parity, the Canonical/Swapped cluster split,
//! and the axis-slot-swap invariant). The pieces that are pure
//! geometry — the four-corner seed quad, its edge / cell-size bundle,
//! and the 2×-spacing "midpoint violation" rejection — live here so
//! non-calibration consumers can reuse them.

use crate::homography::homography_from_4pt;
use nalgebra::Point2;

pub use crate::square::grow::Seed;

/// Output of a seed finder: the 2×2 quad plus a cell size derived
/// directly from the seed's own edge lengths.
#[derive(Clone, Copy, Debug)]
pub struct SeedOutput {
    pub seed: Seed,
    pub cell_size: f32,
}

/// Grid positions of the four seed corners in the "canonical" seed
/// quad layout used by [`crate::square::grow::bfs_grow`] and the
/// chessboard detector: `A = (0, 0), B = (1, 0), C = (0, 1),
/// D = (1, 1)`.
pub const SEED_QUAD_GRID: [(i32, i32); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];

/// Detect the 2× spacing mislabel, where a 2×2 quad has accidentally
/// been picked across a 2-cell step of the real grid (e.g., real
/// positions `(0,0), (2,0), (0,2), (2,2)` mislabelled as the
/// canonical seed).
///
/// Returns `true` when any of the seed's edge midpoints or its
/// parallelogram center coincides (within `midpoint_tol_rel ×
/// cell_size` of pixel distance) with a real corner **other than
/// the seed quad itself**. Such coincidences indicate the seed
/// has skipped a true intermediate corner — a classic 2× spacing
/// bug.
///
/// `positions` — every corner's pixel position.
/// `seed_quad` — the four corner indices in the seed.
/// `cell_size` — the seed's own estimated cell size.
/// `midpoint_tol_rel` — tolerance as a fraction of `cell_size`.
/// `on_edge_midpoint` — pattern-specific candidate indices to test
///                       against the four edge midpoints. Callers
///                       pass the set whose presence at a midpoint
///                       is a stronger violation signal (e.g.,
///                       "Swapped"-label corners for a chessboard,
///                       which would lie at midpoints if the seed
///                       skipped a Swapped row).
/// `on_parallelogram_center` — pattern-specific candidate indices to
///                              test against the parallelogram center
///                              `(0.5, 0.5)`. Same convention as
///                              `on_edge_midpoint`.
/// `all_positions` — full-population fallback indices to test against
///                    midpoints AND center, regardless of cluster
///                    label. Catches 2× / sqrt(2)× / general N× cases
///                    where the intermediate corner failed Stage-3
///                    clustering and isn't in the pattern-specific
///                    lists. Pass `&[]` to disable.
pub fn seed_has_midpoint_violation(
    positions: &[Point2<f32>],
    seed_quad: [usize; 4],
    cell_size: f32,
    midpoint_tol_rel: f32,
    on_edge_midpoint: &[usize],
    on_parallelogram_center: &[usize],
    all_positions: &[usize],
) -> bool {
    let tol = midpoint_tol_rel * cell_size;
    let tol_sq = tol * tol;

    let [a, b, c, d] = seed_quad;
    let pa = positions[a];
    let pb = positions[b];
    let pc = positions[c];
    let pd = positions[d];

    let midpoints = [
        Point2::from((pa.coords + pb.coords) * 0.5),
        Point2::from((pa.coords + pc.coords) * 0.5),
        Point2::from((pb.coords + pd.coords) * 0.5),
        Point2::from((pc.coords + pd.coords) * 0.5),
    ];

    // The `all_positions` fallback uses a tighter tolerance (half the
    // pattern-aware tolerance) because it admits arbitrary corners,
    // including marker-internal ones that may legitimately fall near
    // a grid-cell midpoint without indicating a 2×-spacing seed bug.
    // The pattern-aware lists are already cluster-filtered, so they
    // can use the wider tolerance.
    let fallback_tol = tol * 0.5;
    let fallback_tol_sq = fallback_tol * fallback_tol;

    for mp in midpoints {
        if any_within(positions, on_edge_midpoint, mp, tol_sq, &seed_quad) {
            return true;
        }
        if any_within(positions, all_positions, mp, fallback_tol_sq, &seed_quad) {
            return true;
        }
    }

    let center = Point2::from((pa.coords + pd.coords) * 0.5);
    if any_within(
        positions,
        on_parallelogram_center,
        center,
        tol_sq,
        &seed_quad,
    ) {
        return true;
    }
    if any_within(
        positions,
        all_positions,
        center,
        fallback_tol_sq,
        &seed_quad,
    ) {
        return true;
    }
    false
}

fn any_within(
    positions: &[Point2<f32>],
    candidates: &[usize],
    target: Point2<f32>,
    tol_sq: f32,
    exclude: &[usize],
) -> bool {
    for &idx in candidates {
        if exclude.contains(&idx) {
            continue;
        }
        let p = positions[idx];
        let dx = p.x - target.x;
        let dy = p.y - target.y;
        if dx * dx + dy * dy <= tol_sq {
            return true;
        }
    }
    false
}

/// Compute a per-seed cell-size estimate: the mean of the four
/// seed-edge lengths. This is the self-consistent cell size that the
/// chessboard detector carries through downstream stages; the
/// advantage over a global cross-cluster distance mode is that the
/// seed's own geometry is always consistent with the value it emits.
///
/// Returns `None` when the seed has zero-length edges (degenerate).
pub fn seed_cell_size(positions: &[Point2<f32>], seed: Seed) -> Option<f32> {
    let p = |i: usize| positions[i];
    let edges = [
        (p(seed.a) - p(seed.b)).norm(),
        (p(seed.a) - p(seed.c)).norm(),
        (p(seed.b) - p(seed.d)).norm(),
        (p(seed.c) - p(seed.d)).norm(),
    ];
    if edges.iter().any(|&e| e <= 0.0) {
        return None;
    }
    Some(edges.iter().sum::<f32>() * 0.25)
}

/// Reassemble the 4 seed corner indices into the flat array layout
/// used by homography helpers (grid corner order: TL, TR, BR, BL).
pub fn seed_homography(
    positions: &[Point2<f32>],
    seed: Seed,
) -> Option<crate::homography::Homography> {
    let img_pts = [
        positions[seed.a],
        positions[seed.b],
        positions[seed.d],
        positions[seed.c],
    ];
    let grid_pts = [
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    ];
    homography_from_4pt(&grid_pts, &img_pts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions_4(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)) -> Vec<Point2<f32>> {
        vec![
            Point2::new(a.0, a.1),
            Point2::new(b.0, b.1),
            Point2::new(c.0, c.1),
            Point2::new(d.0, d.1),
        ]
    }

    #[test]
    fn seed_cell_size_unit_square() {
        let p = positions_4((0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (10.0, 10.0));
        let s = seed_cell_size(
            &p,
            Seed {
                a: 0,
                b: 1,
                c: 2,
                d: 3,
            },
        )
        .unwrap();
        assert!((s - 10.0).abs() < 1e-4);
    }

    #[test]
    fn midpoint_violation_detects_2x_mislabel() {
        // Seed thinks the quad is (0,0),(1,0),(0,1),(1,1) at cell
        // size 10, but an intermediate corner (e.g. swapped at
        // (0.5, 0) in seed-space = (5, 0) in pixels) exists in the
        // cloud.
        let positions = vec![
            Point2::new(0.0, 0.0),   // 0 = A
            Point2::new(20.0, 0.0),  // 1 = B (2× spacing!)
            Point2::new(0.0, 20.0),  // 2 = C
            Point2::new(20.0, 20.0), // 3 = D
            Point2::new(10.0, 0.0),  // 4 = intermediate swapped corner
        ];
        let violation = seed_has_midpoint_violation(
            &positions,
            [0, 1, 2, 3],
            20.0,
            0.3,
            &[4], // "swapped" candidates
            &[],  // no canonical to check center
            &[],  // no all-position fallback (already caught by swapped)
        );
        assert!(violation);
    }

    #[test]
    fn midpoint_violation_absent_on_clean_seed() {
        let positions = positions_4((0.0, 0.0), (10.0, 0.0), (0.0, 10.0), (10.0, 10.0));
        let violation =
            seed_has_midpoint_violation(&positions, [0, 1, 2, 3], 10.0, 0.3, &[], &[], &[]);
        assert!(!violation);
    }

    #[test]
    fn midpoint_violation_detects_2x_via_unclustered_fallback() {
        // Same shape as `midpoint_violation_detects_2x_mislabel` but
        // the intermediate corner failed Stage-3 clustering, so it
        // is NOT in the chessboard-specific `on_edge_midpoint` set.
        // The new `all_positions` fallback must still flag it.
        let positions = vec![
            Point2::new(0.0, 0.0),
            Point2::new(20.0, 0.0),
            Point2::new(0.0, 20.0),
            Point2::new(20.0, 20.0),
            Point2::new(10.0, 0.0), // intermediate, not clustered
        ];
        let all = vec![0, 1, 2, 3, 4];
        let violation = seed_has_midpoint_violation(
            &positions,
            [0, 1, 2, 3],
            20.0,
            0.3,
            &[], // no clustered candidates
            &[],
            &all,
        );
        assert!(violation);
    }
}
