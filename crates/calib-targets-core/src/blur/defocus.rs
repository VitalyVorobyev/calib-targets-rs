//! Physical Scheimpflug thin-lens defocus model with tilted sensor geometry.
//!
//! Relates camera optical parameters (focal length `f`, aperture `N`, axial
//! focus distance `Z_0`, pixel pitch `p`, principal point `(c_x, c_y)`), sensor
//! tip/tilt `(τ_x, τ_y)`, and scene depth `Z` (or a tilted object plane
//! `n_t · X = d_t`) to the predicted 2-D Gaussian blur covariance `Σ_total(u, v)`
//! in image pixel coordinates, together with exact analytic Jacobians and the
//! spatial gradient `(∂Σ/∂u, ∂Σ/∂v)` needed for scatter-PSF corner bias removal.

use serde::{Deserialize, Serialize};

use super::{BlurGradient, BlurKernel};

const PIXEL_BOX_VAR: f64 = 1.0 / 12.0;
const MIN_SENSOR_NZ: f64 = 0.1;

fn default_coc_to_sigma() -> f64 {
    0.25
}

fn default_true() -> bool {
    true
}

fn zero_kernel() -> BlurKernel {
    BlurKernel::new(0.0, 0.0, 0.0)
}

/// Physical thin-lens defocus model supporting a tilted image sensor
/// (Scheimpflug geometry).
///
/// The sensor plane passes through `(0, 0, v_0)` in camera coordinates (where
/// `v_0 = f · Z_0 / (Z_0 − f)` is the axial Gaussian image distance) with
/// orientation `R_s = R_y(τ_y) · R_x(τ_x)` and orthonormal basis
/// `(e_1, e_2, n_s)`:
///
/// ```text
/// e_1 = (cos τ_y,               0,          −sin τ_y)
/// e_2 = (sin τ_y sin τ_x,   cos τ_x,   cos τ_y sin τ_x)
/// n_s = (sin τ_y cos τ_x,  −sin τ_x,   cos τ_y cos τ_x)
/// ```
///
/// Pixel `(u, v)` has metric sensor displacement
/// `(ξ, η) = ((u − c_x) p, (v − c_y) p)` and local axial sensor coordinate
/// `v_s(u, v) = v_0 + e_{1,z} ξ + e_{2,z} η`. For a point at depth `Z > f`, the
/// signed circle of confusion in pixels is
/// `c_px = (v_s (1 − f/Z) − f) / (N p)`, and projecting the circular aperture
/// cone onto the tilted sensor yields the foreshortened blur covariance:
///
/// ```text
/// Σ_total(u, v) = Σ_base + Σ_box + (κ c_px)² · [1 + q_1², q_1 q_2, 1 + q_2²],
/// ```
///
/// where `q_k = e_{k,z} / n_{s,z}`.
///
/// `#[non_exhaustive]`: construct with [`ThinLensDefocusModel::new`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ThinLensDefocusModel {
    /// Lens focal length `f` (mm). Must be `> 0`.
    pub focal_length_mm: f64,
    /// Lens working f-number `N = f / D` (dimensionless). Must be `> 0`.
    pub f_number: f64,
    /// Axial focus distance `Z_0` at the principal point (mm). Must be `> f`.
    pub focus_distance_mm: f64,
    /// Square sensor pixel pitch `p` (mm/px). Must be `> 0`.
    pub pixel_pitch_mm: f64,
    /// Principal point `(c_x, c_y)` (image px).
    pub principal_point_px: [f64; 2],
    /// Sensor tip/tilt angles `[τ_x, τ_y]` (radians) defining `R_s = R_y(τ_y) R_x(τ_x)`.
    #[serde(default)]
    pub sensor_tilt_rad: [f64; 2],
    /// Gaussian-equivalent blur factor `κ` converting signed CoC diameter `c_px`
    /// to standard deviation `κ · c_px`. Default: `0.25` (4σ ≈ disk diameter).
    #[serde(default = "default_coc_to_sigma")]
    pub coc_to_sigma: f64,
    /// Fixed optical aberration / display base blur floor `[Σxx, Σxy, Σyy]` (px²).
    #[serde(default = "zero_kernel")]
    pub base_blur_px2: BlurKernel,
    /// Add unit-box pixel aperture variance `I/12` (px²) to the predicted
    /// covariance. Set `true` when matching detector [`BlurKernel`] estimates,
    /// or `false` for lens-only simulation kernels. Default: `true`.
    #[serde(default = "default_true")]
    pub include_pixel_box: bool,
}

impl ThinLensDefocusModel {
    /// Construct a thin-lens defocus model with zero sensor tilt, `coc_to_sigma = 0.25`,
    /// zero base blur, and `include_pixel_box = true`.
    pub fn new(
        focal_length_mm: f64,
        f_number: f64,
        focus_distance_mm: f64,
        pixel_pitch_mm: f64,
        principal_point_px: [f64; 2],
    ) -> Self {
        Self {
            focal_length_mm,
            f_number,
            focus_distance_mm,
            pixel_pitch_mm,
            principal_point_px,
            sensor_tilt_rad: [0.0, 0.0],
            coc_to_sigma: default_coc_to_sigma(),
            base_blur_px2: zero_kernel(),
            include_pixel_box: true,
        }
    }

    /// Set the sensor tip/tilt angles `[τ_x, τ_y]` in radians.
    #[must_use]
    pub fn with_sensor_tilt_rad(mut self, tilt_rad: [f64; 2]) -> Self {
        self.sensor_tilt_rad = tilt_rad;
        self
    }

    /// Set the CoC-diameter-to-Gaussian-σ scale factor `κ`.
    #[must_use]
    pub fn with_coc_to_sigma(mut self, kappa: f64) -> Self {
        self.coc_to_sigma = kappa;
        self
    }

    /// Set the baseline blur floor `Σ_base` (px²).
    #[must_use]
    pub fn with_base_blur(mut self, base: BlurKernel) -> Self {
        self.base_blur_px2 = base;
        self
    }

    /// Control whether `I/12` pixel-box variance is added to predictions.
    #[must_use]
    pub fn with_include_pixel_box(mut self, include: bool) -> Self {
        self.include_pixel_box = include;
        self
    }

    /// Orthonormal rotated sensor basis `(e_1, e_2, n_s)` in camera coordinates.
    pub fn sensor_basis(&self) -> ([f64; 3], [f64; 3], [f64; 3]) {
        let (sx, cx) = self.sensor_tilt_rad[0].sin_cos();
        let (sy, cy) = self.sensor_tilt_rad[1].sin_cos();
        let e1 = [cy, 0.0, -sy];
        let e2 = [sy * sx, cx, cy * sx];
        let n_s = [sy * cx, -sx, cy * cx];
        (e1, e2, n_s)
    }

    /// Nominal Gaussian image distance `v_0 = f · Z_0 / (Z_0 − f)` at the
    /// principal point (mm), returning `None` if parameters are invalid or
    /// the sensor normal is nearly grazing (`n_z <= 0.1`).
    pub fn axial_image_distance_mm(&self) -> Option<f64> {
        let valid_scalars = self.focal_length_mm.is_finite()
            && self.f_number.is_finite()
            && self.focus_distance_mm.is_finite()
            && self.pixel_pitch_mm.is_finite()
            && self.coc_to_sigma.is_finite()
            && self.principal_point_px[0].is_finite()
            && self.principal_point_px[1].is_finite()
            && self.sensor_tilt_rad[0].is_finite()
            && self.sensor_tilt_rad[1].is_finite()
            && self.base_blur_px2.xx.is_finite()
            && self.base_blur_px2.xy.is_finite()
            && self.base_blur_px2.yy.is_finite();
        if !valid_scalars {
            return None;
        }
        if self.focal_length_mm <= 0.0
            || self.f_number <= 0.0
            || self.focus_distance_mm <= self.focal_length_mm
            || self.pixel_pitch_mm <= 0.0
            || self.coc_to_sigma < 0.0
        {
            return None;
        }
        let (_, _, n_s) = self.sensor_basis();
        if n_s[2] <= MIN_SENSOR_NZ {
            return None;
        }
        let denom = self.focus_distance_mm - self.focal_length_mm;
        if denom <= 1e-12 {
            return None;
        }
        Some(self.focal_length_mm * self.focus_distance_mm / denom)
    }

    /// Axial sensor coordinate `v_s(u, v) = v_0 + e_{1,z} ξ + e_{2,z} η` (mm).
    pub fn sensor_distance_at_pixel(&self, u: f64, v: f64) -> Option<f64> {
        if !(u.is_finite() && v.is_finite()) {
            return None;
        }
        let v0 = self.axial_image_distance_mm()?;
        let (e1, e2, _) = self.sensor_basis();
        let xi = (u - self.principal_point_px[0]) * self.pixel_pitch_mm;
        let eta = (v - self.principal_point_px[1]) * self.pixel_pitch_mm;
        let vs = v0 + e1[2] * xi + e2[2] * eta;
        (vs > 0.0).then_some(vs)
    }

    /// Depth `Z_sharp(u, v)` lying in exact Scheimpflug focus at pixel `(u, v)`,
    /// returning `None` if `v_s <= f` (beyond hyperfocal / behind infinity).
    pub fn sharp_focus_depth_mm(&self, u: f64, v: f64) -> Option<f64> {
        let vs = self.sensor_distance_at_pixel(u, v)?;
        let denom = vs - self.focal_length_mm;
        (denom > 1e-12).then(|| self.focal_length_mm * vs / denom)
    }

    /// Predict the total blur covariance and exact analytic parameter Jacobians at
    /// image pixel `(u, v)` and object depth `depth_z_mm`.
    pub fn predict(&self, u: f64, v: f64, depth_z_mm: f64) -> Option<DefocusPrediction> {
        let state = self.eval_raw(u, v, depth_z_mm)?;
        Some(DefocusPrediction {
            kernel: BlurKernel::new(
                state.sigma[0] as f32,
                state.sigma[1] as f32,
                state.sigma[2] as f32,
            ),
            signed_coc_px: state.coc_px,
            d_sigma_dz: state.d_sigma_dz,
            d_sigma_dz0: state.d_sigma_dz0,
            d_sigma_dtau: state.d_sigma_dtau,
            d_sigma_dkappa: state.d_sigma_dkappa,
        })
    }

    /// Predict the spatial blur gradient `(∂Σ/∂u, ∂Σ/∂v)` (px²/px) given object
    /// depth `depth_z_mm` and depth derivatives `(dz_du, dz_dv)` (mm/px).
    pub fn predict_blur_gradient(
        &self,
        u: f64,
        v: f64,
        depth_z_mm: f64,
        dz_du: f64,
        dz_dv: f64,
    ) -> Option<BlurGradient> {
        if !(dz_du.is_finite() && dz_dv.is_finite()) {
            return None;
        }
        let state = self.eval_raw(u, v, depth_z_mm)?;
        let np = self.f_number * self.pixel_pitch_mm;
        let one_minus_f_over_z = 1.0 - self.focal_length_mm / depth_z_mm;
        let dc_dz = state.vs * self.focal_length_mm / (np * depth_z_mm * depth_z_mm);
        let d_c_du = state.g1 * one_minus_f_over_z / self.f_number + dc_dz * dz_du;
        let d_c_dv = state.g2 * one_minus_f_over_z / self.f_number + dc_dz * dz_dv;
        let scale = 2.0 * self.coc_to_sigma * state.w;
        let d_sigma_dx = scale_tri(scale * d_c_du, state.a_tilt);
        let d_sigma_dy = scale_tri(scale * d_c_dv, state.a_tilt);
        Some(BlurGradient::new(
            [
                d_sigma_dx[0] as f32,
                d_sigma_dx[1] as f32,
                d_sigma_dx[2] as f32,
            ],
            [
                d_sigma_dy[0] as f32,
                d_sigma_dy[1] as f32,
                d_sigma_dy[2] as f32,
            ],
        ))
    }

    /// Intersect pixel ray `(u, v)` on the tilted sensor with an object plane
    /// `n_t · X = d_t` in camera coordinates (`plane_normal_cam` is normalized
    /// internally; `plane_distance_mm` is the perpendicular distance from the
    /// optical centre along that unit normal) and return both the defocus
    /// prediction and exact total spatial [`BlurGradient`].
    pub fn predict_plane_blur_gradient(
        &self,
        u: f64,
        v: f64,
        plane_normal_cam: [f64; 3],
        plane_distance_mm: f64,
    ) -> Option<(DefocusPrediction, BlurGradient)> {
        if !(u.is_finite()
            && v.is_finite()
            && plane_distance_mm.is_finite()
            && plane_distance_mm > 0.0
            && plane_normal_cam.iter().all(|x| x.is_finite()))
        {
            return None;
        }
        let n_norm = (plane_normal_cam[0].powi(2)
            + plane_normal_cam[1].powi(2)
            + plane_normal_cam[2].powi(2))
        .sqrt();
        if n_norm <= 1e-12 {
            return None;
        }
        let nt = [
            plane_normal_cam[0] / n_norm,
            plane_normal_cam[1] / n_norm,
            plane_normal_cam[2] / n_norm,
        ];
        let v0 = self.axial_image_distance_mm()?;
        let (e1, e2, _) = self.sensor_basis();
        let p = self.pixel_pitch_mm;
        let xi = (u - self.principal_point_px[0]) * p;
        let eta = (v - self.principal_point_px[1]) * p;
        let p_s = [
            xi * e1[0] + eta * e2[0],
            xi * e1[1] + eta * e2[1],
            v0 + xi * e1[2] + eta * e2[2],
        ];
        let denom = nt[0] * p_s[0] + nt[1] * p_s[1] + nt[2] * p_s[2];
        if denom <= 1e-12 {
            return None;
        }
        let vs = p_s[2];
        let z = plane_distance_mm * vs / denom;
        let nt_dot_e1 = nt[0] * e1[0] + nt[1] * e1[1] + nt[2] * e1[2];
        let nt_dot_e2 = nt[0] * e2[0] + nt[1] * e2[1] + nt[2] * e2[2];
        let dz_du = p * plane_distance_mm * (e1[2] * denom - vs * nt_dot_e1) / (denom * denom);
        let dz_dv = p * plane_distance_mm * (e2[2] * denom - vs * nt_dot_e2) / (denom * denom);
        let pred = self.predict(u, v, z)?;
        let grad = self.predict_blur_gradient(u, v, z, dz_du, dz_dv)?;
        Some((pred, grad))
    }

    fn eval_raw(&self, u: f64, v: f64, depth_z_mm: f64) -> Option<RawDefocusState> {
        if !(u.is_finite() && v.is_finite() && depth_z_mm.is_finite())
            || depth_z_mm <= self.focal_length_mm
        {
            return None;
        }
        let v0 = self.axial_image_distance_mm()?;
        let (sx, cx) = self.sensor_tilt_rad[0].sin_cos();
        let (sy, cy) = self.sensor_tilt_rad[1].sin_cos();
        let g1 = -sy;
        let g2 = cy * sx;
        let nz = cy * cx;
        let xi = (u - self.principal_point_px[0]) * self.pixel_pitch_mm;
        let eta = (v - self.principal_point_px[1]) * self.pixel_pitch_mm;
        let vs = v0 + g1 * xi + g2 * eta;
        if vs <= 0.0 {
            return None;
        }
        let f = self.focal_length_mm;
        let np = self.f_number * self.pixel_pitch_mm;
        let one_minus_f_over_z = 1.0 - f / depth_z_mm;
        let coc_px = (vs * one_minus_f_over_z - f) / np;
        let w = self.coc_to_sigma * coc_px;
        let w2 = w * w;
        let q1 = g1 / nz;
        let q2 = g2 / nz;
        let a_tilt = [1.0 + q1 * q1, q1 * q2, 1.0 + q2 * q2];
        let box_var = if self.include_pixel_box {
            PIXEL_BOX_VAR
        } else {
            0.0
        };
        let sigma = [
            f64::from(self.base_blur_px2.xx) + box_var + w2 * a_tilt[0],
            f64::from(self.base_blur_px2.xy) + w2 * a_tilt[1],
            f64::from(self.base_blur_px2.yy) + box_var + w2 * a_tilt[2],
        ];
        let dc_dz = vs * f / (np * depth_z_mm * depth_z_mm);
        let d_sigma_dz = scale_tri(2.0 * self.coc_to_sigma * w * dc_dz, a_tilt);
        let z0_denom = self.focus_distance_mm - f;
        let dv0_dz0 = -(f * f) / (z0_denom * z0_denom);
        let dc_dz0 = one_minus_f_over_z * dv0_dz0 / np;
        let d_sigma_dz0 = scale_tri(2.0 * self.coc_to_sigma * w * dc_dz0, a_tilt);
        let d_sigma_dkappa = scale_tri(2.0 * self.coc_to_sigma * coc_px * coc_px, a_tilt);

        let tau_deriv = |dg1: f64, dg2: f64, dnz: f64| -> [f64; 3] {
            let dvs = xi * dg1 + eta * dg2;
            let dw = self.coc_to_sigma * one_minus_f_over_z * dvs / np;
            let dw2 = 2.0 * w * dw;
            let inv_nz2 = 1.0 / (nz * nz);
            let dq1 = (dg1 * nz - g1 * dnz) * inv_nz2;
            let dq2 = (dg2 * nz - g2 * dnz) * inv_nz2;
            let da = [2.0 * q1 * dq1, dq1 * q2 + q1 * dq2, 2.0 * q2 * dq2];
            [
                dw2 * a_tilt[0] + w2 * da[0],
                dw2 * a_tilt[1] + w2 * da[1],
                dw2 * a_tilt[2] + w2 * da[2],
            ]
        };
        // ∂(g1, g2, nz)/∂τ_x = (0, cos τ_y cos τ_x, −cos τ_y sin τ_x)
        let d_tau_x = tau_deriv(0.0, nz, -g2);
        // ∂(g1, g2, nz)/∂τ_y = (−cos τ_y, −sin τ_y sin τ_x, −sin τ_y cos τ_x)
        let d_tau_y = tau_deriv(-cy, -sy * sx, -sy * cx);

        Some(RawDefocusState {
            sigma,
            coc_px,
            vs,
            g1,
            g2,
            w,
            a_tilt,
            d_sigma_dz,
            d_sigma_dz0,
            d_sigma_dtau: [d_tau_x, d_tau_y],
            d_sigma_dkappa,
        })
    }
}

struct RawDefocusState {
    sigma: [f64; 3],
    coc_px: f64,
    vs: f64,
    g1: f64,
    g2: f64,
    w: f64,
    a_tilt: [f64; 3],
    d_sigma_dz: [f64; 3],
    d_sigma_dz0: [f64; 3],
    d_sigma_dtau: [[f64; 3]; 2],
    d_sigma_dkappa: [f64; 3],
}

fn scale_tri(s: f64, tri: [f64; 3]) -> [f64; 3] {
    [s * tri[0], s * tri[1], s * tri[2]]
}

/// Output of [`ThinLensDefocusModel::predict`].
///
/// `#[non_exhaustive]`: read fields directly.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DefocusPrediction {
    /// Predicted total Gaussian blur covariance `Σ_total` (px²).
    pub kernel: BlurKernel,
    /// Signed circle-of-confusion diameter (px); positive when the image plane
    /// lies behind nominal focus (`v_s > v_i`).
    pub signed_coc_px: f64,
    /// Exact derivative `∂Σ/∂Z` (px²/mm).
    pub d_sigma_dz: [f64; 3],
    /// Exact derivative `∂Σ/∂Z_0` (px²/mm).
    pub d_sigma_dz0: [f64; 3],
    /// Exact derivatives `[∂Σ/∂τ_x, ∂Σ/∂τ_y]` (px²/rad).
    pub d_sigma_dtau: [[f64; 3]; 2],
    /// Exact derivative `∂Σ/∂κ` (px² per unit `coc_to_sigma`).
    pub d_sigma_dkappa: [f64; 3],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_model() -> ThinLensDefocusModel {
        ThinLensDefocusModel::new(16.0, 2.8, 600.0, 0.00345, [640.0, 512.0])
            .with_sensor_tilt_rad([0.045, -0.032])
            .with_base_blur(BlurKernel::new(0.08, -0.01, 0.11))
    }

    #[test]
    fn untilted_model_matches_classical_thin_lens_coc() {
        let model = ThinLensDefocusModel::new(25.0, 4.0, 800.0, 0.005, [320.0, 240.0])
            .with_include_pixel_box(false);
        let depth = 950.0;
        let pred = model.predict(120.0, 400.0, depth).expect("valid");
        let f = model.focal_length_mm;
        let coc_classic_px = (f * f * (depth - model.focus_distance_mm))
            / (model.f_number * depth * (model.focus_distance_mm - f) * model.pixel_pitch_mm);
        assert!(
            (pred.signed_coc_px - coc_classic_px).abs() < 1e-12,
            "coc={} vs classic={}",
            pred.signed_coc_px,
            coc_classic_px
        );
        let expected_var = (model.coc_to_sigma * coc_classic_px).powi(2);
        assert!((f64::from(pred.kernel.xx) - expected_var).abs() < 1e-5);
        assert!((f64::from(pred.kernel.yy) - expected_var).abs() < 1e-5);
        assert!(pred.kernel.xy.abs() < 1e-12);
    }

    #[test]
    fn scheimpflug_plane_has_zero_signed_coc() {
        let model = sample_model();
        for (u, v) in [(200.0, 300.0), (640.0, 512.0), (1100.0, 820.0)] {
            let z_sharp = model.sharp_focus_depth_mm(u, v).expect("valid sharp depth");
            let pred = model.predict(u, v, z_sharp).expect("valid prediction");
            assert!(
                pred.signed_coc_px.abs() < 1e-12,
                "signed_coc={}",
                pred.signed_coc_px
            );
            let box_var = PIXEL_BOX_VAR;
            assert!((f64::from(pred.kernel.xx) - (0.08 + box_var)).abs() < 1e-6);
            assert!((f64::from(pred.kernel.xy) + 0.01).abs() < 1e-6);
            assert!((f64::from(pred.kernel.yy) - (0.11 + box_var)).abs() < 1e-6);
        }
    }

    #[test]
    fn analytic_jacobians_and_plane_gradient_match_finite_differences() {
        let model = sample_model();
        let (u, v, z) = (910.0, 290.0, 520.0);
        let raw = model.eval_raw(u, v, z).expect("valid");
        let h_z = 1e-3;
        let h_k = 1e-5;

        let check_tri =
            |name: &str, analytic: [f64; 3], plus: [f64; 3], minus: [f64; 3], step: f64| {
                for k in 0..3 {
                    let fd = (plus[k] - minus[k]) / (2.0 * step);
                    let scale = analytic[k].abs().max(fd.abs()).max(1e-6);
                    let rel = (analytic[k] - fd).abs() / scale;
                    assert!(
                        rel < 1e-7,
                        "{name}[{k}]: analytic={} vs fd={fd} (rel={rel:.2e})",
                        analytic[k]
                    );
                }
            };

        // d_sigma_dz
        let p_zp = model.eval_raw(u, v, z + h_z).unwrap().sigma;
        let p_zm = model.eval_raw(u, v, z - h_z).unwrap().sigma;
        check_tri("d_sigma_dz", raw.d_sigma_dz, p_zp, p_zm, h_z);

        // d_sigma_dz0
        let mut mp = model;
        mp.focus_distance_mm += h_z;
        let mut mm = model;
        mm.focus_distance_mm -= h_z;
        check_tri(
            "d_sigma_dz0",
            raw.d_sigma_dz0,
            mp.eval_raw(u, v, z).unwrap().sigma,
            mm.eval_raw(u, v, z).unwrap().sigma,
            h_z,
        );

        // d_sigma_dkappa
        let mut kp = model;
        kp.coc_to_sigma += h_k;
        let mut km = model;
        km.coc_to_sigma -= h_k;
        check_tri(
            "d_sigma_dkappa",
            raw.d_sigma_dkappa,
            kp.eval_raw(u, v, z).unwrap().sigma,
            km.eval_raw(u, v, z).unwrap().sigma,
            h_k,
        );

        // d_sigma_dtau
        let ht = 1e-7;
        for axis in 0..2 {
            let mut tp = model;
            tp.sensor_tilt_rad[axis] += ht;
            let mut tm = model;
            tm.sensor_tilt_rad[axis] -= ht;
            check_tri(
                &format!("d_sigma_dtau[{axis}]"),
                raw.d_sigma_dtau[axis],
                tp.eval_raw(u, v, z).unwrap().sigma,
                tm.eval_raw(u, v, z).unwrap().sigma,
                ht,
            );
        }

        // predict_plane_blur_gradient
        let n_plane = [0.22, -0.35, 0.91];
        let d_plane = 540.0;
        let (_, grad) = model
            .predict_plane_blur_gradient(u, v, n_plane, d_plane)
            .expect("valid plane grad");
        let hp = 1e-4;
        let sigma_at_plane = |pu: f64, pv: f64| -> [f64; 3] {
            let v0 = model.axial_image_distance_mm().unwrap();
            let (e1, e2, _) = model.sensor_basis();
            let norm = (n_plane[0].powi(2) + n_plane[1].powi(2) + n_plane[2].powi(2)).sqrt();
            let nt = [n_plane[0] / norm, n_plane[1] / norm, n_plane[2] / norm];
            let xi = (pu - model.principal_point_px[0]) * model.pixel_pitch_mm;
            let eta = (pv - model.principal_point_px[1]) * model.pixel_pitch_mm;
            let ps = [
                xi * e1[0] + eta * e2[0],
                xi * e1[1] + eta * e2[1],
                v0 + xi * e1[2] + eta * e2[2],
            ];
            let pz = d_plane * ps[2] / (nt[0] * ps[0] + nt[1] * ps[1] + nt[2] * ps[2]);
            model.eval_raw(pu, pv, pz).unwrap().sigma
        };
        let gx_plus = sigma_at_plane(u + hp, v);
        let gx_minus = sigma_at_plane(u - hp, v);
        let gy_plus = sigma_at_plane(u, v + hp);
        let gy_minus = sigma_at_plane(u, v - hp);
        for k in 0..3 {
            let fdx = (gx_plus[k] - gx_minus[k]) / (2.0 * hp);
            let fdy = (gy_plus[k] - gy_minus[k]) / (2.0 * hp);
            assert!(
                (f64::from(grad.d_sigma_dx[k]) - fdx).abs() < 1e-6,
                "grad x[{k}]: {} vs {fdx}",
                grad.d_sigma_dx[k]
            );
            assert!(
                (f64::from(grad.d_sigma_dy[k]) - fdy).abs() < 1e-6,
                "grad y[{k}]: {} vs {fdy}",
                grad.d_sigma_dy[k]
            );
        }
    }
}
