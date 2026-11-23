//! Radiometric camera transfer curves and probit ink/microcapsule bleed.
//!
//! Camera nonlinearity (`sRGB`, fixed/free encoding gamma) is applied in the
//! **forward model** (`I_obs = f(I_lin)`) rather than by pre-linearizing pixel
//! values, so additive sensor noise stays homoscedastic in observed gray
//! levels.
//!
//! Ink spread / e-ink microcapsule edge bleed is modeled as a probit threshold
//! shift `B_bleed = Φ(Φ⁻¹(B) + κ)` on the blurred blackness `B ∈ (0, 1)`. On
//! every straight edge under any blur `σ_⊥`, `Φ⁻¹(B) = d / σ_⊥`, so adding `κ`
//! shifts all black/white boundaries uniformly into white cells by `κ · σ_⊥`
//! without double-counting shared disk diameters or self-overlapping at
//! X-junction corners.

use serde::{Deserialize, Serialize};

use super::gauss_polygon::{cdf, pdf};

/// Lower clamp bound for the free/fixed power-law gamma parameter.
pub(super) const MIN_GAMMA: f64 = 0.25;
/// Upper clamp bound for the free/fixed power-law gamma parameter.
pub(super) const MAX_GAMMA: f64 = 5.0;
/// Bounds for the free probit bleed shift `κ` (in standard deviations).
pub(super) const MAX_BLEED_KAPPA: f64 = 1.5;

/// Linear toe for power-law gamma curves so `u <= 0` during LM trial steps
/// never produces `NaN` or singular derivatives while remaining C¹ everywhere.
const U_TOE: f64 = 1e-4;
const INV_255: f64 = 1.0 / 255.0;

/// Radiometric tone-response model relating linear scene irradiance `I_lin`
/// (on `[0, 255]`) to observed image gray levels `I_obs`.
///
/// The default is [`RadiometricModel::Linear`], which adds zero free
/// parameters and keeps contrast `(A, C)` strictly linear for VarPro
/// elimination.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RadiometricModel {
    /// Linear camera response (`I_obs = I_lin`). Default.
    #[default]
    Linear,
    /// Standard IEC 61966-2-1 sRGB encoding transfer curve applied to `I_lin`.
    Srgb,
    /// Known power-law encoding gamma: `I_obs = 255 · (I_lin / 255)^(1 / gamma)`.
    FixedGamma {
        /// Encoding gamma (`> 0`, e.g. `2.2`).
        gamma: f64,
    },
    /// Jointly fit a free power-law encoding gamma alongside geometry and blur.
    FreeGamma {
        /// Starting gamma seed (`> 0`, e.g. `2.2`).
        initial_gamma: f64,
    },
}

impl RadiometricModel {
    /// Whether this model is linear in `(A, C)`.
    #[inline]
    pub(super) fn is_linear(self) -> bool {
        matches!(self, Self::Linear)
    }

    /// Whether gamma is a free optimization parameter.
    #[inline]
    pub(super) fn is_free_gamma(self) -> bool {
        matches!(self, Self::FreeGamma { .. })
    }

    /// Initial seed value for `theta[I_GAMMA]`.
    #[inline]
    pub(super) fn initial_gamma(self) -> f64 {
        match self {
            Self::Linear => 1.0,
            Self::Srgb => 2.2,
            Self::FixedGamma { gamma }
            | Self::FreeGamma {
                initial_gamma: gamma,
            } => {
                if gamma.is_finite() {
                    gamma.clamp(MIN_GAMMA, MAX_GAMMA)
                } else {
                    2.2
                }
            }
        }
    }

    /// Forward curve `I_obs = f(I_lin, γ)` and partial derivatives
    /// `(I_obs, ∂I_obs / ∂I_lin, ∂I_obs / ∂γ)`.
    #[inline]
    pub(super) fn forward(self, lin: f64, free_gamma: f64) -> (f64, f64, f64) {
        match self {
            Self::Linear => (lin, 1.0, 0.0),
            Self::Srgb => {
                let u = lin * INV_255;
                if u <= 0.003_130_8 {
                    (255.0 * 12.92 * u, 12.92, 0.0)
                } else {
                    const P: f64 = 1.0 / 2.4;
                    let up = u.powf(P);
                    let obs = 255.0 * (1.055 * up - 0.055);
                    let d_lin = 1.055 * P * u.powf(P - 1.0);
                    (obs, d_lin, 0.0)
                }
            }
            Self::FixedGamma { gamma } => {
                let g = if gamma.is_finite() {
                    gamma.clamp(MIN_GAMMA, MAX_GAMMA)
                } else {
                    2.2
                };
                power_forward(lin, g, false)
            }
            Self::FreeGamma { .. } => {
                let g = if free_gamma.is_finite() {
                    free_gamma.clamp(MIN_GAMMA, MAX_GAMMA)
                } else {
                    2.2
                };
                power_forward(lin, g, true)
            }
        }
    }

    /// Approximate inverse mapping `I_lin ≈ f⁻¹(I_obs)` for seeding `(A, C)` in
    /// the initial isotropic blur profile.
    #[inline]
    pub(super) fn inverse(self, obs: f64) -> f64 {
        match self {
            Self::Linear => obs,
            Self::Srgb => {
                let y = (obs * INV_255).clamp(0.0, 1.0);
                let u = if y <= 0.040_45 {
                    y / 12.92
                } else {
                    ((y + 0.055) / 1.055).powf(2.4)
                };
                255.0 * u
            }
            Self::FixedGamma { gamma }
            | Self::FreeGamma {
                initial_gamma: gamma,
            } => {
                let g = if gamma.is_finite() {
                    gamma.clamp(MIN_GAMMA, MAX_GAMMA)
                } else {
                    2.2
                };
                255.0 * (obs * INV_255).clamp(0.0, 1.0).powf(g)
            }
        }
    }
}

#[inline]
fn power_forward(lin: f64, g: f64, free: bool) -> (f64, f64, f64) {
    let u = lin * INV_255;
    let p = 1.0 / g;
    let inv_g2 = 1.0 / (g * g);
    if u > U_TOE {
        let up = u.powf(p);
        let obs = 255.0 * up;
        let d_lin = p * u.powf(p - 1.0);
        let d_gamma = if free { -obs * u.ln() * inv_g2 } else { 0.0 };
        (obs, d_lin, d_gamma)
    } else {
        let t = U_TOE.powf(p);
        let s = p * U_TOE.powf(p - 1.0);
        let du = u - U_TOE;
        let obs = 255.0 * (t + s * du);
        let d_gamma = if free {
            let ln_u = U_TOE.ln();
            let dt_dg = -t * ln_u * inv_g2;
            let ds_dg = -s * (g + ln_u) * inv_g2;
            255.0 * (dt_dg + ds_dg * du)
        } else {
            0.0
        };
        (obs, s, d_gamma)
    }
}

/// Standard normal inverse CDF `Φ⁻¹(p)` via Peter Acklam's rational
/// approximation followed by two Halley refinement steps against `cdf`/`pdf`.
pub(super) fn inv_normal_cdf(p: f64) -> f64 {
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239e0,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838e0,
        -2.549_732_539_343_734e0,
        4.374_664_141_464_968e0,
        2.938_163_982_698_783e0,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996e0,
        3.754_408_661_907_416e0,
    ];
    const P_LOW: f64 = 0.024_25;
    const P_HIGH: f64 = 1.0 - P_LOW;

    let mut z = if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= P_HIGH {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };

    // Two Halley steps polish the rational seed to full f64 accuracy.
    for _ in 0..2 {
        let fp = pdf(z);
        if fp > 0.0 {
            let ratio = (cdf(z) - p) / fp;
            z -= ratio / (1.0 + 0.5 * z * ratio);
        }
    }
    z
}

/// Evaluate probit ink bleed `B_bleed = Φ(Φ⁻¹(B) + κ)` together with
/// `(B_bleed, ∂B_bleed / ∂B, ∂B_bleed / ∂κ)`.
#[inline]
pub(super) fn bleed_eval(b: f64, kappa: f64, active: bool) -> (f64, f64, f64) {
    if !active && kappa == 0.0 {
        return (b, 1.0, 0.0);
    }
    const EPS: f64 = 1e-9;
    if b <= EPS {
        return (0.0, 0.0, 0.0);
    }
    if b >= 1.0 - EPS {
        return (1.0, 0.0, 0.0);
    }
    let z = inv_normal_cdf(b);
    let zk = z + kappa;
    let b_bleed = cdf(zk);
    let db_db = (-kappa * z - 0.5 * kappa * kappa).exp();
    let db_dkappa = pdf(zk);
    (b_bleed, db_db, db_dkappa)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv_normal_cdf_inverts_cdf_across_range() {
        for k in -55..=55 {
            let z = k as f64 * 0.1;
            let p = cdf(z);
            let recovered = inv_normal_cdf(p);
            let roundtrip = cdf(recovered);
            let tol = if z.abs() <= 4.0 { 1e-11 } else { 1e-9 };
            assert!(
                (roundtrip - p).abs() < 1e-13 && (recovered - z).abs() < tol,
                "z={z}: recovered {recovered} vs {z} (cdf err={})",
                (roundtrip - p).abs()
            );
        }
    }

    #[test]
    fn radiometric_forward_derivatives_match_finite_differences() {
        let models = [
            (RadiometricModel::Linear, 1.0, false),
            (RadiometricModel::Srgb, 2.2, false),
            (RadiometricModel::FixedGamma { gamma: 2.2 }, 2.2, false),
            (
                RadiometricModel::FreeGamma { initial_gamma: 1.8 },
                2.1,
                true,
            ),
        ];
        for (model, gamma, check_gamma) in models {
            for lin in [-1.0, 0.0, 0.02, 1.0, 50.0, 180.0, 240.0] {
                let (obs, d_lin, d_gamma) = model.forward(lin, gamma);
                let eps = 1e-5;
                let (p, _, _) = model.forward(lin + eps, gamma);
                let (m, _, _) = model.forward(lin - eps, gamma);
                let fd_lin = (p - m) / (2.0 * eps);
                assert!(
                    (d_lin - fd_lin).abs() < 1e-5 * (1.0 + fd_lin.abs()),
                    "{model:?} lin={lin} obs={obs}: {d_lin} vs {fd_lin}"
                );
                if check_gamma {
                    let (pg, _, _) = model.forward(lin, gamma + eps);
                    let (mg, _, _) = model.forward(lin, gamma - eps);
                    let fd_g = (pg - mg) / (2.0 * eps);
                    assert!(
                        (d_gamma - fd_g).abs() < 1e-5 * (1.0 + fd_g.abs()),
                        "{model:?} lin={lin}: d_gamma {d_gamma} vs {fd_g}"
                    );
                }
            }
        }
    }

    #[test]
    fn bleed_eval_derivatives_and_edge_shift_exact() {
        let kappa = 0.25;
        for step in 1..20 {
            let b = step as f64 / 20.0;
            let (bb, db_db, db_dk) = bleed_eval(b, kappa, true);
            let eps = 1e-6;
            let (bp, _, _) = bleed_eval(b + eps, kappa, true);
            let (bm, _, _) = bleed_eval(b - eps, kappa, true);
            let fd_b = (bp - bm) / (2.0 * eps);
            assert!(
                (db_db - fd_b).abs() < 1e-7 * (1.0 + fd_b.abs()),
                "b={b}: db_db {db_db} vs {fd_b}"
            );
            let (kp, _, _) = bleed_eval(b, kappa + eps, true);
            let (km, _, _) = bleed_eval(b, kappa - eps, true);
            let fd_k = (kp - km) / (2.0 * eps);
            assert!(
                (db_dk - fd_k).abs() < 1e-7 * (1.0 + fd_k.abs()),
                "b={b}: db_dk {db_dk} vs {fd_k}, val={bb}"
            );
        }
        // Straight blurred edge Φ(d / σ) shifts uniformly by -κ·σ in d.
        let sigma = 1.8;
        let d = 0.7;
        let (shifted, _, _) = bleed_eval(cdf(d / sigma), kappa, true);
        let exact = cdf((d + kappa * sigma) / sigma);
        assert!((shifted - exact).abs() < 1e-12);
    }
}
