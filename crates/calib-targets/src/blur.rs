//! Per-corner blur of a detected board.
//!
//! The blur at each labelled corner — what a tilted-sensor (Scheimpflug)
//! focus model, a depth-from-defocus fit or an image-quality check observes —
//! measured from the raw pixels around the corner against the board's known
//! print. It is separate from detection: run a detector first, then pass its
//! result here with the same image.
//!
//! ```no_run
//! use calib_targets::{blur, detect};
//! use calib_targets::chessboard::ChessboardParams;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let img = image::open("board.png")?.to_luma8();
//! let detection =
//!     detect::detect_chessboard(&img, &detect::default_chess_config(), &ChessboardParams::default())?;
//! for (corner, blur) in detection.corners.iter().zip(blur::chessboard_blur(&img, &detection)) {
//!     match blur {
//!         Ok(b) => println!("{:?}: σ = {:.2} ± {:.2} px", corner.grid, b.sigma_px, b.sigma_std_error_px),
//!         Err(why) => println!("{:?}: no estimate ({why})", corner.grid),
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # What is measured
//!
//! At every corner, a patch of the image is fitted with the board's print
//! (squares, PuzzleBoard dots, marker circles) blurred by a Gaussian kernel
//! of free covariance `Σ`, integrated over each pixel's area, under a free
//! local homography, contrast and illumination gradient. [`CornerBlur`]
//! carries `Σ` ([`BlurKernel`], px², image axes), the covariance of its
//! estimate, `σ = √(tr Σ / 2)` with its standard error, and the fit's
//! residuals ([`BlurFitQuality`]).
//!
//! `Σ` is the *total* blur in image pixels: lens and pixel aperture together
//! (an ideal, perfectly focused camera has `Σ = I/12`, its unit-box pixel).
//! On a real lens it is the Gaussian that blurs this pattern most like the
//! lens does — a width over the patch's spatial frequencies, not the PSF's
//! second moment. For a PSF with a sharp core and heavy tails it is smaller
//! than the second moment and does not add in quadrature; it also depends on
//! the cell size and the pattern. Compare numbers measured on the same board.
//!
//! # Inputs
//!
//! Pass the image **at its original resolution**, and the detection in that
//! image's pixel coordinates. If you detected on an upscaled or otherwise
//! resampled copy, map the corner positions back first: resampling blurs
//! the image (a 2× bilinear upscale adds about 0.2 px² to `Σ` in original
//! pixels) and the noise of interpolated pixels is correlated.
//!
//! The print must be the standard one: plain squares (no inner-square
//! inset), PuzzleBoard dots of a third of a square, marker-board circles as
//! the [`MarkerBoardSpec`] states.
//!
//! # When a corner has no estimate
//!
//! Every corner gets `Ok` or a [`BlurDecline`] reason. A corner is declined
//! rather than given a number its standard error does not cover: when the
//! blur is too wide for the known print around the corner (comparable to
//! the cell, or reaching unlabelled print from a corner at the edge of the
//! detection), when it is sharper than the pixel aperture (an in-camera
//! sharpened image) or not resolved at all, when too few labelled neighbours
//! fix the local geometry, or when the fit fails. A blur the patch can only
//! partly separate from the contrast is delivered with the larger standard
//! error that says so. The estimate
//! uses only cells next to a labelled corner, so the corners at the edge of
//! a detection see a smaller window.
//!
//! # Cost
//!
//! A few milliseconds per corner, single-threaded; the corners are
//! independent, so a caller may split a detection across threads.

use std::collections::HashMap;

use nalgebra::Point2;

use crate::chessboard::ChessboardDetection;
use crate::core::blur::{estimate_corner_blur, BlurPattern};
use crate::core::{estimate_homography_rect_to_img, Homography};
use crate::detect::gray_view;
use crate::marker::{CirclePolarity, MarkerBoardDetection, MarkerBoardSpec};
use crate::puzzleboard::code_maps;
use crate::puzzleboard::PuzzleBoardDetection;

pub use crate::core::blur::{BlurDecline, BlurFitQuality, BlurKernel, CornerBlur};

/// Cells either side of a corner the pattern is built for (the estimator's
/// window reaches two cells, plus its blur margin).
const REACH: i32 = 4;

/// PuzzleBoard dot diameter, as a fraction of a square (the PuzzleBoard
/// paper's and the printable generator's default).
const PUZZLE_DOT_DIAMETER_REL: f64 = 1.0 / 3.0;

/// Largest sagitta, px, of a polygon edge approximating a printed circle.
/// The area-preserving polygon then deviates from the circle by at most
/// about half of it, a few hundredths of a pixel, below the localisation
/// noise of any edge.
const MAX_CIRCLE_SAGITTA_PX: f64 = 0.05;

/// The blur at every corner of a chessboard detection, in
/// `detection.corners` order. See the [module docs](self).
///
/// A plain chessboard's colour parity is not observable from its labels, so
/// the contrast's sign is fitted per corner.
pub fn chessboard_blur(
    image: &::image::GrayImage,
    detection: &ChessboardDetection,
) -> Vec<Result<CornerBlur, BlurDecline>> {
    let lattice = Lattice::new(detection.corners.iter().map(|c| (c.grid, c.position)));
    detection
        .corners
        .iter()
        .map(|c| {
            lattice.estimate(image, (c.grid.u, c.grid.v), |local| {
                // Arbitrary parity: the fit's contrast sign absorbs it.
                local.checker(|a, b| (a + b).rem_euclid(2) == 0);
            })
        })
        .collect()
}

/// The blur at every corner of a PuzzleBoard detection, in
/// `detection.corners` order. See the [module docs](self).
///
/// Assumes the standard dot diameter of a third of a square.
pub fn puzzleboard_blur(
    image: &::image::GrayImage,
    detection: &PuzzleBoardDetection,
) -> Vec<Result<CornerBlur, BlurDecline>> {
    let lattice = Lattice::new(detection.corners.iter().map(|c| (c.grid, c.position)));
    detection
        .corners
        .iter()
        .map(|c| {
            let (u, v) = (c.grid.u, c.grid.v);
            lattice.estimate(image, (u, v), |local| {
                // Master square (c, r) is black iff r + c is even; corner
                // (u, v) is its top-left corner.
                let black = |a: i32, b: i32| (u + a + v + b).rem_euclid(2) == 0;
                local.polarity_known = true;
                local.checker(black);
                local.puzzle_dots(u, v, black);
            })
        })
        .collect()
}

/// The blur at every corner of a marker-board detection, in
/// `detection.corners` order. See the [module docs](self).
///
/// `board` is the layout the detection was made with (its circles and their
/// diameter). Without a resolved alignment the circles' cells are unknown
/// and every corner is declined ([`BlurDecline::PatternUnknown`]).
pub fn marker_board_blur(
    image: &::image::GrayImage,
    board: &MarkerBoardSpec,
    detection: &MarkerBoardDetection,
) -> Vec<Result<CornerBlur, BlurDecline>> {
    if detection.alignment.is_none() {
        return vec![Err(BlurDecline::PatternUnknown); detection.corners.len()];
    }
    let lattice = Lattice::new(detection.corners.iter().map(|c| (c.grid, c.position)));
    let radius = 0.5 * f64::from(board.circle_diameter_rel);
    detection
        .corners
        .iter()
        .map(|c| {
            let (u, v) = (c.grid.u, c.grid.v);
            lattice.estimate(image, (u, v), |local| {
                // Inner corner (u, v) is the top-left corner of board square
                // (u + 1, v + 1); square (sx, sy) is black iff sx + sy is even.
                let black = |a: i32, b: i32| (u + v + a + b).rem_euclid(2) == 0;
                local.polarity_known = true;
                local.checker(black);
                for circle in &board.circles {
                    // The circle's square, relative to the corner.
                    let (a, b) = (circle.cell.i - u - 1, circle.cell.j - v - 1);
                    if !local.known(a, b) {
                        continue;
                    }
                    let fill = match circle.polarity {
                        CirclePolarity::Black => 1.0,
                        CirclePolarity::White => 0.0,
                        // A polarity this function does not know: the
                        // circle's print is uncertain.
                        _ => {
                            let disk = local.disk([f64::from(a) + 0.5, f64::from(b) + 0.5], radius);
                            local.uncertain.push(disk);
                            continue;
                        }
                    };
                    let under = if black(a, b) { 1.0 } else { 0.0 };
                    if fill != under {
                        let disk = local.disk([f64::from(a) + 0.5, f64::from(b) + 0.5], radius);
                        local.polygons.push((disk, fill - under));
                    }
                }
            })
        })
        .collect()
}

/// The labelled corners of a detection, by grid label.
struct Lattice {
    positions: HashMap<(i32, i32), Point2<f64>>,
}

impl Lattice {
    fn new(corners: impl Iterator<Item = (crate::Coord, Point2<f32>)>) -> Self {
        Self {
            positions: corners
                .map(|(g, p)| ((g.u, g.v), Point2::new(f64::from(p.x), f64::from(p.y))))
                .collect(),
        }
    }

    /// Board units (cells, corner at the origin) → image px, from the corner
    /// and its labelled 8-neighbours.
    fn local_frame(&self, (u, v): (i32, i32)) -> Option<Homography<f64>> {
        let mut src = Vec::with_capacity(9);
        let mut dst = Vec::with_capacity(9);
        for dv in -1..=1 {
            for du in -1..=1 {
                if let Some(p) = self.positions.get(&(u + du, v + dv)) {
                    src.push(Point2::new(f64::from(du), f64::from(dv)));
                    dst.push(*p);
                }
            }
        }
        if src.len() < 4 {
            return None;
        }
        estimate_homography_rect_to_img(&src, &dst)
    }

    /// Run the estimator at corner `(u, v)` with the pattern `build` lays
    /// down over the known cells.
    fn estimate(
        &self,
        image: &::image::GrayImage,
        (u, v): (i32, i32),
        build: impl FnOnce(&mut LocalPattern),
    ) -> Result<CornerBlur, BlurDecline> {
        let frame = self.local_frame((u, v)).ok_or(BlurDecline::NoGeometry)?;
        let mut local = LocalPattern::new(self, (u, v), &frame);
        build(&mut local);
        estimate_corner_blur(&gray_view(image), &local.finish(), &frame)
    }
}

/// The print around one corner under construction.
struct LocalPattern {
    /// Signed polygons, board units.
    polygons: Vec<(Vec<Point2<f64>>, f64)>,
    /// Convex regions whose print is uncertain.
    uncertain: Vec<Vec<Point2<f64>>>,
    /// Whether black is known to be the darker ink.
    polarity_known: bool,
    /// Known cells relative to the corner, indexed `[b + REACH][a + REACH]`.
    known: [[bool; 2 * REACH as usize]; 2 * REACH as usize],
    /// Local scale, px per cell, for the circle polygons.
    scale_px: f64,
}

impl LocalPattern {
    /// The cells next to a labelled corner are known: every labelled corner
    /// is an X-junction of four printed squares.
    fn new(lattice: &Lattice, (u, v): (i32, i32), frame: &Homography<f64>) -> Self {
        let mut known = [[false; 2 * REACH as usize]; 2 * REACH as usize];
        for b in -REACH..REACH {
            for a in -REACH..REACH {
                let labelled = (0..=1).any(|db| {
                    (0..=1).any(|da| lattice.positions.contains_key(&(u + a + da, v + b + db)))
                });
                if labelled {
                    known[(b + REACH) as usize][(a + REACH) as usize] = true;
                }
            }
        }
        // √|det J| of the board → image map at the corner.
        let m = frame.h / frame.h[(2, 2)];
        let c = [m[(0, 2)], m[(1, 2)]];
        let det = (m[(0, 0)] - c[0] * m[(2, 0)]) * (m[(1, 1)] - c[1] * m[(2, 1)])
            - (m[(0, 1)] - c[0] * m[(2, 1)]) * (m[(1, 0)] - c[1] * m[(2, 0)]);
        Self {
            polygons: Vec::new(),
            uncertain: Vec::new(),
            polarity_known: false,
            known,
            scale_px: det.abs().sqrt(),
        }
    }

    fn finish(self) -> BlurPattern {
        let mut pattern = BlurPattern::new().with_known_polarity(self.polarity_known);
        for b in -REACH..REACH {
            for a in -REACH..REACH {
                if self.known(a, b) {
                    pattern = pattern.with_known_cell(a, b);
                }
            }
        }
        for (vertices, blackness) in self.polygons {
            pattern = pattern.with_polygon(vertices, blackness);
        }
        for region in self.uncertain {
            pattern = pattern.with_uncertain_region(region);
        }
        pattern
    }

    fn known(&self, a: i32, b: i32) -> bool {
        (-REACH..REACH).contains(&a)
            && (-REACH..REACH).contains(&b)
            && self.known[(b + REACH) as usize][(a + REACH) as usize]
    }

    /// Lay down the black squares of the known cells; `black(a, b)` names
    /// the colour of cell `(a, b)`.
    fn checker(&mut self, black: impl Fn(i32, i32) -> bool) {
        for b in -REACH..REACH {
            for a in -REACH..REACH {
                if self.known(a, b) && black(a, b) {
                    let (x, y) = (f64::from(a), f64::from(b));
                    let square = vec![
                        Point2::new(x, y),
                        Point2::new(x + 1.0, y),
                        Point2::new(x + 1.0, y + 1.0),
                        Point2::new(x, y + 1.0),
                    ];
                    self.polygons.push((square, 1.0));
                }
            }
        }
    }

    /// The PuzzleBoard's edge dots around corner `(u, v)` (master
    /// coordinates). A dot on an edge between two known cells is printed —
    /// the edge is inside the board — and shows as the half-disk over the
    /// square of the other colour. A dot on the known region's border may lie
    /// on the board's outer edge, where nothing is printed: it is marked
    /// uncertain.
    fn puzzle_dots(&mut self, u: i32, v: i32, black: impl Fn(i32, i32) -> bool) {
        let radius = 0.5 * PUZZLE_DOT_DIAMETER_REL;
        for b in -REACH..REACH {
            for a in -REACH..REACH {
                if !self.known(a, b) {
                    continue;
                }
                // Each known cell owns its bottom and right edges; its top
                // and left edges are owned by the neighbour, so they are
                // handled here only when that neighbour is not known.
                let (c, r) = (u + a, v + b);
                let edges = [
                    // (centre, the other cell, master bit)
                    ([0.5, 1.0], (a, b + 1), code_maps::horizontal_edge_bit(r, c)),
                    ([1.0, 0.5], (a + 1, b), code_maps::vertical_edge_bit(r, c)),
                    (
                        [0.5, 0.0],
                        (a, b - 1),
                        code_maps::horizontal_edge_bit(r - 1, c),
                    ),
                    (
                        [0.0, 0.5],
                        (a - 1, b),
                        code_maps::vertical_edge_bit(r, c - 1),
                    ),
                ];
                for (k, (offset, other, bit)) in edges.into_iter().enumerate() {
                    let owner = k < 2;
                    let other_known = self.known(other.0, other.1);
                    if !owner && other_known {
                        continue;
                    }
                    let centre = [f64::from(a) + offset[0], f64::from(b) + offset[1]];
                    if !other_known {
                        let disk = self.disk(centre, radius);
                        self.uncertain.push(disk);
                        continue;
                    }
                    // bit 0 → black dot, visible over the white cell;
                    // bit 1 → white dot, visible over the black cell.
                    let here_black = black(a, b);
                    let (side, weight) = if bit == 0 {
                        (if here_black { other } else { (a, b) }, 1.0)
                    } else {
                        (if here_black { (a, b) } else { other }, -1.0)
                    };
                    // Direction from the edge into the visible cell.
                    let dir = [
                        f64::from(side.0) + 0.5 - centre[0],
                        f64::from(side.1) + 0.5 - centre[1],
                    ];
                    let half = self.half_disk(centre, radius, dir);
                    self.polygons.push((half, weight));
                }
            }
        }
    }

    /// Edges for a circle of `radius` cells: the sagitta stays below
    /// [`MAX_CIRCLE_SAGITTA_PX`] at the local scale.
    fn circle_segments(&self, radius: f64) -> usize {
        let r_px = (radius * self.scale_px).max(1e-3);
        let n = if r_px <= MAX_CIRCLE_SAGITTA_PX {
            8.0
        } else {
            std::f64::consts::PI / (1.0 - MAX_CIRCLE_SAGITTA_PX / r_px).acos()
        };
        (n.ceil() as usize).clamp(8, 256)
    }

    /// An area-preserving polygon for the disk at `centre`.
    fn disk(&self, centre: [f64; 2], radius: f64) -> Vec<Point2<f64>> {
        let n = self.circle_segments(radius);
        let step = std::f64::consts::TAU / n as f64;
        // Polygon area n/2 · R² sin(2π/n) = π r².
        let rr = radius * (std::f64::consts::TAU / (n as f64 * step.sin())).sqrt();
        (0..n)
            .map(|k| {
                let t = k as f64 * step;
                Point2::new(centre[0] + rr * t.cos(), centre[1] + rr * t.sin())
            })
            .collect()
    }

    /// An area-preserving polygon for the half of the disk at `centre` on
    /// the side `dir` of the diameter perpendicular to `dir`.
    fn half_disk(&self, centre: [f64; 2], radius: f64, dir: [f64; 2]) -> Vec<Point2<f64>> {
        let n = self.circle_segments(radius).div_ceil(2).max(4);
        let step = std::f64::consts::PI / n as f64;
        // Area n/2 · R² sin(π/n) = π r² / 2.
        let rr = radius * (std::f64::consts::PI / (n as f64 * step.sin())).sqrt();
        let base = dir[1].atan2(dir[0]) - std::f64::consts::FRAC_PI_2;
        (0..=n)
            .map(|k| {
                let t = base + k as f64 * step;
                Point2::new(centre[0] + rr * t.cos(), centre[1] + rr * t.sin())
            })
            .collect()
    }
}
