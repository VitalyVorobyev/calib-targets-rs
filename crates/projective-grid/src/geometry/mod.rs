//! Geometry helpers shared by grid tasks.
//!
//! Owns the crate's homography estimators: the public [`estimate_projective`]
//! (returning `nalgebra::Projective2`, used for the final lattice fit and
//! consistency check; always solved in `f64`) and the [`homography`]
//! submodule's Hartley-normalised DLT + quality (returning the [`Homography`]
//! wrapper in the caller's float type, used for local per-cell fits in seed,
//! validate, and extension). Both share one normalised DLT core.

pub mod homography;
pub use homography::{
    estimate_homography, estimate_homography_with_quality, homography_from_4pt,
    homography_from_4pt_with_quality, Homography, HomographyQuality,
};

use nalgebra::{Matrix3, Point2, Projective2, Vector3};

use crate::error::{GridError, Result};
use crate::float::{lit, Float};

/// Estimate a projective transform from model-plane points to image points.
///
/// A least-squares homogeneous DLT: both point sets are Hartley-normalised and
/// the system is solved in `f64` regardless of `F`, so exact correspondences
/// fit to rounding error even with image coordinates in the thousands of
/// pixels. The returned matrix is scaled so its bottom-right entry is `1`.
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

    // Solve in f64 whatever `F` is: image coordinates in the thousands of
    // pixels leave an f32 system too little precision for a sub-pixel fit.
    let model_f64 = to_f64_points(model_points).ok_or(GridError::DegenerateGeometry)?;
    let image_f64 = to_f64_points(image_points).ok_or(GridError::DegenerateGeometry)?;
    let fitted = homography::estimate_homography_dlt(&model_f64, &image_f64)
        .ok_or(GridError::DegenerateGeometry)?;
    let matrix: Matrix3<F> = fitted.h.map(nalgebra::convert::<f64, F>);
    if matrix.iter().any(|x| !x.is_finite()) {
        return Err(GridError::DegenerateGeometry);
    }

    Ok(Projective2::from_matrix_unchecked(matrix))
}

/// Widen points to `f64`; `None` when a coordinate is not representable.
fn to_f64_points<F: Float>(points: &[Point2<F>]) -> Option<Vec<Point2<f64>>> {
    points
        .iter()
        .map(|p| Some(Point2::new(p.x.to_subset()?, p.y.to_subset()?)))
        .collect()
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

    /// Exact projected centres of a 1,239-node hex board (37 rows of 33 / 34
    /// nodes) under perspective, in `f32` image coordinates in the low
    /// thousands of pixels. Returns `(model, image)` correspondences.
    fn perspective_hex_board() -> (Vec<Point2<f32>>, Vec<Point2<f32>>) {
        let h = Matrix3::new(
            11.18, -44.43, 1713.9, //
            51.10, 13.89, 146.0, //
            0.0018, 0.00426, 1.0,
        );
        let sqrt3_2 = 3.0_f64.sqrt() * 0.5;
        let mut model = Vec::new();
        let mut image = Vec::new();
        for row in 0..37 {
            let (count, shift) = if row % 2 == 0 { (33, 0.5) } else { (34, 0.0) };
            for col in 0..count {
                let m = Vector3::new(col as f64 + shift, row as f64 * sqrt3_2, 1.0);
                let p = h * m;
                model.push(Point2::new(m.x as f32, m.y as f32));
                image.push(Point2::new((p.x / p.z) as f32, (p.y / p.z) as f32));
            }
        }
        (model, image)
    }

    fn max_residual_px(h: &Projective2<f32>, model: &[Point2<f32>], image: &[Point2<f32>]) -> f32 {
        model
            .iter()
            .zip(image)
            .map(|(&m, &p)| (apply_projective(h, m).unwrap() - p).norm())
            .fold(0.0, f32::max)
    }

    /// Exact data must fit exactly. An `f32` DLT with `h33 = 1` and no
    /// conditioning left pixels of residual on this board, so a 2 px residual
    /// gate rejected correct labels.
    #[test]
    fn estimate_projective_is_exact_on_a_large_perspective_board() {
        let (model, image) = perspective_hex_board();
        assert_eq!(model.len(), 1239);
        let h = estimate_projective(&model, &image).unwrap();
        let max = max_residual_px(&h, &model, &image);
        assert!(max < 0.01, "max residual {max} px on exact centres");
    }

    /// The fit must not depend on where the model frame's origin sits: the
    /// detector rebases labels and folds them by a lattice symmetry before
    /// fitting, so the same points arrive with shifted, mirrored model coords.
    #[test]
    fn estimate_projective_is_exact_under_model_frame_changes() {
        let (model, image) = perspective_hex_board();
        for (sx, tx, ty) in [(1.0, 40.0, 30.0), (-1.0, 0.0, 0.0), (-1.0, 200.0, -150.0)] {
            let moved: Vec<Point2<f32>> = model
                .iter()
                .map(|m| Point2::new(sx * m.x + tx, m.y + ty))
                .collect();
            let h = estimate_projective(&moved, &image).unwrap();
            let max = max_residual_px(&h, &moved, &image);
            assert!(
                max < 0.01,
                "max residual {max} px, model frame ({sx}, {tx}, {ty})"
            );
        }
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
