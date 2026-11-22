//! The per-corner fit: window selection, initialisation, Levenberg–Marquardt,
//! the window-growth / decline rules, and the HAC covariance.

use super::frame::{in_support, pixel_bounds, select_pixels, Frame};
use super::gauss_polygon::CUT;
use super::model::{
    cholesky2, eig2, sigma_of, BoardPolygon, BoxModel, Model, Pixel, Vector, Want, I_A, I_C, I_H,
    I_SIGMA, NP,
};
use super::solve::{
    bartlett_bandwidth, hac_covariance, levenberg_marquardt, photometric_vif,
    residual_autocorrelation, sigma_px_gradient, Reparam,
};
use super::{BlurDecline, BlurFitQuality, BlurKernel, BlurPattern, CornerBlur};
use crate::{GrayImageView, Homography};

/// Window half-widths tried, in cells: start small, grow while the blur is
/// confounded with the photometry.
const WINDOWS: [f64; 5] = [0.5, 0.75, 1.0, 1.5, 2.0];

/// Variance inflation of `σ` by the photometric parameters beyond which a
/// larger window is tried (the confounding doubles its standard error).
/// Not a decline: a confounded estimate's standard error says so, and it
/// stays honest (measured up to σ/cell 0.38 on interior corners).
const MAX_VIF: f64 = 4.0;

/// Pixels stay this many blur standard deviations (plus the pixel's
/// half-width) clear of any cell whose print is unknown: the leakage of a
/// wrong half-plane there is `Q(3) = 0.13 %` of the contrast, below one
/// 8-bit step at full contrast.
const SUPPORT_MARGIN_SIGMAS: f64 = 3.0;

/// The unit box's variance per axis, px².
pub(super) const PIXEL_VAR: f64 = 1.0 / 12.0;

/// Minimum pixels per free parameter.
const MIN_PIXELS_PER_PARAM: usize = 4;

/// Bounds of the initial isotropic scan, px.
const SCAN_MIN_SIGMA: f64 = 0.3;
const SCAN_STEP: f64 = 2.0;

/// Total blur (smallest principal σ, px) from which the pixel box is
/// treated as Gaussian, one sample per pixel. The bias a single Gaussian
/// leaves falls with the box's excess kurtosis relative to the blur,
/// `(1/120)/σ⁴`: measured on the synthetic suite it is about `0.06 %/σ⁴`
/// (0.6 % at 0.58 px, 0.1 % at 1.04 px), below 0.02 % from 1.3 px — a tenth
/// of the smallest standard error the estimator reports. Below it, the
/// four-point mixture (see `model`).
const GAUSSIAN_BOX_MIN_SIGMA: f64 = 1.3;

fn box_model_for(sigma: [f64; 3]) -> BoxModel {
    if eig2(sigma).0.sqrt() >= GAUSSIAN_BOX_MIN_SIGMA {
        BoxModel::Gaussian
    } else {
        BoxModel::Mixture
    }
}

/// The best isotropic blur on a σ grid with everything else simplified:
/// `θ`'s homography held, no illumination gradient, the photometry solved
/// linearly at each σ.
struct Profile {
    sigma: f64,
    sse: f64,
    a: f64,
    c: f64,
}

/// Profile the misfit over isotropic `σ` — geometric steps of
/// [`SCAN_STEP`] from [`SCAN_MIN_SIGMA`] to half a cell, refined by a
/// parabola in `log σ` through the best step — at `theta`'s homography, on
/// `pixels`, with the pixel box treated as Gaussian (one sample per pixel).
/// With `stop_at_rise` the scan ends once the misfit rises past its minimum
/// (a seed needs no more); otherwise it covers the whole range and finds the
/// best basin.
fn sigma_profile(
    model: &Model<'_>,
    theta: &Vector,
    pixels: &[Pixel],
    stop_at_rise: bool,
) -> Option<Profile> {
    let scan = Model {
        box_model: BoxModel::Gaussian,
        ..*model
    };
    // value ≈ C + A (1 − B): least squares in (A, C) at one σ.
    let fit_at = |s: f64| -> Option<Profile> {
        let b = scan.blackness_iso(theta, s, pixels);
        let (mut s11, mut s12, mut s22, mut b1, mut b2, mut yy) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for (px, bk) in pixels.iter().zip(&b) {
            let f = 1.0 - bk;
            s11 += f * f;
            s12 += f;
            s22 += 1.0;
            b1 += f * px.value;
            b2 += px.value;
            yy += px.value * px.value;
        }
        let det = s11 * s22 - s12 * s12;
        (det > 1e-9 * s11 * s22).then(|| {
            let a = (b1 * s22 - b2 * s12) / det;
            let c = (s11 * b2 - s12 * b1) / det;
            Profile {
                sigma: s,
                sse: (yy - a * b1 - c * b2).max(0.0),
                a,
                c,
            }
        })
    };
    let mut steps: Vec<(f64, f64)> = Vec::new();
    let mut best: Option<usize> = None;
    let mut s = SCAN_MIN_SIGMA;
    while s <= 0.5 * model.scale {
        if let Some(p) = fit_at(s) {
            steps.push((s, p.sse));
            let k = steps.len() - 1;
            if best.is_none_or(|b| p.sse < steps[b].1) {
                best = Some(k);
            } else if stop_at_rise {
                break;
            }
        }
        s *= SCAN_STEP;
    }
    let b = best?;
    let mut sigma = steps[b].0;
    if b > 0 && b + 1 < steps.len() {
        let (y0, y1, y2) = (steps[b - 1].1, steps[b].1, steps[b + 1].1);
        let curv = y0 - 2.0 * y1 + y2;
        if curv > 0.0 {
            let offset = (0.5 * (y0 - y2) / curv).clamp(-1.0, 1.0);
            sigma *= SCAN_STEP.powf(offset);
        }
    }
    let refined = fit_at(sigma)?;
    if refined.sse <= steps[b].1 {
        Some(refined)
    } else {
        fit_at(steps[b].0)
    }
}

/// The caller's homography with the profile's isotropic blur and
/// photometry, no gradient.
fn seed_from(frame: &Frame, p: &Profile) -> Vector {
    let mut th = Vector::zeros();
    for (k, v) in frame.h.iter().enumerate() {
        th[I_H + k] = *v;
    }
    th[I_SIGMA] = p.sigma * p.sigma;
    th[I_SIGMA + 2] = p.sigma * p.sigma;
    th[I_A] = p.a;
    th[I_C] = p.c;
    th
}

/// An even subsample of at most `n` pixels for the σ profile.
fn profile_subsample(pixels: &[Pixel], n: usize) -> Vec<Pixel> {
    let stride = (pixels.len() / n).max(1);
    pixels.iter().step_by(stride).copied().collect()
}

/// Pixels the seed's σ profile runs on.
const SEED_PROFILE_PIXELS: usize = 150;
/// Pixels the basin check compares on: enough to tell a different basin
/// (a spurious win only costs a restart that converges back).
const BASIN_PROFILE_PIXELS: usize = 64;

/// Initial `θ`: the caller's homography and the seed of [`sigma_profile`].
fn initial_theta(model: &Model<'_>, frame: &Frame, pixels: &[Pixel]) -> Option<Vector> {
    let mut th = Vector::zeros();
    for (k, v) in frame.h.iter().enumerate() {
        th[I_H + k] = *v;
    }
    let sub = profile_subsample(pixels, SEED_PROFILE_PIXELS);
    Some(seed_from(frame, &sigma_profile(model, &th, &sub, true)?))
}

/// A seed in a better basin than the fit `theta`, if there is one.
///
/// The fit nests the isotropic, gradient-free profile at the caller's
/// homography, so at a global optimum no σ of that profile fits the patch
/// better. The comparison runs on a subsample of the patch; when the profile
/// wins there, the fit sits in a worse basin — typically reached when the
/// blur is wide against the cell and a smaller window could not separate it
/// from the contrast — and is restarted from the profile's best.
fn better_basin(
    model: &Model<'_>,
    frame: &Frame,
    theta: &Vector,
    pixels: &[Pixel],
) -> Option<Vector> {
    let sub = profile_subsample(pixels, BASIN_PROFILE_PIXELS);
    let fitted = model.evaluate(
        theta,
        &sub,
        Want {
            residuals: true,
            ..Want::default()
        },
    )?;
    let mut h = Vector::zeros();
    for (k, v) in frame.h.iter().enumerate() {
        h[I_H + k] = *v;
    }
    let p = sigma_profile(model, &h, &sub, false)?;
    (p.sse < fitted.cost).then(|| seed_from(frame, &p))
}

/// One window's converged fit.
struct WindowFit {
    rho: f64,
    theta: Vector,
    pixels: Vec<Pixel>,
    box_model: BoxModel,
    vif: f64,
    reparam: Reparam,
}

/// Fit at window `rho`, starting from `theta` (or from the scan when
/// `None`), re-selecting the pixels until their margin covers the fitted
/// blur.
fn fit_window(
    image: &GrayImageView<'_>,
    pattern: &BlurPattern,
    polygons: &[BoardPolygon],
    frame: &Frame,
    rho: f64,
    theta: Option<Vector>,
    last: bool,
) -> Result<WindowFit, BlurDecline> {
    let fixed = &[false; NP];
    let free = fixed.iter().filter(|f| !**f).count();
    let margin_for = |t: &Vector| SUPPORT_MARGIN_SIGMAS * eig2(sigma_of(t)).1.sqrt() + 0.5;
    let model_for = |box_model: BoxModel, pixels: &[Pixel]| Model {
        polygons,
        c0: frame.c0,
        scale: frame.scale,
        box_model,
        fixed: *fixed,
        pixel_bounds: pixel_bounds(pixels),
    };
    let select = |margin: f64| -> Result<Vec<Pixel>, BlurDecline> {
        let pixels = select_pixels(image, pattern, frame, rho, margin);
        if pixels.len() < MIN_PIXELS_PER_PARAM * free {
            return Err(BlurDecline::TooFewPixels);
        }
        Ok(pixels)
    };
    // Seed: the previous window's fit, or the scan on a provisional patch.
    let (mut theta, mut margin_px, mut pixels) = match theta {
        Some(t) => {
            let m = margin_for(&t);
            (t, m, select(m)?)
        }
        None => {
            let provisional = select(0.5)?;
            let t = initial_theta(
                &model_for(BoxModel::Gaussian, &provisional),
                frame,
                &provisional,
            )
            .ok_or(BlurDecline::NotConverged)?;
            let m = margin_for(&t);
            (t, m, select(m)?)
        }
    };
    for _ in 0..4 {
        let box_model = box_model_for(sigma_of(&theta));
        let model = model_for(box_model, &pixels);
        if !model.admissible(&theta) {
            return Err(BlurDecline::NotConverged);
        }
        let first = model
            .evaluate(
                &theta,
                &pixels,
                Want {
                    jacobian: true,
                    ..Want::default()
                },
            )
            .ok_or(BlurDecline::NotConverged)?;
        let mut re = Reparam::new(&model, &theta, &first.jtj, rho);
        let (mut th, mut eval) = levenberg_marquardt(&model, theta, &pixels, first, &re)?;
        // A fit the window will grow past is only a seed; check the basin
        // of the one that may be delivered.
        let delivered = last
            || (holds_profile(rho, frame, max_sigma(&th))
                && photometric_vif(&eval.jtj, &re, sigma_of(&th)).is_some_and(|v| v <= MAX_VIF));
        if let Some(seed) = delivered
            .then(|| better_basin(&model, frame, &th, &pixels))
            .flatten()
        {
            if model.admissible(&seed) {
                if let Some(first) = model.evaluate(
                    &seed,
                    &pixels,
                    Want {
                        jacobian: true,
                        ..Want::default()
                    },
                ) {
                    let re2 = Reparam::new(&model, &seed, &first.jtj, rho);
                    let (t2, e2) = levenberg_marquardt(&model, seed, &pixels, first, &re2)?;
                    if e2.cost < eval.cost {
                        th = t2;
                        eval = e2;
                        re = re2;
                    }
                }
            }
        }
        theta = th;
        let sigma = sigma_of(&th);
        let needed = margin_for(&th);
        // Reselect only when the fitted blur reaches past the margin the
        // pixels were chosen with; refit only when that changes the patch.
        let mut same_patch = true;
        if needed > margin_px * 1.05 {
            margin_px = needed;
            let next = select(margin_px)?;
            same_patch = next.len() == pixels.len()
                && next
                    .iter()
                    .zip(&pixels)
                    .all(|(a, b)| a.x == b.x && a.y == b.y);
            pixels = next;
        }
        if same_patch && box_model_for(sigma) == box_model {
            let vif = photometric_vif(&eval.jtj, &re, sigma).ok_or(BlurDecline::Singular)?;
            return Ok(WindowFit {
                rho,
                theta: th,
                pixels,
                box_model,
                vif,
                reparam: re,
            });
        }
    }
    Err(BlurDecline::NotConverged)
}

impl WindowFit {
    /// Whether this window is final: the blur is told from the photometry
    /// ([`MAX_VIF`]) and the window holds the corner's own edge profile
    /// ([`holds_profile`]). Otherwise a larger window is tried.
    fn accepted(&self, frame: &Frame) -> bool {
        self.vif <= MAX_VIF && holds_profile(self.rho, frame, max_sigma(&self.theta))
    }
}

/// Whether the corner itself is among the pixels the fit may use: it lies
/// inside the known print by at least the clearance ([`SUPPORT_MARGIN_SIGMAS`]
/// blur widths plus half a pixel) that keeps unknown print out of the patch.
///
/// Where the clearance swallows the corner — a corner at the edge of the
/// labelled region under a blur comparable to the cell — the patch sees the
/// pattern on one side only, and σ is extrapolated rather than observed
/// (measured: boundary corners honest at σ/cell 0.16 and 0.25, where the
/// corner stays observed; underestimated by up to 7.5 SE at 0.375, where it
/// does not).
fn observes_corner(pattern: &BlurPattern, frame: &Frame, sigma: f64) -> bool {
    let margin_px = SUPPORT_MARGIN_SIGMAS * sigma + 0.5;
    in_support(pattern, [0.0, 0.0], margin_px / frame.scale_min)
}

/// Whether a window of half-width `rho` cells holds the blurred edges
/// through the corner out to the model's own neglect radius, [`CUT`]
/// standard deviations (plus the pixel's half-width), along the narrowest
/// direction of the local frame. A narrower window cuts the profile's tails
/// off: the blur then trades against the geometry and the contrast
/// non-linearly, and the linearised standard error no longer covers it
/// (measured: errors beyond 3 SE in 4 % of corners whose window was 3–4 σ
/// wide, none from 5 σ).
fn holds_profile(rho: f64, frame: &Frame, sigma: f64) -> bool {
    rho * frame.scale_min >= CUT * sigma + 0.5
}

/// The kernel's largest principal standard deviation at `theta`, px.
fn max_sigma(theta: &Vector) -> f64 {
    eig2(sigma_of(theta)).1.sqrt()
}

/// See [`super::estimate_corner_blur`].
pub(super) fn estimate(
    image: &GrayImageView<'_>,
    pattern: &BlurPattern,
    board_to_image: &Homography<f64>,
) -> Result<CornerBlur, BlurDecline> {
    let frame = Frame::new(board_to_image).ok_or(BlurDecline::NoGeometry)?;
    let polygons: Vec<BoardPolygon> = pattern
        .polygons
        .iter()
        .map(|(v, w)| BoardPolygon {
            vertices: v.clone(),
            weight: *w,
        })
        .collect();
    let fixed = [false; NP];
    let mut theta = None;
    let mut best: Option<WindowFit> = None;
    let mut last_err = BlurDecline::TooFewPixels;
    let mut last_count = 0usize;
    for (k, &rho) in WINDOWS.iter().enumerate() {
        let last = k + 1 == WINDOWS.len();
        // A window too narrow for the blur fitted so far would only be
        // rejected: go straight to one that can hold it.
        if theta.is_some_and(|t| !last && !holds_profile(rho, &frame, max_sigma(&t))) {
            continue;
        }
        match fit_window(image, pattern, &polygons, &frame, rho, theta, last) {
            Ok(fit) => {
                // A larger window that adds no pixels cannot help.
                let grew = fit.pixels.len() > last_count;
                last_count = fit.pixels.len();
                theta = Some(fit.theta);
                let done = fit.accepted(&frame);
                if grew || best.is_none() {
                    best = Some(fit);
                }
                if done || !grew {
                    break;
                }
            }
            Err(e) => {
                last_err = e;
                if best.is_some() {
                    break;
                }
            }
        }
    }
    let fit = best.ok_or(last_err)?;
    let sigma = max_sigma(&fit.theta);
    if !holds_profile(fit.rho, &frame, sigma) || !observes_corner(pattern, &frame, sigma) {
        return Err(BlurDecline::TooBlurred);
    }
    finish(&polygons, &frame, &fixed, fit, pattern)
}

/// Covariances, goodness of fit and the remaining decline rules.
fn finish(
    polygons: &[BoardPolygon],
    frame: &Frame,
    fixed: &[bool; NP],
    fit: WindowFit,
    pattern: &BlurPattern,
) -> Result<CornerBlur, BlurDecline> {
    let polarity_known = pattern.polarity_known;
    let WindowFit {
        rho,
        theta,
        pixels,
        box_model,
        reparam,
        ..
    } = fit;
    if polarity_known && theta[I_A] <= 0.0 {
        return Err(BlurDecline::PolarityMismatch);
    }
    let model = Model {
        polygons,
        c0: frame.c0,
        scale: frame.scale,
        box_model,
        fixed: *fixed,
        pixel_bounds: pixel_bounds(&pixels),
    };
    let full = model
        .evaluate(
            &theta,
            &pixels,
            Want {
                jacobian: true,
                scores: true,
                residuals: true,
            },
        )
        .ok_or(BlurDecline::Singular)?;
    let bread = reparam
        .inverse(&full.jtj, &[true; NP])
        .ok_or(BlurDecline::Singular)?;
    let sigma = sigma_of(&theta);
    let autocorrelation = residual_autocorrelation(&pixels, &full.residuals);
    let lag = bartlett_bandwidth(autocorrelation, pixels.len());
    let nfree = reparam.free_count();
    let cov = hac_covariance(&bread, &pixels, &full.scores, lag, nfree);
    let mut cov3 = [[0.0f64; 3]; 3];
    for (r, row) in cov3.iter_mut().enumerate() {
        for (c, v) in row.iter_mut().enumerate() {
            *v = cov[(I_SIGMA + r, I_SIGMA + c)];
        }
    }
    let g = sigma_px_gradient(sigma);
    let var = (g.transpose() * cov * g)[(0, 0)];
    if !(var.is_finite() && var > 0.0) {
        return Err(BlurDecline::Singular);
    }
    // The model's physical domain is Σ ⪰ I/12 (a lens kernel added to the
    // unit box); an estimate outside it is not a measurement of the model —
    // a sharpened image, an aperture smaller than the pixel, or a fit the
    // patch does not support.
    if eig2(sigma).0 < PIXEL_VAR {
        return Err(BlurDecline::SharperThanPixel);
    }
    // The reported σ ± SE linearises √(tr Σ / 2); that is meaningful only
    // while the trace is resolved from zero, its 2-SE interval excluding 0
    // (equivalently SE(σ)/σ < 1/4). Beyond it the kernel is not measured.
    let var_trace = cov3[0][0] + 2.0 * cov3[0][2] + cov3[2][2];
    if sigma[0] + sigma[2] <= 2.0 * var_trace.max(0.0).sqrt() {
        return Err(BlurDecline::Unresolved);
    }
    let n = pixels.len() as f64;
    let rms = (full.cost / (n - nfree as f64).max(1.0)).sqrt();
    let contrast = theta[I_A].abs();
    debug_assert!(cholesky2(sigma).is_some());
    Ok(CornerBlur::new(
        BlurKernel::new(sigma[0] as f32, sigma[1] as f32, sigma[2] as f32),
        cov3.map(|row| row.map(|v| v as f32)),
        BlurFitQuality::new(
            rho as f32,
            pixels.len(),
            contrast as f32,
            rms as f32,
            autocorrelation as f32,
        ),
    ))
}
