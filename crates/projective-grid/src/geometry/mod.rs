//! Geometry helpers shared by grid tasks.
//!
//! Owns the crate's homography estimators: the public [`estimate_projective`]
//! (plain DLT returning `nalgebra::Projective2`, used for the final lattice
//! fit and consistency check) and the [`homography`] submodule's Hartley-
//! normalised DLT + quality (returning the [`Homography`] wrapper, used for
//! local per-cell fits in seed, validate, and extension).

pub mod homography;
pub use homography::{
    estimate_homography, estimate_homography_with_quality, homography_from_4pt,
    homography_from_4pt_with_quality, Homography, HomographyQuality,
};

use nalgebra::{DMatrix, DVector, Matrix3, Point2, Projective2, Vector3};

use crate::error::{GridError, Result};
use crate::float::{lit, Float};

/// Estimate a projective transform from model-plane points to image points.
///
/// Returns [`GridError::InsufficientEvidence`] for fewer than four
/// correspondences and [`GridError::DegenerateGeometry`] when either point set
/// has no two-dimensional spread or the direct linear transform cannot produce
/// a finite homography.
pub fn estimate_projective<F: Float>(
    model_points: &[Point2<F>],
    image_points: &[Point2<F>],
) -> Result<Projective2<F>> {
    if model_points.len() != image_points.len() {
        return Err(GridError::InconsistentInput(format!(
            "model/image correspondence count mismatch: model={}, image={}",
            model_points.len(),
            image_points.len()
        )));
    }
    if model_points.len() < 4 {
        return Err(GridError::InsufficientEvidence);
    }
    if !has_two_dimensional_spread(model_points) || !has_two_dimensional_spread(image_points) {
        return Err(GridError::DegenerateGeometry);
    }

    let rows = model_points.len() * 2;
    let mut a = DMatrix::<F>::zeros(rows, 8);
    let mut b = DVector::<F>::zeros(rows);
    for (idx, (src, dst)) in model_points.iter().zip(image_points).enumerate() {
        let x = src.x;
        let y = src.y;
        let u = dst.x;
        let v = dst.y;
        let r0 = 2 * idx;
        let r1 = r0 + 1;

        a[(r0, 0)] = x;
        a[(r0, 1)] = y;
        a[(r0, 2)] = F::one();
        a[(r0, 6)] = -u * x;
        a[(r0, 7)] = -u * y;
        b[r0] = u;

        a[(r1, 3)] = x;
        a[(r1, 4)] = y;
        a[(r1, 5)] = F::one();
        a[(r1, 6)] = -v * x;
        a[(r1, 7)] = -v * y;
        b[r1] = v;
    }

    let svd = a.svd(true, true);
    let eps = lit::<F>(1e-12);
    let h = svd
        .solve(&b, eps)
        .map_err(|_| GridError::DegenerateGeometry)?;
    let matrix = Matrix3::new(h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], F::one());
    if matrix.iter().any(|x| !x.is_finite()) {
        return Err(GridError::DegenerateGeometry);
    }

    Ok(Projective2::from_matrix_unchecked(matrix))
}

/// Apply a projective transform to a point and return `None` when the
/// homogeneous denominator is zero or non-finite.
pub fn apply_projective<F: Float>(
    transform: &Projective2<F>,
    point: Point2<F>,
) -> Option<Point2<F>> {
    let h = transform.matrix();
    let p = h * Vector3::new(point.x, point.y, F::one());
    let eps = lit::<F>(1e-12);
    if !p.z.is_finite() || p.z.abs() <= eps {
        return None;
    }
    Some(Point2::new(p.x / p.z, p.y / p.z))
}

fn has_two_dimensional_spread<F: Float>(points: &[Point2<F>]) -> bool {
    let eps = lit::<F>(1e-8);
    for a in 0..points.len() {
        for b in (a + 1)..points.len() {
            for c in (b + 1)..points.len() {
                let ab = points[b] - points[a];
                let ac = points[c] - points[a];
                let cross = ab.x * ac.y - ab.y * ac.x;
                if cross.abs() > eps {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_projective_recovers_translation_scale() {
        let src = [
            Point2::new(0.0_f64, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(1.0, 1.0),
        ];
        let dst = src.map(|p| Point2::new(10.0 + 2.0 * p.x, -3.0 + 3.0 * p.y));
        let h = estimate_projective(&src, &dst).unwrap();
        let q = apply_projective(&h, Point2::new(0.25, 0.5)).unwrap();
        assert!((q.x - 10.5).abs() < 1e-9);
        assert!((q.y + 1.5).abs() < 1e-9);
    }

    #[test]
    fn estimate_projective_rejects_collinear_model_points() {
        let src = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(3.0, 0.0),
        ];
        let dst = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(3.0, 0.0),
        ];
        assert_eq!(
            estimate_projective(&src, &dst).unwrap_err(),
            GridError::DegenerateGeometry
        );
    }
}
