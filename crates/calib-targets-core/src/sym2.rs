//! Shared 2×2 symmetric matrix math for covariances and blur kernels.

/// Symmetric 2×2 matrix `[[xx, xy], [xy, yy]]` in `f64` precision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sym2x2 {
    pub xx: f64,
    pub xy: f64,
    pub yy: f64,
}

impl Sym2x2 {
    /// Construct from explicit `f64` entries.
    #[inline]
    pub const fn new(xx: f64, xy: f64, yy: f64) -> Self {
        Self { xx, xy, yy }
    }

    /// Construct from `f32` entries promoted to `f64`.
    #[inline]
    pub fn from_f32(xx: f32, xy: f32, yy: f32) -> Self {
        Self {
            xx: f64::from(xx),
            xy: f64::from(xy),
            yy: f64::from(yy),
        }
    }

    /// Eigenvalues `(lambda_min, lambda_max)` ordered ascending.
    #[inline]
    pub fn eigenvalues(self) -> (f64, f64) {
        let mean = 0.5 * (self.xx + self.yy);
        let r = (0.5 * (self.xx - self.yy)).hypot(self.xy);
        (mean - r, mean + r)
    }

    /// Angle of the major eigenvector from `+x` in radians, in `(−π/2, π/2]`.
    #[inline]
    pub fn major_axis_angle(self) -> f64 {
        0.5 * (2.0 * self.xy).atan2(self.xx - self.yy)
    }

    /// Principal axes `(sigma_major, sigma_minor, angle)` in `f32`, clamping any
    /// negative eigenvalues at `0.0` before taking square roots.
    #[inline]
    pub fn principal_axes(self) -> (f32, f32, f32) {
        let (lo, hi) = self.eigenvalues();
        (
            hi.max(0.0).sqrt() as f32,
            lo.max(0.0).sqrt() as f32,
            self.major_axis_angle() as f32,
        )
    }

    /// Squared Mahalanobis distance `dᵀ Σ⁻¹ d` for `d = (dx, dy)`, or `None`
    /// when the matrix is not strictly positive definite.
    #[inline]
    pub fn mahalanobis_sq(self, dx: f64, dy: f64) -> Option<f64> {
        let det = self.xx * self.yy - self.xy * self.xy;
        if !(self.xx > 0.0 && det > 0.0 && det.is_finite()) {
            return None;
        }
        Some((self.yy * dx * dx - 2.0 * self.xy * dx * dy + self.xx * dy * dy) / det)
    }

    /// Congruence transform `G Σ Gᵀ` for row-major `g`.
    #[inline]
    pub fn transformed(self, g: &[[f64; 2]; 2]) -> Self {
        let [[a, b], [c, d]] = *g;
        Self {
            xx: a * a * self.xx + 2.0 * a * b * self.xy + b * b * self.yy,
            xy: a * c * self.xx + (a * d + b * c) * self.xy + b * d * self.yy,
            yy: c * c * self.xx + 2.0 * c * d * self.xy + d * d * self.yy,
        }
    }

    /// Lower Cholesky factor `(l11, l21, l22)` of `self`, or `None` when the
    /// matrix is not positive definite.
    #[inline]
    pub fn cholesky(self) -> Option<[f64; 3]> {
        if self.xx.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return None;
        }
        let l11 = self.xx.sqrt();
        let l21 = self.xy / l11;
        let r = self.yy - l21 * l21;
        if r.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return None;
        }
        Some([l11, l21, r.sqrt()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eigenvalues_principal_axes_and_cholesky_agree() {
        let (s, c) = 30f64.to_radians().sin_cos();
        let rot = [[c, -s], [s, c]];
        let base = Sym2x2::new(4.0, 0.0, 1.0).transformed(&rot);
        let (lo, hi) = base.eigenvalues();
        assert!((lo - 1.0).abs() < 1e-12);
        assert!((hi - 4.0).abs() < 1e-12);

        let (maj, min, ang) = base.principal_axes();
        assert!((maj - 2.0).abs() < 1e-6);
        assert!((min - 1.0).abs() < 1e-6);
        assert!((ang - 30f32.to_radians()).abs() < 1e-6);

        let l = base.cholesky().expect("SPD");
        assert!((l[0] * l[0] - base.xx).abs() < 1e-12);
        assert!((l[0] * l[1] - base.xy).abs() < 1e-12);
        assert!((l[1] * l[1] + l[2] * l[2] - base.yy).abs() < 1e-12);
        assert!(Sym2x2::new(1.0, 2.0, 1.0).cholesky().is_none());
    }
}
