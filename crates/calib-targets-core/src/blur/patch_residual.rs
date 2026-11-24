//! Solver-agnostic sparse photometric patch residual for joint bundle adjustment.
//!
//! [`PatchResidual`] extracts the raw unclipped image patch around one detected
//! grid corner and evaluates per-pixel photometric residuals `r_i = y_i − Î_i`
//! together with exact analytic Jacobians w.r.t.:
//! - the local normalized homography `H̃ = N · H` (8 DOF),
//! - pure image-plane corner displacement `(Δx, Δy)` under fixed perspective,
//! - the 2-D blur covariance `Σ = (Σxx, Σxy, Σyy)`,
//! - optional encoding gamma `γ` and probit bleed `κ`.
//!
//! When [`RadiometricModel::Linear`] is active and `(A, C)` are left free
//! (`fixed_ac: None`), `(A, C)` are eliminated analytically via exact
//! Golub–Pereyra Variable Projection (`ṙ = −u + Φ(v − w)`), exposing zero
//! per-patch contrast/black-level state to the outer camera/defocus BA solver.

use nalgebra::{Matrix3, Point2};
use serde::{Deserialize, Serialize};

use super::fit::box_model_for;
use super::frame::{pixel_bounds, select_pixels, Frame};
use super::model::{
    cholesky2, eig2, BoardPolygon, BoxModel, Model, Pixel, Vector, I_BLEED, I_H, I_SIGMA, NP,
};
use super::radiometric::{bleed_eval, RadiometricModel, MAX_BLEED_KAPPA, MAX_GAMMA, MIN_GAMMA};
use super::{BlurDecline, BlurGradient, BlurKernel, BlurPattern, PhotometricFitOptions};
use crate::{GrayImageView, Homography};

/// Minimum unclipped in-support pixels required to form a patch residual.
const MIN_PATCH_PIXELS: usize = 16;

/// Gram condition threshold below which `(A, C)` VarPro elimination is treated
/// as singular.
const AC_COND_TOL: f64 = 1e-11;

/// Maximum per-parameter count packed per pixel during VarPro evaluation
/// (`h[0..8]` + `sigma[0..3]` + optional `kappa`).
const MAX_PATCH_PARAMS: usize = 12;

/// One observed patch pixel extracted from the source grayscale image.
///
/// `#[non_exhaustive]`: read fields directly.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PatchPixel {
    /// Pixel column index (pixel centre at integer `x`).
    pub x: i32,
    /// Pixel row index (pixel centre at integer `y`).
    pub y: i32,
    /// Raw observed gray value on `[1, 254]`.
    pub value: f64,
}

/// Evaluation state supplied to [`PatchResidual::evaluate`] by an outer solver.
///
/// `#[non_exhaustive]`: construct with [`PatchEvalInput::new`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PatchEvalInput {
    /// Current local board → image homography (origin at this corner, board
    /// units = grid cells, image units = pixels with centres at integers).
    pub homography: Homography<f64>,
    /// Current 2-D total blur covariance `Σ` (px²). When [`Self::blur_gradient`]
    /// is `Some`, this covariance is evaluated at [`PatchResidual::nominal_corner`].
    pub kernel: BlurKernel,
    /// Optional known spatial gradient `(∂Σ/∂x, ∂Σ/∂y)` (px²/px) for first-order
    /// scatter-PSF corner bias modeling.
    pub blur_gradient: Option<BlurGradient>,
    /// Normalized linear illumination shading slope `[α, β]` (`S(p) = 1 + α d_x + β d_y`).
    pub illumination_gradient: [f64; 2],
    /// Power-law encoding gamma used when `options.radiometric` is
    /// [`RadiometricModel::FreeGamma`].
    pub gamma: f64,
    /// Probit ink/microcapsule bleed `κ` used when `options.fit_ink_bleed` is `true`.
    pub bleed_kappa: f64,
    /// Optional fixed `[A, C]` (contrast, black level). When `None` and
    /// `options.radiometric` is [`RadiometricModel::Linear`], `(A, C)` are
    /// projected analytically via Golub–Pereyra VarPro with exact derivatives.
    /// For nonlinear [`RadiometricModel`]s, callers needing exact `(A, C)`
    /// coupling should supply `Some([a, c])` or solve `(A, C)` outside; when
    /// `None` under nonlinear radiometry, `(A, C)` are estimated in linearized
    /// reflectance space and held fixed in the Jacobian.
    pub fixed_ac: Option<[f64; 2]>,
}

impl PatchEvalInput {
    /// Construct an evaluation input with zero illumination gradient, zero blur
    /// gradient, default gamma `2.2`, zero bleed, and VarPro `(A, C)` projection.
    pub fn new(homography: Homography<f64>, kernel: BlurKernel) -> Self {
        Self {
            homography,
            kernel,
            blur_gradient: None,
            illumination_gradient: [0.0, 0.0],
            gamma: 2.2,
            bleed_kappa: 0.0,
            fixed_ac: None,
        }
    }

    /// Set the spatial blur gradient `(∂Σ/∂x, ∂Σ/∂y)`.
    #[must_use]
    pub fn with_blur_gradient(mut self, gradient: BlurGradient) -> Self {
        self.blur_gradient = Some(gradient);
        self
    }

    /// Set the normalized linear illumination slope `[α, β]`.
    #[must_use]
    pub fn with_illumination_gradient(mut self, gradient: [f64; 2]) -> Self {
        self.illumination_gradient = gradient;
        self
    }

    /// Set the encoding gamma for [`RadiometricModel::FreeGamma`].
    #[must_use]
    pub fn with_gamma(mut self, gamma: f64) -> Self {
        self.gamma = gamma;
        self
    }

    /// Set the probit ink bleed shift `κ`.
    #[must_use]
    pub fn with_bleed_kappa(mut self, kappa: f64) -> Self {
        self.bleed_kappa = kappa;
        self
    }

    /// Pin `[A, C]` explicitly instead of projecting them inside [`PatchResidual::evaluate`].
    #[must_use]
    pub fn with_fixed_ac(mut self, ac: [f64; 2]) -> Self {
        self.fixed_ac = Some(ac);
        self
    }
}

/// Residuals and exact Jacobians returned by [`PatchResidual::evaluate`].
///
/// Residuals follow `r_i = y_i − Î_i`, so the Gauss–Newton normal equations are
/// `(Jᵀ J) Δθ = −Jᵀ r`.
///
/// `#[non_exhaustive]`: read fields directly.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub struct PatchEval {
    /// Per-pixel residuals `y_i − Î_i`, in [`PatchResidual::pixels`] order.
    pub residuals: Vec<f64>,
    /// Fitted or fixed black-to-white contrast `A` (linearized gray levels).
    pub contrast: f64,
    /// Fitted or fixed black level `C` (linearized gray levels).
    pub black_level: f64,
    /// Sum of squared residuals `∑ r_i²`.
    pub cost: f64,
    /// Analytic derivative `∂r_i / ∂h̃` w.r.t. normalized homography `H̃ = N · H`.
    pub d_res_d_h: Vec<[f64; 8]>,
    /// Analytic derivative `∂r_i / ∂(Δx, Δy)` for pure image translation
    /// `T(Δx, Δy) · H`, in `px⁻¹`.
    pub d_res_d_corner: Vec<[f64; 2]>,
    /// Analytic derivative `∂r_i / ∂(Σxx, Σxy, Σyy)`.
    pub d_res_d_sigma: Vec<[f64; 3]>,
    /// Analytic derivative `∂r_i / ∂γ` when `options.radiometric` is
    /// [`RadiometricModel::FreeGamma`].
    pub d_res_d_gamma: Option<Vec<f64>>,
    /// Analytic derivative `∂r_i / ∂κ` when `options.fit_ink_bleed` is `true`.
    pub d_res_d_bleed: Option<Vec<f64>>,
}

/// Extracted local image patch and printed polygons for sparse photometric bundle
/// adjustment around one corner.
///
/// `#[non_exhaustive]`: construct with [`PatchResidual::extract`].
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct PatchResidual {
    polygons: Vec<BoardPolygon>,
    pixels: Vec<PatchPixel>,
    raw_pixels: Vec<Pixel>,
    pixel_bounds: [f64; 4],
    c0: [f64; 2],
    scale: f64,
    norm_mat: Matrix3<f64>,
    nominal_h: [f64; 8],
    window_cells: f64,
    options: PhotometricFitOptions,
}

impl PatchResidual {
    /// Extract unclipped in-support patch pixels and printed polygons around one
    /// corner given `nominal_frame` (board cells → image px).
    pub fn extract(
        image: &GrayImageView<'_>,
        pattern: &BlurPattern,
        nominal_frame: &Homography<f64>,
        window_cells: f64,
        options: &PhotometricFitOptions,
    ) -> Result<Self, BlurDecline> {
        if !(window_cells.is_finite() && window_cells > 0.0) {
            return Err(BlurDecline::NoGeometry);
        }
        let frame = Frame::new(nominal_frame).ok_or(BlurDecline::NoGeometry)?;
        let polygons: Vec<BoardPolygon> = pattern
            .polygons
            .iter()
            .map(|(v, w)| BoardPolygon {
                vertices: v.clone(),
                weight: *w,
            })
            .collect();
        if polygons.is_empty() {
            return Err(BlurDecline::PatternUnknown);
        }
        let margin_px = 0.5 + (0.15 * frame.scale_min).min(0.35 * window_cells * frame.scale_min);
        let raw_pixels = select_pixels(image, pattern, &frame, window_cells, margin_px);
        if raw_pixels.len() < MIN_PATCH_PIXELS {
            return Err(BlurDecline::TooFewPixels);
        }
        let pb = pixel_bounds(&raw_pixels);
        let pixels = raw_pixels
            .iter()
            .map(|p| PatchPixel {
                x: p.x,
                y: p.y,
                value: p.value,
            })
            .collect();
        let inv_s = 1.0 / frame.scale;
        let norm_mat = Matrix3::new(
            inv_s,
            0.0,
            -frame.c0[0] * inv_s,
            0.0,
            inv_s,
            -frame.c0[1] * inv_s,
            0.0,
            0.0,
            1.0,
        );
        Ok(Self {
            polygons,
            pixels,
            raw_pixels,
            pixel_bounds: pb,
            c0: frame.c0,
            scale: frame.scale,
            norm_mat,
            nominal_h: frame.h,
            window_cells,
            options: *options,
        })
    }

    /// Observed patch pixels used in [`Self::evaluate`].
    pub fn pixels(&self) -> &[PatchPixel] {
        &self.pixels
    }

    /// Number of pixels in the extracted patch.
    pub fn pixel_count(&self) -> usize {
        self.pixels.len()
    }

    /// Board-space window half-width (cells) passed at extraction.
    pub fn window_cells(&self) -> f64 {
        self.window_cells
    }

    /// Nominal corner image position (px) from the extraction frame.
    pub fn nominal_corner(&self) -> Point2<f64> {
        Point2::new(self.c0[0], self.c0[1])
    }

    /// Local cell scale `√|det J|` (px/cell) at extraction.
    pub fn nominal_scale_px(&self) -> f64 {
        self.scale
    }

    /// Normalized 8-entry extraction homography `H̃_0`.
    pub fn nominal_homography(&self) -> [f64; 8] {
        self.nominal_h
    }

    /// Convert an arbitrary board → image [`Homography`] into this patch's fixed
    /// normalized coordinate frame `N · H`, returning `None` if `h₂₂ ≈ 0` or
    /// non-finite.
    pub fn normalize_homography(&self, h: &Homography<f64>) -> Option<[f64; 8]> {
        let m = self.norm_mat * h.h;
        let w = m[(2, 2)];
        if !(w.is_finite() && w.abs() > 1e-12) {
            return None;
        }
        let inv_w = 1.0 / w;
        let out = [
            m[(0, 0)] * inv_w,
            m[(0, 1)] * inv_w,
            m[(0, 2)] * inv_w,
            m[(1, 0)] * inv_w,
            m[(1, 1)] * inv_w,
            m[(1, 2)] * inv_w,
            m[(2, 0)] * inv_w,
            m[(2, 1)] * inv_w,
        ];
        out.iter().all(|v| v.is_finite()).then_some(out)
    }

    /// Evaluate residuals and exact analytic Jacobians at `input`, returning
    /// `None` if the covariance `Σ` is not positive definite above the box
    /// floor, parameters are non-finite, or the `(A, C)` fit is singular.
    pub fn evaluate(&self, input: &PatchEvalInput) -> Option<PatchEval> {
        let sigma = [
            f64::from(input.kernel.xx),
            f64::from(input.kernel.xy),
            f64::from(input.kernel.yy),
        ];
        if sigma.iter().any(|v| !v.is_finite())
            || input.illumination_gradient.iter().any(|v| !v.is_finite())
            || !input.gamma.is_finite()
            || !input.bleed_kappa.is_finite()
        {
            return None;
        }
        let box_model = box_model_for(sigma);
        let (eig_lo, _) = eig2(sigma);
        if eig_lo < box_model.min_eigenvalue() {
            return None;
        }
        let h_tilde = self.normalize_homography(&input.homography)?;
        let bleed_kappa = if self.options.fit_ink_bleed {
            input.bleed_kappa.clamp(-MAX_BLEED_KAPPA, MAX_BLEED_KAPPA)
        } else {
            0.0
        };
        let gamma = input.gamma.clamp(MIN_GAMMA, MAX_GAMMA);

        let mut theta = Vector::zeros();
        theta[I_SIGMA] = sigma[0];
        theta[I_SIGMA + 1] = sigma[1];
        theta[I_SIGMA + 2] = sigma[2];
        for k in 0..8 {
            theta[I_H + k] = h_tilde[k];
        }
        theta[I_BLEED] = bleed_kappa;

        let model = Model {
            polygons: &self.polygons,
            c0: self.c0,
            scale: self.scale,
            box_model,
            fixed: [false; NP],
            pixel_bounds: self.pixel_bounds,
            radiometric: self.options.radiometric,
            fit_ink_bleed: self.options.fit_ink_bleed,
            blur_gradient: input.blur_gradient,
        };
        let (polys, linv) = model.pass_polygons(&theta)?;
        let sv = box_model.shift_var();
        let l = cholesky2([sigma[0] - sv, sigma[1], sigma[2] - sv])?;
        let dls = Model::dl_dsigma(l);
        let scatter = input
            .blur_gradient
            .map(|grad| super::model::ScatterPass::new(grad, l, linv, dls));
        let inv_n = 1.0
            / match box_model {
                BoxModel::Mixture => 4.0,
                BoxModel::Gaussian => 1.0,
            };

        let m = self.raw_pixels.len();
        let [alpha, beta] = input.illumination_gradient;
        let fit_bleed = self.options.fit_ink_bleed;
        let mut bs = Vec::with_capacity(m);
        let mut shades = Vec::with_capacity(m);
        let mut dbs = Vec::with_capacity(m);

        for px in &self.raw_pixels {
            let (b_raw, dl, dh) = model.sample_pixel(&polys, scatter.as_ref(), *px, linv, true);
            let (b, db_db_raw, db_dkappa) = bleed_eval(b_raw, bleed_kappa, fit_bleed);
            let dx = (f64::from(px.x) - self.c0[0]) / self.scale;
            let dy = (f64::from(px.y) - self.c0[1]) / self.scale;
            let shade = 1.0 + alpha * dx + beta * dy;
            let factor = inv_n * db_db_raw;
            let mut db = [0.0; MAX_PATCH_PARAMS];
            for k in 0..8 {
                db[k] = factor * dh[k];
            }
            for si in 0..3 {
                db[8 + si] =
                    factor * (dl[0] * dls[0][si] + dl[1] * dls[1][si] + dl[2] * dls[2][si]);
            }
            if fit_bleed {
                db[11] = db_dkappa;
            }
            bs.push(b);
            shades.push(shade);
            dbs.push(db);
        }

        if self.options.radiometric.is_linear() && input.fixed_ac.is_none() {
            self.eval_linear_varpro(&bs, &shades, &dbs, &h_tilde)
        } else {
            self.eval_forward_curve(input, gamma, &bs, &shades, &dbs, &h_tilde)
        }
    }

    fn eval_linear_varpro(
        &self,
        bs: &[f64],
        shades: &[f64],
        dbs: &[[f64; MAX_PATCH_PARAMS]],
        h_tilde: &[f64; 8],
    ) -> Option<PatchEval> {
        let m = self.raw_pixels.len();
        let (mut g00, mut g01, mut g11, mut r0, mut r1) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for i in 0..m {
            let p0 = shades[i] * (1.0 - bs[i]);
            let p1 = shades[i];
            let y = self.raw_pixels[i].value;
            g00 += p0 * p0;
            g01 += p0 * p1;
            g11 += p1 * p1;
            r0 += p0 * y;
            r1 += p1 * y;
        }
        let inv_g = invert_2x2_sym(g00, g01, g11)?;
        let a = inv_g[0][0] * r0 + inv_g[0][1] * r1;
        let c = inv_g[1][0] * r0 + inv_g[1][1] * r1;
        if !(a.is_finite() && c.is_finite()) {
            return None;
        }

        let mut residuals = Vec::with_capacity(m);
        let mut cost = 0.0;
        let n_params = if self.options.fit_ink_bleed { 12 } else { 11 };
        let mut phi_t_u = [[0.0f64; 2]; MAX_PATCH_PARAMS];
        let mut dphi_t_r0 = [0.0f64; MAX_PATCH_PARAMS];

        for i in 0..m {
            let p0 = shades[i] * (1.0 - bs[i]);
            let p1 = shades[i];
            let r = self.raw_pixels[i].value - (a * p0 + c * p1);
            residuals.push(r);
            cost += r * r;
            let minus_a_s = -a * shades[i];
            let minus_s_r = -shades[i] * r;
            for p in 0..n_params {
                let dbp = dbs[i][p];
                let u = minus_a_s * dbp;
                phi_t_u[p][0] += p0 * u;
                phi_t_u[p][1] += p1 * u;
                dphi_t_r0[p] += minus_s_r * dbp;
            }
        }

        let mut vw = [[0.0f64; 2]; MAX_PATCH_PARAMS];
        for p in 0..n_params {
            let diff0 = phi_t_u[p][0] - dphi_t_r0[p];
            let diff1 = phi_t_u[p][1];
            vw[p][0] = inv_g[0][0] * diff0 + inv_g[0][1] * diff1;
            vw[p][1] = inv_g[1][0] * diff0 + inv_g[1][1] * diff1;
        }

        let mut d_res_d_h = Vec::with_capacity(m);
        let mut d_res_d_corner = Vec::with_capacity(m);
        let mut d_res_d_sigma = Vec::with_capacity(m);
        let mut d_res_d_bleed = self.options.fit_ink_bleed.then(|| Vec::with_capacity(m));

        for i in 0..m {
            let p0 = shades[i] * (1.0 - bs[i]);
            let p1 = shades[i];
            let a_s = a * shades[i];
            let mut jh = [0.0; 8];
            for k in 0..8 {
                jh[k] = a_s * dbs[i][k] + p0 * vw[k][0] + p1 * vw[k][1];
            }
            d_res_d_corner.push(corner_jac_from_h(&jh, h_tilde, self.scale));
            d_res_d_h.push(jh);
            d_res_d_sigma.push([
                a_s * dbs[i][8] + p0 * vw[8][0] + p1 * vw[8][1],
                a_s * dbs[i][9] + p0 * vw[9][0] + p1 * vw[9][1],
                a_s * dbs[i][10] + p0 * vw[10][0] + p1 * vw[10][1],
            ]);
            if let Some(ref mut jb) = d_res_d_bleed {
                jb.push(a_s * dbs[i][11] + p0 * vw[11][0] + p1 * vw[11][1]);
            }
        }

        Some(PatchEval {
            residuals,
            contrast: a,
            black_level: c,
            cost,
            d_res_d_h,
            d_res_d_corner,
            d_res_d_sigma,
            d_res_d_gamma: None,
            d_res_d_bleed,
        })
    }

    fn eval_forward_curve(
        &self,
        input: &PatchEvalInput,
        gamma: f64,
        bs: &[f64],
        shades: &[f64],
        dbs: &[[f64; MAX_PATCH_PARAMS]],
        h_tilde: &[f64; 8],
    ) -> Option<PatchEval> {
        let (a, c) = match input.fixed_ac {
            Some([a, c]) => {
                if !(a.is_finite() && c.is_finite()) {
                    return None;
                }
                (a, c)
            }
            None => {
                let [a, c] = self.solve_ac_linearized(gamma, bs, shades)?;
                (a, c)
            }
        };
        let m = self.raw_pixels.len();
        let is_free_gamma = self.options.radiometric.is_free_gamma();
        let mut residuals = Vec::with_capacity(m);
        let mut cost = 0.0;
        let mut d_res_d_h = Vec::with_capacity(m);
        let mut d_res_d_corner = Vec::with_capacity(m);
        let mut d_res_d_sigma = Vec::with_capacity(m);
        let mut d_res_d_gamma = is_free_gamma.then(|| Vec::with_capacity(m));
        let mut d_res_d_bleed = self.options.fit_ink_bleed.then(|| Vec::with_capacity(m));

        for i in 0..m {
            let lin = shades[i] * (c + a * (1.0 - bs[i]));
            let (obs, d_obs_d_lin, d_obs_d_gamma) = self.options.radiometric.forward(lin, gamma);
            let r = self.raw_pixels[i].value - obs;
            residuals.push(r);
            cost += r * r;

            let coeff = d_obs_d_lin * shades[i] * a;
            let mut jh = [0.0; 8];
            for k in 0..8 {
                jh[k] = coeff * dbs[i][k];
            }
            d_res_d_corner.push(corner_jac_from_h(&jh, h_tilde, self.scale));
            d_res_d_h.push(jh);
            d_res_d_sigma.push([coeff * dbs[i][8], coeff * dbs[i][9], coeff * dbs[i][10]]);
            if let Some(ref mut jg) = d_res_d_gamma {
                jg.push(-d_obs_d_gamma);
            }
            if let Some(ref mut jb) = d_res_d_bleed {
                jb.push(coeff * dbs[i][11]);
            }
        }

        Some(PatchEval {
            residuals,
            contrast: a,
            black_level: c,
            cost,
            d_res_d_h,
            d_res_d_corner,
            d_res_d_sigma,
            d_res_d_gamma,
            d_res_d_bleed,
        })
    }

    fn solve_ac_linearized(&self, gamma: f64, bs: &[f64], shades: &[f64]) -> Option<[f64; 2]> {
        let (mut g00, mut g01, mut g11, mut r0, mut r1) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (i, px) in self.raw_pixels.iter().enumerate() {
            let lin_y = match self.options.radiometric {
                RadiometricModel::FreeGamma { .. } => {
                    255.0 * (px.value / 255.0).clamp(0.0, 1.0).powf(gamma)
                }
                other => other.inverse(px.value),
            };
            let p0 = shades[i] * (1.0 - bs[i]);
            let p1 = shades[i];
            g00 += p0 * p0;
            g01 += p0 * p1;
            g11 += p1 * p1;
            r0 += p0 * lin_y;
            r1 += p1 * lin_y;
        }
        let inv_g = invert_2x2_sym(g00, g01, g11)?;
        let a = inv_g[0][0] * r0 + inv_g[0][1] * r1;
        let c = inv_g[1][0] * r0 + inv_g[1][1] * r1;
        (a.is_finite() && c.is_finite()).then_some([a, c])
    }
}

fn invert_2x2_sym(g00: f64, g01: f64, g11: f64) -> Option<[[f64; 2]; 2]> {
    let det = g00 * g11 - g01 * g01;
    if !(det.is_finite() && g00 > 0.0 && g11 > 0.0 && det > AC_COND_TOL * g00 * g11) {
        return None;
    }
    let inv_det = 1.0 / det;
    Some([
        [g11 * inv_det, -g01 * inv_det],
        [-g01 * inv_det, g00 * inv_det],
    ])
}

#[inline]
fn corner_jac_from_h(jh: &[f64; 8], h_tilde: &[f64; 8], scale: f64) -> [f64; 2] {
    let inv_s = 1.0 / scale;
    [
        inv_s * (h_tilde[6] * jh[0] + h_tilde[7] * jh[1] + jh[2]),
        inv_s * (h_tilde[6] * jh[3] + h_tilde[7] * jh[4] + jh[5]),
    ]
}

#[cfg(test)]
#[path = "patch_residual_tests.rs"]
mod tests;
