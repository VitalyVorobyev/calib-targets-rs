use crate::float_helpers::lit;
use crate::Float;
use nalgebra::{Matrix3, Point2, SMatrix, SVector, Vector3};

/// A 3×3 projective homography matrix.
///
/// Maps 2D points between two projective planes: `p_dst ~ H * p_src`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homography<F: Float = f32> {
    pub h: Matrix3<F>,
}

/// Numerical quality of a homography matrix.
///
/// Computed from the SVD of the 3×3 matrix `H`. Use these fields to gate
/// downstream consumers (mesh cells, validators) against degenerate
/// solutions: a near-singular `H` arises when the source or destination
/// quad has three near-collinear points and would lead to large
/// re-projection error away from the fitted points.
///
/// `condition` is a unitless ratio; the other two are in the same units
/// as `H`'s entries.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct HomographyQuality<F: Float = f32> {
    /// Largest singular value of `H`.
    pub max_singular_value: F,
    /// Smallest singular value of `H`. Approaches zero as the source or
    /// destination quad degenerates (three collinear points).
    pub min_singular_value: F,
    /// Condition number `max_singular_value / min_singular_value`. Healthy
    /// homographies for calibration targets sit below ~100; values above
    /// ~10⁴ indicate the fit is dominated by noise on the smallest
    /// principal direction.
    pub condition: F,
    /// Determinant of `H`. Sign indicates orientation; `|det|` decays to
    /// zero as the map becomes singular.
    pub determinant: F,
}

impl<F: Float> HomographyQuality<F> {
    /// Compute quality from a homography matrix.
    pub fn from_homography(h: &Homography<F>) -> Self {
        let svd = h.h.svd(false, false);
        let mut s_max = F::zero();
        let mut s_min = F::max_value().unwrap_or_else(|| lit(1e30));
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
            F::max_value().unwrap_or_else(|| lit(1e30))
        };
        let determinant = h.h.determinant();
        Self {
            max_singular_value: s_max,
            min_singular_value: s_min,
            condition,
            determinant,
        }
    }

    /// `true` when `min_singular_value < threshold`. Useful as a single
    /// boolean gate for `mesh::from_corners_with_min_singular_value` and
    /// similar opt-in conditioning checks.
    pub fn is_ill_conditioned(&self, min_singular_value_threshold: F) -> bool {
        self.min_singular_value < min_singular_value_threshold
    }
}

impl<F: Float> Homography<F> {
    pub fn new(h: Matrix3<F>) -> Self {
        Self { h }
    }

    pub fn from_array(rows: [[F; 3]; 3]) -> Self {
        Self::new(Matrix3::from_row_slice(&[
            rows[0][0], rows[0][1], rows[0][2], rows[1][0], rows[1][1], rows[1][2], rows[2][0],
            rows[2][1], rows[2][2],
        ]))
    }

    pub fn to_array(&self) -> [[F; 3]; 3] {
        [
            [self.h[(0, 0)], self.h[(0, 1)], self.h[(0, 2)]],
            [self.h[(1, 0)], self.h[(1, 1)], self.h[(1, 2)]],
            [self.h[(2, 0)], self.h[(2, 1)], self.h[(2, 2)]],
        ]
    }

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

// ---- Hartley normalization ----

fn hartley_normalization<F: Float>(cx: F, cy: F, mean_dist: F) -> Matrix3<F> {
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

fn normalize_points<F: Float>(pts: &[Point2<F>]) -> (Vec<Point2<F>>, Matrix3<F>) {
    let n: F = lit(pts.len() as f64);
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

fn normalize_points4<F: Float>(pts: &[Point2<F>; 4]) -> ([Point2<F>; 4], Matrix3<F>) {
    let n: F = lit(4.0);
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

fn normalize_homography<F: Float>(h: Matrix3<F>) -> Option<Matrix3<F>> {
    let s = h[(2, 2)];
    if s.abs() < lit(1e-12) {
        return None;
    }
    Some(h / s)
}

fn denormalize_homography<F: Float>(
    hn: Matrix3<F>,
    t_src: Matrix3<F>,
    t_dst: Matrix3<F>,
) -> Option<Matrix3<F>> {
    let t_dst_inv = t_dst.try_inverse()?;
    Some(t_dst_inv * hn * t_src)
}

/// Estimate H such that `p_dst ~ H * p_src` from N >= 4 point correspondences,
/// returning the matrix together with its [`HomographyQuality`].
///
/// Wraps [`estimate_homography`]; callers that already discard the quality
/// data can keep using that. Use this entry point to reject cells whose
/// minimum singular value falls below a tolerance, or to log conditioning
/// information for diagnostic surfaces.
pub fn estimate_homography_with_quality<F: Float>(
    src_pts: &[Point2<F>],
    dst_pts: &[Point2<F>],
) -> Option<(Homography<F>, HomographyQuality<F>)> {
    let h = estimate_homography(src_pts, dst_pts)?;
    let q = HomographyQuality::from_homography(&h);
    Some((h, q))
}

/// 4-point variant of [`estimate_homography_with_quality`].
pub fn homography_from_4pt_with_quality<F: Float>(
    src: &[Point2<F>; 4],
    dst: &[Point2<F>; 4],
) -> Option<(Homography<F>, HomographyQuality<F>)> {
    let h = homography_from_4pt(src, dst)?;
    let q = HomographyQuality::from_homography(&h);
    Some((h, q))
}

/// Estimate H such that `p_dst ~ H * p_src` from N >= 4 point correspondences.
///
/// Uses Hartley normalization + DLT for N > 4 and a direct 4-point solver for N == 4.
pub fn estimate_homography<F: Float>(
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

    let n = src_pts.len();
    let zero = F::zero();
    let neg_one = -F::one();

    // Accumulate Aᵀ A directly into a stack 9×9 without ever
    // materialising the (2N × 9) matrix A. For correspondence
    // `(x, y) ↦ (u, v)` the two DLT rows are
    //   row₂ₖ   = [-x, -y, -1,  0,  0,  0, ux, uy, u]
    //   row₂ₖ₊₁ = [ 0,  0,  0, -x, -y, -1, vx, vy, v]
    // and Aᵀ A = Σₖ (rowᵀ row) summed over both rows of each pair.
    let mut m: SMatrix<F, 9, 9> = SMatrix::zeros();
    for k in 0..n {
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

    // The right singular vector of A for σ_min is the eigenvector of
    // Aᵀ A for the smallest eigenvalue: A = U Σ Vᵀ ⇒ Aᵀ A = V Σ² Vᵀ.
    // Hartley normalisation keeps cond(A) ≲ 10³, so cond(Aᵀ A) ≲ 10⁶ —
    // comfortably within f32 precision. Sign is unconstrained here but
    // pinned downstream by `normalize_homography` dividing by h[(2,2)].
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

/// Compute H from exactly 4 point correspondences: `dst ~ H * src`.
///
/// Uses Hartley normalization for numerical stability.
pub fn homography_from_4pt<F: Float>(
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

    let hn = Matrix3::<F>::new(
        x[0],
        x[1],
        x[2], //
        x[3],
        x[4],
        x[5], //
        x[6],
        x[7],
        F::one(),
    );

    let h_den = denormalize_homography(hn, t_src, t_dst)?;
    let h_den = normalize_homography(h_den)?;

    Some(Homography::new(h_den))
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
    fn inverse_round_trips_points() {
        let h = Homography::new(Matrix3::new(
            1.2, 0.1, 5.0, //
            -0.05, 0.9, 3.0, //
            0.001, 0.0005, 1.0,
        ));
        let inv = h.inverse().expect("invertible");

        for p in [
            Point2::new(0.0_f32, 0.0),
            Point2::new(50.0_f32, -20.0),
            Point2::new(320.0_f32, 200.0),
        ] {
            let q = h.apply(p);
            let back = inv.apply(q);
            assert_close(back, p, 1e-3);
        }
    }

    #[test]
    fn four_point_specialization_recovers_h() {
        let ground_truth = Homography::new(Matrix3::new(
            0.8, 0.05, 120.0, //
            -0.02, 1.1, 80.0, //
            0.0009, -0.0004, 1.0,
        ));

        let rect = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(180.0_f32, 0.0),
            Point2::new(180.0_f32, 130.0),
            Point2::new(0.0_f32, 130.0),
        ];
        let dst = rect.map(|p| ground_truth.apply(p));

        let recovered = homography_from_4pt(&rect, &dst).expect("recoverable");

        for p in [
            Point2::new(0.0_f32, 0.0),
            Point2::new(60.0, 40.0),
            Point2::new(150.0, 120.0),
        ] {
            assert_close(recovered.apply(p), ground_truth.apply(p), 1e-3);
        }
    }

    #[test]
    fn dlt_handles_overdetermined_case() {
        let ground_truth = Homography::new(Matrix3::new(
            1.0, 0.2, 12.0, //
            -0.1, 0.9, 6.0, //
            0.0006, 0.0004, 1.0,
        ));

        let rect: Vec<Point2<f32>> = (0..3)
            .flat_map(|y| (0..3).map(move |x| Point2::new(x as f32 * 40.0, y as f32 * 50.0)))
            .collect();
        let img: Vec<Point2<f32>> = rect.iter().map(|&p| ground_truth.apply(p)).collect();

        let estimated = estimate_homography(&rect, &img).expect("estimate");
        for p in [
            Point2::new(0.0_f32, 0.0),
            Point2::new(60.0, 40.0),
            Point2::new(80.0, 90.0),
            Point2::new(80.0, 100.0),
        ] {
            assert_close(estimated.apply(p), ground_truth.apply(p), 1e-3);
        }
    }

    #[test]
    fn mismatched_input_lengths_fail() {
        let rect = [Point2::new(0.0_f32, 0.0); 4];
        let img = [Point2::new(1.0_f32, 1.0); 3];
        assert!(estimate_homography(&rect, &img).is_none());
    }

    #[test]
    fn quality_reports_finite_metrics_for_clean_homography() {
        let rect = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(100.0, 0.0),
            Point2::new(100.0, 100.0),
            Point2::new(0.0, 100.0),
        ];
        // Mild rotation + translation — well-defined homography.
        let dst = [
            Point2::new(50.0, 50.0),
            Point2::new(150.0, 60.0),
            Point2::new(140.0, 160.0),
            Point2::new(40.0, 150.0),
        ];
        let (_, q) = homography_from_4pt_with_quality(&rect, &dst).expect("h");
        // All metrics are finite and non-degenerate for a clean fit.
        assert!(q.max_singular_value.is_finite() && q.max_singular_value > 0.0);
        assert!(q.min_singular_value > 0.0);
        assert!(q.condition.is_finite());
        assert!(q.determinant.abs() > 1e-3);
        // For a clean homography the smallest singular value is at the same
        // order as the determinant of the upper-left 2×2 — i.e., not near
        // machine epsilon.
        assert!(
            q.min_singular_value > 1e-2,
            "min_sv {} unexpectedly tiny on a clean fit",
            q.min_singular_value
        );
    }

    #[test]
    fn quality_min_sv_separates_clean_from_degenerate() {
        // The min singular value of H — and the ratio σ_min / σ_max — both
        // collapse when the destination quad has three near-collinear
        // points. Test on a unit-scale source so absolute and relative
        // metrics tell the same story.
        let rect = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        let clean_dst = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(2.0, 0.0),
            Point2::new(2.0, 2.0),
            Point2::new(0.0, 2.0),
        ];
        let degen_dst = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1e-6),
            Point2::new(0.0, 1e-6),
        ];
        let (_, q_clean) = homography_from_4pt_with_quality(&rect, &clean_dst).expect("clean");
        let (_, q_degen) = homography_from_4pt_with_quality(&rect, &degen_dst).expect("degen");

        assert!(
            q_clean.min_singular_value > q_degen.min_singular_value * 100.0,
            "clean min_sv {} must be much larger than degenerate {}",
            q_clean.min_singular_value,
            q_degen.min_singular_value
        );
        // Reciprocal condition (σ_min / σ_max) is the scale-invariant flag.
        let recip_clean = q_clean.min_singular_value / q_clean.max_singular_value;
        let recip_degen = q_degen.min_singular_value / q_degen.max_singular_value;
        assert!(
            recip_clean > 0.1,
            "clean recip_cond {recip_clean} too small"
        );
        assert!(
            recip_degen < 1e-3,
            "degenerate recip_cond {recip_degen} too large"
        );
    }

    #[test]
    fn is_ill_conditioned_threshold_works() {
        let rect = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
        ];
        let degen_dst = [
            Point2::new(0.0_f32, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1e-6),
            Point2::new(0.0, 1e-6),
        ];
        let (_, q) = homography_from_4pt_with_quality(&rect, &degen_dst).expect("h");
        assert!(q.is_ill_conditioned(1e-3));
        assert!(!q.is_ill_conditioned(1e-12));
    }

    #[test]
    fn estimate_with_quality_matches_direct_call() {
        let ground_truth: Homography<f32> = Homography::new(Matrix3::new(
            1.0, 0.2, 12.0, //
            -0.1, 0.9, 6.0, //
            0.0006, 0.0004, 1.0,
        ));
        let rect: Vec<Point2<f32>> = (0..3)
            .flat_map(|y| (0..3).map(move |x| Point2::new(x as f32 * 40.0, y as f32 * 50.0)))
            .collect();
        let img: Vec<Point2<f32>> = rect.iter().map(|&p| ground_truth.apply(p)).collect();

        let h = estimate_homography(&rect, &img).expect("h");
        let (h_with_q, _) = estimate_homography_with_quality(&rect, &img).expect("h+q");
        for r in 0..3 {
            for c in 0..3 {
                assert!((h.h[(r, c)] - h_with_q.h[(r, c)]).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn f64_round_trip() {
        let h: Homography<f64> = Homography::new(Matrix3::new(
            1.2, 0.1, 5.0, //
            -0.05, 0.9, 3.0, //
            0.001, 0.0005, 1.0,
        ));
        let inv = h.inverse().expect("invertible");

        for p in [
            Point2::new(0.0_f64, 0.0),
            Point2::new(50.0_f64, -20.0),
            Point2::new(320.0_f64, 200.0),
        ] {
            let q = h.apply(p);
            let back = inv.apply(q);
            assert!((back.x - p.x).abs() < 1e-10);
            assert!((back.y - p.y).abs() < 1e-10);
        }
    }

    #[test]
    fn f64_estimate_homography() {
        let ground_truth: Homography<f64> = Homography::new(Matrix3::new(
            1.0, 0.2, 12.0, //
            -0.1, 0.9, 6.0, //
            0.0006, 0.0004, 1.0,
        ));

        let rect: Vec<Point2<f64>> = (0..3)
            .flat_map(|y| (0..3).map(move |x| Point2::new(x as f64 * 40.0, y as f64 * 50.0)))
            .collect();
        let img: Vec<Point2<f64>> = rect.iter().map(|&p| ground_truth.apply(p)).collect();

        let estimated = estimate_homography(&rect, &img).expect("estimate");
        for p in [
            Point2::new(0.0_f64, 0.0),
            Point2::new(60.0, 40.0),
            Point2::new(80.0, 90.0),
        ] {
            let a = estimated.apply(p);
            let b = ground_truth.apply(p);
            assert!((a.x - b.x).abs() < 1e-8);
            assert!((a.y - b.y).abs() < 1e-8);
        }
    }

    /// Reference DLT path matching the SVD-based implementation that
    /// existed before the normal-equations + 9×9 sym-eig rewrite. Kept
    /// inline so the cross-check test below has a frozen baseline that
    /// the production path can drift away from only within the
    /// numerical-equivalence contract.
    fn dlt_via_svd_reference(
        src_pts: &[Point2<f32>],
        dst_pts: &[Point2<f32>],
    ) -> Option<Homography<f32>> {
        if src_pts.len() != dst_pts.len() || src_pts.len() < 4 {
            return None;
        }
        let (r, tr) = normalize_points(src_pts);
        let (im, ti) = normalize_points(dst_pts);

        let n = src_pts.len();
        let rows = 2 * n;
        let mut a = nalgebra::DMatrix::<f32>::zeros(rows, 9);
        for k in 0..n {
            let x = r[k].x;
            let y = r[k].y;
            let u = im[k].x;
            let v = im[k].y;
            a[(2 * k, 0)] = -x;
            a[(2 * k, 1)] = -y;
            a[(2 * k, 2)] = -1.0;
            a[(2 * k, 6)] = u * x;
            a[(2 * k, 7)] = u * y;
            a[(2 * k, 8)] = u;

            a[(2 * k + 1, 3)] = -x;
            a[(2 * k + 1, 4)] = -y;
            a[(2 * k + 1, 5)] = -1.0;
            a[(2 * k + 1, 6)] = v * x;
            a[(2 * k + 1, 7)] = v * y;
            a[(2 * k + 1, 8)] = v;
        }
        let svd = a.svd(true, true);
        let vt = svd.v_t?;
        let last = vt.nrows().checked_sub(1)?;
        let h = vt.row(last);
        let hn =
            Matrix3::<f32>::from_row_slice(&[h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7], h[8]]);
        let h_den = denormalize_homography(hn, tr, ti)?;
        let h_den = normalize_homography(h_den)?;
        Some(Homography::new(h_den))
    }

    /// Tiny deterministic xorshift32 PRNG so the random-battery test
    /// is reproducible without pulling in a `rand` dev-dependency for
    /// one helper.
    struct XorShift32(u32);
    impl XorShift32 {
        fn new(seed: u32) -> Self {
            Self(seed.max(1))
        }
        fn next_u32(&mut self) -> u32 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            self.0 = x;
            x
        }
        /// Uniform draw in `(-1, 1)`.
        fn unit(&mut self) -> f32 {
            (self.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
        }
    }

    #[test]
    fn dlt_matches_old_svd_path_on_random_battery() {
        // Forward-error contract: for a random battery of well-
        // conditioned ground-truth homographies, the new
        // normal-equations + 9×9 sym-eig path produces warped points
        // that agree with the SVD reference to ≤ 0.01 px on a 100-px
        // scale (equivalent to ~1e-4 relative). Both paths likewise
        // agree with the ground-truth H to ≤ 0.01 px. Comparing in
        // pixel-domain is the contract that actually matters
        // downstream: H is consumed via `apply(...)` to predict cell
        // positions, never read entry-by-entry.
        //
        // Per-entry relative error on `H` itself can drift up to ~1e-3
        // because Hartley-normalised cond(A) ≈ 10²-10³ and the
        // normal-equations route squares it (cond(AᵀA) ≈ 10⁴-10⁶);
        // in f32 the resulting backward error on the smallest
        // eigenvector can hit 1e-3 on individual entries. The
        // pixel-domain test absorbs the gauge-like cancellation that
        // makes per-entry comparison overly pessimistic.
        let mut rng = XorShift32::new(42);

        let mut max_fwd_err_new = 0.0f32;
        let mut max_fwd_err_ref = 0.0f32;
        let mut max_pair_err = 0.0f32;
        let mut sample_count = 0usize;

        for _ in 0..1000 {
            // Random ground-truth H with mild perspective.
            let gt = Homography::new(Matrix3::new(
                1.0 + 0.5 * rng.unit(),
                0.2 * rng.unit(),
                50.0 * rng.unit(),
                0.2 * rng.unit(),
                1.0 + 0.5 * rng.unit(),
                50.0 * rng.unit(),
                0.001 * rng.unit(),
                0.001 * rng.unit(),
                1.0,
            ));
            // 12 source points uniformly on ~cell-grid scale.
            let src: Vec<Point2<f32>> = (0..12)
                .map(|_| Point2::new(100.0 * rng.unit(), 100.0 * rng.unit()))
                .collect();
            let dst: Vec<Point2<f32>> = src.iter().map(|&p| gt.apply(p)).collect();

            let Some(new_h) = estimate_homography(&src, &dst) else {
                continue;
            };
            let Some(ref_h) = dlt_via_svd_reference(&src, &dst) else {
                continue;
            };

            // Forward errors against ground truth at the source points.
            for &p in &src {
                let new_p = new_h.apply(p);
                let ref_p = ref_h.apply(p);
                let gt_p = gt.apply(p);
                let new_err = ((new_p.x - gt_p.x).powi(2) + (new_p.y - gt_p.y).powi(2)).sqrt();
                let ref_err = ((ref_p.x - gt_p.x).powi(2) + (ref_p.y - gt_p.y).powi(2)).sqrt();
                let pair_err = ((new_p.x - ref_p.x).powi(2) + (new_p.y - ref_p.y).powi(2)).sqrt();
                if new_err > max_fwd_err_new {
                    max_fwd_err_new = new_err;
                }
                if ref_err > max_fwd_err_ref {
                    max_fwd_err_ref = ref_err;
                }
                if pair_err > max_pair_err {
                    max_pair_err = pair_err;
                }
            }

            sample_count += 1;
        }

        assert!(
            sample_count > 900,
            "expected most random samples to be valid, got {sample_count}"
        );
        // Both paths agree with ground truth to well below 0.01 px on
        // a 100-px scale.
        assert!(
            max_fwd_err_new < 1e-2,
            "new path max forward error {max_fwd_err_new} px exceeds 1e-2"
        );
        assert!(
            max_fwd_err_ref < 1e-2,
            "reference SVD max forward error {max_fwd_err_ref} px exceeds 1e-2"
        );
        // New vs reference: similarly bounded since both are within
        // their backward-error budget against gt.
        assert!(
            max_pair_err < 1e-2,
            "new vs reference max pixel divergence {max_pair_err} px exceeds 1e-2"
        );
    }
}
