//! Float-generic 2D affine transform [`Affine2<F>`].
//!
//! Pure 2D linear algebra — no grid semantics. Used for per-triangle warps
//! in hex-grid meshes and for any affine fit on the square side.

use nalgebra::{Matrix2, Point2, Vector2};

use crate::float::Float;

/// A 2D affine transform: `dst = linear * [src_x, src_y]ᵀ + translation`.
///
/// Renamed from the legacy `AffineTransform2D`. The field layout is the
/// same; only the type name shortens.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct Affine2<F: Float> {
    /// 2×2 linear part.
    pub linear: Matrix2<F>,
    /// Translation part.
    pub translation: Vector2<F>,
}

impl<F: Float> Affine2<F> {
    /// Construct an affine transform from a linear part and a translation.
    pub fn new(linear: Matrix2<F>, translation: Vector2<F>) -> Self {
        Self {
            linear,
            translation,
        }
    }

    /// Compute the affine transform mapping the `src` triangle to the `dst`
    /// triangle.
    ///
    /// Returns `None` when the source triangle is degenerate (its two edges
    /// from `src[0]` are linearly dependent — i.e. the points are
    /// collinear).
    pub fn from_triangle_correspondence(src: [Point2<F>; 3], dst: [Point2<F>; 3]) -> Option<Self> {
        let ds1 = src[1] - src[0];
        let ds2 = src[2] - src[0];
        let dd1 = dst[1] - dst[0];
        let dd2 = dst[2] - dst[0];

        let src_mat = Matrix2::new(ds1.x, ds2.x, ds1.y, ds2.y);
        let src_inv = src_mat.try_inverse()?;

        let dst_mat = Matrix2::new(dd1.x, dd2.x, dd1.y, dd2.y);
        let linear = dst_mat * src_inv;

        let t = dst[0] - linear * Vector2::new(src[0].x, src[0].y);
        let translation = Vector2::new(t.x, t.y);

        Some(Self {
            linear,
            translation,
        })
    }

    /// Apply the transform to a 2D point.
    #[inline]
    pub fn apply(&self, p: Point2<F>) -> Point2<F> {
        let v = self.linear * Vector2::new(p.x, p.y) + self.translation;
        Point2::new(v.x, v.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::float::lit;

    fn assert_identity_triangle<F: Float>() {
        let tri = [
            Point2::<F>::new(F::zero(), F::zero()),
            Point2::new(F::one(), F::zero()),
            Point2::new(F::zero(), F::one()),
        ];
        let aff = Affine2::<F>::from_triangle_correspondence(tri, tri).expect("non-degenerate");
        // Mapping any point through an identity correspondence is a no-op
        // up to F's default epsilon.
        for p in [
            Point2::<F>::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(0.3_f32), lit::<F>(0.7_f32)),
            Point2::new(lit::<F>(2.0_f32), lit::<F>(-1.5_f32)),
        ] {
            let q = aff.apply(p);
            let tol = lit::<F>(1e-5_f32);
            assert!((q.x - p.x).abs() < tol);
            assert!((q.y - p.y).abs() < tol);
        }
    }

    fn assert_degenerate_returns_none<F: Float>() {
        // Three collinear points → no inverse for the 2×2 difference matrix.
        let collinear = [
            Point2::<F>::new(F::zero(), F::zero()),
            Point2::new(F::one(), F::zero()),
            Point2::new(lit::<F>(2.0_f32), F::zero()),
        ];
        let dst = [
            Point2::<F>::new(F::zero(), F::zero()),
            Point2::new(F::one(), F::zero()),
            Point2::new(lit::<F>(2.0_f32), F::zero()),
        ];
        assert!(Affine2::<F>::from_triangle_correspondence(collinear, dst).is_none());
    }

    fn assert_translation_recovered<F: Float>() {
        let tri = [
            Point2::<F>::new(F::zero(), F::zero()),
            Point2::new(F::one(), F::zero()),
            Point2::new(F::zero(), F::one()),
        ];
        let shift = Vector2::new(lit::<F>(5.0_f32), lit::<F>(-3.0_f32));
        let dst = tri.map(|p| Point2::new(p.x + shift.x, p.y + shift.y));
        let aff = Affine2::<F>::from_triangle_correspondence(tri, dst).expect("non-degenerate");
        let q = aff.apply(Point2::new(lit::<F>(2.0_f32), lit::<F>(2.0_f32)));
        assert!((q.x - (lit::<F>(2.0_f32) + shift.x)).abs() < lit::<F>(1e-5_f32));
        assert!((q.y - (lit::<F>(2.0_f32) + shift.y)).abs() < lit::<F>(1e-5_f32));
    }

    #[test]
    fn identity_triangle_f32() {
        assert_identity_triangle::<f32>();
    }
    #[test]
    fn identity_triangle_f64() {
        assert_identity_triangle::<f64>();
    }
    #[test]
    fn degenerate_returns_none_f32() {
        assert_degenerate_returns_none::<f32>();
    }
    #[test]
    fn degenerate_returns_none_f64() {
        assert_degenerate_returns_none::<f64>();
    }
    #[test]
    fn translation_recovered_f32() {
        assert_translation_recovered::<f32>();
    }
    #[test]
    fn translation_recovered_f64() {
        assert_translation_recovered::<f64>();
    }
}
