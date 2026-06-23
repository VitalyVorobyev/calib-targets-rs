use crate::circle_score::{
    score_circle_in_square, CircleCandidate, CirclePolarity, CircleScoreParams,
};
use crate::coords::CellCoords;
use calib_targets_core::{complete_cell_corners, corner_map_bounds, CornerMap, GrayImageView};

#[cfg(feature = "tracing")]
use tracing::instrument;

#[cfg_attr(
    feature = "tracing",
    instrument(level = "info", skip(img, map, score_params, roi), fields(width = img.width, height = img.height))
)]
pub(crate) fn detect_circles_via_square_warp(
    img: &GrayImageView<'_>,
    map: &CornerMap, // (i,j) -> pixel corner
    score_params: &CircleScoreParams,
    // optional ROI in grid cell coords to avoid scanning whole board:
    roi: Option<(i32, i32, i32, i32)>, // (i_min, j_min, i_max, j_max) on CELL indices
) -> Vec<CircleCandidate> {
    // Corner bounds → cell-index range: cells span [min .. max-1] on each axis.
    let Some((min_i, min_j, max_i, max_j)) = corner_map_bounds(map) else {
        return Vec::new();
    };
    let cell_min_i = min_i;
    let cell_min_j = min_j;
    let cell_max_i = max_i - 1;
    let cell_max_j = max_j - 1;
    if cell_max_i < cell_min_i || cell_max_j < cell_min_j {
        return Vec::new();
    }

    let (mut scan_i0, mut scan_j0, mut scan_i1, mut scan_j1) =
        roi.unwrap_or((cell_min_i, cell_min_j, cell_max_i, cell_max_j));

    scan_i0 = scan_i0.max(cell_min_i);
    scan_j0 = scan_j0.max(cell_min_j);
    scan_i1 = scan_i1.min(cell_max_i);
    scan_j1 = scan_j1.min(cell_max_j);
    if scan_i0 > scan_i1 || scan_j0 > scan_j1 {
        return Vec::new();
    }

    let mut out = Vec::new();

    for j in scan_j0..=scan_j1 {
        for i in scan_i0..=scan_i1 {
            // corners_img is [TL, TR, BR, BL] in image space
            let Some(corners_img) = complete_cell_corners(map, i, j) else {
                continue;
            };

            let cell = CellCoords { i, j };
            if let Some(c) = score_circle_in_square(img, &corners_img, cell, score_params) {
                out.push(c);
            }
        }
    }

    out
}

/// Utility to keep top K candidates per polarity (simple, stable).
pub(crate) fn top_k_by_polarity(
    mut v: Vec<CircleCandidate>,
    k_white: usize,
    k_black: usize,
) -> (Vec<CircleCandidate>, Vec<CircleCandidate>) {
    v.sort_by(|a, b| {
        b.contrast
            .partial_cmp(&a.contrast)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut whites = Vec::new();
    let mut blacks = Vec::new();

    for c in v {
        match c.polarity {
            CirclePolarity::White if whites.len() < k_white => whites.push(c),
            CirclePolarity::Black if blacks.len() < k_black => blacks.push(c),
            _ => {}
        }
        if whites.len() >= k_white && blacks.len() >= k_black {
            break;
        }
    }

    (whites, blacks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circle_score::CircleScoreParams;
    use calib_targets_core::{GrayImageView, GridCoords};
    use nalgebra::Point2;

    fn dummy_image() -> GrayImageView<'static> {
        GrayImageView {
            width: 1,
            height: 1,
            data: &[0u8],
        }
    }

    #[test]
    fn detect_circles_empty_map_returns_empty() {
        let img = dummy_image();
        let map = CornerMap::new();
        let out = detect_circles_via_square_warp(&img, &map, &CircleScoreParams::default(), None);
        assert!(out.is_empty());
    }

    #[test]
    fn detect_circles_insufficient_corners_returns_empty() {
        let img = dummy_image();
        let mut map = CornerMap::new();
        map.insert(GridCoords { i: 0, j: 0 }, Point2::new(0.0, 0.0));
        map.insert(GridCoords { i: 1, j: 0 }, Point2::new(1.0, 0.0));

        let out = detect_circles_via_square_warp(&img, &map, &CircleScoreParams::default(), None);
        assert!(out.is_empty());
    }
}
