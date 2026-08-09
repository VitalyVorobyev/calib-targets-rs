//! Neighbour-midpoint position prediction on a labelled lattice.
//!
//! Given a map of already-labelled lattice coordinates to image positions,
//! [`predict_grid_position`] predicts where a coordinate *should* sit by
//! averaging the midpoints of its opposite neighbour pairs, one pair per
//! lattice axis family (2 on square, 3 on hex axial). The prediction is
//! purely local and homography-free, so it tolerates lens distortion that a
//! single global projective fit cannot model.
//!
//! Because it only ever *interpolates* between opposite neighbours, the
//! prediction is available exactly where it is reliable: a coordinate on the
//! outer frontier of the labelled set (no complete opposite pair) yields
//! `None` rather than an extrapolated guess. Typical uses are smoothness
//! checks (compare a detected position against its neighbour-midpoint
//! prediction to flag outliers) and seeding a local search for a lattice
//! point that the feature detector missed.

use std::collections::HashMap;

use nalgebra::Point2;

use crate::float::{lit, Float};

use super::{Coord, LatticeKind};

/// A neighbour-midpoint position prediction from [`predict_grid_position`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct PredictedPosition<F: Float> {
    /// Predicted image position: the average of the available opposite-pair
    /// midpoints.
    pub position: Point2<F>,
    /// Number of complete opposite neighbour pairs the prediction averages
    /// (1..= the family's axis count). More pairs constrain the prediction
    /// more tightly.
    pub n_axis_pairs: usize,
}

/// Predict a lattice coordinate's image position from its labelled
/// neighbours.
///
/// For each axis family of `kind`, if both opposite neighbours of `at` are
/// present in `labelled`, their midpoint predicts `at`; the returned position
/// averages the available midpoints. Returns `None` when no complete
/// opposite pair exists — this function interpolates only, it never
/// extrapolates outward from the labelled set.
///
/// On a square lattice the pairs are `(±1, 0)` and `(0, ±1)`; on a hex axial
/// lattice they are `(±1, 0)`, `(0, ±1)`, and `±(1, -1)`.
///
/// ```
/// use nalgebra::Point2;
/// use projective_grid::{Coord, LatticeKind};
/// use projective_grid::expert::lattice::predict_grid_position;
/// use std::collections::HashMap;
///
/// let mut labelled: HashMap<Coord, Point2<f32>> = HashMap::new();
/// labelled.insert(Coord::new(-1, 0), Point2::new(10.0, 50.0));
/// labelled.insert(Coord::new(1, 0), Point2::new(90.0, 50.0));
///
/// let pred = predict_grid_position(&labelled, Coord::new(0, 0), LatticeKind::Square).unwrap();
/// assert_eq!(pred.position, Point2::new(50.0, 50.0));
/// assert_eq!(pred.n_axis_pairs, 1);
///
/// // A frontier coordinate with no opposite pair is not predicted.
/// assert!(predict_grid_position(&labelled, Coord::new(2, 0), LatticeKind::Square).is_none());
/// ```
pub fn predict_grid_position<F: Float>(
    labelled: &HashMap<Coord, Point2<F>>,
    at: Coord,
    kind: LatticeKind,
) -> Option<PredictedPosition<F>> {
    let half: F = lit(0.5);
    let mut sum_x = F::zero();
    let mut sum_y = F::zero();
    let mut n_axis_pairs = 0usize;

    for &off in kind.neighbour_offsets() {
        // Each opposite pair {off, -off} appears twice in the neighbour set;
        // keep the lexicographically-positive representative.
        if off.u < 0 || (off.u == 0 && off.v < 0) {
            continue;
        }
        let fwd = Coord::new(at.u + off.u, at.v + off.v);
        let bwd = Coord::new(at.u - off.u, at.v - off.v);
        if let (Some(pf), Some(pb)) = (labelled.get(&fwd), labelled.get(&bwd)) {
            sum_x += half * (pf.x + pb.x);
            sum_y += half * (pf.y + pb.y);
            n_axis_pairs += 1;
        }
    }

    if n_axis_pairs == 0 {
        return None;
    }
    let n: F = lit(n_axis_pairs as f32);
    Some(PredictedPosition {
        position: Point2::new(sum_x / n, sum_y / n),
        n_axis_pairs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nalgebra::Matrix3;

    fn assert_close(actual: Point2<f64>, expected: Point2<f64>) {
        let err = (actual - expected).norm();
        assert!(
            err < 1e-9,
            "expected {expected:?}, got {actual:?} (err {err:e})"
        );
    }

    fn square_grid(n: i32, spacing: f64) -> HashMap<Coord, Point2<f64>> {
        let mut map = HashMap::new();
        for v in 0..n {
            for u in 0..n {
                map.insert(
                    Coord::new(u, v),
                    Point2::new(u as f64 * spacing, v as f64 * spacing),
                );
            }
        }
        map
    }

    fn hex_grid(radius: i32, spacing: f64) -> HashMap<Coord, Point2<f64>> {
        let sqrt3 = 3.0f64.sqrt();
        let mut map = HashMap::new();
        for q in -radius..=radius {
            for r in -radius..=radius {
                if (q + r).abs() > radius {
                    continue;
                }
                let x = spacing * (q as f64 + r as f64 * 0.5);
                let y = spacing * (r as f64 * sqrt3 / 2.0);
                map.insert(Coord::new(q, r), Point2::new(x, y));
            }
        }
        map
    }

    fn warp(map: &mut HashMap<Coord, Point2<f64>>, h: &Matrix3<f64>) {
        for p in map.values_mut() {
            let w = h * nalgebra::Vector3::new(p.x, p.y, 1.0);
            *p = Point2::new(w.x / w.z, w.y / w.z);
        }
    }

    #[test]
    fn square_interior_predicts_exactly() {
        let grid = square_grid(5, 40.0);
        let pred = predict_grid_position(&grid, Coord::new(2, 2), LatticeKind::Square).unwrap();
        assert_close(pred.position, Point2::new(80.0, 80.0));
        assert_eq!(pred.n_axis_pairs, 2);
    }

    #[test]
    fn square_missing_cell_is_predicted_from_neighbours() {
        let mut grid = square_grid(5, 40.0);
        grid.remove(&Coord::new(2, 2));
        let pred = predict_grid_position(&grid, Coord::new(2, 2), LatticeKind::Square).unwrap();
        assert_close(pred.position, Point2::new(80.0, 80.0));
        assert_eq!(pred.n_axis_pairs, 2);
    }

    #[test]
    fn square_edge_uses_single_pair() {
        let grid = square_grid(5, 40.0);
        // (2, 0): vertical pair needs (2, -1) which does not exist.
        let pred = predict_grid_position(&grid, Coord::new(2, 0), LatticeKind::Square).unwrap();
        assert_eq!(pred.n_axis_pairs, 1);
        assert_close(pred.position, Point2::new(80.0, 0.0));
    }

    #[test]
    fn square_corner_has_no_pair() {
        let grid = square_grid(5, 40.0);
        assert!(predict_grid_position(&grid, Coord::new(0, 0), LatticeKind::Square).is_none());
        assert!(predict_grid_position(&grid, Coord::new(-1, 2), LatticeKind::Square).is_none());
    }

    #[test]
    fn hex_interior_averages_three_pairs() {
        let grid = hex_grid(3, 60.0);
        let pred = predict_grid_position(&grid, Coord::new(0, 0), LatticeKind::Hex).unwrap();
        assert_eq!(pred.n_axis_pairs, 3);
        assert_close(pred.position, Point2::new(0.0, 0.0));
    }

    #[test]
    fn hex_frontier_yields_none() {
        let grid = hex_grid(2, 60.0);
        // (3, 0) is outside the radius-2 disc with no bracketing neighbours.
        assert!(predict_grid_position(&grid, Coord::new(3, 0), LatticeKind::Hex).is_none());
    }

    #[test]
    fn perspective_warp_keeps_prediction_close() {
        // Midpoint interpolation is exact for affine maps and first-order
        // accurate under projective warps; a mild perspective term must keep
        // the residual well under a pixel at 40 px pitch.
        let h = Matrix3::new(
            0.9, 0.05, 12.0, //
            -0.04, 1.1, -7.0, //
            2e-4, 1e-4, 1.0,
        );
        for (kind, mut grid) in [
            (LatticeKind::Square, square_grid(7, 40.0)),
            (LatticeKind::Hex, hex_grid(3, 40.0)),
        ] {
            warp(&mut grid, &h);
            for (&at, &truth) in &grid {
                let Some(pred) = predict_grid_position(&grid, at, kind) else {
                    continue;
                };
                let err = (pred.position - truth).norm();
                assert!(
                    err < 0.5,
                    "{kind:?} at {at:?}: prediction off by {err:.3} px"
                );
            }
        }
    }

    #[test]
    fn displaced_point_does_not_poison_its_own_prediction() {
        // The prediction at `at` never reads `labelled[at]`, so a displaced
        // detection is still compared against a clean neighbour consensus.
        let mut grid = square_grid(5, 40.0);
        grid.insert(Coord::new(2, 2), Point2::new(95.0, 71.0));
        let pred = predict_grid_position(&grid, Coord::new(2, 2), LatticeKind::Square).unwrap();
        assert_close(pred.position, Point2::new(80.0, 80.0));
    }
}
