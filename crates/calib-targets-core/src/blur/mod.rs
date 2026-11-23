//! Per-corner blur and joint photometric refinement from the known printed pattern.
//!
//! Given the raw image, the ideal pattern printed around one corner (signed
//! polygons in board units) and a starting board → image homography, the
//! estimator fits the pixels of a patch around the corner with
//!
//! ```text
//! I_lin(p) = (1 + α d_x + β d_y) · (C + A · (1 − B_bleed(p))),
//! I_obs(p) = f_rad(I_lin(p)),
//! ```
//!
//! where `B_Σ` is the exact blackness of the pattern blurred by a Gaussian
//! of covariance `Σ` and integrated over the pixel's unit box, `B_bleed` is an
//! optional probit ink/microcapsule bleed shift `Φ(Φ⁻¹(B_Σ) + κ)`, `f_rad` is an
//! optional radiometric tone curve ([`RadiometricModel`]), and the pattern is
//! mapped through a free local homography `H`.
//!
//! [`fit_corner_photometric`] delivers the jointly refined corner position,
//! 2×2 heteroskedasticity-and-autocorrelation-consistent (HAC) position
//! covariance, refined local grid axes, optional surface-separated
//! [`DisplaySurfaceModel`] covariance, and (when resolved above the pixel
//! aperture) the [`CornerBlur`] kernel estimate. [`estimate_corner_blur`]
//! wraps it with default options and returns `Result<CornerBlur, BlurDecline>`.
//!
//! # What the numbers mean
//!
//! `Σ` ([`CornerBlur::kernel`]) is the covariance of the Gaussian-equivalent
//! *total* blur kernel in image pixels: lens, target surface (e.g. e-ink
//! microcapsules) and pixel aperture together, so a perfectly focused ideal
//! camera still has `Σ = I/12` (the unit box). When a [`DisplaySurfaceModel`]
//! is supplied, [`CornerPhotometricFit::camera_blur`] subtracts the projected
//! display covariance to isolate camera-only blur. [`CornerBlur::sigma_px`] is
//! `√(tr Σ / 2)`.
//!
//! # Spatially varying PSF (scatter bias correction)
//!
//! Under a smooth spatially varying scatter-form PSF (typical on tilted
//! sensors), a local constant-`Σ` fit shifts every X-junction by an exact
//! first-order divergence bias derived from `∂Σ/∂x` and `∂Σ/∂y`
//! ([`BlurGradient::scatter_corner_bias`]). Fitting a smooth [`BlurField`] across
//! a board's converged corners and supplying [`BlurField::gradient_at`] removes
//! that bias without extra per-pixel parameters.
//!
//! # Window and declines
//!
//! Fatal declines (`NoGeometry`, `PatternUnknown`, `TooFewPixels`,
//! `NotConverged`, `Singular`, `PolarityMismatch`, `TooBlurred`) fail
//! [`fit_corner_photometric`]. Blur-only gates ([`BlurDecline::SharperThanPixel`],
//! [`BlurDecline::Unresolved`]) leave `blur: None` with `blur_decline: Some(why)`
//! while still delivering the refined corner position, covariance, and axes.

mod defocus;
mod field;
mod fit;
mod frame;
mod gauss_polygon;
mod model;
mod patch_residual;
mod radiometric;
mod solve;
#[cfg(test)]
mod tests;
mod varpro;

use nalgebra::Point2;
use serde::{Deserialize, Serialize};

use crate::sym2::Sym2x2;
use crate::{AxisEstimate, GrayImageView, Homography, PositionCovariance};

pub use defocus::{DefocusPrediction, ThinLensDefocusModel};
pub use field::{BlurField, BlurGradient};
pub use patch_residual::{PatchEval, PatchEvalInput, PatchPixel, PatchResidual};
pub use radiometric::RadiometricModel;

/// Board cells the known-print region may name, either side of the corner:
/// the window reaches two cells, plus the blur margin.
const KNOWN_RADIUS: i32 = 4;
const KNOWN_SIDE: usize = 2 * KNOWN_RADIUS as usize;

/// The printed pattern around one corner, in *board units*: one unit is one
/// grid cell, the corner is at the origin, `x` and `y` follow the grid's `i`
/// and `j`. Cell `(a, b)` is the unit square `[a, a + 1] × [b, b + 1]`.
///
/// The print is a white background plus signed polygons: a polygon of
/// blackness `+1` paints black over white, `−1` white over black. The
/// polygons must describe the print completely inside every
/// [known cell](Self::with_known_cell); outside them the print is unknown and
/// the estimator keeps its pixels clear of it, as it does of every
/// [uncertain region](Self::with_uncertain_region).
///
/// `#[non_exhaustive]`: construct with [`BlurPattern::new`].
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct BlurPattern {
    polygons: Vec<(Vec<[f64; 2]>, f64)>,
    known: [bool; KNOWN_SIDE * KNOWN_SIDE],
    uncertain: Vec<Vec<[f64; 2]>>,
    polarity_known: bool,
}

impl BlurPattern {
    /// An empty pattern: no known cells, unknown polarity.
    pub fn new() -> Self {
        Self {
            polygons: Vec::new(),
            known: [false; KNOWN_SIDE * KNOWN_SIDE],
            uncertain: Vec::new(),
            polarity_known: false,
        }
    }

    /// Declare cell `(a, b)` known: the polygons describe its print. Cells
    /// more than four cells from the corner are ignored (the estimator never
    /// looks that far).
    #[must_use]
    pub fn with_known_cell(mut self, a: i32, b: i32) -> Self {
        if let Some(k) = Self::known_index(a, b) {
            self.known[k] = true;
        }
        self
    }

    /// Add a simple polygon (board units, either orientation) of signed
    /// `blackness` (`+1` black over white, `−1` white over black).
    #[must_use]
    pub fn with_polygon(mut self, vertices: Vec<Point2<f64>>, blackness: f64) -> Self {
        if vertices.len() >= 3 {
            self.polygons
                .push((vertices.iter().map(|p| [p.x, p.y]).collect(), blackness));
        }
        self
    }

    /// Mark a convex region (board units) whose print is uncertain — inside a
    /// known cell, but possibly different from the polygons (e.g. a dot that
    /// is printed only if the neighbouring cell is on the board).
    #[must_use]
    pub fn with_uncertain_region(mut self, vertices: Vec<Point2<f64>>) -> Self {
        if vertices.len() >= 3 {
            self.uncertain
                .push(vertices.iter().map(|p| [p.x, p.y]).collect());
        }
        self
    }

    /// Whether the polygons' black is known to be the darker ink. `false`
    /// (the default) lets the contrast take either sign — the parity of a
    /// plain chessboard is not observable from its labels.
    #[must_use]
    pub fn with_known_polarity(mut self, known: bool) -> Self {
        self.polarity_known = known;
        self
    }

    fn known_index(a: i32, b: i32) -> Option<usize> {
        let (ia, ib) = (a + KNOWN_RADIUS, b + KNOWN_RADIUS);
        let range = 0..KNOWN_SIDE as i32;
        (range.contains(&ia) && range.contains(&ib)).then(|| ib as usize * KNOWN_SIDE + ia as usize)
    }

    fn is_known(&self, a: i32, b: i32) -> bool {
        Self::known_index(a, b).is_some_and(|k| self.known[k])
    }
}

impl Default for BlurPattern {
    fn default() -> Self {
        Self::new()
    }
}

/// The covariance matrix `Σ` of the Gaussian-equivalent blur kernel, px².
///
/// `#[non_exhaustive]`: construct with [`BlurKernel::new`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct BlurKernel {
    /// `Σxx`, px².
    pub xx: f32,
    /// `Σxy`, px².
    pub xy: f32,
    /// `Σyy`, px².
    pub yy: f32,
}

impl BlurKernel {
    /// Construct from the three distinct entries (px²).
    pub fn new(xx: f32, xy: f32, yy: f32) -> Self {
        Self { xx, xy, yy }
    }

    /// Principal axes: `(σ_major, σ_minor, angle)` — the standard deviations
    /// along the major and minor axis (px) and the major axis's angle from
    /// the image `x` axis (radians, `y` down, in `(−π/2, π/2]`).
    pub fn axes(&self) -> (f32, f32, f32) {
        Sym2x2::from_f32(self.xx, self.xy, self.yy).principal_axes()
    }
}

/// How well the model fitted the patch.
///
/// `#[non_exhaustive]`: construct with [`BlurFitQuality::new`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlurFitQuality {
    /// Half-width of the board-space window, cells.
    pub window_cells: f32,
    /// Pixels fitted.
    pub pixels: usize,
    /// Fitted black-to-white contrast `|A|`, linearised gray levels.
    pub contrast: f32,
    /// Residual RMS, observed gray levels. Compare with the image noise; well
    /// above it, the pattern or the Gaussian kernel does not describe the patch.
    pub residual_rms: f32,
    /// Lag-1 autocorrelation of the residuals over neighbouring pixels
    /// (0 for white noise). Large values mean structured misfit; the
    /// standard errors account for it, the estimate's meaning may not.
    pub residual_autocorrelation: f32,
}

impl BlurFitQuality {
    /// Construct from every field.
    pub fn new(
        window_cells: f32,
        pixels: usize,
        contrast: f32,
        residual_rms: f32,
        residual_autocorrelation: f32,
    ) -> Self {
        Self {
            window_cells,
            pixels,
            contrast,
            residual_rms,
            residual_autocorrelation,
        }
    }
}

/// The blur at one corner.
///
/// `#[non_exhaustive]`: construct with [`CornerBlur::new`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CornerBlur {
    /// `√(tr Σ / 2)`, px: the kernel's mean standard deviation.
    pub sigma_px: f32,
    /// Standard error of [`Self::sigma_px`], px.
    pub sigma_std_error_px: f32,
    /// The kernel covariance `Σ`, px² (see the [module docs](self)).
    pub kernel: BlurKernel,
    /// Covariance of the estimate of `(Σxx, Σxy, Σyy)`, px⁴. A combination
    /// of `Σ` the patch constrains weakly (with one edge direction, the width
    /// along that edge) shows here as a large variance, not as a prior.
    pub kernel_covariance: [[f32; 3]; 3],
    /// Goodness of fit.
    pub fit: BlurFitQuality,
}

impl CornerBlur {
    /// Construct from the kernel and its estimate's covariance; `sigma_px`
    /// and its standard error are derived from them.
    pub fn new(kernel: BlurKernel, kernel_covariance: [[f32; 3]; 3], fit: BlurFitQuality) -> Self {
        let tr = 0.5 * (f64::from(kernel.xx) + f64::from(kernel.yy));
        let sigma = tr.max(0.0).sqrt();
        let g = 0.25 / sigma.max(f64::MIN_POSITIVE);
        let var = g
            * g
            * (f64::from(kernel_covariance[0][0])
                + 2.0 * f64::from(kernel_covariance[0][2])
                + f64::from(kernel_covariance[2][2]));
        Self {
            sigma_px: sigma as f32,
            sigma_std_error_px: var.max(0.0).sqrt() as f32,
            kernel,
            kernel_covariance,
            fit,
        }
    }
}

/// Why a corner got no blur estimate.
#[non_exhaustive]
#[derive(thiserror::Error, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlurDecline {
    /// No usable local board → image frame: too few labelled neighbours, or
    /// a degenerate homography.
    #[error("no local board frame (too few labelled neighbours)")]
    NoGeometry,
    /// The printed layout around the corner is not known (e.g. a marker
    /// board whose circles were not aligned).
    #[error("the printed pattern around the corner is not known")]
    PatternUnknown,
    /// Too few unclipped pixels with a known print around the corner.
    #[error("too few usable pixels")]
    TooFewPixels,
    /// The fit did not converge.
    #[error("the fit did not converge")]
    NotConverged,
    /// The fit's normal matrix is singular.
    #[error("the fit is singular")]
    Singular,
    /// The blur is too wide for the known print around the corner: the
    /// largest window cannot hold the corner's blurred edges, the corner
    /// lies within the blur's reach of print that is not known, or `σ`
    /// exceeds 0.3 of the local cell, the end of the range the estimator is
    /// validated on.
    #[error("blur too wide for the known print around the corner")]
    TooBlurred,
    /// The kernel is not resolved: its standard error is comparable to its
    /// size, so `σ` and its standard error are not meaningful.
    #[error("blur not resolved")]
    Unresolved,
    /// The blur is significantly below the unit-box pixel aperture: the
    /// image was sharpened, or the aperture is not the unit box.
    #[error("sharper than the pixel aperture")]
    SharperThanPixel,
    /// The fitted contrast contradicts the known print's polarity.
    #[error("contrast polarity contradicts the print")]
    PolarityMismatch,
}

/// Physical model of a discrete digital display surface (e.g. an e-ink
/// bookreader screen) used as the calibration target.
///
/// Accounts for display pixel pitch quantization (`1 / (12 N_d²)` variance in
/// board units), microcapsule/cover-glass modulation transfer (`mtf_sigma_display_px`),
/// finite bit-depth gray levels (e.g. 16 levels for 4-bit E Ink Carta), and
/// rasterization of curved features onto the integer display pixel lattice.
///
/// `#[non_exhaustive]`: construct with [`DisplaySurfaceModel::new`] or
/// [`DisplaySurfaceModel::pocketbook_eink`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DisplaySurfaceModel {
    /// Number of physical display pixels per board cell (`square_size_px` in
    /// `ScreenTargetMetadata`). Must be `> 0`.
    pub display_pixels_per_cell: f64,
    /// Intrinsic surface MTF Gaussian standard deviation in display pixels
    /// (typically `~0.5` for e-ink microcapsules + front cover glass).
    #[serde(default)]
    pub mtf_sigma_display_px: f64,
    /// Finite gray quantization levels of the display controller (`Some(16)`
    /// for 4-bit E Ink Carta, `None` for continuous tone).
    #[serde(default)]
    pub gray_levels: Option<u16>,
    /// Rasterize curved features (PuzzleBoard dots, MarkerBoard circles) onto
    /// the integer display pixel grid with `8×8` subpixel coverage matching
    /// `calib-targets-print`'s screen renderer.
    #[serde(default)]
    pub rasterize_curved_features: bool,
}

impl DisplaySurfaceModel {
    /// Generic discrete display model with `display_pixels_per_cell` pixels per
    /// checker square, zero surface MTF, continuous tone, and analytic curved
    /// geometry.
    pub fn new(display_pixels_per_cell: f64) -> Self {
        Self {
            display_pixels_per_cell: display_pixels_per_cell.max(1.0),
            mtf_sigma_display_px: 0.0,
            gray_levels: None,
            rasterize_curved_features: false,
        }
    }

    /// Preset tuned for an E Ink Carta / PocketBook e-reader displaying a
    /// native-resolution screen target (`square_size_px = display_pixels_per_cell`):
    /// `mtf_sigma_display_px = 0.5`, `gray_levels = Some(16)`, and
    /// `rasterize_curved_features = true`.
    pub fn pocketbook_eink(display_pixels_per_cell: f64) -> Self {
        Self {
            display_pixels_per_cell: display_pixels_per_cell.max(1.0),
            mtf_sigma_display_px: 0.5,
            gray_levels: Some(16),
            rasterize_curved_features: true,
        }
    }

    /// Override [`Self::mtf_sigma_display_px`] (display pixels).
    #[must_use]
    pub fn with_mtf_sigma_display_px(mut self, sigma_dpx: f64) -> Self {
        self.mtf_sigma_display_px = sigma_dpx.max(0.0);
        self
    }

    /// Override [`Self::gray_levels`].
    #[must_use]
    pub fn with_gray_levels(mut self, levels: Option<u16>) -> Self {
        self.gray_levels = levels;
        self
    }

    /// Override [`Self::rasterize_curved_features`].
    #[must_use]
    pub fn with_rasterize_curved_features(mut self, rasterize: bool) -> Self {
        self.rasterize_curved_features = rasterize;
        self
    }

    /// Project the isotropic display-plane blur variance `(1/12 + mtf²) / N_d²`
    /// through the local board-to-image Jacobian `[j_u, j_v]` (each column
    /// `[dx/dq, dy/dq]` in px/cell) to yield the foreshortened anisotropic
    /// display covariance `Σ_display` in image px².
    pub fn image_covariance(&self, jacobian_cols: [[f64; 2]; 2]) -> BlurKernel {
        let nd = self.display_pixels_per_cell.max(1.0);
        let mtf = self.mtf_sigma_display_px.max(0.0);
        let factor = (1.0 / 12.0 + mtf * mtf) / (nd * nd);
        let [ju, jv] = jacobian_cols;
        BlurKernel::new(
            (factor * (ju[0] * ju[0] + jv[0] * jv[0])) as f32,
            (factor * (ju[0] * ju[1] + jv[0] * jv[1])) as f32,
            (factor * (ju[1] * ju[1] + jv[1] * jv[1])) as f32,
        )
    }
}

fn default_true() -> bool {
    true
}

fn default_puzzle_dot_diameter_rel() -> f64 {
    1.0 / 3.0
}

/// Options controlling per-corner joint photometric + blur refinement
/// ([`fit_corner_photometric`]).
///
/// `#[non_exhaustive]`: construct with [`PhotometricFitOptions::default`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct PhotometricFitOptions {
    /// Camera radiometric tone-response model. Default: `Linear`.
    #[serde(default)]
    pub radiometric: RadiometricModel,
    /// Jointly estimate a probit ink/microcapsule bleed parameter `κ`.
    /// Default: `false`. Note: best used with a known tone curve (`Linear`,
    /// `Srgb`, or `FixedGamma`) rather than `FreeGamma` since both modulate
    /// black-vs-white area symmetry.
    #[serde(default)]
    pub fit_ink_bleed: bool,
    /// Optional physical target display surface (e.g. PocketBook e-ink).
    #[serde(default)]
    pub display: Option<DisplaySurfaceModel>,
    /// Known spatial blur gradient `(∂Σ/∂x, ∂Σ/∂y)` at this corner for
    /// analytic scatter-PSF corner bias removal.
    #[serde(default)]
    pub known_blur_gradient: Option<BlurGradient>,
    /// Eliminate linear contrast & black level `(A, C)` via Variable
    /// Projection (Kaufman's approximation) when `radiometric` is `Linear`. Default: `true`.
    #[serde(default = "default_true")]
    pub use_varpro: bool,
}

impl Default for PhotometricFitOptions {
    fn default() -> Self {
        Self {
            radiometric: RadiometricModel::Linear,
            fit_ink_bleed: false,
            display: None,
            known_blur_gradient: None,
            use_varpro: true,
        }
    }
}

impl PhotometricFitOptions {
    /// Set [`Self::radiometric`].
    #[must_use]
    pub fn with_radiometric(mut self, radiometric: RadiometricModel) -> Self {
        self.radiometric = radiometric;
        self
    }

    /// Enable or disable [`Self::fit_ink_bleed`].
    #[must_use]
    pub fn with_fit_ink_bleed(mut self, fit_ink_bleed: bool) -> Self {
        self.fit_ink_bleed = fit_ink_bleed;
        self
    }

    /// Attach a [`DisplaySurfaceModel`].
    #[must_use]
    pub fn with_display(mut self, display: DisplaySurfaceModel) -> Self {
        self.display = Some(display);
        self
    }

    /// Supply a known [`BlurGradient`] for scatter-PSF corner bias removal.
    #[must_use]
    pub fn with_known_blur_gradient(mut self, gradient: BlurGradient) -> Self {
        self.known_blur_gradient = Some(gradient);
        self
    }

    /// Enable or disable VarPro `(A, C)` elimination.
    #[must_use]
    pub fn with_use_varpro(mut self, use_varpro: bool) -> Self {
        self.use_varpro = use_varpro;
        self
    }
}

/// Options controlling board-wide photometric corner refinement.
///
/// `#[non_exhaustive]`: construct with [`BoardPhotometricOptions::default`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct BoardPhotometricOptions {
    /// Per-corner photometric fitting options.
    #[serde(default)]
    pub fit: PhotometricFitOptions,
    /// Printed PuzzleBoard edge-dot diameter as a fraction of the square side.
    /// Default: `1.0 / 3.0`.
    #[serde(default = "default_puzzle_dot_diameter_rel")]
    pub puzzle_dot_diameter_rel: f64,
    /// Fit a smooth 2-D [`BlurField`] across converged corners in Pass 1 and
    /// remove each corner's spatial scatter-PSF divergence bias in Pass 2 when
    /// `fit.known_blur_gradient` is `None`. Default: `true`.
    #[serde(default = "default_true")]
    pub two_pass_scatter_correction: bool,
}

impl Default for BoardPhotometricOptions {
    fn default() -> Self {
        Self {
            fit: PhotometricFitOptions::default(),
            puzzle_dot_diameter_rel: default_puzzle_dot_diameter_rel(),
            two_pass_scatter_correction: true,
        }
    }
}

impl BoardPhotometricOptions {
    /// Set [`Self::fit`].
    #[must_use]
    pub fn with_fit(mut self, fit: PhotometricFitOptions) -> Self {
        self.fit = fit;
        self
    }

    /// Override [`Self::puzzle_dot_diameter_rel`].
    #[must_use]
    pub fn with_puzzle_dot_diameter_rel(mut self, rel: f64) -> Self {
        self.puzzle_dot_diameter_rel = rel;
        self
    }

    /// Enable or disable [`Self::two_pass_scatter_correction`].
    #[must_use]
    pub fn with_two_pass_scatter_correction(mut self, enabled: bool) -> Self {
        self.two_pass_scatter_correction = enabled;
        self
    }
}

/// Full per-corner result from [`fit_corner_photometric`].
///
/// Delivers refined sub-pixel position, HAC position covariance, refined local
/// grid axes, goodness-of-fit diagnostics, and (when resolved above the pixel
/// aperture) [`Self::blur`]. If only blur validation declines
/// (`SharperThanPixel` or `Unresolved`), [`Self::blur`] is `None` with
/// [`Self::blur_decline`] set while geometry fields remain valid and usable.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CornerPhotometricFit {
    /// Refined sub-pixel corner position in image coordinates, px.
    pub position: Point2<f32>,
    /// 2×2 HAC covariance of [`Self::position`], px².
    pub position_covariance: PositionCovariance,
    /// Refined local board axes `(axis_u, axis_v)` with delta-method standard
    /// errors in radians.
    pub axes: [AxisEstimate; 2],
    /// Resolved blur estimate (`None` when [`Self::blur_decline`] is `Some`).
    pub blur: Option<CornerBlur>,
    /// Reason `blur` was declined on an otherwise converged geometric fit.
    pub blur_decline: Option<BlurDecline>,
    /// Projected display surface blur `Σ_display` in image px², when a
    /// [`DisplaySurfaceModel`] was provided.
    pub display_blur: Option<BlurKernel>,
    /// Patch residual & contrast diagnostics.
    pub fit: BlurFitQuality,
    /// Estimated encoding gamma when [`RadiometricModel::FreeGamma`] was used.
    pub gamma: Option<f32>,
    /// Estimated ink/microcapsule bleed in board cells (`κ · σ / scale`) when
    /// `fit_ink_bleed` was enabled.
    pub ink_bleed_cells: Option<f32>,
    /// Scatter-PSF corner correction `[Δx, Δy]` (px): how far a spatially
    /// varying blur moves this corner relative to a constant-`Σ` fit.
    /// **Already included in [`Self::position`]** — informational, never add it
    /// again. A fit run with
    /// [`known_blur_gradient`](PhotometricFitOptions::known_blur_gradient)
    /// reports the analytic first-order estimate (its forward model already
    /// contains the gradient); [`Self::apply_scatter_correction`] reports the
    /// shift it applied; the facade's two-pass `*_photometric` helpers report
    /// the measured pass-2 minus pass-1 shift. `None` when no gradient was used.
    pub scatter_correction_px: Option<[f32; 2]>,
}

impl CornerPhotometricFit {
    /// Convert `(blur, blur_decline)` into the standalone `Result<CornerBlur, BlurDecline>`
    /// returned by [`estimate_corner_blur`].
    pub fn blur_result(&self) -> Result<CornerBlur, BlurDecline> {
        match (self.blur, self.blur_decline) {
            (Some(b), _) => Ok(b),
            (None, Some(why)) => Err(why),
            (None, None) => Err(BlurDecline::Unresolved),
        }
    }

    /// Return `Σ_camera = Σ_total − Σ_display` (eigenvalues clamped `>= 0`) when
    /// `blur` succeeded, or `None` if `blur` declined. When no display model was
    /// set, returns `blur.kernel`.
    pub fn camera_blur(&self) -> Option<BlurKernel> {
        let total = self.blur?.kernel;
        let Some(disp) = self.display_blur else {
            return Some(total);
        };
        let mut s = Sym2x2::new(
            f64::from(total.xx) - f64::from(disp.xx),
            f64::from(total.xy) - f64::from(disp.xy),
            f64::from(total.yy) - f64::from(disp.yy),
        );
        let (lo, _) = s.eigenvalues();
        if lo < 0.0 {
            s.xx -= lo;
            s.yy -= lo;
        }
        Some(BlurKernel::new(s.xx as f32, s.xy as f32, s.yy as f32))
    }

    /// Apply a spatial [`BlurGradient`] scatter-PSF correction to this converged
    /// fit in place, shifting [`Self::position`] and updating
    /// [`Self::scatter_correction_px`].
    pub fn apply_scatter_correction(&mut self, gradient: &BlurGradient) {
        let Some(bias) = gradient
            .scatter_corner_bias(f64::from(self.axes[0].angle), f64::from(self.axes[1].angle))
        else {
            return;
        };
        let corr = [-bias[0] as f32, -bias[1] as f32];
        if corr[0].is_finite() && corr[1].is_finite() {
            let prev = self.scatter_correction_px.unwrap_or([0.0, 0.0]);
            self.position.x =
                (f64::from(self.position.x) - f64::from(prev[0]) + f64::from(corr[0])) as f32;
            self.position.y =
                (f64::from(self.position.y) - f64::from(prev[1]) + f64::from(corr[1])) as f32;
            self.scatter_correction_px = Some(corr);
        }
    }
}

/// Output of a board-wide photometric corner refinement pass.
///
/// `#[non_exhaustive]`: construct via [`Self::new`].
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoardPhotometricRefinement {
    /// Per-corner photometric fit results in input corner order.
    pub corners: Vec<Result<CornerPhotometricFit, BlurDecline>>,
    /// Smooth 2-D blur surface fitted across converged corners (`None` if no
    /// corner produced a resolved [`CornerBlur`]).
    pub blur_field: Option<BlurField>,
}

impl BoardPhotometricRefinement {
    /// Construct from per-corner results and optional board-wide blur field.
    pub fn new(
        corners: Vec<Result<CornerPhotometricFit, BlurDecline>>,
        blur_field: Option<BlurField>,
    ) -> Self {
        Self {
            corners,
            blur_field,
        }
    }
}

/// Estimate the blur at one corner with default linear options.
///
/// `image` is the raw image the corner was found in, at its original
/// resolution (resampling adds blur of its own); `pattern` the print around
/// the corner in board units; `board_to_image` a starting homography from
/// board units (corner at the origin) to image pixels (pixel centres at
/// integer coordinates), e.g. from the corner and its labelled neighbours.
pub fn estimate_corner_blur(
    image: &GrayImageView<'_>,
    pattern: &BlurPattern,
    board_to_image: &Homography<f64>,
) -> Result<CornerBlur, BlurDecline> {
    fit_corner_photometric(
        image,
        pattern,
        board_to_image,
        &PhotometricFitOptions::default(),
    )?
    .blur_result()
}

/// Jointly refine corner position, covariance, local board axes, photometry,
/// and blur covariance at one corner.
///
/// See module docs for the generative pixel model, decline separation, and
/// optional display/scatter extensions.
pub fn fit_corner_photometric(
    image: &GrayImageView<'_>,
    pattern: &BlurPattern,
    board_to_image: &Homography<f64>,
    options: &PhotometricFitOptions,
) -> Result<CornerPhotometricFit, BlurDecline> {
    fit::fit_photometric(image, pattern, board_to_image, options)
}
