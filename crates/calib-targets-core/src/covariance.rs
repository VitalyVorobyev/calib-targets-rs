//! Per-corner position covariance.

use serde::{Deserialize, Serialize};

/// Covariance of a corner's image position, in px² in the input image frame.
///
/// The symmetric 2×2 matrix `[[xx, xy], [xy, yy]]` the `chess-corners`
/// localizer reports for a refined corner: how far the reported position is
/// likely to be from the true corner, from image noise propagated through the
/// localizer and from the localizer's own systematic error together. See the
/// "Uncertainty" part of the
/// [chess-corners book](https://vitalyvorobyev.github.io/chess-corners-rs/book/part-08-uncertainty.html)
/// for the model and how well it holds.
///
/// # When a corner has none
///
/// A corner's `position_covariance` is `None` when no covariance was measured:
/// the corner config's `uncertainty` is `Off`; its `upscale` is active
/// (interpolated pixels carry correlated noise); it uses the ML refiner; the
/// frame's noise estimate declined (typical of JPEG input — supply
/// `noise_sigma`); or a saddle-point refinement did not converge for that
/// corner.
///
/// The model assumes white pixel noise in the image the detector ran on. If the
/// caller resampled or blurred that image first (resizing it, or
/// `calib_targets::preprocess`), the reported covariance is too small, and in
/// the resampled image's pixels.
///
/// Serialized as a plain `{ "xx": .., "xy": .., "yy": .. }` object.
///
/// `#[non_exhaustive]`: construct with [`PositionCovariance::new`] or convert
/// from [`chess_corners::PositionCovariance`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PositionCovariance {
    /// Variance of the x coordinate, px².
    pub xx: f32,
    /// Covariance of the x and y coordinates, px².
    pub xy: f32,
    /// Variance of the y coordinate, px².
    pub yy: f32,
}

impl PositionCovariance {
    /// Construct a covariance from its three distinct entries (px²).
    pub fn new(xx: f32, xy: f32, yy: f32) -> Self {
        Self { xx, xy, yy }
    }

    /// Standard deviation along the least certain direction, in px: the square
    /// root of the larger eigenvalue.
    ///
    /// A single scalar for ranking or gating corners by localization quality.
    /// A negative eigenvalue (a matrix that is not positive semi-definite)
    /// counts as `0`.
    pub fn sigma_max_px(&self) -> f32 {
        let (xx, xy, yy) = (f64::from(self.xx), f64::from(self.xy), f64::from(self.yy));
        let mean = 0.5 * (xx + yy);
        let half_diff = 0.5 * (xx - yy);
        let lambda_max = mean + half_diff.hypot(xy);
        lambda_max.max(0.0).sqrt() as f32
    }

    /// Squared Mahalanobis distance `dᵀ Σ⁻¹ d` of the offset `d = (dx, dy)`
    /// (px), or `None` when the matrix is not positive definite (and so has
    /// no inverse to measure with).
    ///
    /// Under the covariance's Gaussian model this is χ²-distributed with two
    /// degrees of freedom, so e.g. `> 5.99` rejects at the 95 % level.
    pub fn mahalanobis_sq(&self, dx: f32, dy: f32) -> Option<f32> {
        let (xx, xy, yy) = (f64::from(self.xx), f64::from(self.xy), f64::from(self.yy));
        let det = xx * yy - xy * xy;
        if !(xx > 0.0 && det > 0.0 && det.is_finite()) {
            return None;
        }
        let (dx, dy) = (f64::from(dx), f64::from(dy));
        Some(((yy * dx * dx - 2.0 * xy * dx * dy + xx * dy * dy) / det) as f32)
    }

    /// The covariance of `G·p` for a position `p` with this covariance:
    /// `G Σ Gᵀ`.
    ///
    /// `g` is row-major (`g[row][col]`), the linear part of the map the
    /// position goes through — e.g. `[[s, 0], [0, s]]` for a uniform rescale
    /// by `s`, or the Jacobian of a homography at the point. Translations do
    /// not change a covariance.
    pub fn transformed(&self, g: &[[f32; 2]; 2]) -> Self {
        let [[a, b], [c, d]] = g.map(|row| row.map(f64::from));
        let (xx, xy, yy) = (f64::from(self.xx), f64::from(self.xy), f64::from(self.yy));
        Self {
            xx: (a * a * xx + 2.0 * a * b * xy + b * b * yy) as f32,
            xy: (a * c * xx + (a * d + b * c) * xy + b * d * yy) as f32,
            yy: (c * c * xx + 2.0 * c * d * xy + d * d * yy) as f32,
        }
    }
}

impl From<chess_corners::PositionCovariance> for PositionCovariance {
    fn from(c: chess_corners::PositionCovariance) -> Self {
        Self::new(c.xx, c.xy, c.yy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() <= 1e-5 * a.abs().max(b.abs()).max(1.0)
    }

    #[test]
    fn isotropic() {
        let c = PositionCovariance::new(0.04, 0.0, 0.04);
        assert!(close(c.sigma_max_px(), 0.2));
        // d = (0.2, 0) is one sigma: d² = 1.
        assert!(close(c.mahalanobis_sq(0.2, 0.0).unwrap(), 1.0));
        assert!(close(c.mahalanobis_sq(0.0, -0.4).unwrap(), 4.0));
        // A uniform rescale by 2 scales the variances by 4.
        let s = c.transformed(&[[2.0, 0.0], [0.0, 2.0]]);
        assert!(close(s.xx, 0.16) && close(s.yy, 0.16) && close(s.xy, 0.0));
    }

    #[test]
    fn rotated_anisotropic() {
        // Variances 0.09 / 0.01 along the axes, rotated by 30°: R diag Rᵀ.
        let base = PositionCovariance::new(0.09, 0.0, 0.01);
        let (s, c) = 30f32.to_radians().sin_cos();
        let rot = [[c, -s], [s, c]];
        let r = base.transformed(&rot);
        // Rotation preserves the eigenvalues, so sigma_max is unchanged.
        assert!(close(r.sigma_max_px(), 0.3));
        assert!(close(r.xx + r.yy, 0.10));
        assert!(r.xy.abs() > 1e-3, "rotated covariance must couple x and y");
        // One sigma along the major (rotated x) axis is d² = 1, along the
        // minor axis too.
        assert!(close(r.mahalanobis_sq(0.3 * c, 0.3 * s).unwrap(), 1.0));
        assert!(close(r.mahalanobis_sq(-0.1 * s, 0.1 * c).unwrap(), 1.0));
        // Rotating back recovers the axis-aligned matrix.
        let back = r.transformed(&[[c, s], [-s, c]]);
        assert!(close(back.xx, 0.09) && close(back.yy, 0.01) && back.xy.abs() < 1e-6);
    }

    #[test]
    fn degenerate_is_not_invertible() {
        // Rank one: all mass along x = y.
        let line = PositionCovariance::new(0.01, 0.01, 0.01);
        assert!(line.mahalanobis_sq(0.1, 0.0).is_none());
        assert!(close(line.sigma_max_px(), 0.02f32.sqrt()));
        // Zero and indefinite matrices.
        let zero = PositionCovariance::new(0.0, 0.0, 0.0);
        assert!(zero.mahalanobis_sq(0.0, 0.0).is_none());
        assert_eq!(zero.sigma_max_px(), 0.0);
        let indefinite = PositionCovariance::new(0.01, 0.1, 0.01);
        assert!(indefinite.mahalanobis_sq(0.1, 0.1).is_none());
        assert!(PositionCovariance::new(f32::NAN, 0.0, 1.0)
            .mahalanobis_sq(0.1, 0.1)
            .is_none());
        let negative = PositionCovariance::new(-1.0, 0.0, -1.0);
        assert_eq!(negative.sigma_max_px(), 0.0);
    }

    #[test]
    fn serde_is_a_plain_object() {
        let c = PositionCovariance::new(0.25, -0.5, 1.0);
        let v = serde_json::to_value(c).unwrap();
        assert_eq!(v, serde_json::json!({ "xx": 0.25, "xy": -0.5, "yy": 1.0 }));
        let back: PositionCovariance = serde_json::from_value(v).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn converts_from_upstream() {
        let up = chess_corners::PositionCovariance::new(0.1, 0.02, 0.3);
        assert_eq!(
            PositionCovariance::from(up),
            PositionCovariance::new(0.1, 0.02, 0.3)
        );
    }
}
