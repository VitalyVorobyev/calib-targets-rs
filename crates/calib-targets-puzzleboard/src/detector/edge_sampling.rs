//! Sample per-edge bits from a detected chessboard graph.

use calib_targets_core::{sample_bilinear, GrayImageView, LabeledCorner};
use nalgebra::Point2;

use crate::code_maps::{EdgeOrientation, ObservedEdge};

/// Sample one bit at the midpoint of an interior chessboard edge.
///
/// PuzzleBoard convention: every interior edge carries one dot at its
/// midpoint. The dot colour encodes the bit:
/// - **bit = 0**: dot is **white** (bright cell colour),
/// - **bit = 1**: dot is **black** (dark cell colour).
///
/// The visible half-moon of the dot sits on the cell of opposite colour, so
/// the intensity at the exact edge midpoint is a direct readout of the dot
/// colour. We classify by comparing the sampled mean to the local midpoint
/// of the bright/dark references.
///
/// Returns `(bit, confidence)` with `confidence ∈ [0, 1]` (1 = crisp match).
pub(crate) fn sample_edge_bit(
    view: &GrayImageView<'_>,
    p_u: Point2<f32>,
    p_v: Point2<f32>,
    ref_bright: f32,
    ref_dark: f32,
    sample_radius_rel: f32,
) -> (u8, f32) {
    let mid = Point2::new(0.5 * (p_u.x + p_v.x), 0.5 * (p_u.y + p_v.y));
    let dx = p_v.x - p_u.x;
    let dy = p_v.y - p_u.y;
    let edge_len = (dx * dx + dy * dy).sqrt();
    let radius = (edge_len * sample_radius_rel).max(1.0);

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
            let sx = mid.x + fdi;
            let sy = mid.y + fdj;
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

    let bit = if mean < midpoint { 1u8 } else { 0u8 };
    let confidence = ((midpoint - mean).abs() / (0.5 * span)).clamp(0.0, 1.0);
    (bit, confidence)
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
) -> ObservedEdge {
    ObservedEdge {
        row,
        col,
        orientation: EdgeOrientation::Horizontal,
        bit,
        confidence,
    }
}

#[inline]
pub(crate) fn observed_vertical_edge(row: i32, col: i32, bit: u8, confidence: f32) -> ObservedEdge {
    ObservedEdge {
        row,
        col,
        orientation: EdgeOrientation::Vertical,
        bit,
        confidence,
    }
}

/// Lookup a corner by labelled `(i, j)` grid coordinates.
pub(crate) fn corner_at(
    corners: &[LabeledCorner],
    target_i: i32,
    target_j: i32,
) -> Option<&LabeledCorner> {
    corners.iter().find(|c| match c.grid {
        Some(g) => g.i == target_i && g.j == target_j,
        None => false,
    })
}
