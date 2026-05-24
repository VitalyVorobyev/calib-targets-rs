//! DLT design-matrix conditioning.
//!
//! This module replaces the legacy `HomographyQuality::is_ill_conditioned`
//! boolean predicate (Gap 3 in `docs/algorithmic_gaps.md`). The legacy
//! predicate was scale-dependent — its threshold meant different things
//! at different image resolutions — so it has been dropped from the public
//! surface. What remains is a **diagnostic** [`DltConditioning<F>`]
//! report.
//!
//! ## What we measure
//!
//! The DLT design matrix `A` is *intentionally* rank-deficient by exactly
//! one — the unknown homography vector `h` lives in its null space, so the
//! smallest singular value `σ₉(A) ≈ 0` by construction. The diagnostic that
//! actually tracks "how well can we identify the null direction" is the
//! ratio between `σ₁` (the largest singular value) and `σ₈` (the
//! *second-smallest*). When the inputs are well-conditioned, the gap
//! between `σ₈` and `σ₉` is large; when three of the destination points
//! collapse onto a line, even `σ₈` falls toward zero and the homography is
//! no longer cleanly identifiable.
//!
//! Hartley normalisation translates and isotropically scales the points so
//! their centroid is at the origin and the mean distance from the centroid
//! is `√2`. After this, the design-matrix singular values are
//! scale-invariant and the second-smallest-aware condition number is a
//! meaningful, comparable number across image resolutions. **Even so, do
//! not gate stability on the raw condition number** — use pixel-unit
//! reprojection residuals in the caller's units. This struct is for
//! logging and post-hoc analysis only.

use nalgebra::{Matrix3, Point2, SMatrix, SVector, Vector3};

use crate::float::{lit, Float};

/// Singular-value report for the DLT design matrix used to estimate a
/// homography. **Diagnostic only.**
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct DltConditioning<F: Float> {
    /// Largest singular value of the Hartley-normalised DLT matrix
    /// (`σ_max ≡ σ₁`).
    pub max_singular: F,
    /// Second-smallest singular value of the Hartley-normalised DLT matrix
    /// (`σ₈`). The actual smallest singular value is the null direction
    /// — the homography itself — and is ≈ 0 by construction. Tracking
    /// `σ₈` instead captures how cleanly the null direction is identified.
    pub second_smallest_singular: F,
    /// Smallest singular value of the Hartley-normalised DLT matrix
    /// (`σ₉`). Diagnostic only: expected to be ≈ 0 on well-posed inputs,
    /// climbs only when the residual error after fitting is non-zero.
    pub min_singular: F,
    /// Condition number `max_singular / second_smallest_singular`, the
    /// scale-aware diagnostic. When the inputs are well-conditioned this
    /// sits below O(10²); near-collinear destination points push it above
    /// O(10⁴).
    pub condition_number: F,
    /// Number of point correspondences fed into the DLT.
    pub n_correspondences: usize,
}

/// Compute the singular-value report for the DLT design matrix that would
/// estimate `dst ~ H * src`.
///
/// Returns `None` when the inputs are mismatched in length or shorter than
/// the minimum of 4 correspondences. **Scale-aware diagnostic only**: for
/// stability gating use pixel-unit reprojection residuals.
pub fn dlt_conditioning<F: Float>(
    src: &[Point2<F>],
    dst: &[Point2<F>],
) -> Option<DltConditioning<F>> {
    if src.len() != dst.len() || src.len() < 4 {
        return None;
    }
    let n = src.len();
    let (src_n, _) = hartley_normalize(src);
    let (dst_n, _) = hartley_normalize(dst);

    // Accumulate Aᵀ A as in the homography estimator. Symmetric-eigen of
    // AᵀA gives the squared singular values; we recover σ = √λ.
    let zero = F::zero();
    let neg_one = -F::one();
    let mut m: SMatrix<F, 9, 9> = SMatrix::zeros();
    for k in 0..n {
        let x = src_n[k].x;
        let y = src_n[k].y;
        let u = dst_n[k].x;
        let v = dst_n[k].y;
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
    // Sort eigenvalues ascending; clamp negative (FP wobble) to zero.
    let mut lambdas: [F; 9] = [F::zero(); 9];
    for (slot, l) in lambdas.iter_mut().zip(eig.eigenvalues.iter().copied()) {
        *slot = if l < F::zero() { F::zero() } else { l };
    }
    lambdas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let min_singular = lambdas[0].sqrt();
    let second_smallest_singular = lambdas[1].sqrt();
    let max_singular = lambdas[8].sqrt();
    let condition_number = if second_smallest_singular > F::default_epsilon() {
        max_singular / second_smallest_singular
    } else {
        F::max_value().unwrap_or_else(|| lit::<F>(1e30_f32))
    };
    Some(DltConditioning {
        min_singular,
        second_smallest_singular,
        max_singular,
        condition_number,
        n_correspondences: n,
    })
}

/// Hartley-normalise a point set: translate centroid to origin, isotropically
/// scale so the mean distance from the centroid is `√2`.
fn hartley_normalize<F: Float>(pts: &[Point2<F>]) -> (Vec<Point2<F>>, Matrix3<F>) {
    let n = lit::<F>(pts.len() as f32);
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
    let s = if mean_dist > lit::<F>(1e-12_f32) {
        lit::<F>(2.0_f32).sqrt() / mean_dist
    } else {
        F::one()
    };
    let t = Matrix3::new(
        s,
        F::zero(),
        -s * cx,
        F::zero(),
        s,
        -s * cy,
        F::zero(),
        F::zero(),
        F::one(),
    );
    let mut out = Vec::with_capacity(pts.len());
    for p in pts {
        let v = t * Vector3::new(p.x, p.y, F::one());
        out.push(Point2::new(v[0], v[1]));
    }
    (out, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_returns_none_on_mismatch<F: Float>() {
        let src: Vec<Point2<F>> = (0..4)
            .map(|k| Point2::new(lit::<F>(k as f32), F::zero()))
            .collect();
        let dst: Vec<Point2<F>> = (0..3)
            .map(|k| Point2::new(lit::<F>(k as f32), F::zero()))
            .collect();
        assert!(dlt_conditioning::<F>(&src, &dst).is_none());
    }

    fn assert_clean_grid_has_low_condition<F: Float>() {
        let mut src = Vec::new();
        let mut dst = Vec::new();
        for j in 0..3 {
            for i in 0..3 {
                let s = Point2::new(
                    lit::<F>(i as f32) * lit::<F>(40.0_f32),
                    lit::<F>(j as f32) * lit::<F>(50.0_f32),
                );
                let d = Point2::new(
                    s.x + lit::<F>(0.05_f32) * s.y,
                    s.y + lit::<F>(0.02_f32) * s.x,
                );
                src.push(s);
                dst.push(d);
            }
        }
        let report = dlt_conditioning::<F>(&src, &dst).expect("report");
        assert_eq!(report.n_correspondences, 9);
        assert!(report.condition_number.is_finite());
        // Clean Hartley-normalised cond₈(A) = σ₁/σ₈ is typically below ~10².
        assert!(report.condition_number < lit::<F>(100.0_f32));
        assert!(report.second_smallest_singular > F::default_epsilon());
    }

    fn assert_degenerate_inflates_condition<F: Float>() {
        // Genuine rank-loss degeneracy: 3 of the 5 source-destination
        // correspondences are colinear in both endpoints. The DLT design
        // matrix's `σ₈` collapses because those correspondences contribute
        // a near-rank-deficient row block.
        let src: Vec<Point2<F>> = vec![
            // Four colinear sources on the y-axis...
            Point2::new(F::zero(), F::zero()),
            Point2::new(F::zero(), F::one()),
            Point2::new(F::zero(), lit::<F>(2.0_f32)),
            Point2::new(F::zero(), lit::<F>(3.0_f32)),
            // ...and one off-axis source.
            Point2::new(F::one(), F::one()),
        ];
        let dst: Vec<Point2<F>> = vec![
            Point2::new(F::zero(), F::zero()),
            Point2::new(F::zero(), F::one()),
            Point2::new(F::zero(), lit::<F>(2.0_f32)),
            Point2::new(F::zero(), lit::<F>(3.0_f32)),
            Point2::new(F::one(), F::one()),
        ];
        // Clean reference: same 5 points but the colinear quadruple
        // disperses off the y-axis.
        let clean_src: Vec<Point2<F>> = vec![
            Point2::new(F::zero(), F::zero()),
            Point2::new(F::one(), F::zero()),
            Point2::new(F::one(), F::one()),
            Point2::new(F::zero(), F::one()),
            Point2::new(lit::<F>(0.5_f32), lit::<F>(0.5_f32)),
        ];
        let clean_dst: Vec<Point2<F>> = vec![
            Point2::new(F::zero(), F::zero()),
            Point2::new(F::one(), F::zero()),
            Point2::new(F::one(), F::one()),
            Point2::new(F::zero(), F::one()),
            Point2::new(lit::<F>(0.5_f32), lit::<F>(0.5_f32)),
        ];
        let degen = dlt_conditioning::<F>(&src, &dst).expect("degen");
        let clean = dlt_conditioning::<F>(&clean_src, &clean_dst).expect("clean");
        assert!(
            degen.condition_number > clean.condition_number * lit::<F>(10.0_f32),
            "degen cond {:?}, clean cond {:?}",
            degen.condition_number,
            clean.condition_number,
        );
    }

    #[test]
    fn returns_none_on_mismatch_f32() {
        assert_returns_none_on_mismatch::<f32>();
    }
    #[test]
    fn returns_none_on_mismatch_f64() {
        assert_returns_none_on_mismatch::<f64>();
    }
    #[test]
    fn clean_grid_low_condition_f32() {
        assert_clean_grid_has_low_condition::<f32>();
    }
    #[test]
    fn clean_grid_low_condition_f64() {
        assert_clean_grid_has_low_condition::<f64>();
    }
    #[test]
    fn degenerate_inflates_condition_f32() {
        assert_degenerate_inflates_condition::<f32>();
    }
    #[test]
    fn degenerate_inflates_condition_f64() {
        assert_degenerate_inflates_condition::<f64>();
    }
}
