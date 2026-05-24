//! Float-generic 2D projective homography: DLT, 4-point direct, Hartley
//! normalisation.
//!
//! Provides the [`Homography<F>`] matrix wrapper, a 4-point direct solver,
//! an N-point Hartley-normalised DLT estimator, and the
//! [`HomographyDiagnostics<F>`] singular-value summary.
//!
//! ## Replaces `HomographyQuality::is_ill_conditioned`
//!
//! The legacy crate exposed a boolean `is_ill_conditioned(min_sv_threshold)`
//! gate on `HomographyQuality`. That predicate is scale-dependent — the
//! SVD ratios of the unnormalised 3×3 H depend on coordinate scale and
//! translation magnitude — and is not a stable production metric across
//! image resolutions. Gap 3 in `docs/algorithmic_gaps.md` calls this out.
//!
//! This module:
//!
//! * keeps the singular-value fields as a *diagnostic*
//!   ([`HomographyDiagnostics`]),
//! * does not export any boolean predicate, and
//! * relies on **pixel-unit reprojection residuals** as the stability gate
//!   downstream. Callers that need a scale-aware conditioning number should
//!   consult [`super::conditioning::dlt_conditioning`] instead.

use nalgebra::{Matrix3, Point2, SMatrix, SVector, Vector3};

use crate::float::{lit, Float};

/// A 3×3 projective homography matrix.
///
/// Maps 2D points between two projective planes: `p_dst ~ H * p_src`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct Homography<F: Float> {
    /// The raw 3×3 matrix. Defined up to an overall scale; estimators in
    /// this module normalise it so the bottom-right entry is `1`.
    pub h: Matrix3<F>,
}

/// Singular-value diagnostics for a homography matrix.
///
/// **Diagnostic only.** Use these fields to log conditioning information
/// for debugging surfaces or post-hoc inspection. They are **not** a stable
/// gate across image resolutions — the singular values of the unnormalised
/// 3×3 H depend on coordinate scale and translation magnitude. For stability
/// gating, use pixel-unit reprojection residuals or
/// [`super::conditioning::dlt_conditioning`].
///
/// The legacy `HomographyQuality::is_ill_conditioned` boolean predicate is
/// intentionally absent from this struct.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct HomographyDiagnostics<F: Float> {
    /// Largest singular value of `H`.
    pub max_singular_value: F,
    /// Smallest singular value of `H`. Approaches zero as the source or
    /// destination quad degenerates (three collinear points).
    pub min_singular_value: F,
    /// Condition number `max_singular_value / min_singular_value`.
    pub condition: F,
    /// Determinant of `H`. Sign indicates orientation; `|det|` decays to
    /// zero as the map becomes singular.
    pub determinant: F,
}

impl<F: Float> HomographyDiagnostics<F> {
    /// Compute the diagnostics from a homography matrix.
    pub fn from_homography(h: &Homography<F>) -> Self {
        let svd = h.h.svd(false, false);
        let mut s_max = F::zero();
        let mut s_min = F::max_value().unwrap_or_else(|| lit::<F>(1e30_f32));
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
            F::max_value().unwrap_or_else(|| lit::<F>(1e30_f32))
        };
        let determinant = h.h.determinant();
        Self {
            max_singular_value: s_max,
            min_singular_value: s_min,
            condition,
            determinant,
        }
    }
}

impl<F: Float> Homography<F> {
    /// Wrap an existing 3×3 matrix as a homography. The matrix is taken
    /// as-is; no normalisation is applied.
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

    /// A homography backed by the all-zeros matrix. Not invertible — used
    /// only as a placeholder before a real estimate is available.
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
    let s = if mean_dist > lit::<F>(1e-12_f32) {
        lit::<F>(2.0_f32).sqrt() / mean_dist
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
    let n: F = lit::<F>(pts.len() as f32);
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
    let n: F = lit::<F>(4.0_f32);
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
    if s.abs() < lit::<F>(1e-12_f32) {
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

/// Estimate H such that `p_dst ~ H * p_src` from N ≥ 4 point correspondences,
/// returning the matrix together with [`HomographyDiagnostics`].
///
/// Wraps [`estimate_homography`]; callers that already discard the
/// diagnostic data can keep using that. Use this entry point to log
/// conditioning information on a diagnostic surface.
pub fn estimate_homography_with_diagnostics<F: Float>(
    src_pts: &[Point2<F>],
    dst_pts: &[Point2<F>],
) -> Option<(Homography<F>, HomographyDiagnostics<F>)> {
    let h = estimate_homography(src_pts, dst_pts)?;
    let q = HomographyDiagnostics::from_homography(&h);
    Some((h, q))
}

/// 4-point variant of [`estimate_homography_with_diagnostics`].
pub fn homography_from_4pt_with_diagnostics<F: Float>(
    src: &[Point2<F>; 4],
    dst: &[Point2<F>; 4],
) -> Option<(Homography<F>, HomographyDiagnostics<F>)> {
    let h = homography_from_4pt(src, dst)?;
    let q = HomographyDiagnostics::from_homography(&h);
    Some((h, q))
}

/// Estimate H such that `p_dst ~ H * p_src` from N ≥ 4 point correspondences.
///
/// Uses Hartley normalisation + DLT for N > 4 and a direct 4-point solver
/// for N == 4. Returns `None` when the input lengths disagree or are below
/// the minimum of 4.
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
/// Uses Hartley normalisation for numerical stability.
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

    fn assert_close<F: Float>(a: Point2<F>, b: Point2<F>, tol: F) {
        let dx = (a.x - b.x).abs();
        let dy = (a.y - b.y).abs();
        assert!(dx < tol && dy < tol, "points diverge beyond {tol:?}",);
    }

    fn assert_inverse_round_trips<F: Float>() {
        let h = Homography::<F>::new(Matrix3::new(
            lit::<F>(1.2_f32),
            lit::<F>(0.1_f32),
            lit::<F>(5.0_f32),
            lit::<F>(-0.05_f32),
            lit::<F>(0.9_f32),
            lit::<F>(3.0_f32),
            lit::<F>(0.001_f32),
            lit::<F>(0.0005_f32),
            F::one(),
        ));
        let inv = h.inverse().expect("invertible");
        let tol = lit::<F>(1e-3_f32);
        for p in [
            Point2::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(50.0_f32), lit::<F>(-20.0_f32)),
            Point2::new(lit::<F>(320.0_f32), lit::<F>(200.0_f32)),
        ] {
            let q = h.apply(p);
            let back = inv.apply(q);
            assert_close::<F>(back, p, tol);
        }
    }

    fn assert_four_point_specialisation_recovers<F: Float>() {
        let gt = Homography::<F>::new(Matrix3::new(
            lit::<F>(0.8_f32),
            lit::<F>(0.05_f32),
            lit::<F>(120.0_f32),
            lit::<F>(-0.02_f32),
            lit::<F>(1.1_f32),
            lit::<F>(80.0_f32),
            lit::<F>(0.0009_f32),
            lit::<F>(-0.0004_f32),
            F::one(),
        ));
        let rect = [
            Point2::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(180.0_f32), F::zero()),
            Point2::new(lit::<F>(180.0_f32), lit::<F>(130.0_f32)),
            Point2::new(F::zero(), lit::<F>(130.0_f32)),
        ];
        let dst = rect.map(|p| gt.apply(p));
        let recovered = homography_from_4pt::<F>(&rect, &dst).expect("recoverable");
        let tol = lit::<F>(1e-3_f32);
        for p in [
            Point2::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(60.0_f32), lit::<F>(40.0_f32)),
            Point2::new(lit::<F>(150.0_f32), lit::<F>(120.0_f32)),
        ] {
            assert_close::<F>(recovered.apply(p), gt.apply(p), tol);
        }
    }

    fn assert_dlt_overdetermined<F: Float>() {
        let gt = Homography::<F>::new(Matrix3::new(
            F::one(),
            lit::<F>(0.2_f32),
            lit::<F>(12.0_f32),
            lit::<F>(-0.1_f32),
            lit::<F>(0.9_f32),
            lit::<F>(6.0_f32),
            lit::<F>(0.0006_f32),
            lit::<F>(0.0004_f32),
            F::one(),
        ));
        let rect: Vec<Point2<F>> = (0..3)
            .flat_map(|y| {
                (0..3).map(move |x| {
                    Point2::new(
                        lit::<F>(x as f32) * lit::<F>(40.0_f32),
                        lit::<F>(y as f32) * lit::<F>(50.0_f32),
                    )
                })
            })
            .collect();
        let img: Vec<Point2<F>> = rect.iter().map(|&p| gt.apply(p)).collect();
        let estimated = estimate_homography::<F>(&rect, &img).expect("estimate");
        let tol = lit::<F>(1e-3_f32);
        for p in [
            Point2::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(60.0_f32), lit::<F>(40.0_f32)),
            Point2::new(lit::<F>(80.0_f32), lit::<F>(90.0_f32)),
            Point2::new(lit::<F>(80.0_f32), lit::<F>(100.0_f32)),
        ] {
            assert_close::<F>(estimated.apply(p), gt.apply(p), tol);
        }
    }

    fn assert_mismatched_lengths_fail<F: Float>() {
        let rect = [Point2::<F>::new(F::zero(), F::zero()); 4];
        let img = vec![Point2::<F>::new(F::one(), F::one()); 3];
        assert!(estimate_homography::<F>(&rect, &img).is_none());
    }

    fn assert_diagnostics_finite_for_clean<F: Float>() {
        let rect = [
            Point2::<F>::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(100.0_f32), F::zero()),
            Point2::new(lit::<F>(100.0_f32), lit::<F>(100.0_f32)),
            Point2::new(F::zero(), lit::<F>(100.0_f32)),
        ];
        let dst = [
            Point2::new(lit::<F>(50.0_f32), lit::<F>(50.0_f32)),
            Point2::new(lit::<F>(150.0_f32), lit::<F>(60.0_f32)),
            Point2::new(lit::<F>(140.0_f32), lit::<F>(160.0_f32)),
            Point2::new(lit::<F>(40.0_f32), lit::<F>(150.0_f32)),
        ];
        let (_, q) = homography_from_4pt_with_diagnostics::<F>(&rect, &dst).expect("h");
        assert!(q.max_singular_value > F::zero());
        assert!(q.min_singular_value > F::zero());
        assert!(q.determinant.abs() > lit::<F>(1e-3_f32));
        // Clean homography sits well above machine epsilon.
        assert!(q.min_singular_value > lit::<F>(1e-2_f32));
    }

    fn assert_diagnostics_separates_degenerate<F: Float>() {
        let rect = [
            Point2::<F>::new(F::zero(), F::zero()),
            Point2::new(F::one(), F::zero()),
            Point2::new(F::one(), F::one()),
            Point2::new(F::zero(), F::one()),
        ];
        let clean_dst = [
            Point2::<F>::new(F::zero(), F::zero()),
            Point2::new(lit::<F>(2.0_f32), F::zero()),
            Point2::new(lit::<F>(2.0_f32), lit::<F>(2.0_f32)),
            Point2::new(F::zero(), lit::<F>(2.0_f32)),
        ];
        let degen_dst = [
            Point2::<F>::new(F::zero(), F::zero()),
            Point2::new(F::one(), F::zero()),
            Point2::new(F::one(), lit::<F>(1e-6_f32)),
            Point2::new(F::zero(), lit::<F>(1e-6_f32)),
        ];
        let (_, q_clean) =
            homography_from_4pt_with_diagnostics::<F>(&rect, &clean_dst).expect("clean");
        let (_, q_degen) =
            homography_from_4pt_with_diagnostics::<F>(&rect, &degen_dst).expect("degen");
        // Clean min_sv is much larger than degenerate min_sv.
        assert!(q_clean.min_singular_value > q_degen.min_singular_value * lit::<F>(100.0_f32));
        // Reciprocal condition is the scale-invariant flag.
        let recip_clean = q_clean.min_singular_value / q_clean.max_singular_value;
        let recip_degen = q_degen.min_singular_value / q_degen.max_singular_value;
        assert!(recip_clean > lit::<F>(0.1_f32));
        assert!(recip_degen < lit::<F>(1e-3_f32));
    }

    fn assert_with_diagnostics_matches_direct<F: Float>() {
        let gt = Homography::<F>::new(Matrix3::new(
            F::one(),
            lit::<F>(0.2_f32),
            lit::<F>(12.0_f32),
            lit::<F>(-0.1_f32),
            lit::<F>(0.9_f32),
            lit::<F>(6.0_f32),
            lit::<F>(0.0006_f32),
            lit::<F>(0.0004_f32),
            F::one(),
        ));
        let rect: Vec<Point2<F>> = (0..3)
            .flat_map(|y| {
                (0..3).map(move |x| {
                    Point2::new(
                        lit::<F>(x as f32) * lit::<F>(40.0_f32),
                        lit::<F>(y as f32) * lit::<F>(50.0_f32),
                    )
                })
            })
            .collect();
        let img: Vec<Point2<F>> = rect.iter().map(|&p| gt.apply(p)).collect();
        let h = estimate_homography::<F>(&rect, &img).expect("h");
        let (h_with, _) = estimate_homography_with_diagnostics::<F>(&rect, &img).expect("h+d");
        let tol = lit::<F>(1e-6_f32);
        for r in 0..3 {
            for c in 0..3 {
                assert!((h.h[(r, c)] - h_with.h[(r, c)]).abs() < tol);
            }
        }
    }

    #[test]
    fn inverse_round_trips_f32() {
        assert_inverse_round_trips::<f32>();
    }
    #[test]
    fn inverse_round_trips_f64() {
        assert_inverse_round_trips::<f64>();
    }
    #[test]
    fn four_point_specialisation_f32() {
        assert_four_point_specialisation_recovers::<f32>();
    }
    #[test]
    fn four_point_specialisation_f64() {
        assert_four_point_specialisation_recovers::<f64>();
    }
    #[test]
    fn dlt_overdetermined_f32() {
        assert_dlt_overdetermined::<f32>();
    }
    #[test]
    fn dlt_overdetermined_f64() {
        assert_dlt_overdetermined::<f64>();
    }
    #[test]
    fn mismatched_lengths_fail_f32() {
        assert_mismatched_lengths_fail::<f32>();
    }
    #[test]
    fn mismatched_lengths_fail_f64() {
        assert_mismatched_lengths_fail::<f64>();
    }
    #[test]
    fn diagnostics_finite_for_clean_f32() {
        assert_diagnostics_finite_for_clean::<f32>();
    }
    #[test]
    fn diagnostics_finite_for_clean_f64() {
        assert_diagnostics_finite_for_clean::<f64>();
    }
    #[test]
    fn diagnostics_separates_degenerate_f32() {
        assert_diagnostics_separates_degenerate::<f32>();
    }
    #[test]
    fn diagnostics_separates_degenerate_f64() {
        assert_diagnostics_separates_degenerate::<f64>();
    }
    #[test]
    fn with_diagnostics_matches_direct_f32() {
        assert_with_diagnostics_matches_direct::<f32>();
    }
    #[test]
    fn with_diagnostics_matches_direct_f64() {
        assert_with_diagnostics_matches_direct::<f64>();
    }
}
