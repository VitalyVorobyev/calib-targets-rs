//! Per-corner blur estimation from the known printed pattern.
//!
//! Given the raw image, the ideal pattern printed around one corner (signed
//! polygons in board units) and a starting board → image homography, the
//! estimator fits the pixels of a patch around the corner with
//!
//! ```text
//! I(p) = (1 + α d_x + β d_y) · (C + A · (1 − B_Σ(p))),
//! ```
//!
//! where `B_Σ` is the exact blackness of the pattern blurred by a Gaussian
//! of covariance `Σ` and integrated over the pixel's unit box, mapped through
//! a free local homography. `Σ`, the photometry (`A`, `C`, `α`, `β`) and the
//! eight homography entries are fitted together by Levenberg–Marquardt with
//! analytic derivatives. The Gaussian mass of each polygon is exact (Owen's
//! `T` function, see the book's references, `owen1956`); the pixel box is a
//! four-point Gaussian mixture matching its moments to fourth order while
//! the blur is under 1.3 px, a single Gaussian beyond. Homography directions
//! the window cannot see (on a plain chessboard at half a cell, the scales
//! along the two lines and the perspective) keep their values from the
//! caller's homography.
//!
//! # What the numbers mean
//!
//! `Σ` ([`CornerBlur::kernel`]) is the covariance of the Gaussian-equivalent
//! *total* blur kernel in image pixels: lens and pixel aperture together, so
//! a perfectly focused ideal camera still has `Σ = I/12` (the unit box). On a
//! real lens it is the Gaussian whose blur of *this* pattern best matches
//! the observed one: a width over the patch's spatial frequencies, not the
//! PSF's second moment, and for a heavy-tailed PSF not additive in
//! quadrature. [`CornerBlur::sigma_px`] is `√(tr Σ / 2)`.
//!
//! # Window and declines
//!
//! The patch is the board square `[−ρ, ρ]²` around the corner, starting at
//! `ρ = ½` cell. Only pixels three blur widths clear of any cell whose print
//! the caller does not know are used. The window grows, up to two cells,
//! while the blur cannot be told from the contrast on the patch (its variance
//! inflated more than fourfold by the photometric parameters) or while the
//! window is narrower than the corner's blurred edges (five standard
//! deviations). A confounded estimate is delivered with the larger standard
//! error that says so. The corner is declined
//! ([`BlurDecline::TooBlurred`]) when the window it ends with cannot hold the
//! fitted blur's edges, when the corner itself falls within the clearance of
//! unknown print (the patch would see one side of the pattern only), or when
//! `σ` exceeds 0.3 of the local cell (`√|det J|` of the homography at the
//! corner). That last limit is the end of the validated range, not a tuned
//! threshold: up to it the synthetic suite measures no bias beyond ±0.1 % and
//! honest standard errors; beyond it the estimates that survive are biased
//! low while their standard errors still claim precision. Because a
//! low-biased estimate can fall back inside the limit, the rule declines most
//! but not all corners whose true blur is beyond it (see the book's *Corner
//! blur* page for the measured remainder). The fit that may be delivered must also
//! beat every isotropic blur at the caller's geometry on the same pixels (a
//! model it contains); if one does better, the fit is in a worse basin and
//! restarts from it. A kernel estimate outside the physical domain
//! `Σ ⪰ I/12` is declined ([`BlurDecline::SharperThanPixel`]: a sharpened
//! image, an aperture smaller than the pixel, or a fit the patch does not
//! support), and so is one whose trace is not resolved from zero at two
//! standard errors ([`BlurDecline::Unresolved`]), where the linearised
//! `σ ± SE` stops meaning anything. A miss is acceptable; a `σ` biased beyond
//! its standard error is not.
//!
//! # Uncertainty
//!
//! Residuals of real images are spatially correlated (the misfit of the
//! model is smooth over the blur's width, and sensors and demosaicing
//! correlate neighbours), so the standard errors are
//! heteroskedasticity-and-autocorrelation consistent: a sandwich with a
//! Bartlett kernel over the pixel grid (`neweywest1987`), its width from the
//! residuals' own lag-1 autocorrelation (`andrews1991`). [`BlurFitQuality`]
//! reports the residual RMS and that autocorrelation so a consumer can see
//! misfit.

mod fit;
mod frame;
mod gauss_polygon;
mod model;
mod solve;

use nalgebra::Point2;
use serde::{Deserialize, Serialize};

use crate::{GrayImageView, Homography};

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
        crate::sym2::Sym2x2::from_f32(self.xx, self.xy, self.yy).principal_axes()
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
    /// Fitted black-to-white contrast `|A|`, gray levels.
    pub contrast: f32,
    /// Residual RMS, gray levels. Compare with the image noise; well above
    /// it, the pattern or the Gaussian kernel does not describe the patch.
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

/// Estimate the blur at one corner.
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
    fit::estimate(image, pattern, board_to_image)
}
