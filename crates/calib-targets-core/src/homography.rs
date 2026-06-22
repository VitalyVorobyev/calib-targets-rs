//! Projective homography type and estimators.
//!
//! Homographies in this crate map source-frame points to destination-frame
//! points as `p_dst ~ H * p_src`. Detector code uses this for rectified-grid
//! to image-frame mappings; residuals are measured in image pixels.

use crate::{sample_bilinear_u8, GrayImage, GrayImageView};
use nalgebra::{Matrix3, Point2, Projective2, RealField, SMatrix, SVector, Vector3};

fn lit<F: RealField + Copy>(val: f64) -> F {
    F::from_subset(&val)
}

/// A 3×3 projective homography matrix.
///
/// Maps 2D points between two projective planes: `p_dst ~ H * p_src`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homography<F = f32> {
    /// The raw 3×3 matrix. Defined up to an overall scale; estimators in this
    /// module normalize it so the bottom-right entry is `1`.
    pub h: Matrix3<F>,
}

/// Numerical quality of a homography matrix.
///
/// **Diagnostic only — not a scale-stable stability gate.** The
/// absolute-magnitude fields (`min_singular_value`, `determinant`) depend on
/// the coordinate scale of the points `H` was fit from, so a single absolute
/// threshold is not portable across image scales. Use for inspection and
/// relative comparison; for production gating prefer a pixel-unit re-projection
/// residual.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct HomographyQuality<F = f32> {
    /// Largest singular value of `H`.
    pub max_singular_value: F,
    /// Smallest singular value of `H`.
    pub min_singular_value: F,
    /// Condition number `max_singular_value / min_singular_value`.
    pub condition: F,
    /// Determinant of `H`.
    pub determinant: F,
}

impl<F: RealField + Copy> HomographyQuality<F> {
    /// Compute quality from a homography matrix.
    pub fn from_homography(h: &Homography<F>) -> Self {
        let svd = h.h.svd(false, false);
        let mut s_max = F::zero();
        let mut s_min = lit(1e30);
        for k in 0..3 {
            let s = svd.singular_values[k];
            if s > s_max {
                s_max = s;
            }
            if s < s_min {
                s_min = s;
            }
        }
        let condition = if s_min > F::default_epsilon() {
            s_max / s_min
        } else {
            lit(1e30)
        };
        Self {
            max_singular_value: s_max,
            min_singular_value: s_min,
            condition,
            determinant: h.h.determinant(),
        }
    }

    /// `true` when `min_singular_value < threshold`.
    ///
    /// **Diagnostic only**: `min_singular_value` scales with the coordinate
    /// magnitudes of `H`, so a single threshold is not portable across image
    /// scales. Not a production stability gate.
    pub fn is_ill_conditioned(&self, min_singular_value_threshold: F) -> bool {
        self.min_singular_value < min_singular_value_threshold
    }
}

impl<F: RealField + Copy> Homography<F> {
    /// Wrap an existing 3×3 matrix as a homography. The matrix is taken as-is;
    /// no normalization is applied.
    pub fn new(h: Matrix3<F>) -> Self {
        Self { h }
    }

    /// Build a homography from a row-major `[[row0], [row1], [row2]]` array.
    pub fn from_array(rows: [[F; 3]; 3]) -> Self {
        Self::new(Matrix3::from_row_slice(&[
            rows[0][0], rows[0][1], rows[0][2], rows[1][0], rows[1][1], rows[1][2], rows[2][0],
            rows[2][1], rows[2][2],
        ]))
    }

    /// Return the matrix as a row-major `[[row0], [row1], [row2]]` array.
    pub fn to_array(&self) -> [[F; 3]; 3] {
        [
            [self.h[(0, 0)], self.h[(0, 1)], self.h[(0, 2)]],
            [self.h[(1, 0)], self.h[(1, 1)], self.h[(1, 2)]],
            [self.h[(2, 0)], self.h[(2, 1)], self.h[(2, 2)]],
        ]
    }

    /// A homography backed by the all-zeros matrix. Not invertible; used only
    /// as a placeholder before a real estimate is available.
    pub fn zero() -> Self {
        Self {
            h: Matrix3::zeros(),
        }
    }

    /// Apply the homography to a 2D point.
    #[inline]
    pub fn apply(&self, p: Point2<F>) -> Point2<F> {
        let v = self.h * Vector3::new(p.x, p.y, F::one());
        let w = v[2];
        Point2::new(v[0] / w, v[1] / w)
    }

    /// Compute the inverse homography, if the matrix is invertible.
    pub fn inverse(&self) -> Option<Self> {
        self.h.try_inverse().map(Self::new)
    }
}

/// Estimate H such that `p_dst ~ H * p_src` from N >= 4 point
/// correspondences.
///
/// Uses Hartley normalization + DLT for N > 4 and a direct 4-point solver for
/// N == 4.
pub fn estimate_homography_rect_to_img<F: RealField + Copy>(
    src_pts: &[Point2<F>],
    dst_pts: &[Point2<F>],
) -> Option<Homography<F>> {
    if src_pts.len() != dst_pts.len() || src_pts.len() < 4 {
        return None;
    }

    if src_pts.len() == 4 {
        let src: &[Point2<F>; 4] = src_pts.try_into().ok()?;
        let dst: &[Point2<F>; 4] = dst_pts.try_into().ok()?;
        return homography_from_4pt(src, dst);
    }

    let (r, tr) = normalize_points(src_pts);
    let (im, ti) = normalize_points(dst_pts);

    let mut m: SMatrix<F, 9, 9> = SMatrix::zeros();
    let zero = F::zero();
    let neg_one = -F::one();
    for k in 0..src_pts.len() {
        let x = r[k].x;
        let y = r[k].y;
        let u = im[k].x;
        let v = im[k].y;

        let row1 = SVector::<F, 9>::from_column_slice(&[
            -x,
            -y,
            neg_one,
            zero,
            zero,
            zero,
            u * x,
            u * y,
            u,
        ]);
        let row2 = SVector::<F, 9>::from_column_slice(&[
            zero,
            zero,
            zero,
            -x,
            -y,
            neg_one,
            v * x,
            v * y,
            v,
        ]);
        m += row1 * row1.transpose();
        m += row2 * row2.transpose();
    }

    let eig = m.symmetric_eigen();
    let mut min_idx = 0usize;
    let mut min_val = eig.eigenvalues[0];
    for k in 1..9 {
        if eig.eigenvalues[k] < min_val {
            min_val = eig.eigenvalues[k];
            min_idx = k;
        }
    }
    let h = eig.eigenvectors.column(min_idx);
    let hn = Matrix3::<F>::from_row_slice(&[h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], h[8]]);
    let h_den = denormalize_homography(hn, tr, ti)?;
    let h_den = normalize_homography(h_den)?;
    Some(Homography::new(h_den))
}

/// Estimate H such that `p_dst ~ H * p_src` and return numerical quality
/// metrics.
pub fn estimate_homography_with_quality<F: RealField + Copy>(
    src_pts: &[Point2<F>],
    dst_pts: &[Point2<F>],
) -> Option<(Homography<F>, HomographyQuality<F>)> {
    let h = estimate_homography_rect_to_img(src_pts, dst_pts)?;
    let q = HomographyQuality::from_homography(&h);
    Some((h, q))
}

/// Compute H from exactly 4 point correspondences: `dst ~ H * src`.
///
/// Uses Hartley normalization for numerical stability.
pub fn homography_from_4pt<F: RealField + Copy>(
    src: &[Point2<F>; 4],
    dst: &[Point2<F>; 4],
) -> Option<Homography<F>> {
    let (src_n, t_src) = normalize_points4(src);
    let (dst_n, t_dst) = normalize_points4(dst);

    let mut a = SMatrix::<F, 8, 8>::zeros();
    let mut b = SVector::<F, 8>::zeros();

    for k in 0..4 {
        let x = src_n[k].x;
        let y = src_n[k].y;
        let u = dst_n[k].x;
        let v = dst_n[k].y;

        let r0 = 2 * k;
        a[(r0, 0)] = x;
        a[(r0, 1)] = y;
        a[(r0, 2)] = F::one();
        a[(r0, 6)] = -u * x;
        a[(r0, 7)] = -u * y;
        b[r0] = u;

        let r1 = 2 * k + 1;
        a[(r1, 3)] = x;
        a[(r1, 4)] = y;
        a[(r1, 5)] = F::one();
        a[(r1, 6)] = -v * x;
        a[(r1, 7)] = -v * y;
        b[r1] = v;
    }

    let x = a.lu().solve(&b)?;
    let hn = Matrix3::<F>::new(x[0], x[1], x[2], x[3], x[4], x[5], x[6], x[7], F::one());

    let h_den = denormalize_homography(hn, t_src, t_dst)?;
    let h_den = normalize_homography(h_den)?;
    Some(Homography::new(h_den))
}

/// 4-point variant of [`estimate_homography_with_quality`].
pub fn homography_from_4pt_with_quality<F: RealField + Copy>(
    src: &[Point2<F>; 4],
    dst: &[Point2<F>; 4],
) -> Option<(Homography<F>, HomographyQuality<F>)> {
    let h = homography_from_4pt(src, dst)?;
    let q = HomographyQuality::from_homography(&h);
    Some((h, q))
}

/// Convert [`Homography`] into the next crate's projective transform
/// representation. The underlying 3×3 matrix is copied directly.
#[inline]
pub fn homography_to_next(h: Homography<f32>) -> Projective2<f32> {
    Projective2::from_matrix_unchecked(h.h)
}

/// Project a [`Projective2<f32>`] back into [`Homography`].
#[inline]
pub fn homography_from_next(h: Projective2<f32>) -> Homography<f32> {
    Homography::new(h.into_inner())
}

/// Warp into a rectified image: for each destination pixel center, map to
/// source image coordinates via `H_img_from_rect` and bilinear-sample.
pub fn warp_perspective_gray(
    src: &GrayImageView<'_>,
    h_img_from_rect: Homography,
    out_w: usize,
    out_h: usize,
) -> GrayImage {
    let mut out = vec![0u8; out_w * out_h];

    for y in 0..out_h {
        for x in 0..out_w {
            let pr = Point2::new(x as f32 + 0.5, y as f32 + 0.5);
            let pi = h_img_from_rect.apply(pr);
            let v = sample_bilinear_u8(src, pi.x, pi.y);
            out[y * out_w + x] = v;
        }
    }

    GrayImage {
        width: out_w,
        height: out_h,
        data: out,
    }
}

fn hartley_normalization<F: RealField + Copy>(cx: F, cy: F, mean_dist: F) -> Matrix3<F> {
    let s = if mean_dist > lit(1e-12) {
        lit::<F>(2.0).sqrt() / mean_dist
    } else {
        F::one()
    };

    Matrix3::new(
        s,
        F::zero(),
        -s * cx,
        F::zero(),
        s,
        -s * cy,
        F::zero(),
        F::zero(),
        F::one(),
    )
}

fn normalize_points<F: RealField + Copy>(pts: &[Point2<F>]) -> (Vec<Point2<F>>, Matrix3<F>) {
    let n = lit(pts.len() as f64);
    let mut cx = F::zero();
    let mut cy = F::zero();
    for p in pts {
        cx += p.x;
        cy += p.y;
    }
    cx /= n;
    cy /= n;

    let mut mean_dist = F::zero();
    for p in pts {
        let dx = p.x - cx;
        let dy = p.y - cy;
        mean_dist += (dx * dx + dy * dy).sqrt();
    }
    mean_dist /= n;

    let t = hartley_normalization(cx, cy, mean_dist);
    let mut out = Vec::with_capacity(pts.len());
    for p in pts {
        let v = t * Vector3::new(p.x, p.y, F::one());
        out.push(Point2::new(v[0], v[1]));
    }
    (out, t)
}

fn normalize_points4<F: RealField + Copy>(pts: &[Point2<F>; 4]) -> ([Point2<F>; 4], Matrix3<F>) {
    let n = lit(4.0);
    let mut cx = F::zero();
    let mut cy = F::zero();
    for p in pts {
        cx += p.x;
        cy += p.y;
    }
    cx /= n;
    cy /= n;

    let mut mean_dist = F::zero();
    for p in pts {
        let dx = p.x - cx;
        let dy = p.y - cy;
        mean_dist += (dx * dx + dy * dy).sqrt();
    }
    mean_dist /= n;

    let t = hartley_normalization(cx, cy, mean_dist);
    let mut out = [Point2::new(F::zero(), F::zero()); 4];
    for (i, p) in pts.iter().enumerate() {
        let v = t * Vector3::new(p.x, p.y, F::one());
        out[i] = Point2::new(v[0], v[1]);
    }
    (out, t)
}

fn normalize_homography<F: RealField + Copy>(h: Matrix3<F>) -> Option<Matrix3<F>> {
    let s = h[(2, 2)];
    if s.abs() < lit(1e-12) {
        return None;
    }
    Some(h / s)
}

fn denormalize_homography<F: RealField + Copy>(
    hn: Matrix3<F>,
    t_src: Matrix3<F>,
    t_dst: Matrix3<F>,
) -> Option<Matrix3<F>> {
    let t_dst_inv = t_dst.try_inverse()?;
    Some(t_dst_inv * hn * t_src)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: Point2<f32>, b: Point2<f32>, tol: f32) {
        let dx = (a.x - b.x).abs();
        let dy = (a.y - b.y).abs();
        assert!(
            dx < tol && dy < tol,
            "expected ({:.6},{:.6}) ~ ({:.6},{:.6}) within {}",
            a.x,
            a.y,
            b.x,
            b.y,
            tol
        );
    }

    #[test]
    fn four_point_fit_maps_tl_tr_br_bl() {
        let rect = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(100.0, 0.0),
            Point2::new(100.0, 80.0),
            Point2::new(0.0, 80.0),
        ];
        let image = [
            Point2::new(20.0_f32, 30.0),
            Point2::new(130.0, 25.0),
            Point2::new(140.0, 120.0),
            Point2::new(15.0, 115.0),
        ];

        let h = homography_from_4pt(&rect, &image).expect("fit");
        for (src, dst) in rect.into_iter().zip(image) {
            assert_close(h.apply(src), dst, 1e-3);
        }
    }

    #[test]
    fn overdetermined_estimate_recovers_clean_grid() {
        let ground_truth = Homography::new(Matrix3::new(
            1.0, 0.2, 12.0, -0.1, 0.9, 6.0, 0.0006, 0.0004, 1.0,
        ));
        let rect: Vec<Point2<f32>> = (0..3)
            .flat_map(|y| (0..3).map(move |x| Point2::new(x as f32 * 40.0, y as f32 * 50.0)))
            .collect();
        let img: Vec<Point2<f32>> = rect.iter().map(|&p| ground_truth.apply(p)).collect();

        let estimated = estimate_homography_rect_to_img(&rect, &img).expect("estimate");
        for p in [
            Point2::new(0.0_f32, 0.0),
            Point2::new(60.0, 40.0),
            Point2::new(80.0, 90.0),
        ] {
            assert_close(estimated.apply(p), ground_truth.apply(p), 1e-3);
        }
    }

    #[test]
    fn projective2_bridge_round_trips() {
        let h = Homography::new(Matrix3::new(
            1.2, 0.1, 5.0, -0.05, 0.9, 3.0, 0.001, 0.0005, 1.0,
        ));
        let bridged = homography_from_next(homography_to_next(h));
        for r in 0..3 {
            for c in 0..3 {
                assert_eq!(bridged.h[(r, c)], h.h[(r, c)]);
            }
        }
    }

    #[test]
    fn quality_reports_finite_metrics_for_clean_homography() {
        let rect = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(100.0, 0.0),
            Point2::new(100.0, 100.0),
            Point2::new(0.0, 100.0),
        ];
        let dst = [
            Point2::new(50.0_f32, 50.0),
            Point2::new(150.0, 60.0),
            Point2::new(140.0, 160.0),
            Point2::new(40.0, 150.0),
        ];
        let (_, q) = homography_from_4pt_with_quality(&rect, &dst).expect("h");
        assert!(q.max_singular_value.is_finite() && q.max_singular_value > 0.0);
        assert!(q.min_singular_value > 0.0);
        assert!(q.condition.is_finite());
        assert!(q.determinant.abs() > 1e-3);
    }
}
