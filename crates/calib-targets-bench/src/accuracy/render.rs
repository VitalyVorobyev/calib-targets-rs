//! Ground-truth renderer for the accuracy suite.
//!
//! Renders a board's printable vector description
//! ([`calib_targets::printable::board_primitives`]) through a known camera and
//! returns the 8-bit image together with the exact image position of any
//! board point, so detected corners can be scored against the truth.
//!
//! # Image formation
//!
//! 1. **Geometry.** A board point `P` (mm from the board's top-left, x right,
//!    y down) maps to an *ideal* image point `H · P` — a pinhole camera looking
//!    at the board plane, see [`Camera`]. An optional radial lens term then
//!    moves it to the *observed* point. The lens is specified by its inverse,
//!    the map an undistortion would apply: `ideal = c + (d − c)(1 + k₁ r_d²)`
//!    with `r_d = |d − c| / f` — closed form where the renderer needs it (once
//!    per sample) and inverted by Newton iteration where the truth needs it
//!    (once per corner).
//! 2. **Area integration.** Each output pixel is the integral of the scene over
//!    its square, convolved with a Gaussian lens blur of σ_b px. The scene is
//!    sampled on a grid of `S × S` points per pixel, one point at a seeded
//!    random position inside each sub-cell (stratified sampling: the rounding
//!    error of a straight edge becomes zero-mean noise instead of a bias tied
//!    to the sub-grid), and the samples are weighted by the pixel's box
//!    kernel convolved with the blur, `K(t) = Φ((t + ½)/σ_b) − Φ((t − ½)/σ_b)`
//!    per axis.
//! 3. **Sensor.** Reflectance maps linearly to gray levels between
//!    [`RenderSpec::black`] and [`RenderSpec::white`]; seeded white Gaussian
//!    noise of σ_n gray levels is added and the result is rounded and clamped
//!    to 8 bits.
//!
//! # Coordinates
//!
//! The renderer works in *area* coordinates, where pixel `(x, y)` covers
//! `[x, x + 1) × [y, y + 1)`. Detector output puts pixel **centres** at integer
//! coordinates (a corner on the boundary between pixel columns 15 and 16 is
//! reported at `x = 15.5`), so [`Rendered::truth`] returns area coordinates
//! minus `½` — the convention every detection result uses.

use calib_targets::printable::{Fill, Primitive};
use nalgebra::{Matrix3, Point2, Vector3};

/// A board's primitives, indexed for point queries.
pub struct Scene {
    width_mm: f64,
    height_mm: f64,
    shapes: Vec<Shape>,
    /// Shape indices (paint order) overlapping each bin, row-major.
    bins: Vec<Vec<u32>>,
    bin_mm: f64,
    bins_x: usize,
    bins_y: usize,
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Rect {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        reflectance: f32,
    },
    Disk {
        cx: f64,
        cy: f64,
        r2: f64,
        reflectance: f32,
    },
    RectWithHole {
        outer: [f64; 4],
        hole: [f64; 4],
        reflectance: f32,
    },
}

fn reflectance(fill: &Fill) -> f32 {
    match fill {
        Fill::Black => 0.0,
        Fill::White => 1.0,
        // `board_primitives` paints only black and white; anything else is
        // a mid gray rather than a panic.
        _ => 0.5,
    }
}

impl Shape {
    /// `[x0, y0, x1, y1]` bounding box.
    fn bounds(&self) -> [f64; 4] {
        match *self {
            Self::Rect { x0, y0, x1, y1, .. } => [x0, y0, x1, y1],
            Self::Disk { cx, cy, r2, .. } => {
                let r = r2.sqrt();
                [cx - r, cy - r, cx + r, cy + r]
            }
            Self::RectWithHole { outer, .. } => outer,
        }
    }

    /// The shape's reflectance at `(x, y)`, or `None` outside it.
    fn sample(&self, x: f64, y: f64) -> Option<f32> {
        let inside = |b: &[f64; 4]| x >= b[0] && x < b[2] && y >= b[1] && y < b[3];
        match *self {
            Self::Rect {
                x0,
                y0,
                x1,
                y1,
                reflectance,
            } => inside(&[x0, y0, x1, y1]).then_some(reflectance),
            Self::Disk {
                cx,
                cy,
                r2,
                reflectance,
            } => ((x - cx).powi(2) + (y - cy).powi(2) < r2).then_some(reflectance),
            Self::RectWithHole {
                outer,
                hole,
                reflectance,
            } => {
                if !inside(&outer) {
                    None
                } else if inside(&hole) {
                    Some(1.0)
                } else {
                    Some(reflectance)
                }
            }
        }
    }
}

impl Scene {
    /// Index `primitives` (board space, mm) of a `width_mm × height_mm` board.
    /// `bin_mm` is the spatial-index cell, e.g. the square size.
    pub fn new(primitives: &[Primitive], width_mm: f64, height_mm: f64, bin_mm: f64) -> Self {
        let shapes: Vec<Shape> = primitives
            .iter()
            .filter_map(|p| match p {
                Primitive::Rect {
                    x_mm,
                    y_mm,
                    width_mm,
                    height_mm,
                    fill,
                } => Some(Shape::Rect {
                    x0: *x_mm,
                    y0: *y_mm,
                    x1: x_mm + width_mm,
                    y1: y_mm + height_mm,
                    reflectance: reflectance(fill),
                }),
                Primitive::Circle {
                    cx_mm,
                    cy_mm,
                    radius_mm,
                    fill,
                } => Some(Shape::Disk {
                    cx: *cx_mm,
                    cy: *cy_mm,
                    r2: radius_mm * radius_mm,
                    reflectance: reflectance(fill),
                }),
                Primitive::RectWithHole {
                    x_mm,
                    y_mm,
                    width_mm,
                    height_mm,
                    hole_x_mm,
                    hole_y_mm,
                    hole_width_mm,
                    hole_height_mm,
                    fill,
                } => Some(Shape::RectWithHole {
                    outer: [*x_mm, *y_mm, x_mm + width_mm, y_mm + height_mm],
                    hole: [
                        *hole_x_mm,
                        *hole_y_mm,
                        hole_x_mm + hole_width_mm,
                        hole_y_mm + hole_height_mm,
                    ],
                    reflectance: reflectance(fill),
                }),
                _ => None,
            })
            .collect();
        let bins_x = (width_mm / bin_mm).ceil().max(1.0) as usize;
        let bins_y = (height_mm / bin_mm).ceil().max(1.0) as usize;
        let mut bins = vec![Vec::new(); bins_x * bins_y];
        for (idx, shape) in shapes.iter().enumerate() {
            let [x0, y0, x1, y1] = shape.bounds();
            let bx0 = ((x0 / bin_mm).floor().max(0.0) as usize).min(bins_x - 1);
            let by0 = ((y0 / bin_mm).floor().max(0.0) as usize).min(bins_y - 1);
            let bx1 = ((x1 / bin_mm).floor().max(0.0) as usize).min(bins_x - 1);
            let by1 = ((y1 / bin_mm).floor().max(0.0) as usize).min(bins_y - 1);
            for by in by0..=by1 {
                for bx in bx0..=bx1 {
                    bins[by * bins_x + bx].push(idx as u32);
                }
            }
        }
        Self {
            width_mm,
            height_mm,
            shapes,
            bins,
            bin_mm,
            bins_x,
            bins_y,
        }
    }

    /// Reflectance at board point `(x, y)` mm: the last-painted shape covering
    /// it, white paper outside the board and where nothing is painted.
    pub fn reflectance(&self, x: f64, y: f64) -> f32 {
        if !(x >= 0.0 && y >= 0.0 && x < self.width_mm && y < self.height_mm) {
            return 1.0;
        }
        let bx = ((x / self.bin_mm) as usize).min(self.bins_x - 1);
        let by = ((y / self.bin_mm) as usize).min(self.bins_y - 1);
        self.bins[by * self.bins_x + bx]
            .iter()
            .rev()
            .find_map(|&i| self.shapes[i as usize].sample(x, y))
            .unwrap_or(1.0)
    }
}

/// How the board is seen: a pinhole camera, optionally with radial distortion.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    /// Board mm → ideal image point, in area coordinates.
    pub h: Matrix3<f64>,
    /// Inverse of [`Self::h`].
    pub h_inv: Matrix3<f64>,
    /// Distortion centre: the principal point (area coordinates).
    pub centre: Point2<f64>,
    /// Focal length, px — the radius unit of the distortion term.
    pub focal_px: f64,
    /// Radial coefficient of the undistortion map (0 = no distortion).
    pub k1: f64,
}

/// Parameters of a board pose, from which [`Camera::looking_at`] builds the
/// camera.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    /// Size of one board square at the board centre, px.
    pub cell_px: f64,
    /// Board square size, mm.
    pub square_mm: f64,
    /// In-plane rotation of the board, degrees.
    pub rotation_deg: f64,
    /// Rotation of the board plane about its horizontal axis, degrees
    /// (0 = fronto-parallel).
    pub tilt_deg: f64,
    /// Radial distortion coefficient (see [`Camera::k1`]).
    pub k1: f64,
}

impl Camera {
    /// A camera looking at the centre of a `width_mm × height_mm` board in
    /// `pose`, framed so the board plus `margin_px` of paper fits the image.
    /// Returns the camera and the image size.
    pub fn looking_at(
        width_mm: f64,
        height_mm: f64,
        pose: &Pose,
        margin_px: f64,
    ) -> (Self, [usize; 2]) {
        // A moderately wide lens: focal length ~1.2× the board's diagonal in
        // pixels at the nominal scale. The distance follows from the scale.
        let px_per_mm = pose.cell_px / pose.square_mm;
        let focal_px = 1.2 * (width_mm.hypot(height_mm) * px_per_mm);
        let distance_mm = focal_px / px_per_mm;
        let (st, ct) = pose.tilt_deg.to_radians().sin_cos();
        let (sr, cr) = pose.rotation_deg.to_radians().sin_cos();
        // R = Rz(rotation) · Rx(tilt), board centred on the optical axis.
        let rx = Matrix3::new(1.0, 0.0, 0.0, 0.0, ct, -st, 0.0, st, ct);
        let rz = Matrix3::new(cr, -sr, 0.0, sr, cr, 0.0, 0.0, 0.0, 1.0);
        let r = rz * rx;
        // Board mm (from top-left) → camera frame: shift to the centre, then
        // [r1 r2 t] with t = (0, 0, distance).
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
        let k =
            |cx: f64, cy: f64| Matrix3::new(focal_px, 0.0, cx, 0.0, focal_px, cy, 0.0, 0.0, 1.0);
        // Project the board outline with the principal point at the origin to
        // find the frame, then centre the board in it.
        let h0 = k(0.0, 0.0) * rt * shift;
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for (x, y) in [
            (0.0, 0.0),
            (width_mm, 0.0),
            (width_mm, height_mm),
            (0.0, height_mm),
        ] {
            let p = apply(&h0, Point2::new(x, y));
            lo = [lo[0].min(p.x), lo[1].min(p.y)];
            hi = [hi[0].max(p.x), hi[1].max(p.y)];
        }
        // The epsilon keeps an exactly integral extent from rounding up.
        let size = [
            (hi[0] - lo[0] + 2.0 * margin_px - 1e-9).ceil() as usize,
            (hi[1] - lo[1] + 2.0 * margin_px - 1e-9).ceil() as usize,
        ];
        let cx = margin_px - lo[0] + 0.5 * (size[0] as f64 - (hi[0] - lo[0] + 2.0 * margin_px));
        let cy = margin_px - lo[1] + 0.5 * (size[1] as f64 - (hi[1] - lo[1] + 2.0 * margin_px));
        let h = k(cx, cy) * rt * shift;
        let camera = Self {
            h,
            h_inv: h
                .try_inverse()
                .expect("a board in front of the camera maps invertibly"),
            // The lens is centred on the principal point (where the board
            // centre lands).
            centre: Point2::new(cx, cy),
            focal_px,
            k1: pose.k1,
        };
        (camera, size)
    }

    /// Board point (mm) → observed image point, area coordinates.
    pub fn project(&self, board_mm: Point2<f64>) -> Point2<f64> {
        self.distort(apply(&self.h, board_mm))
    }

    /// Observed image point (area coordinates) → board point, mm.
    pub fn back_project(&self, image: Point2<f64>) -> Point2<f64> {
        apply(&self.h_inv, self.undistort(image))
    }

    /// The lens's undistortion map, observed → ideal (closed form).
    fn undistort(&self, d: Point2<f64>) -> Point2<f64> {
        if self.k1 == 0.0 {
            return d;
        }
        let v = d - self.centre;
        let r2 = v.norm_squared() / (self.focal_px * self.focal_px);
        self.centre + v * (1.0 + self.k1 * r2)
    }

    /// Ideal → observed: invert `t (1 + k₁ t²) = s` for the normalised radius
    /// by Newton iteration (converges in a few steps for |k₁ s²| ≪ 1).
    fn distort(&self, ideal: Point2<f64>) -> Point2<f64> {
        if self.k1 == 0.0 {
            return ideal;
        }
        let v = ideal - self.centre;
        let s = v.norm() / self.focal_px;
        if s == 0.0 {
            return ideal;
        }
        let mut t = s;
        for _ in 0..50 {
            let f = t * (1.0 + self.k1 * t * t) - s;
            let df = 1.0 + 3.0 * self.k1 * t * t;
            let step = f / df;
            t -= step;
            if step.abs() < 1e-15 {
                break;
            }
        }
        self.centre + v * (t / s)
    }
}

fn apply(h: &Matrix3<f64>, p: Point2<f64>) -> Point2<f64> {
    let v = h * Vector3::new(p.x, p.y, 1.0);
    Point2::new(v.x / v.z, v.y / v.z)
}

/// How to form the image.
#[derive(Clone, Copy, Debug)]
pub struct RenderSpec {
    /// Sub-samples per pixel along each axis (`S`; `S²` per pixel).
    pub supersample: usize,
    /// Gaussian lens blur, px (0 = none: a plain box integral).
    pub blur_sigma_px: f64,
    /// White Gaussian sensor noise, gray levels (0 = none).
    pub noise_sigma: f64,
    /// Gray level of black ink.
    pub black: f64,
    /// Gray level of white paper.
    pub white: f64,
    /// Seed for the sub-sample jitter.
    pub jitter_seed: u64,
    /// Seed for the sensor noise.
    pub noise_seed: u64,
}

/// A rendered 8-bit image.
pub struct Rendered {
    /// Row-major gray pixels.
    pub pixels: Vec<u8>,
    /// Width, px.
    pub width: usize,
    /// Height, px.
    pub height: usize,
}

impl Rendered {
    /// Where board point `board_mm` truly lies, in the detector-output
    /// convention (pixel centres at integer coordinates).
    pub fn truth(camera: &Camera, board_mm: Point2<f64>) -> Point2<f64> {
        let p = camera.project(board_mm);
        Point2::new(p.x - 0.5, p.y - 0.5)
    }
}

/// The irradiance (reflectance, before the sensor) integrated over each
/// pixel and blurred: noise-free, unquantised, `[0, 1]`.
pub struct Irradiance {
    values: Vec<f64>,
    width: usize,
    height: usize,
}

/// The scene sampled `S × S` times per pixel over an image plus an apron,
/// ready to be integrated at any blur up to the one it was sampled for.
pub struct Supersampled {
    samples: Vec<f32>,
    s: usize,
    /// Apron width, px, on every side.
    reach: usize,
    width: usize,
    height: usize,
}

/// Kernel support, px, either side of a pixel centre, for blur `sigma`.
fn reach_for(sigma: f64) -> usize {
    (0.5 + 4.0 * sigma.max(0.0)).ceil() as usize
}

/// Sample the scene through `camera` for a `size[0] × size[1]` image, `s`
/// stratified samples per pixel per axis, with enough apron for a blur of up
/// to `max_blur_sigma_px`.
pub fn supersample(
    scene: &Scene,
    camera: &Camera,
    size: [usize; 2],
    s: usize,
    max_blur_sigma_px: f64,
    jitter_seed: u64,
) -> Supersampled {
    let [width, height] = size;
    let s = s.max(1);
    let reach = reach_for(max_blur_sigma_px);
    let hi_w = (width + 2 * reach) * s;
    let hi_h = (height + 2 * reach) * s;
    let mut rng = SplitMix(jitter_seed);
    let mut samples = vec![0.0f32; hi_w * hi_h];
    for hy in 0..hi_h {
        for hx in 0..hi_w {
            let jx = rng.unit();
            let jy = rng.unit();
            let x = (hx as f64 + jx) / s as f64 - reach as f64;
            let y = (hy as f64 + jy) / s as f64 - reach as f64;
            let b = camera.back_project(Point2::new(x, y));
            samples[hy * hi_w + hx] = scene.reflectance(b.x, b.y);
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

impl Supersampled {
    /// Integrate every output pixel with the pixel box convolved with a
    /// Gaussian blur of `blur_sigma_px` (no larger than the sampled apron
    /// allows).
    pub fn integrate(&self, blur_sigma_px: f64) -> Irradiance {
        let Self {
            s,
            reach: apron,
            width,
            height,
            ..
        } = *self;
        let reach = reach_for(blur_sigma_px);
        assert!(reach <= apron, "blur exceeds the sampled apron");
        let weights = kernel_weights(s, reach, blur_sigma_px.max(0.0));
        let taps = weights.len(); // = s · (2·reach + 1)
        let hi_w = (width + 2 * apron) * s;
        let hi_h = (height + 2 * apron) * s;
        // Output pixel x's window starts at hi-res column (x + apron − reach)·s.
        let skip = (apron - reach) * s;

        // Separable weighting: rows first (each hi-res row → `width`
        // outputs), then columns.
        let mut rows = vec![0.0f64; width * hi_h];
        for hy in 0..hi_h {
            let line = &self.samples[hy * hi_w..(hy + 1) * hi_w];
            for x in 0..width {
                let start = skip + x * s;
                let mut acc = 0.0f64;
                for (w, v) in weights.iter().zip(&line[start..start + taps]) {
                    acc += w * f64::from(*v);
                }
                rows[hy * width + x] = acc;
            }
        }
        let mut values = vec![0.0f64; width * height];
        for y in 0..height {
            let start = skip + y * s;
            for x in 0..width {
                let mut acc = 0.0f64;
                for (k, w) in weights.iter().enumerate() {
                    acc += w * rows[(start + k) * width + x];
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
}

/// [`supersample`] then [`Supersampled::integrate`] at the spec's blur.
pub fn integrate(
    scene: &Scene,
    camera: &Camera,
    size: [usize; 2],
    spec: &RenderSpec,
) -> Irradiance {
    supersample(
        scene,
        camera,
        size,
        spec.supersample,
        spec.blur_sigma_px,
        spec.jitter_seed,
    )
    .integrate(spec.blur_sigma_px)
}

impl Irradiance {
    /// Map to gray levels, add seeded sensor noise, round and clamp to 8 bits.
    pub fn expose(&self, spec: &RenderSpec) -> Rendered {
        let mut rng = SplitMix(spec.noise_seed);
        let pixels = self
            .values
            .iter()
            .map(|&rho| {
                let level = spec.black + rho * (spec.white - spec.black);
                let noisy = if spec.noise_sigma > 0.0 {
                    level + spec.noise_sigma * rng.gauss()
                } else {
                    level
                };
                noisy.round().clamp(0.0, 255.0) as u8
            })
            .collect();
        Rendered {
            pixels,
            width: self.width,
            height: self.height,
        }
    }

    /// The noise-free value of pixel `(x, y)` in `[0, 1]`.
    pub fn at(&self, x: usize, y: usize) -> f64 {
        self.values[y * self.width + x]
    }
}

/// Weights of the `s · (2 · reach + 1)` sub-samples in one axis of an output
/// pixel's window: the pixel's unit box convolved with a Gaussian of `sigma`
/// px, at each sub-cell centre, times the sub-cell width. They sum to 1.
fn kernel_weights(s: usize, reach: usize, sigma: f64) -> Vec<f64> {
    let n = s * (2 * reach + 1);
    let mut w: Vec<f64> = (0..n)
        .map(|j| {
            // Offset of sub-cell j's centre from the output pixel centre, px.
            let t = (j as f64 + 0.5) / s as f64 - reach as f64 - 0.5;
            if sigma > 0.0 {
                phi((t + 0.5) / sigma) - phi((t - 0.5) / sigma)
            } else if t.abs() < 0.5 {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    let total: f64 = w.iter().sum();
    for v in &mut w {
        *v /= total;
    }
    w
}

/// Standard normal CDF.
fn phi(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

/// `erf` to ~1e-14: the Maclaurin series for |x| < 2.5, the continued
/// fraction of `erfc` (Lentz) beyond. Abramowitz–Stegun 7.1.26 (1.5e-7) would
/// leave a visible asymmetry in the kernel at this precision.
fn erf(x: f64) -> f64 {
    let ax = x.abs();
    let r = if ax < 2.5 {
        // Maclaurin series: erf x = 2/√π Σ (−1)ⁿ x^{2n+1} / (n! (2n+1)).
        let mut term = ax;
        let mut sum = ax;
        let x2 = ax * ax;
        let mut n = 0.0f64;
        loop {
            n += 1.0;
            term *= -x2 / n;
            let add = term / (2.0 * n + 1.0);
            sum += add;
            if add.abs() <= 1e-17 * sum.abs() {
                break;
            }
        }
        sum * 2.0 / std::f64::consts::PI.sqrt()
    } else {
        // Lentz continued fraction for erfc.
        let x2 = ax * ax;
        let mut f = ax;
        let mut c = ax;
        let mut d = 0.0f64;
        for k in 1..300 {
            let a = 0.5 * k as f64;
            d = ax + a * d;
            d = if d == 0.0 { 1e-300 } else { 1.0 / d };
            c = ax + a / c;
            let delta = c * d;
            f *= delta;
            if (delta - 1.0).abs() < 1e-16 {
                break;
            }
        }
        1.0 - (-x2).exp() / (f * std::f64::consts::PI.sqrt())
    };
    r.copysign(x)
}

/// SplitMix64: a tiny, seedable, well-mixed generator (no RNG dependency).
struct SplitMix(u64);

impl SplitMix {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `(0, 1)`.
    fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    /// Standard normal (Box–Muller).
    fn gauss(&mut self) -> f64 {
        let (u1, u2) = (self.unit(), self.unit());
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn half_plane_scene(edge_mm: f64) -> Scene {
        // Black for x < edge, white beyond, over a 100 × 100 mm board.
        let prims = vec![Primitive::Rect {
            x_mm: 0.0,
            y_mm: 0.0,
            width_mm: edge_mm,
            height_mm: 100.0,
            fill: Fill::Black,
        }];
        Scene::new(&prims, 100.0, 100.0, 10.0)
    }

    /// 1 mm = 1 px, no rotation: an identity-scale camera.
    fn flat_camera() -> (Camera, [usize; 2]) {
        Camera::looking_at(
            100.0,
            100.0,
            &Pose {
                cell_px: 10.0,
                square_mm: 10.0,
                rotation_deg: 0.0,
                tilt_deg: 0.0,
                k1: 0.0,
            },
            0.0,
        )
    }

    #[test]
    fn erf_matches_reference_values() {
        for (x, want) in [
            (0.0, 0.0),
            (0.5, 0.520_499_877_813_046_5),
            (1.0, 0.842_700_792_949_714_9),
            (2.0, 0.995_322_265_018_952_7),
            (3.0, 0.999_977_909_503_001_4),
            (-1.0, -0.842_700_792_949_714_9),
        ] {
            assert!((erf(x) - want).abs() < 1e-13, "erf({x}) = {}", erf(x));
        }
    }

    #[test]
    fn kernel_weights_are_a_symmetric_partition_of_unity() {
        for sigma in [0.0, 0.5, 1.5] {
            let w = kernel_weights(8, 7, sigma);
            assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-12);
            for (a, b) in w.iter().zip(w.iter().rev()) {
                assert!((a - b).abs() < 1e-15);
            }
        }
    }

    /// A straight vertical edge at a known sub-pixel offset: each pixel of the
    /// column it crosses integrates to the covered fraction, with or without
    /// blur (the blur is symmetric, so it moves no edge).
    #[test]
    fn straight_edge_renders_its_covered_fraction() {
        let (camera, size) = flat_camera();
        assert_eq!(size, [100, 100]);
        for edge in [40.0, 40.25, 40.5, 40.8] {
            let scene = half_plane_scene(edge);
            for sigma in [0.0, 1.0] {
                let spec = RenderSpec {
                    supersample: 16,
                    blur_sigma_px: sigma,
                    noise_sigma: 0.0,
                    black: 0.0,
                    white: 1.0,
                    jitter_seed: 7,
                    noise_seed: 0,
                };
                let irr = integrate(&scene, &camera, size, &spec);
                // Sum across a row, away from the board's own border: the
                // white area right of the edge, px. The kernel is a
                // symmetric partition of unity, so blur conserves it.
                for y in [20, 50, 80] {
                    let white: f64 = (10..90).map(|x| irr.at(x, y)).sum();
                    assert!(
                        (white - (90.0 - edge)).abs() < 0.03,
                        "edge {edge} σ {sigma}: white area {white}"
                    );
                }
                if sigma == 0.0 {
                    let frac = irr.at(40, 50);
                    let want = 41.0 - edge.min(41.0);
                    assert!((frac - want).abs() < 0.03, "edge {edge}: {frac} vs {want}");
                }
            }
        }
    }

    /// The truth of a rendered checker corner matches the analytic projection,
    /// including the pixel-centre convention, the perspective and the lens.
    #[test]
    fn truth_is_the_exact_projection() {
        let pose = Pose {
            cell_px: 16.0,
            square_mm: 10.0,
            rotation_deg: 30.0,
            tilt_deg: 25.0,
            k1: 0.15,
        };
        let (camera, size) = Camera::looking_at(100.0, 80.0, &pose, 12.0);
        // Round trip board → image → board.
        for (x, y) in [(10.0, 10.0), (50.0, 40.0), (90.0, 70.0)] {
            let p = camera.project(Point2::new(x, y));
            let b = camera.back_project(p);
            assert!((b.x - x).abs() < 1e-9 && (b.y - y).abs() < 1e-9);
            assert!(p.x > 0.0 && p.y > 0.0 && p.x < size[0] as f64 && p.y < size[1] as f64);
        }
        // The board centre is the principal point's image (no rotation shifts
        // it), and the detector convention is half a pixel up-left of it.
        let c = camera.project(Point2::new(50.0, 40.0));
        let t = Rendered::truth(&camera, Point2::new(50.0, 40.0));
        assert!((c.x - 0.5 - t.x).abs() < 1e-12 && (c.y - 0.5 - t.y).abs() < 1e-12);
        // Scale at the centre is the nominal cell size.
        let step = camera.project(Point2::new(51.0, 40.0)) - c;
        assert!(
            (step.norm() * 10.0 - 16.0).abs() < 0.2,
            "{}",
            step.norm() * 10.0
        );
    }

    #[test]
    fn noise_is_seeded_and_has_the_requested_level() {
        let (camera, size) = flat_camera();
        let scene = Scene::new(&[], 100.0, 100.0, 10.0);
        let spec = RenderSpec {
            supersample: 1,
            blur_sigma_px: 0.0,
            noise_sigma: 4.0,
            black: 30.0,
            white: 130.0,
            jitter_seed: 1,
            noise_seed: 99,
        };
        let irr = integrate(&scene, &camera, size, &spec);
        let a = irr.expose(&spec);
        let b = irr.expose(&spec);
        assert_eq!(a.pixels, b.pixels);
        let n = a.pixels.len() as f64;
        let mean = a.pixels.iter().map(|&p| f64::from(p)).sum::<f64>() / n;
        let var = a
            .pixels
            .iter()
            .map(|&p| (f64::from(p) - mean).powi(2))
            .sum::<f64>()
            / n;
        assert!((mean - 130.0).abs() < 0.2, "mean {mean}");
        // Rounding adds 1/12 gray² of variance.
        assert!(
            (var.sqrt() - (16.0f64 + 1.0 / 12.0).sqrt()).abs() < 0.15,
            "σ {}",
            var.sqrt()
        );
    }
}
