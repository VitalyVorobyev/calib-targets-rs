use nalgebra::Point2;
use std::collections::HashMap;

use crate::circle_score::{
    score_circle_in_square, CircleCandidate, CirclePolarity, CircleScoreParams,
};
use calib_targets_core::{GrayImageView, GridCoords};

pub fn detect_circles_via_square_warp(
    img: &GrayImageView<'_>,
    map: &HashMap<GridCoords, Point2<f32>>, // (i,j)->pixel corner
    score_params: &CircleScoreParams,
    // optional ROI in grid cell coords to avoid scanning whole board:
    roi: Option<(i32, i32, i32, i32)>, // (i_min, j_min, i_max, j_max) on CELL indices
) -> Vec<CircleCandidate> {
    // Determine scan bounds from map
    let mut min_i = i32::MAX;
    let mut min_j = i32::MAX;
    let mut max_i = i32::MIN;
    let mut max_j = i32::MIN;

    for g in map.keys() {
        min_i = min_i.min(g.i);
        min_j = min_j.min(g.j);
        max_i = max_i.max(g.i);
        max_j = max_j.max(g.j);
    }

    // cell indices range: i in [min_i .. max_i-1], j in [min_j .. max_j-1]
    let (scan_i0, scan_j0, scan_i1, scan_j1) = roi.unwrap_or((min_i, min_j, max_i - 1, max_j - 1));

    let mut out = Vec::new();

    for j in scan_j0..=scan_j1 {
        for i in scan_i0..=scan_i1 {
            // corners TL,TR,BR,BL in image space
            let g00 = GridCoords { i, j };
            let g10 = GridCoords { i: i + 1, j };
            let g11 = GridCoords { i: i + 1, j: j + 1 };
            let g01 = GridCoords { i, j: j + 1 };

            let (Some(&p00), Some(&p10), Some(&p11), Some(&p01)) =
                (map.get(&g00), map.get(&g10), map.get(&g11), map.get(&g01))
            else {
                continue;
            };

            let corners_img = [p00, p10, p11, p01]; // TL,TR,BR,BL

            if let Some(c) = score_circle_in_square(img, &corners_img, (i, j), score_params) {
                out.push(c);
            }
        }
    }

    out
}

/// Utility to keep top K candidates per polarity (simple, stable).
pub fn top_k_by_polarity(
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
