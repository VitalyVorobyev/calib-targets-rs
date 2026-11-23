//! Blur-aware, projectively symmetric edge-bit and cell-reference sampling.
//!
//! For each interior PuzzleBoard edge flanked by two complete cells `(H_a, H_b)`:
//! - Local bright/dark cell references are estimated over a `3×3` eroded
//!   projective interior grid `[0.32, 0.50, 0.68]²` (trimmed mean rejecting
//!   min/max), clearing all printed edge dots (`radius <= 1/6`) and suppressing
//!   speckle / quantization noise.
//! - Each bit is read with a symmetric projective matched filter pairing
//!   `H_a(u, 1 - d)` against `H_b(u, d)` (or transposed for vertical edges)
//!   plus a clamped parabolic grid-line arch offset `Δp_curve` from adjacent
//!   corners. Averaging symmetric cross-edge pairs cancels the step-edge
//!   contrast, perspective area asymmetry, and first-order normal illumination
//!   gradient simultaneously, avoiding the blurry-edge displacement bias caused
//!   by selecting `argmax(confidence)` over shifted candidates.

use std::collections::HashMap;

use calib_targets_core::{homography_from_4pt, sample_bilinear, GrayImageView, LabeledCorner};
use nalgebra::{Point2, Vector2};

use crate::code_maps::{EdgeOrientation, PuzzleBoardObservedEdge};

/// Eroded 1-D coordinates inside `[0, 1]` clearing all four edge dots
/// (whose standard radius is `1/6 ≈ 0.167`).
const INTERIOR_GRID: [f32; 3] = [0.32, 0.50, 0.68];

/// Relative chord offsets along and perpendicular to an edge for the 2-D
/// Gaussian matched filter (scaled by `r = sample_radius_rel`).
const EDGE_U_OFFSETS: [f32; 5] = [-1.0, -0.5, 0.0, 0.5, 1.0];
const EDGE_D_OFFSETS: [f32; 3] = [0.25, 0.60, 0.95];

struct EdgeSampleCtx<'a> {
    view: &'a GrayImageView<'a>,
    cell_a: [Point2<f32>; 4],
    cell_b: [Point2<f32>; 4],
    p_u: Point2<f32>,
    p_v: Point2<f32>,
    prev: Option<Point2<f32>>,
    next: Option<Point2<f32>>,
    ref_bright: f32,
    ref_dark: f32,
    sample_radius_rel: f32,
    orientation: EdgeOrientation,
}

/// Read one dot per interior edge of a labelled corner set.
///
/// Every corner listed in `inliers` contributes its rightward horizontal edge
/// and its downward vertical edge — but only when the whole neighbourhood the
/// sampler needs was detected too: the two adjoining cells supply the local
/// bright/dark references and projective cell frames, and neighbours beyond the
/// edge's endpoints estimate local line curvature. That gate guarantees every
/// emitted observation lies on an interior board edge.
#[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip_all))]
pub(crate) fn sample_all_edges(
    image: &GrayImageView<'_>,
    corners: &[LabeledCorner],
    inliers: &[usize],
    sample_radius_rel: f32,
) -> Vec<PuzzleBoardObservedEdge> {
    let mut out = Vec::with_capacity(inliers.len() * 2);

    let grid_map: HashMap<(i32, i32), &LabeledCorner> = corners
        .iter()
        .filter_map(|c| c.grid.map(|g| ((g.u, g.v), c)))
        .collect();

    // Convention: `Coord.u` = column, `.v` = row.
    // Rightward horizontal edge `(c, r) -> (c+1, r)` anchors at `(r, c)`.
    // Downward vertical edge `(c, r) -> (c, r+1)` anchors at `(r, c)`.
    let mut is_inlier = vec![false; corners.len()];
    for &i in inliers {
        if let Some(slot) = is_inlier.get_mut(i) {
            *slot = true;
        }
    }

    for (idx, lc) in corners.iter().enumerate() {
        if !is_inlier[idx] {
            continue;
        }
        let Some(grid) = lc.grid else {
            continue;
        };
        let r = grid.v;
        let c = grid.u;

        // Rightward horizontal edge.
        if let Some(right) = corner_at_map(&grid_map, c + 1, r) {
            if let (Some(top_left), Some(top_right), Some(bot_right), Some(bot_left)) = (
                corner_at_map(&grid_map, c, r - 1),
                corner_at_map(&grid_map, c + 1, r - 1),
                corner_at_map(&grid_map, c + 1, r + 1),
                corner_at_map(&grid_map, c, r + 1),
            ) {
                let cell_a = [
                    top_left.position,
                    top_right.position,
                    right.position,
                    lc.position,
                ];
                let cell_b = [
                    lc.position,
                    right.position,
                    bot_right.position,
                    bot_left.position,
                ];
                let (bright, dark) = local_cell_references(image, cell_a, cell_b);
                let (bit, conf) = sample_edge_matched(&EdgeSampleCtx {
                    view: image,
                    cell_a,
                    cell_b,
                    p_u: lc.position,
                    p_v: right.position,
                    prev: corner_at_map(&grid_map, c - 1, r).map(|p| p.position),
                    next: corner_at_map(&grid_map, c + 2, r).map(|p| p.position),
                    ref_bright: bright,
                    ref_dark: dark,
                    sample_radius_rel,
                    orientation: EdgeOrientation::Horizontal,
                });
                out.push(observed_horizontal_edge(r, c, bit, conf));
            }
        }

        // Downward vertical edge.
        if let Some(below) = corner_at_map(&grid_map, c, r + 1) {
            if let (Some(tl), Some(tr), Some(br), Some(bl)) = (
                corner_at_map(&grid_map, c - 1, r),
                corner_at_map(&grid_map, c + 1, r),
                corner_at_map(&grid_map, c + 1, r + 1),
                corner_at_map(&grid_map, c - 1, r + 1),
            ) {
                let cell_a = [tl.position, lc.position, below.position, bl.position];
                let cell_b = [lc.position, tr.position, br.position, below.position];
                let (bright, dark) = local_cell_references(image, cell_a, cell_b);
                let (bit, conf) = sample_edge_matched(&EdgeSampleCtx {
                    view: image,
                    cell_a,
                    cell_b,
                    p_u: lc.position,
                    p_v: below.position,
                    prev: corner_at_map(&grid_map, c, r - 1).map(|p| p.position),
                    next: corner_at_map(&grid_map, c, r + 2).map(|p| p.position),
                    ref_bright: bright,
                    ref_dark: dark,
                    sample_radius_rel,
                    orientation: EdgeOrientation::Vertical,
                });
                out.push(observed_vertical_edge(r, c, bit, conf));
            }
        }
    }
    out
}

fn in_image_bounds(view: &GrayImageView<'_>, p: Point2<f32>) -> bool {
    p.x.is_finite()
        && p.y.is_finite()
        && p.x >= 0.0
        && p.y >= 0.0
        && p.x < view.width as f32
        && p.y < view.height as f32
}

fn classify_intensity(mean: f32, ref_bright: f32, ref_dark: f32) -> (u8, f32) {
    let (lo, hi) = if ref_bright > ref_dark {
        (ref_dark, ref_bright)
    } else {
        (ref_bright, ref_dark)
    };
    let span = (hi - lo).max(1e-3);
    let midpoint = 0.5 * (lo + hi);
    let bit = if mean > midpoint { 1u8 } else { 0u8 };
    let confidence = ((midpoint - mean).abs() / (0.5 * span)).clamp(0.0, 1.0);
    (bit, confidence)
}

fn sample_edge_matched(ctx: &EdgeSampleCtx<'_>) -> (u8, f32) {
    let dx = ctx.p_v.x - ctx.p_u.x;
    let dy = ctx.p_v.y - ctx.p_u.y;
    let edge_len = dx.hypot(dy);
    if !(edge_len.is_finite() && edge_len > 1e-3) {
        return (0, 0.0);
    }

    let linear_mid = Point2::new(0.5 * (ctx.p_u.x + ctx.p_v.x), 0.5 * (ctx.p_u.y + ctx.p_v.y));
    let mut delta_curve = match quadratic_midpoint(ctx.prev, ctx.p_u, ctx.p_v, ctx.next) {
        Some(qmid) => Vector2::new(qmid.x - linear_mid.x, qmid.y - linear_mid.y),
        None => Vector2::zeros(),
    };
    let delta_len = delta_curve.norm();
    let max_arch = 0.20 * edge_len;
    if delta_len > max_arch && delta_len > 0.0 {
        delta_curve *= max_arch / delta_len;
    }

    let r = ctx.sample_radius_rel.clamp(0.02, 0.22);
    let sigma_w = 0.5 * r;
    let inv_two_sig2 = 0.5 / (sigma_w * sigma_w);
    let rect = unit_cell_rect();

    if let (Some(ha), Some(hb)) = (
        homography_from_4pt(&rect, &ctx.cell_a),
        homography_from_4pt(&rect, &ctx.cell_b),
    ) {
        let mut sum_w = 0.0f32;
        let mut sum_val = 0.0f32;
        for &su in &EDGE_U_OFFSETS {
            let u = 0.5 + su * r;
            let du = su * r;
            let arch = (1.0 - (2.0 * u - 1.0).powi(2)).max(0.0);
            let arch_off = delta_curve * arch;
            for &sd in &EDGE_D_OFFSETS {
                let d = sd * r;
                let (qa, qb) = match ctx.orientation {
                    EdgeOrientation::Horizontal => (Point2::new(u, 1.0 - d), Point2::new(u, d)),
                    EdgeOrientation::Vertical => (Point2::new(1.0 - d, u), Point2::new(d, u)),
                };
                let pa_raw = ha.apply(qa);
                let pb_raw = hb.apply(qb);
                let pa = Point2::new(pa_raw.x + arch_off.x, pa_raw.y + arch_off.y);
                let pb = Point2::new(pb_raw.x + arch_off.x, pb_raw.y + arch_off.y);
                if !in_image_bounds(ctx.view, pa) || !in_image_bounds(ctx.view, pb) {
                    continue;
                }
                let w = (-((du * du + d * d) * inv_two_sig2)).exp();
                let va = sample_bilinear(ctx.view, pa.x, pa.y);
                let vb = sample_bilinear(ctx.view, pb.x, pb.y);
                sum_w += w;
                sum_val += w * 0.5 * (va + vb);
            }
        }
        if sum_w > 0.0 {
            return classify_intensity(sum_val / sum_w, ctx.ref_bright, ctx.ref_dark);
        }
    }

    // Fallback when quad homography is degenerate: symmetric normal-pair
    // Gaussian matched filter around the clamped curve midpoint.
    let t = Vector2::new(dx / edge_len, dy / edge_len);
    let n = Vector2::new(-t.y, t.x);
    let center = Point2::new(linear_mid.x + delta_curve.x, linear_mid.y + delta_curve.y);
    let r_px = (edge_len * r).max(1.0);
    let sigma_px = 0.5 * r_px;
    let inv_two_sig_px2 = 0.5 / (sigma_px * sigma_px);
    let mut sum_w = 0.0f32;
    let mut sum_val = 0.0f32;
    for &su in &EDGE_U_OFFSETS {
        let u_off = t * (su * r_px);
        for &sd in &EDGE_D_OFFSETS {
            let n_off = n * (sd * r_px);
            let pa = Point2::new(center.x + u_off.x + n_off.x, center.y + u_off.y + n_off.y);
            let pb = Point2::new(center.x + u_off.x - n_off.x, center.y + u_off.y - n_off.y);
            if !in_image_bounds(ctx.view, pa) || !in_image_bounds(ctx.view, pb) {
                continue;
            }
            let r2 = (su * r_px).powi(2) + (sd * r_px).powi(2);
            let w = (-r2 * inv_two_sig_px2).exp();
            let va = sample_bilinear(ctx.view, pa.x, pa.y);
            let vb = sample_bilinear(ctx.view, pb.x, pb.y);
            sum_w += w;
            sum_val += w * 0.5 * (va + vb);
        }
    }
    if sum_w <= 0.0 {
        return (0, 0.0);
    }
    classify_intensity(sum_val / sum_w, ctx.ref_bright, ctx.ref_dark)
}

fn unit_cell_rect() -> [Point2<f32>; 4] {
    [
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    ]
}

fn quadratic_midpoint(
    prev: Option<Point2<f32>>,
    p0: Point2<f32>,
    p1: Point2<f32>,
    next: Option<Point2<f32>>,
) -> Option<Point2<f32>> {
    match (prev, next) {
        (Some(pm1), Some(p2)) => Some(Point2::new(
            -0.0625 * pm1.x + 0.5625 * p0.x + 0.5625 * p1.x - 0.0625 * p2.x,
            -0.0625 * pm1.y + 0.5625 * p0.y + 0.5625 * p1.y - 0.0625 * p2.y,
        )),
        (Some(pm1), None) => Some(Point2::new(
            -0.125 * pm1.x + 0.75 * p0.x + 0.375 * p1.x,
            -0.125 * pm1.y + 0.75 * p0.y + 0.375 * p1.y,
        )),
        (None, Some(p2)) => Some(Point2::new(
            0.375 * p0.x + 0.75 * p1.x - 0.125 * p2.x,
            0.375 * p0.y + 0.75 * p1.y - 0.125 * p2.y,
        )),
        (None, None) => None,
    }
}

fn interior_cell_reference(
    view: &GrayImageView<'_>,
    rect: &[Point2<f32>; 4],
    quad: [Point2<f32>; 4],
) -> f32 {
    if let Some(h) = homography_from_4pt(rect, &quad) {
        let mut samples = Vec::with_capacity(9);
        for &v in &INTERIOR_GRID {
            for &u in &INTERIOR_GRID {
                let p = h.apply(Point2::new(u, v));
                if in_image_bounds(view, p) {
                    samples.push(sample_bilinear(view, p.x, p.y));
                }
            }
        }
        if samples.len() >= 5 {
            samples.sort_by(f32::total_cmp);
            // Trimmed mean dropping min and max suppresses e-ink/sensor speckle.
            let mid = &samples[1..samples.len() - 1];
            return mid.iter().sum::<f32>() / mid.len() as f32;
        }
        if !samples.is_empty() {
            return samples.iter().sum::<f32>() / samples.len() as f32;
        }
    }
    let cx = 0.25 * (quad[0].x + quad[1].x + quad[2].x + quad[3].x);
    let cy = 0.25 * (quad[0].y + quad[1].y + quad[2].y + quad[3].y);
    let c = Point2::new(cx, cy);
    if !in_image_bounds(view, c) {
        return 128.0;
    }
    sample_bilinear(view, c.x, c.y)
}

/// Compute `(bright, dark)` reference levels from the eroded projective
/// interior of two adjoining chessboard squares.
pub(crate) fn local_cell_references(
    view: &GrayImageView<'_>,
    cell_a_corners: [Point2<f32>; 4],
    cell_b_corners: [Point2<f32>; 4],
) -> (f32, f32) {
    let rect = unit_cell_rect();
    let a = interior_cell_reference(view, &rect, cell_a_corners);
    let b = interior_cell_reference(view, &rect, cell_b_corners);
    if a > b {
        (a, b)
    } else {
        (b, a)
    }
}

#[inline]
pub(crate) fn observed_horizontal_edge(
    row: i32,
    col: i32,
    bit: u8,
    confidence: f32,
) -> PuzzleBoardObservedEdge {
    PuzzleBoardObservedEdge {
        row,
        col,
        orientation: EdgeOrientation::Horizontal,
        bit,
        confidence,
    }
}

#[inline]
pub(crate) fn observed_vertical_edge(
    row: i32,
    col: i32,
    bit: u8,
    confidence: f32,
) -> PuzzleBoardObservedEdge {
    PuzzleBoardObservedEdge {
        row,
        col,
        orientation: EdgeOrientation::Vertical,
        bit,
        confidence,
    }
}

/// Lookup a corner by labelled `(u, v)` grid coordinates using a pre-built
/// map — O(1) amortised.
pub(crate) fn corner_at_map<'a>(
    map: &'a HashMap<(i32, i32), &'a LabeledCorner>,
    target_i: i32,
    target_j: i32,
) -> Option<&'a LabeledCorner> {
    map.get(&(target_i, target_j)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_maps;
    use calib_targets_core::{Coord, Homography};
    use nalgebra::Matrix3;

    fn assert_close(a: Point2<f32>, b: Point2<f32>, tol: f32) {
        assert!((a.x - b.x).abs() <= tol, "x: {} vs {}", a.x, b.x);
        assert!((a.y - b.y).abs() <= tol, "y: {} vs {}", a.y, b.y);
    }

    #[test]
    fn quadratic_midpoint_follows_curved_grid_line() {
        let curve = |t: f32| Point2::new(t, t * t);
        let mid = quadratic_midpoint(Some(curve(-1.0)), curve(0.0), curve(1.0), Some(curve(2.0)))
            .expect("midpoint");
        assert_close(mid, curve(0.5), 1e-6);
    }

    #[test]
    fn sample_all_edges_handles_inlier_mask_and_bounds() {
        let width = 40usize;
        let height = 40usize;
        let mut data = vec![128u8; width * height];
        for y in 0..height {
            for x in 0..width {
                let ci = (x as i32 - 10).div_euclid(10);
                let cj = (y as i32 - 10).div_euclid(10);
                let bright = (ci + cj).rem_euclid(2) == 0;
                data[y * width + x] = if bright { 230 } else { 25 };
            }
        }
        // Bright (bit=1) dot at horizontal edge between (0, 1) and (1, 1) -> (15, 20).
        for dy in -2..=2 {
            for dx in -2..=2 {
                data[(20 + dy) as usize * width + (15 + dx) as usize] = 240;
            }
        }
        // Dark (bit=0) dot at vertical edge between (1, 0) and (1, 1) -> (20, 15).
        for dy in -2..=2 {
            for dx in -2..=2 {
                data[(15 + dy) as usize * width + (20 + dx) as usize] = 15;
            }
        }
        let view = GrayImageView {
            width,
            height,
            data: &data,
        };
        let mut corners = Vec::new();
        for v in 0..=2 {
            for u in 0..=2 {
                corners.push(
                    LabeledCorner::new(
                        Point2::new((10 + u * 10) as f32, (10 + v * 10) as f32),
                        1.0,
                    )
                    .with_grid(Coord::new(u, v)),
                );
            }
        }
        let inliers = [0usize, 1, 2, 3, 5, 6, 7, 8, 999];
        let edges = sample_all_edges(&view, &corners, &inliers, 0.12);
        let h_edge = edges
            .iter()
            .find(|e| e.orientation == EdgeOrientation::Horizontal && e.row == 1 && e.col == 0)
            .expect("horizontal edge at (row=1, col=0)");
        assert_eq!(h_edge.bit, 1);
        let v_edge = edges
            .iter()
            .find(|e| e.orientation == EdgeOrientation::Vertical && e.row == 0 && e.col == 1)
            .expect("vertical edge at (row=0, col=1)");
        assert_eq!(v_edge.bit, 0);
        assert!(!edges.iter().any(|e| e.row == 1 && e.col == 1));
    }

    fn puzzle_blackness_at(bx: f32, by: f32, rows: i32, cols: i32) -> f32 {
        let ci = bx.floor() as i32;
        let cj = by.floor() as i32;
        if ci < 0 || cj < 0 || ci >= cols || cj >= rows {
            return 0.0;
        }
        let mut black = (ci + cj).rem_euclid(2) == 0;
        let r = 1.0f32 / 6.0;
        let r2 = r * r;
        let edges = [
            ((ci as f32 + 0.5, cj as f32), cj > 0, (cj - 1, ci), true),
            (
                (ci as f32 + 0.5, (cj + 1) as f32),
                cj + 1 < rows,
                (cj, ci),
                true,
            ),
            ((ci as f32, cj as f32 + 0.5), ci > 0, (cj, ci - 1), false),
            (
                ((ci + 1) as f32, cj as f32 + 0.5),
                ci + 1 < cols,
                (cj, ci),
                false,
            ),
        ];
        for ((ex, ey), inside, (mr, mc), is_h) in edges {
            if !inside {
                continue;
            }
            let d2 = (bx - ex).powi(2) + (by - ey).powi(2);
            if d2 <= r2 {
                let bit = if is_h {
                    code_maps::horizontal_edge_bit(mr, mc)
                } else {
                    code_maps::vertical_edge_bit(mr, mc)
                };
                black = bit == 0;
            }
        }
        if black {
            1.0
        } else {
            0.0
        }
    }

    fn gaussian_blur(buf: &[f32], width: usize, height: usize, sigma: f32) -> Vec<f32> {
        if sigma <= 0.1 {
            return buf.to_vec();
        }
        let radius = (3.5 * sigma).ceil() as isize;
        let mut k = Vec::with_capacity((2 * radius + 1) as usize);
        let mut ksum = 0.0f32;
        for d in -radius..=radius {
            let w = (-0.5 * (d as f32 / sigma).powi(2)).exp();
            k.push(w);
            ksum += w;
        }
        for w in &mut k {
            *w /= ksum;
        }
        let mut tmp = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut acc = 0.0f32;
                for (ki, &w) in k.iter().enumerate() {
                    let sx = (x as isize + ki as isize - radius).clamp(0, width as isize - 1);
                    acc += w * buf[y * width + sx as usize];
                }
                tmp[y * width + x] = acc;
            }
        }
        let mut out = vec![0.0f32; width * height];
        for y in 0..height {
            for x in 0..width {
                let mut acc = 0.0f32;
                for (ki, &w) in k.iter().enumerate() {
                    let sy = (y as isize + ki as isize - radius).clamp(0, height as isize - 1);
                    acc += w * tmp[sy as usize * width + x];
                }
                out[y * width + x] = acc;
            }
        }
        out
    }

    struct SyntheticSpec<'a> {
        width: usize,
        height: usize,
        rows: i32,
        cols: i32,
        board_to_img: &'a Homography<f32>,
        sigma_px: f32,
        grad_x: f32,
        grad_y: f32,
        eink_16_noise: bool,
    }

    fn render_synthetic_puzzleboard(
        s: &SyntheticSpec<'_>,
    ) -> (Vec<u8>, Vec<LabeledCorner>, Vec<usize>) {
        let SyntheticSpec {
            width,
            height,
            rows,
            cols,
            board_to_img,
            sigma_px,
            grad_x,
            grad_y,
            eink_16_noise,
        } = *s;
        let img_to_board = board_to_img.inverse().expect("invertible");
        let mut lin = vec![0.0f32; width * height];
        const SUB: usize = 3;
        for y in 0..height {
            for x in 0..width {
                let mut acc = 0.0f32;
                for sy in 0..SUB {
                    for sx in 0..SUB {
                        let px = x as f32 - 0.5 + (sx as f32 + 0.5) / SUB as f32;
                        let py = y as f32 - 0.5 + (sy as f32 + 0.5) / SUB as f32;
                        let b = img_to_board.apply(Point2::new(px, py));
                        acc += puzzle_blackness_at(b.x, b.y, rows, cols);
                    }
                }
                lin[y * width + x] = acc / ((SUB * SUB) as f32);
            }
        }
        let blurred = gaussian_blur(&lin, width, height, sigma_px);
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut data = vec![0u8; width * height];
        for y in 0..height {
            for x in 0..width {
                let dx = (x as f32 - 0.5 * width as f32) / width as f32;
                let dy = (y as f32 - 0.5 * height as f32) / height as f32;
                let ill = 1.0 + grad_x * dx + grad_y * dy;
                let refl = 215.0 - 175.0 * blurred[y * width + x];
                let mut val = ill * refl;
                if eink_16_noise {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    let n = ((state >> 11) as f32 / (1u64 << 53) as f32 - 0.5) * 6.0;
                    // 16-level E Ink quantization + noise.
                    val = ((val + n) / 17.0).round() * 17.0;
                }
                data[y * width + x] = val.round().clamp(0.0, 255.0) as u8;
            }
        }
        let mut corners = Vec::new();
        for v in 1..rows {
            for u in 1..cols {
                let p = board_to_img.apply(Point2::new(u as f32, v as f32));
                corners.push(LabeledCorner::new(p, 1.0).with_grid(Coord::new(u, v)));
            }
        }
        let inliers: Vec<usize> = (0..corners.len()).collect();
        (data, corners, inliers)
    }

    fn assert_all_decoded_bits_exact(observed: &[PuzzleBoardObservedEdge]) {
        assert!(!observed.is_empty());
        for edge in observed {
            let expected = match edge.orientation {
                EdgeOrientation::Horizontal => {
                    code_maps::horizontal_edge_bit(edge.row - 1, edge.col)
                }
                EdgeOrientation::Vertical => code_maps::vertical_edge_bit(edge.row, edge.col - 1),
            };
            assert_eq!(
                edge.bit, expected,
                "mismatch at {:?} ({}, {}) conf={}",
                edge.orientation, edge.row, edge.col, edge.confidence
            );
        }
    }

    #[test]
    fn gap33_heavy_blur_perspective_and_eink_gradient_decode_100_percent() {
        // 1) Heavy blur: σ / cell = 0.22 (24 px cells, σ = 5.28 px)
        let h_blur = Homography::new(Matrix3::new(
            24.0, 0.0, 20.0, 0.0, 24.0, 20.0, 0.0, 0.0, 1.0,
        ));
        let (d1, c1, i1) = render_synthetic_puzzleboard(&SyntheticSpec {
            width: 280,
            height: 280,
            rows: 10,
            cols: 10,
            board_to_img: &h_blur,
            sigma_px: 5.28,
            grad_x: 0.0,
            grad_y: 0.0,
            eink_16_noise: false,
        });
        let v1 = GrayImageView {
            width: 280,
            height: 280,
            data: &d1,
        };
        assert_all_decoded_bits_exact(&sample_all_edges(&v1, &c1, &i1, 0.12));

        // 2) Strong 45-degree projective foreshortening (2x+ cell area variation)
        let h_persp = Homography::new(Matrix3::new(
            28.0, 1.5, 40.0, -1.0, 24.0, 35.0, 0.0022, 0.0012, 1.0,
        ));
        let (d2, c2, i2) = render_synthetic_puzzleboard(&SyntheticSpec {
            width: 320,
            height: 320,
            rows: 9,
            cols: 9,
            board_to_img: &h_persp,
            sigma_px: 2.2,
            grad_x: 0.0,
            grad_y: 0.0,
            eink_16_noise: false,
        });
        let v2 = GrayImageView {
            width: 320,
            height: 320,
            data: &d2,
        };
        assert_all_decoded_bits_exact(&sample_all_edges(&v2, &c2, &i2, 0.12));

        // 3) Strong linear illumination gradient + 4-bit e-ink quantization + noise
        let h_eink = Homography::new(Matrix3::new(
            22.0, 0.3, 25.0, -0.2, 22.0, 25.0, 0.0005, 0.0, 1.0,
        ));
        let (d3, c3, i3) = render_synthetic_puzzleboard(&SyntheticSpec {
            width: 280,
            height: 280,
            rows: 10,
            cols: 10,
            board_to_img: &h_eink,
            sigma_px: 2.6,
            grad_x: 0.40,
            grad_y: -0.35,
            eink_16_noise: true,
        });
        let v3 = GrayImageView {
            width: 280,
            height: 280,
            data: &d3,
        };
        assert_all_decoded_bits_exact(&sample_all_edges(&v3, &c3, &i3, 0.12));
    }
}
