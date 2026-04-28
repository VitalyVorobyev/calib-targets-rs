//! Generic 2D affine transform.

use crate::Float;
use nalgebra::{Matrix2, Point2, Vector2};

/// A 2D affine transform: `dst = M * [src_x, src_y]^T + t`.
///
/// Used for per-triangle warps in the hex-grid mesh and for any
/// future square-grid affine fit. Pure 2D linear algebra — no grid
/// semantics.
#[derive(Clone, Copy, Debug)]
pub struct AffineTransform2D<F: Float = f32> {
    /// 2x2 linear part.
    pub linear: Matrix2<F>,
    /// Translation part.
    pub translation: Vector2<F>,
}

impl<F: Float> AffineTransform2D<F> {
    /// Compute the affine transform mapping `src` triangle to `dst` triangle.
    ///
    /// Returns `None` if the source triangle is degenerate (collinear points).
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
    pub fn apply(&self, p: Point2<F>) -> Point2<F> {
        let v = self.linear * Vector2::new(p.x, p.y) + self.translation;
        Point2::new(v.x, v.y)
    }
}
