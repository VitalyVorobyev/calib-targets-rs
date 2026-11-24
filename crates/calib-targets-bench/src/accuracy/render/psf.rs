//! Extended PSF models, Scheimpflug tilted-sensor scatter rendering, and
//! physical e-ink display surface rendering for ground-truth accuracy tests.

use calib_targets::blur::{DisplaySurfaceModel, ThinLensDefocusModel};
use nalgebra::{Matrix3, Point2, Vector3};

use super::{apply, reach_for, Camera, Irradiance, Pose, Scene, SplitMix, Supersampled};

/// Quarter-pixel steps per pixel in [`Supersampled::integrate_psf`].
const QUARTER: usize = 4;
const QUARTER_PX: f64 = 1.0 / QUARTER as f64;

/// A lens point-spread function beyond the isotropic Gaussian of
/// [`Supersampled::integrate`], for the blur study's PSF conditions.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Psf {
    /// An arbitrary 2-D positive-definite Gaussian covariance `[Σxx, Σxy, Σyy]` px².
    Gaussian {
        /// `[Σxx, Σxy, Σyy]`, px².
        covariance: [f64; 3],
    },
    /// A uniform disk (a defocus pillbox) of `radius` px.
    Disk {
        /// Radius, px.
        radius: f64,
    },
    /// Heavy-tailed Moffat profile `(1 + r² / α²)^(-β)` (`beta > 2.0` for finite
    /// second moment `σ² = α² / (2 (β - 2))` per axis).
    Moffat {
        /// Core scale parameter `α`, px.
        alpha: f64,
        /// Tail exponent `β > 2.0`.
        beta: f64,
    },
    /// Diffraction-limited monochromatic Airy pattern `(2 J₁(x) / x)²` with first
    /// zero at radius `first_zero_radius` (`1.22 λ f/#`) in pixels.
    Airy {
        /// Radius of the first dark ring, px.
        first_zero_radius: f64,
    },
}

impl Psf {
    /// Apron the supersampling needs for this PSF, expressed as the
    /// isotropic-Gaussian `σ` that [`super::supersample`] sizes its apron from.
    pub fn apron_sigma(&self) -> f64 {
        match *self {
            Self::Gaussian { covariance: c } => 1.3 * c[0].max(c[2]).sqrt(),
            Self::Disk { radius } => 0.25 * radius + 0.25,
            Self::Moffat { alpha, beta } => {
                assert!(
                    alpha > 0.0 && beta > 2.0,
                    "Moffat requires alpha > 0, beta > 2"
                );
                let sigma = alpha / (2.0 * (beta - 2.0)).sqrt();
                let reach_px = (6.0 * sigma).max(8.0);
                0.25 * reach_px + 0.25
            }
            Self::Airy { first_zero_radius } => {
                assert!(
                    first_zero_radius > 0.0,
                    "Airy requires first_zero_radius > 0"
                );
                let reach_px = (5.0 * first_zero_radius).max(8.0);
                0.25 * reach_px + 0.25
            }
        }
    }
}

/// Physical model parameters for rendering a discrete e-ink screen target
/// (such as a PocketBook e-reader).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EinkSurfaceSpec {
    /// Physical display pixel density, pixels per inch.
    pub display_ppi: f64,
    /// Board square pitch in millimeters.
    pub cell_mm: f64,
    /// Controller palette levels (e.g. `16` for 4-bit E Ink Carta).
    pub gray_levels: u8,
    /// Surface microcapsule + cover-glass MTF Gaussian σ in display pixels.
    pub surface_mtf_dpx: f64,
    /// Probit microcapsule dark-spread bleed `κ` (std devs at MTF σ).
    pub microcapsule_bleed_kappa: f64,
}

impl EinkSurfaceSpec {
    /// Equivalent [`DisplaySurfaceModel`] descriptor for the detector / refiner.
    pub fn to_display_surface_model(&self) -> DisplaySurfaceModel {
        let dpx_mm = 25.4 / self.display_ppi;
        let nd = self.cell_mm / dpx_mm;
        DisplaySurfaceModel::pocketbook_eink(nd)
            .with_mtf_sigma_display_px(self.surface_mtf_dpx)
            .with_gray_levels(Some(u16::from(self.gray_levels.max(2))))
    }
}

/// A 1-D Gaussian of per-axis variance `var` px² sampled at quarter-pixel
/// steps: `(offsets in steps, weights)`, the weights summing to 1.
fn quarter_gaussian(var: f64) -> Vec<(i64, f64)> {
    if var <= 0.0 {
        return vec![(0, 1.0)];
    }
    let reach = (4.5 * var.sqrt() / QUARTER_PX).ceil() as i64;
    let mut w: Vec<(i64, f64)> = (-reach..=reach)
        .map(|k| {
            let d = k as f64 * QUARTER_PX;
            (k, (-0.5 * d * d / var).exp())
        })
        .collect();
    let total: f64 = w.iter().map(|(_, v)| v).sum();
    for (_, v) in &mut w {
        *v /= total;
    }
    w
}

/// Build the `4×4` box-averaged quarter grid `B[qy, qx]` over `[0, qw) × [0, qh)`,
/// shifted so `B[qy, qx]` is the average of `[qx..qx+4] × [qy..qy+4]`.
fn box_average_quarter(quarter: &[f64], qw: usize, qh: usize) -> Vec<f64> {
    let clamp = |x: usize, lim: usize| x.min(lim - 1);
    let mut horiz = vec![0.0f64; qw * qh];
    for y in 0..qh {
        let row = &quarter[y * qw..(y + 1) * qw];
        for x in 0..qw {
            horiz[y * qw + x] = 0.25
                * (row[x] + row[clamp(x + 1, qw)] + row[clamp(x + 2, qw)] + row[clamp(x + 3, qw)]);
        }
    }
    let mut out = vec![0.0f64; qw * qh];
    for y in 0..qh {
        let y0 = y * qw;
        let y1 = clamp(y + 1, qh) * qw;
        let y2 = clamp(y + 2, qh) * qw;
        let y3 = clamp(y + 3, qh) * qw;
        for x in 0..qw {
            out[y0 + x] = 0.25 * (horiz[y0 + x] + horiz[y1 + x] + horiz[y2 + x] + horiz[y3 + x]);
        }
    }
    out
}

fn convolve_box_quarter_taps(
    box_q: &[f64],
    qw: usize,
    qh: usize,
    apron: usize,
    size: [usize; 2],
    taps: &[(i64, i64, f64)],
) -> Irradiance {
    let [width, height] = size;
    let mut values = vec![0.0f64; width * height];
    let qwm = qw as i64 - 1;
    let qhm = qh as i64 - 1;
    for y in 0..height {
        let cy = ((y + apron) * QUARTER) as i64;
        for x in 0..width {
            let cx = ((x + apron) * QUARTER) as i64;
            let mut acc = 0.0f64;
            for &(dx, dy, w) in taps {
                let ix = (cx + dx).clamp(0, qwm) as usize;
                let iy = (cy + dy).clamp(0, qhm) as usize;
                acc += w * box_q[iy * qw + ix];
            }
            values[y * width + x] = acc;
        }
    }
    Irradiance {
        values,
        width,
        height,
    }
}

fn discrete_cov_from_taps(taps: &[(i64, i64, f64)]) -> [f64; 3] {
    let mut cov = [0.0f64; 3];
    for &(dx, dy, w) in taps {
        let (x, y) = (dx as f64 * QUARTER_PX, dy as f64 * QUARTER_PX);
        cov[0] += w * x * x;
        cov[1] += w * x * y;
        cov[2] += w * y * y;
    }
    cov
}

fn normalize_taps(taps: &mut Vec<(i64, i64, f64)>) {
    let total: f64 = taps.iter().map(|t| t.2).sum();
    assert!(total > 0.0, "empty PSF kernel");
    for t in taps {
        t.2 /= total;
    }
}

impl Supersampled {
    /// Downsample the stratified `S × S` buffer onto the `QUARTER` lattice.
    fn to_quarter_grid(&self) -> (Vec<f64>, usize, usize) {
        let Self {
            s,
            reach: apron,
            width,
            height,
            ..
        } = *self;
        assert!(s % QUARTER == 0, "supersampling must be a multiple of 4");
        let block = s / QUARTER;
        let hi_w = (width + 2 * apron) * s;
        let qw = (width + 2 * apron) * QUARTER;
        let qh = (height + 2 * apron) * QUARTER;
        let mut quarter = vec![0.0f64; qw * qh];
        let norm = 1.0 / (block * block) as f64;
        for qy in 0..qh {
            for qx in 0..qw {
                let mut acc = 0.0f64;
                for by in 0..block {
                    let row = (qy * block + by) * hi_w + qx * block;
                    for v in &self.samples[row..row + block] {
                        acc += f64::from(*v);
                    }
                }
                quarter[qy * qw + qx] = acc * norm;
            }
        }
        (quarter, qw, qh)
    }

    /// Integrate every output pixel with the pixel box convolved with `psf`,
    /// returning the irradiance and the discrete lens kernel covariance `Σ_lens`
    /// (excluding pixel box variance `1/12 I₂`).
    pub fn integrate_psf(&self, psf: &Psf) -> (Irradiance, [f64; 3]) {
        let (quarter, qw, qh) = self.to_quarter_grid();
        let box_q = box_average_quarter(&quarter, qw, qh);
        let mut taps = match *psf {
            Psf::Gaussian { covariance: c } => {
                let det = c[0] * c[2] - c[1] * c[1];
                assert!(
                    c[0] >= 0.0 && c[2] >= 0.0 && det >= 0.0,
                    "Gaussian covariance must be positive semidefinite: {c:?}"
                );
                if c[1].abs() < 1e-12 {
                    let wx = quarter_gaussian(c[0]);
                    let wy = quarter_gaussian(c[2]);
                    let mut kernel_cov = [0.0f64; 3];
                    kernel_cov[0] = wx
                        .iter()
                        .map(|(k, v)| v * (*k as f64 * QUARTER_PX).powi(2))
                        .sum();
                    kernel_cov[2] = wy
                        .iter()
                        .map(|(k, v)| v * (*k as f64 * QUARTER_PX).powi(2))
                        .sum();
                    let irr = convolve_separable_box_quarter(
                        &box_q,
                        qw,
                        qh,
                        self.reach,
                        [self.width, self.height],
                        &wx,
                        &wy,
                    );
                    return (irr, kernel_cov);
                }
                assert!(det > 0.0, "anisotropic Gaussian requires det > 0");
                let inv_det = 1.0 / det;
                let a = c[2] * inv_det;
                let b = -c[1] * inv_det;
                let d = c[0] * inv_det;
                let rx = (5.0 * c[0].sqrt() / QUARTER_PX).ceil() as i64;
                let ry = (5.0 * c[2].sqrt() / QUARTER_PX).ceil() as i64;
                let mut out = Vec::new();
                for dy in -ry..=ry {
                    let py = dy as f64 * QUARTER_PX;
                    for dx in -rx..=rx {
                        let px = dx as f64 * QUARTER_PX;
                        let maha = a * px * px + 2.0 * b * px * py + d * py * py;
                        if maha <= 26.0 {
                            out.push((dx, dy, (-0.5 * maha).exp()));
                        }
                    }
                }
                out
            }
            Psf::Disk { radius } => disk_taps(radius),
            Psf::Moffat { alpha, beta } => {
                assert!(
                    alpha > 0.0 && beta > 2.0,
                    "Moffat requires alpha > 0, beta > 2"
                );
                let sigma = alpha / (2.0 * (beta - 2.0)).sqrt();
                let reach_px = (6.0 * sigma).max(8.0);
                let r = (reach_px / QUARTER_PX).ceil() as i64;
                let inv_a2 = 1.0 / (alpha * alpha);
                let mut out = Vec::new();
                for dy in -r..=r {
                    let py = dy as f64 * QUARTER_PX;
                    for dx in -r..=r {
                        let px = dx as f64 * QUARTER_PX;
                        let r2 = px * px + py * py;
                        if r2 <= reach_px * reach_px {
                            out.push((dx, dy, (1.0 + r2 * inv_a2).powf(-beta)));
                        }
                    }
                }
                out
            }
            Psf::Airy { first_zero_radius } => {
                assert!(
                    first_zero_radius > 0.0,
                    "Airy requires first_zero_radius > 0"
                );
                const J1_ZERO: f64 = 3.831_705_970_207_512_5;
                let reach_px = (5.0 * first_zero_radius).max(8.0);
                let r = (reach_px / QUARTER_PX).ceil() as i64;
                let k = J1_ZERO / first_zero_radius;
                let mut out = Vec::new();
                for dy in -r..=r {
                    let py = dy as f64 * QUARTER_PX;
                    for dx in -r..=r {
                        let px = dx as f64 * QUARTER_PX;
                        let rad = px.hypot(py);
                        if rad <= reach_px {
                            let val = if rad < 1e-12 {
                                1.0
                            } else {
                                let x = k * rad;
                                let amp = 2.0 * libm::j1(x) / x;
                                amp * amp
                            };
                            out.push((dx, dy, val));
                        }
                    }
                }
                out
            }
        };
        normalize_taps(&mut taps);
        let kernel_cov = discrete_cov_from_taps(&taps);
        let irr =
            convolve_box_quarter_taps(&box_q, qw, qh, self.reach, [self.width, self.height], &taps);
        (irr, kernel_cov)
    }

    /// Physical spatially-varying scatter integration (`Σ_lens` queried in image
    /// area coordinates `(ux, uy)` with `x ∈ [0, width), y ∈ [0, height)`).
    ///
    /// Downsamples `self` into half-pixel source cells (`var = 1/48 I₂`) and
    /// scatters each half-pixel onto output integer pixels with normalized
    /// Gaussian kernel `Σ_kern = Σ_lens + 1/16 I₂` (so the total effective PSF
    /// has variance `Σ_lens + 1/12 I₂`).
    pub fn integrate_scatter_defocus<F>(&self, mut sigma_at: F) -> Irradiance
    where
        F: FnMut(f64, f64) -> Option<[f64; 3]>,
    {
        const HALF: usize = 2;
        const HALF_PX: f64 = 0.5;
        const KERN_BOX_COMPENSATION: f64 = 1.0 / 16.0;
        let Self {
            s,
            reach: apron,
            width,
            height,
            ..
        } = *self;
        assert!(s % HALF == 0, "supersampling must be a multiple of 2");
        let block = s / HALF;
        let hi_w = (width + 2 * apron) * s;
        let hw = (width + 2 * apron) * HALF;
        let hh = (height + 2 * apron) * HALF;
        let mut half = vec![0.0f64; hw * hh];
        let norm = 1.0 / (block * block) as f64;
        for hy in 0..hh {
            for hx in 0..hw {
                let mut acc = 0.0f64;
                for by in 0..block {
                    let row = (hy * block + by) * hi_w + hx * block;
                    for v in &self.samples[row..row + block] {
                        acc += f64::from(*v);
                    }
                }
                half[hy * hw + hx] = acc * norm;
            }
        }

        let mut acc = vec![0.0f64; width * height];
        let mut row_weights = Vec::new();
        let src_area = HALF_PX * HALF_PX;
        let w_f = width as f64;
        let h_f = height as f64;

        for hy in 0..hh {
            let uy = (hy as f64 + 0.5) * HALF_PX - apron as f64;
            for hx in 0..hw {
                let ux = (hx as f64 + 0.5) * HALF_PX - apron as f64;
                let Some(lens) = sigma_at(ux, uy) else {
                    continue;
                };
                let sxx = lens[0].max(1e-6) + KERN_BOX_COMPENSATION;
                let sxy = lens[1];
                let syy = lens[2].max(1e-6) + KERN_BOX_COMPENSATION;
                let det = sxx * syy - sxy * sxy;
                if det <= 1e-12 {
                    continue;
                }
                let inv_det = 1.0 / det;
                let a = syy * inv_det;
                let b = -sxy * inv_det;
                let c = sxx * inv_det;
                let rx_f = 4.5 * sxx.sqrt();
                let ry_f = 4.5 * syy.sqrt();
                if ux + rx_f < 0.0 || ux - rx_f > w_f || uy + ry_f < 0.0 || uy - ry_f > h_f {
                    continue;
                }

                let x_lo = (ux - rx_f - 0.5).floor() as i64;
                let x_hi = (ux + rx_f - 0.5).ceil() as i64;
                let y_lo = (uy - ry_f - 0.5).floor() as i64;
                let y_hi = (uy + ry_f - 0.5).ceil() as i64;
                let cols = (x_hi - x_lo + 1) as usize;
                let rows = (y_hi - y_lo + 1) as usize;
                row_weights.clear();
                row_weights.resize(cols * rows, 0.0);

                let step_x = (-a).exp();
                let mut total_w = 0.0f64;
                let dx0 = (x_lo as f64 + 0.5) - ux;
                for r in 0..rows {
                    let dy = ((y_lo + r as i64) as f64 + 0.5) - uy;
                    let quad0 = a * dx0 * dx0 + 2.0 * b * dx0 * dy + c * dy * dy;
                    let mut val = (-0.5 * quad0).exp();
                    let mut ratio = (-a * (dx0 + 0.5) - b * dy).exp();
                    let row_slice = &mut row_weights[r * cols..(r + 1) * cols];
                    for cell in row_slice {
                        *cell = val;
                        total_w += val;
                        val *= ratio;
                        ratio *= step_x;
                    }
                }
                if total_w <= 0.0 {
                    continue;
                }
                let weight_scale = (src_area * half[hy * hw + hx]) / total_w;
                let vx0 = x_lo.max(0);
                let vx1 = x_hi.min(width as i64 - 1);
                let vy0 = y_lo.max(0);
                let vy1 = y_hi.min(height as i64 - 1);
                if vx0 > vx1 || vy0 > vy1 {
                    continue;
                }
                for iy in vy0..=vy1 {
                    let r = (iy - y_lo) as usize;
                    let src_row = &row_weights[r * cols..(r + 1) * cols];
                    let dst_row = &mut acc[iy as usize * width..];
                    for ix in vx0..=vx1 {
                        let k = (ix - x_lo) as usize;
                        dst_row[ix as usize] += weight_scale * src_row[k];
                    }
                }
            }
        }

        Irradiance {
            values: acc,
            width,
            height,
        }
    }

    /// Physical spatially-varying scatter integration along an object plane in
    /// front of a [`ThinLensDefocusModel`] (Scheimpflug tilted-sensor geometry).
    pub fn integrate_scheimpflug(
        &self,
        defocus: &ThinLensDefocusModel,
        plane_normal_cam: [f64; 3],
        plane_distance_mm: f64,
    ) -> Irradiance {
        let lens_only = defocus.with_include_pixel_box(false);
        self.integrate_scatter_defocus(|ux, uy| {
            let (pred, _) = lens_only.predict_plane_blur_gradient(
                ux - 0.5,
                uy - 0.5,
                plane_normal_cam,
                plane_distance_mm,
            )?;
            Some([
                f64::from(pred.kernel.xx),
                f64::from(pred.kernel.xy),
                f64::from(pred.kernel.yy),
            ])
        })
    }
}

fn convolve_separable_box_quarter(
    box_q: &[f64],
    qw: usize,
    qh: usize,
    apron: usize,
    size: [usize; 2],
    wx: &[(i64, f64)],
    wy: &[(i64, f64)],
) -> Irradiance {
    let [width, height] = size;
    let qwm = qw as i64 - 1;
    let qhm = qh as i64 - 1;
    let mut horiz = vec![0.0f64; width * qh];
    for qy in 0..qh {
        let row = &box_q[qy * qw..(qy + 1) * qw];
        for x in 0..width {
            let cx = ((x + apron) * QUARTER) as i64;
            let mut acc = 0.0f64;
            for &(dx, w) in wx {
                acc += w * row[(cx + dx).clamp(0, qwm) as usize];
            }
            horiz[qy * width + x] = acc;
        }
    }
    let mut values = vec![0.0f64; width * height];
    for y in 0..height {
        let cy = ((y + apron) * QUARTER) as i64;
        for x in 0..width {
            let mut acc = 0.0f64;
            for &(dy, w) in wy {
                let iy = (cy + dy).clamp(0, qhm) as usize;
                acc += w * horiz[iy * width + x];
            }
            values[y * width + x] = acc;
        }
    }
    Irradiance {
        values,
        width,
        height,
    }
}

fn disk_taps(radius: f64) -> Vec<(i64, i64, f64)> {
    let reach = (radius / QUARTER_PX).ceil() as i64 + 1;
    let sub = 16;
    let mut taps = Vec::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let mut inside = 0usize;
            for sy in 0..sub {
                for sx in 0..sub {
                    let x = (dx as f64 - 0.5 + (sx as f64 + 0.5) / sub as f64) * QUARTER_PX;
                    let y = (dy as f64 - 0.5 + (sy as f64 + 0.5) / sub as f64) * QUARTER_PX;
                    if x * x + y * y < radius * radius {
                        inside += 1;
                    }
                }
            }
            if inside > 0 {
                taps.push((dx, dy, inside as f64));
            }
        }
    }
    taps
}

impl Camera {
    /// Camera and image geometry for a tilted Scheimpflug sensor described by
    /// `defocus`. Returns `(camera, image_size, synced_defocus, plane_normal_cam, plane_distance_mm)`.
    pub fn looking_at_scheimpflug(
        width_mm: f64,
        height_mm: f64,
        pose: &Pose,
        defocus: &ThinLensDefocusModel,
        margin_px: f64,
    ) -> (Self, [usize; 2], ThinLensDefocusModel, [f64; 3], f64) {
        let px_per_mm = pose.cell_px / pose.square_mm;
        let v0 = defocus
            .axial_image_distance_mm()
            .expect("Scheimpflug defocus model requires focus_distance_mm > focal_length_mm");
        let (e1, e2, ns) = defocus.sensor_basis();
        let p = defocus.pixel_pitch_mm;
        let distance_mm = v0 / (p * px_per_mm);
        let (st, ct) = pose.tilt_deg.to_radians().sin_cos();
        let (sr, cr) = pose.rotation_deg.to_radians().sin_cos();
        let rx = Matrix3::new(1.0, 0.0, 0.0, 0.0, ct, -st, 0.0, st, ct);
        let rz = Matrix3::new(cr, -sr, 0.0, sr, cr, 0.0, 0.0, 0.0, 1.0);
        let r = rz * rx;
        let rs = Matrix3::from_columns(&[
            Vector3::new(e1[0], e1[1], e1[2]),
            Vector3::new(e2[0], e2[1], e2[2]),
            Vector3::new(ns[0], ns[1], ns[2]),
        ]);
        let g1 = e1[2];
        let g2 = e2[2];
        let nz = ns[2];
        let ks = |cx: f64, cy: f64| {
            Matrix3::new(
                v0 * nz / p,
                0.0,
                cx - v0 * g1 / p,
                0.0,
                v0 * nz / p,
                cy - v0 * g2 / p,
                0.0,
                0.0,
                1.0,
            )
        };
        let shift = Matrix3::new(
            1.0,
            0.0,
            -0.5 * width_mm,
            0.0,
            1.0,
            -0.5 * height_mm,
            0.0,
            0.0,
            1.0,
        );
        let rt = Matrix3::from_columns(&[
            r.column(0).into_owned(),
            r.column(1).into_owned(),
            Vector3::new(0.0, 0.0, distance_mm),
        ]);
        let h0 = ks(0.0, 0.0) * rs.transpose() * rt * shift;
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for (x, y) in [
            (0.0, 0.0),
            (width_mm, 0.0),
            (width_mm, height_mm),
            (0.0, height_mm),
        ] {
            let pt = apply(&h0, Point2::new(x, y));
            lo = [lo[0].min(pt.x), lo[1].min(pt.y)];
            hi = [hi[0].max(pt.x), hi[1].max(pt.y)];
        }
        let size = [
            (hi[0] - lo[0] + 2.0 * margin_px - 1e-9).ceil() as usize,
            (hi[1] - lo[1] + 2.0 * margin_px - 1e-9).ceil() as usize,
        ];
        let cx = margin_px - lo[0] + 0.5 * (size[0] as f64 - (hi[0] - lo[0] + 2.0 * margin_px));
        let cy = margin_px - lo[1] + 0.5 * (size[1] as f64 - (hi[1] - lo[1] + 2.0 * margin_px));
        let h = ks(cx, cy) * rs.transpose() * rt * shift;
        let camera = Self {
            h,
            h_inv: h.try_inverse().expect("invertible Scheimpflug homography"),
            centre: Point2::new(cx, cy),
            focal_px: v0 / p,
            k1: pose.k1,
        };
        let mut synced = *defocus;
        synced.principal_point_px = [cx - 0.5, cy - 0.5];
        let n = r.column(2);
        (camera, size, synced, [n[0], n[1], n[2]], n[2] * distance_mm)
    }
}

impl Scene {
    /// Rasterize the board onto a physical e-ink display lattice (`spec`) with
    /// finite gray quantization, cover-glass MTF, and probit microcapsule bleed,
    /// then stratify-supersample through `camera`.
    pub fn supersample_eink(
        &self,
        camera: &Camera,
        size: [usize; 2],
        s: usize,
        max_blur_sigma_px: f64,
        spec: &EinkSurfaceSpec,
        jitter_seed: u64,
    ) -> Supersampled {
        let dpx_mm = 25.4 / spec.display_ppi.max(1.0);
        let dw = (self.width_mm / dpx_mm).ceil().max(1.0) as usize;
        let dh = (self.height_mm / dpx_mm).ceil().max(1.0) as usize;
        let mut disp = vec![1.0f64; dw * dh];
        const SUB: usize = 8;
        let sub_inv = 1.0 / (SUB * SUB) as f64;
        for iy in 0..dh {
            for ix in 0..dw {
                let mut acc = 0.0f64;
                for sy in 0..SUB {
                    let ym = (iy as f64 + (sy as f64 + 0.5) / SUB as f64) * dpx_mm;
                    for sx in 0..SUB {
                        let xm = (ix as f64 + (sx as f64 + 0.5) / SUB as f64) * dpx_mm;
                        acc += f64::from(self.reflectance(xm, ym));
                    }
                }
                let q = acc * sub_inv;
                disp[iy * dw + ix] = if spec.gray_levels >= 2 {
                    let levels = f64::from(spec.gray_levels - 1);
                    (q * levels).round() / levels
                } else {
                    q
                };
            }
        }

        if spec.surface_mtf_dpx > 0.0 {
            let kr = (4.0 * spec.surface_mtf_dpx).ceil() as isize;
            let klen = (2 * kr + 1) as usize;
            let mut k1d = Vec::with_capacity(klen);
            let inv_2s2 = 0.5 / (spec.surface_mtf_dpx * spec.surface_mtf_dpx);
            for d in -kr..=kr {
                k1d.push((-(d * d) as f64 * inv_2s2).exp());
            }
            let ksum: f64 = k1d.iter().sum();
            for v in &mut k1d {
                *v /= ksum;
            }
            let mut tmp = vec![1.0f64; dw * dh];
            for y in 0..dh {
                for x in 0..dw {
                    let mut sum = 0.0f64;
                    for (idx, &w) in k1d.iter().enumerate() {
                        let sx = x as isize + idx as isize - kr;
                        let val = if (0..dw as isize).contains(&sx) {
                            disp[y * dw + sx as usize]
                        } else {
                            1.0
                        };
                        sum += w * val;
                    }
                    tmp[y * dw + x] = sum;
                }
            }
            for y in 0..dh {
                for x in 0..dw {
                    let mut sum = 0.0f64;
                    for (idx, &w) in k1d.iter().enumerate() {
                        let sy = y as isize + idx as isize - kr;
                        let val = if (0..dh as isize).contains(&sy) {
                            tmp[sy as usize * dw + x]
                        } else {
                            1.0
                        };
                        sum += w * val;
                    }
                    disp[y * dw + x] = sum;
                }
            }
        }

        if spec.microcapsule_bleed_kappa != 0.0 {
            for r in &mut disp {
                let b = (1.0 - *r).clamp(1e-9, 1.0 - 1e-9);
                let z = inv_normal_cdf(b) + spec.microcapsule_bleed_kappa;
                let b_bleed = 0.5 * (1.0 + libm::erf(z / std::f64::consts::SQRT_2));
                *r = (1.0 - b_bleed).clamp(0.0, 1.0);
            }
        }

        let [width, height] = size;
        let s = s.max(1);
        let reach = reach_for(max_blur_sigma_px);
        let hi_w = (width + 2 * reach) * s;
        let hi_h = (height + 2 * reach) * s;
        let mut rng = SplitMix(jitter_seed);
        let mut samples = vec![0.0f32; hi_w * hi_h];
        for hy in 0..hi_h {
            for hx in 0..hi_w {
                let x = (hx as f64 + rng.unit()) / s as f64 - reach as f64;
                let y = (hy as f64 + rng.unit()) / s as f64 - reach as f64;
                let b = camera.back_project(Point2::new(x, y));
                let ix = (b.x / dpx_mm).floor() as isize;
                let iy = (b.y / dpx_mm).floor() as isize;
                samples[hy * hi_w + hx] =
                    if (0..dw as isize).contains(&ix) && (0..dh as isize).contains(&iy) {
                        disp[iy as usize * dw + ix as usize] as f32
                    } else {
                        1.0
                    };
            }
        }
        Supersampled {
            samples,
            s,
            reach,
            width,
            height,
        }
    }
}

fn inv_normal_cdf(p: f64) -> f64 {
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
    if p < P_LOW {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - P_LOW {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accuracy::render::{supersample, tests::flat_camera};
    use calib_targets::printable::{Fill, Primitive};

    #[test]
    fn rotated_anisotropic_gaussian_no_panic_and_exact_covariance() {
        let (camera, size) = flat_camera();
        let scene = Scene::new(&[], 100.0, 100.0, 10.0);
        let major = 3.0f64;
        let minor = 1.0f64;
        let ang = 35.0_f64.to_radians();
        let (s, c) = ang.sin_cos();
        let cov = [
            major * major * c * c + minor * minor * s * s,
            (major * major - minor * minor) * s * c,
            major * major * s * s + minor * minor * c * c,
        ];
        // Verify |xy| > min(xx, yy) which would panic the old 3-pass renderer.
        assert!(cov[1].abs() > cov[0].min(cov[2]));
        let psf = Psf::Gaussian { covariance: cov };
        let sampled = supersample(&scene, &camera, size, 8, psf.apron_sigma(), 1);
        let (irr, lens_cov) = sampled.integrate_psf(&psf);
        assert!((irr.at(50, 50) - 1.0).abs() < 1e-12);
        for k in 0..3 {
            assert!(
                (lens_cov[k] - cov[k]).abs() < 1e-3,
                "cov[{k}]: got {} want {}",
                lens_cov[k],
                cov[k]
            );
        }
    }

    #[test]
    fn moffat_and_airy_psf_normalize_and_are_symmetric() {
        let (camera, size) = flat_camera();
        let prims = vec![Primitive::Rect {
            x_mm: 0.0,
            y_mm: 0.0,
            width_mm: 50.0,
            height_mm: 100.0,
            fill: Fill::Black,
        }];
        let scene = Scene::new(&prims, 100.0, 100.0, 10.0);
        for psf in [
            Psf::Moffat {
                alpha: 1.4,
                beta: 3.5,
            },
            Psf::Airy {
                first_zero_radius: 1.8,
            },
        ] {
            let sampled = supersample(&scene, &camera, size, 8, psf.apron_sigma(), 11);
            let (irr, cov) = sampled.integrate_psf(&psf);
            assert!((irr.at(10, 50)).abs() < 1e-3);
            assert!((irr.at(90, 50) - 1.0).abs() < 1e-3);
            assert!(cov[1].abs() < 1e-9);
            assert!((cov[0] - cov[2]).abs() < 1e-9);
            let white: f64 = (10..90).map(|x| irr.at(x, 50)).sum();
            assert!(
                (white - 40.0).abs() < 0.05,
                "edge shifted under symmetric {psf:?}: white={white}"
            );
        }
    }

    #[test]
    fn scatter_defocus_shifts_1d_edge_by_minus_half_grad_sigma_sq() {
        // Black x < 50.0, white x >= 50.0 (edge at area coordinate x = 50.0).
        let (camera, size) = flat_camera();
        let prims = vec![Primitive::Rect {
            x_mm: 0.0,
            y_mm: 0.0,
            width_mm: 50.0,
            height_mm: 100.0,
            fill: Fill::Black,
        }];
        let scene = Scene::new(&prims, 100.0, 100.0, 10.0);
        let sigma0_sq = 3.2f64;
        let g = 0.08f64;
        let sampled = supersample(&scene, &camera, size, 8, 4.0, 17);
        let irr = sampled.integrate_scatter_defocus(|ux, _uy| {
            let var = (sigma0_sq + g * (ux - 50.0)).max(0.2);
            Some([var, 0.0, var])
        });
        // Find x where irradiance crosses 0.5 along row y = 50: the center of pixel x is
        // at area coordinate x + 0.5, so linear interpolation gives the 0.5 crossing.
        let mut crossing = None;
        for x in 45..55 {
            let v0 = irr.at(x, 50);
            let v1 = irr.at(x + 1, 50);
            if v0 <= 0.5 && v1 >= 0.5 {
                let frac = (0.5 - v0) / (v1 - v0);
                crossing = Some(x as f64 + 0.5 + frac);
                break;
            }
        }
        let shift = crossing.expect("crossing") - 50.0;
        let expected = -0.5 * g;
        assert!(
            (shift - expected).abs() < 0.02,
            "scatter edge shift={shift:.4} vs theoretical {expected:.4}"
        );
    }
}
