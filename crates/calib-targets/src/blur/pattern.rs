//! Local print pattern builders around one detected grid corner.
//!
//! Converts chessboard, ChArUco, PuzzleBoard, and MarkerBoard layouts into
//! cell-relative [`BlurPattern`] polygons, including optional discrete-display
//! rasterization of curved features (`8×8` subpixel coverage quantized to e-ink
//! gray levels) matching `calib-targets-print`'s screen renderer.

use std::collections::HashMap;

use nalgebra::Point2;

use crate::charuco::CharucoBoard;
use crate::core::blur::{BlurDecline, BlurPattern, DisplaySurfaceModel};
use crate::core::{estimate_homography_rect_to_img, Coord, Homography};
use crate::marker::{CirclePolarity, MarkerBoardSpec};
use crate::puzzleboard::code_maps;

/// Cells either side of a corner the pattern is built for (the estimator's
/// window reaches two cells, plus its blur margin).
pub(super) const REACH: i32 = 4;

/// Default PuzzleBoard dot diameter as a fraction of a square.
pub(super) const DEFAULT_PUZZLE_DOT_DIAMETER_REL: f64 = 1.0 / 3.0;

/// Largest sagitta (px) of a polygon edge approximating a printed circle.
const MAX_CIRCLE_SAGITTA_PX: f64 = 0.05;

/// Subpixels per axis for discrete-display coverage sampling (matches
/// `calib-targets-print/src/screen.rs`).
const SCREEN_SUB: i32 = 8;

/// Limit 8×8 curved-feature rasterization to dots/circles within 2.5 cells of
/// the corner (well outside the 2.0-cell maximum patch window, distant curved
/// features fall back to unrasterized polygons).
fn rasterized_display(
    centre: [f64; 2],
    display: Option<&DisplaySurfaceModel>,
) -> Option<&DisplaySurfaceModel> {
    display
        .filter(|d| d.rasterize_curved_features && centre[0].abs() <= 2.5 && centre[1].abs() <= 2.5)
}

/// The labelled corners of a detection, indexed by `(u, v)` grid coordinates.
pub(super) struct Lattice {
    positions: HashMap<(i32, i32), Point2<f64>>,
}

impl Lattice {
    pub(super) fn new(corners: impl Iterator<Item = (Coord, Point2<f32>)>) -> Self {
        Self {
            positions: corners
                .map(|(g, p)| ((g.u, g.v), Point2::new(f64::from(p.x), f64::from(p.y))))
                .collect(),
        }
    }

    /// Board units (cells, corner at the origin) -> image px, from the corner
    /// and its labelled 8-neighbours.
    pub(super) fn local_frame(&self, (u, v): (i32, i32)) -> Option<Homography<f64>> {
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

    /// Build the [`BlurPattern`] and local homography at corner `(u, v)`.
    pub(super) fn prepare(
        &self,
        (u, v): (i32, i32),
        build: impl FnOnce(&mut LocalPattern),
    ) -> Result<(BlurPattern, Homography<f64>), BlurDecline> {
        let frame = self.local_frame((u, v)).ok_or(BlurDecline::NoGeometry)?;
        let mut local = LocalPattern::new(self, (u, v), &frame);
        build(&mut local);
        Ok((local.finish(), frame))
    }
}

/// The print around one corner under construction.
pub(super) struct LocalPattern {
    polygons: Vec<(Vec<Point2<f64>>, f64)>,
    uncertain: Vec<Vec<Point2<f64>>>,
    pub(super) polarity_known: bool,
    known: [[bool; 2 * REACH as usize]; 2 * REACH as usize],
    scale_px: f64,
}

impl LocalPattern {
    /// Every square adjoining a labelled corner is marked known.
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

    pub(super) fn known(&self, a: i32, b: i32) -> bool {
        (-REACH..REACH).contains(&a)
            && (-REACH..REACH).contains(&b)
            && self.known[(b + REACH) as usize][(a + REACH) as usize]
    }

    fn unmark_outside_board(&mut self, u: i32, v: i32, cols: i32, rows: i32) {
        for b in -REACH..REACH {
            for a in -REACH..REACH {
                let sx = u + a;
                let sy = v + b;
                if sx < 0 || sy < 0 || sx >= cols || sy >= rows {
                    self.known[(b + REACH) as usize][(a + REACH) as usize] = false;
                }
            }
        }
    }

    fn push_rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, blackness: f64) {
        let verts = vec![
            Point2::new(x0, y0),
            Point2::new(x1, y0),
            Point2::new(x1, y1),
            Point2::new(x0, y1),
        ];
        self.polygons.push((verts, blackness));
    }

    /// Lay down the black squares of the known cells; `black(a, b)` names
    /// whether square `(a, b)` is black.
    pub(super) fn checker(&mut self, black: impl Fn(i32, i32) -> bool) {
        for b in -REACH..REACH {
            for a in -REACH..REACH {
                if self.known(a, b) && black(a, b) {
                    let (x, y) = (f64::from(a), f64::from(b));
                    self.push_rect(x, y, x + 1.0, y + 1.0, 1.0);
                }
            }
        }
    }

    /// Build the ChArUco chessboard + ArUco marker print around inner corner
    /// `(u, v)`. Inner corner `(u, v)` (`u ∈ 1..cols`, `v ∈ 1..rows`) sits at
    /// the top-left of board square `(u, v)`.
    pub(super) fn charuco(&mut self, board: &CharucoBoard, u: i32, v: i32) {
        let spec = board.spec();
        self.unmark_outside_board(u, v, spec.cols as i32, spec.rows as i32);
        self.polarity_known = true;
        let black = |a: i32, b: i32| (u + a + v + b).rem_euclid(2) == 0;
        self.checker(black);

        let bits = spec.dictionary.marker_size();
        let total = bits + 2 * spec.border_bits;
        let m_rel = f64::from(spec.marker_size_rel);
        let margin = 0.5 * (1.0 - m_rel);
        let bit_step = m_rel / total as f64;
        let codes = spec.dictionary.codes();

        for b in -REACH..REACH {
            for a in -REACH..REACH {
                if !self.known(a, b) || black(a, b) {
                    continue;
                }
                let Some(marker_id) = board.marker_id_at(Coord::new(u + a, v + b)) else {
                    continue;
                };
                let Some(&code) = codes.get(marker_id as usize) else {
                    continue;
                };
                let ox = f64::from(a) + margin;
                let oy = f64::from(b) + margin;
                // Black outer marker frame over white square.
                self.push_rect(ox, oy, ox + m_rel, oy + m_rel, 1.0);
                for by in 0..bits {
                    for bx in 0..bits {
                        let idx = by * bits + bx;
                        let is_black = ((code >> idx) & 1) == 1;
                        if !is_black {
                            let rx = ox + (spec.border_bits + bx) as f64 * bit_step;
                            let ry = oy + (spec.border_bits + by) as f64 * bit_step;
                            self.push_rect(rx, ry, rx + bit_step, ry + bit_step, -1.0);
                        }
                    }
                }
            }
        }
    }

    /// Build the PuzzleBoard pattern (checkerboard + master edge half-disks)
    /// around master corner `(u, v)`.
    pub(super) fn puzzle_dots(
        &mut self,
        u: i32,
        v: i32,
        dot_diameter_rel: f64,
        display: Option<&DisplaySurfaceModel>,
    ) {
        let radius = 0.5 * dot_diameter_rel;
        let black = |a: i32, b: i32| (u + a + v + b).rem_euclid(2) == 0;
        self.polarity_known = true;
        self.checker(black);

        for b in -REACH..REACH {
            for a in -REACH..REACH {
                if !self.known(a, b) {
                    continue;
                }
                let (c, r) = (u + a, v + b);
                let edges = [
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
                        self.uncertain.push(self.disk(centre, radius));
                        continue;
                    }
                    let here_black = black(a, b);
                    let (side, weight) = if bit == 0 {
                        (if here_black { other } else { (a, b) }, 1.0)
                    } else {
                        (if here_black { (a, b) } else { other }, -1.0)
                    };
                    let dir = [
                        f64::from(side.0) + 0.5 - centre[0],
                        f64::from(side.1) + 0.5 - centre[1],
                    ];
                    if let Some(d) = rasterized_display(centre, display) {
                        self.rasterized_circle_in_cell(side, centre, radius, Some(dir), weight, d);
                    } else {
                        let half = self.half_disk(centre, radius, dir);
                        self.polygons.push((half, weight));
                    }
                }
            }
        }
    }

    /// Build the marker-board print (checkerboard + three circles) around inner
    /// corner `(u, v)`.
    pub(super) fn marker_board(
        &mut self,
        spec: &MarkerBoardSpec,
        u: i32,
        v: i32,
        display: Option<&DisplaySurfaceModel>,
    ) {
        let radius = 0.5 * f64::from(spec.circle_diameter_rel);
        let black = |a: i32, b: i32| (u + v + a + b).rem_euclid(2) == 0;
        self.polarity_known = true;
        self.checker(black);

        for circle in &spec.circles {
            let (a, b) = (circle.cell.i - u - 1, circle.cell.j - v - 1);
            if !self.known(a, b) {
                continue;
            }
            let centre = [f64::from(a) + 0.5, f64::from(b) + 0.5];
            let fill = match circle.polarity {
                CirclePolarity::Black => 1.0,
                CirclePolarity::White => 0.0,
                _ => {
                    self.uncertain.push(self.disk(centre, radius));
                    continue;
                }
            };
            let under = if black(a, b) { 1.0 } else { 0.0 };
            if fill != under {
                let w = fill - under;
                if let Some(d) = rasterized_display(centre, display) {
                    self.rasterized_circle_in_cell((a, b), centre, radius, None, w, d);
                } else {
                    self.polygons.push((self.disk(centre, radius), w));
                }
            }
        }
    }

    /// Quantize a circle or half-disk inside cell `(ca, cb)` onto the integer
    /// display pixel grid with `8×8` subpixel coverage (matching `screen.rs`)
    /// and run-length merge horizontal pixel spans sharing equal blackness.
    fn rasterized_circle_in_cell(
        &mut self,
        (ca, cb): (i32, i32),
        centre: [f64; 2],
        radius: f64,
        half_plane_dir: Option<[f64; 2]>,
        weight: f64,
        display: &DisplaySurfaceModel,
    ) {
        let n = display.display_pixels_per_cell.round().max(4.0) as i32;
        let nf = f64::from(n);
        let r2 = radius * radius;
        let levels = display.gray_levels.filter(|&l| l >= 2);
        let x_lo = (ca * n).max(((centre[0] - radius) * nf).floor() as i32);
        let x_hi = ((ca + 1) * n).min(((centre[0] + radius) * nf).ceil() as i32);
        let y_lo = (cb * n).max(((centre[1] - radius) * nf).floor() as i32);
        let y_hi = ((cb + 1) * n).min(((centre[1] + radius) * nf).ceil() as i32);

        for iy in y_lo..y_hi {
            let mut run_start: Option<i32> = None;
            let mut run_bucket = 0u32;
            let mut run_q = 0.0f64;
            for ix in x_lo..x_hi {
                let mut hits = 0u32;
                for sy in 0..SCREEN_SUB {
                    let py = (f64::from(iy) + (f64::from(sy) + 0.5) / f64::from(SCREEN_SUB)) / nf;
                    let dy = py - centre[1];
                    for sx in 0..SCREEN_SUB {
                        let px =
                            (f64::from(ix) + (f64::from(sx) + 0.5) / f64::from(SCREEN_SUB)) / nf;
                        let dx = px - centre[0];
                        if dx * dx + dy * dy < r2
                            && half_plane_dir.is_none_or(|dir| dx * dir[0] + dy * dir[1] >= 0.0)
                        {
                            hits += 1;
                        }
                    }
                }
                let cov = f64::from(hits) / f64::from(SCREEN_SUB * SCREEN_SUB);
                let (bucket, q) = match levels {
                    Some(l) => {
                        let b = (cov * f64::from(l - 1)).round() as u32;
                        (b, f64::from(b) / f64::from(l - 1))
                    }
                    None => (hits, cov),
                };
                match run_start {
                    Some(start) if bucket == run_bucket => {
                        let _ = start;
                    }
                    Some(start) => {
                        if run_bucket > 0 {
                            self.push_rect(
                                f64::from(start) / nf,
                                f64::from(iy) / nf,
                                f64::from(ix) / nf,
                                f64::from(iy + 1) / nf,
                                weight * run_q,
                            );
                        }
                        run_start = if bucket > 0 { Some(ix) } else { None };
                        run_bucket = bucket;
                        run_q = q;
                    }
                    None if bucket > 0 => {
                        run_start = Some(ix);
                        run_bucket = bucket;
                        run_q = q;
                    }
                    None => {}
                }
            }
            if let Some(start) = run_start {
                if run_bucket > 0 {
                    self.push_rect(
                        f64::from(start) / nf,
                        f64::from(iy) / nf,
                        f64::from(x_hi) / nf,
                        f64::from(iy + 1) / nf,
                        weight * run_q,
                    );
                }
            }
        }
    }

    fn circle_segments(&self, radius: f64) -> usize {
        let r_px = (radius * self.scale_px).max(1e-3);
        let n = if r_px <= MAX_CIRCLE_SAGITTA_PX {
            8.0
        } else {
            std::f64::consts::PI / (1.0 - MAX_CIRCLE_SAGITTA_PX / r_px).acos()
        };
        (n.ceil() as usize).clamp(8, 256)
    }

    fn disk(&self, centre: [f64; 2], radius: f64) -> Vec<Point2<f64>> {
        let n = self.circle_segments(radius);
        let step = std::f64::consts::TAU / n as f64;
        let rr = radius * (std::f64::consts::TAU / (n as f64 * step.sin())).sqrt();
        (0..n)
            .map(|k| {
                let t = k as f64 * step;
                Point2::new(centre[0] + rr * t.cos(), centre[1] + rr * t.sin())
            })
            .collect()
    }

    fn half_disk(&self, centre: [f64; 2], radius: f64, dir: [f64; 2]) -> Vec<Point2<f64>> {
        let n = self.circle_segments(radius).div_ceil(2).max(4);
        let step = std::f64::consts::PI / n as f64;
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
