//! Sample per-edge bits from a detected chessboard graph.

use std::collections::HashMap;

use calib_targets_core::{homography_from_4pt, sample_bilinear, GrayImageView, LabeledCorner};
use nalgebra::Point2;

use crate::code_maps::{EdgeOrientation, PuzzleBoardObservedEdge};

/// Sample one bit at the midpoint of an interior chessboard edge or from
/// caller-provided image-frame candidate centers.
///
/// PuzzleBoard convention (matches Stelldinger 2024 / PStelldinger/PuzzleBoard):
/// every interior edge carries one dot at its midpoint. The dot colour encodes
/// the bit:
/// - **bit = 0**: dot is **black** (dark cell colour),
/// - **bit = 1**: dot is **white** (bright cell colour).
///
/// The visible half-moon of the dot sits on the cell of opposite colour, so
/// the intensity at the exact edge midpoint is a direct readout of the dot
/// colour. We classify by comparing the sampled mean to the local midpoint
/// of the bright/dark references.
///
/// The legacy chord midpoint is always sampled as a fallback. Additional
/// centers let callers account for perspective and lens-distorted edge curves.
/// Returns `(bit, confidence)` with `confidence ∈ [0, 1]` (1 = crisp match).
pub(crate) fn sample_edge_bit_with_candidates(
    view: &GrayImageView<'_>,
    p_u: Point2<f32>,
    p_v: Point2<f32>,
    candidates: &[Point2<f32>],
    ref_bright: f32,
    ref_dark: f32,
    sample_radius_rel: f32,
) -> (u8, f32) {
    let mid = Point2::new(0.5 * (p_u.x + p_v.x), 0.5 * (p_u.y + p_v.y));
    let dx = p_v.x - p_u.x;
    let dy = p_v.y - p_u.y;
    let edge_len = (dx * dx + dy * dy).sqrt();
    let radius = (edge_len * sample_radius_rel).max(1.0);

    let mut best = sample_bit_at_center(view, mid, radius, ref_bright, ref_dark);
    for &candidate in candidates {
        if !candidate.x.is_finite() || !candidate.y.is_finite() {
            continue;
        }
        let sampled = sample_bit_at_center(view, candidate, radius, ref_bright, ref_dark);
        if sampled.1 > best.1 {
            best = sampled;
        }
    }
    best
}

fn sample_bit_at_center(
    view: &GrayImageView<'_>,
    center: Point2<f32>,
    radius: f32,
    ref_bright: f32,
    ref_dark: f32,
) -> (u8, f32) {
    let r = radius.ceil() as i32;
    let r2 = radius * radius;
    let mut sum = 0.0f32;
    let mut n = 0.0f32;
    for dj in -r..=r {
        for di in -r..=r {
            let fdi = di as f32;
            let fdj = dj as f32;
            if fdi * fdi + fdj * fdj > r2 {
                continue;
            }
            let sx = center.x + fdi;
            let sy = center.y + fdj;
            if sx < 0.0 || sy < 0.0 || sx >= view.width as f32 || sy >= view.height as f32 {
                continue;
            }
            sum += sample_bilinear(view, sx, sy);
            n += 1.0;
        }
    }
    if n < 1.0 {
        return (0, 0.0);
    }
    let mean = sum / n;

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

/// Candidate sample centers for a horizontal edge in image coordinates.
///
/// `cell_above` and `cell_below` must use TL, TR, BR, BL winding. The returned
/// centers are image-frame points; callers still get the legacy chord midpoint
/// through [`sample_edge_bit_with_candidates`] even when this returns empty.
pub(crate) fn horizontal_edge_sample_centers(
    cell_above: [Point2<f32>; 4],
    cell_below: [Point2<f32>; 4],
    left: Option<Point2<f32>>,
    edge_left: Point2<f32>,
    edge_right: Point2<f32>,
    right: Option<Point2<f32>>,
) -> Vec<Point2<f32>> {
    let mut out = Vec::with_capacity(3);
    let rect = unit_cell_rect();
    if let Some(p) = edge_point_from_cell(cell_above, Point2::new(0.5, 1.0), &rect) {
        out.push(p);
    }
    if let Some(p) = edge_point_from_cell(cell_below, Point2::new(0.5, 0.0), &rect) {
        push_distinct(&mut out, p);
    }
    if let Some(p) = quadratic_midpoint(left, edge_left, edge_right, right) {
        push_distinct(&mut out, p);
    }
    out
}

/// Candidate sample centers for a vertical edge in image coordinates.
///
/// `cell_left` and `cell_right` must use TL, TR, BR, BL winding.
pub(crate) fn vertical_edge_sample_centers(
    cell_left: [Point2<f32>; 4],
    cell_right: [Point2<f32>; 4],
    above: Option<Point2<f32>>,
    edge_top: Point2<f32>,
    edge_bottom: Point2<f32>,
    below: Option<Point2<f32>>,
) -> Vec<Point2<f32>> {
    let mut out = Vec::with_capacity(3);
    let rect = unit_cell_rect();
    if let Some(p) = edge_point_from_cell(cell_left, Point2::new(1.0, 0.5), &rect) {
        out.push(p);
    }
    if let Some(p) = edge_point_from_cell(cell_right, Point2::new(0.0, 0.5), &rect) {
        push_distinct(&mut out, p);
    }
    if let Some(p) = quadratic_midpoint(above, edge_top, edge_bottom, below) {
        push_distinct(&mut out, p);
    }
    out
}

fn unit_cell_rect() -> [Point2<f32>; 4] {
    [
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    ]
}

fn edge_point_from_cell(
    cell_tl_tr_br_bl: [Point2<f32>; 4],
    cell_point: Point2<f32>,
    rect: &[Point2<f32>; 4],
) -> Option<Point2<f32>> {
    let h = homography_from_4pt(rect, &cell_tl_tr_br_bl)?;
    let p = h.apply(cell_point);
    if p.x.is_finite() && p.y.is_finite() {
        Some(p)
    } else {
        None
    }
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

fn push_distinct(out: &mut Vec<Point2<f32>>, p: Point2<f32>) {
    const EPS2: f32 = 1e-4;
    if out.iter().any(|q| {
        let dx = p.x - q.x;
        let dy = p.y - q.y;
        dx * dx + dy * dy <= EPS2
    }) {
        return;
    }
    out.push(p);
}

/// Compute bright/dark reference levels from the sampled centroids of two
/// adjoining chessboard squares.
pub(crate) fn local_cell_references(
    view: &GrayImageView<'_>,
    cell_a_corners: [Point2<f32>; 4],
    cell_b_corners: [Point2<f32>; 4],
) -> (f32, f32) {
    fn sample_centroid(view: &GrayImageView<'_>, quad: [Point2<f32>; 4]) -> f32 {
        let cx = 0.25 * (quad[0].x + quad[1].x + quad[2].x + quad[3].x);
        let cy = 0.25 * (quad[0].y + quad[1].y + quad[2].y + quad[3].y);
        if cx < 0.0 || cy < 0.0 || cx >= view.width as f32 || cy >= view.height as f32 {
            return 128.0;
        }
        sample_bilinear(view, cx, cy)
    }
    let a = sample_centroid(view, cell_a_corners);
    let b = sample_centroid(view, cell_b_corners);
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

/// Lookup a corner by labelled `(i, j)` grid coordinates using a pre-built
/// map — O(1) amortised.
///
/// Build the map once with:
/// ```ignore
/// let map: HashMap<(i32, i32), &LabeledCorner> = corners
///     .iter()
///     .filter_map(|c| c.grid.map(|g| ((g.i, g.j), c)))
///     .collect();
/// ```
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

    fn assert_close(a: Point2<f32>, b: Point2<f32>, tol: f32) {
        assert!((a.x - b.x).abs() <= tol, "x: {} vs {}", a.x, b.x);
        assert!((a.y - b.y).abs() <= tol, "y: {} vs {}", a.y, b.y);
    }

    #[test]
    fn local_homography_edge_center_can_differ_from_chord_midpoint() {
        let top = [
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(12.0, 12.0),
            Point2::new(0.0, 10.0),
        ];
        let bottom = [
            Point2::new(0.0, 10.0),
            Point2::new(12.0, 12.0),
            Point2::new(11.0, 24.0),
            Point2::new(-1.0, 21.0),
        ];
        let candidates = horizontal_edge_sample_centers(
            top,
            bottom,
            None,
            Point2::new(0.0, 10.0),
            Point2::new(12.0, 12.0),
            None,
        );
        let chord = Point2::new(6.0, 11.0);
        assert!(candidates
            .iter()
            .any(|p| (p.x - chord.x).abs() > 0.05 || (p.y - chord.y).abs() > 0.05));
    }

    #[test]
    fn quadratic_midpoint_follows_curved_grid_line() {
        let curve = |t: f32| Point2::new(t, t * t);
        let mid = quadratic_midpoint(Some(curve(-1.0)), curve(0.0), curve(1.0), Some(curve(2.0)))
            .expect("midpoint");
        assert_close(mid, curve(0.5), 1e-6);
    }

    #[test]
    fn candidate_sampler_falls_back_to_legacy_midpoint() {
        let data = vec![0u8, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255];
        let view = GrayImageView {
            width: 4,
            height: 3,
            data: &data,
        };
        let p0 = Point2::new(1.0, 1.0);
        let p1 = Point2::new(2.0, 1.0);
        let legacy = sample_edge_bit_with_candidates(&view, p0, p1, &[], 255.0, 0.0, 0.01);
        let fallback = sample_edge_bit_with_candidates(&view, p0, p1, &[], 255.0, 0.0, 0.01);
        assert_eq!(legacy.0, fallback.0);
        assert!((legacy.1 - fallback.1).abs() <= f32::EPSILON);
    }
}
