use super::marker_sampling::CornerMap;
use calib_targets_aruco::{
    scan_decode_markers, GridCell, MarkerDetection, Matcher, ScanDecodeConfig,
};
use calib_targets_chessboard::{
    rectify_from_chessboard_result, ChessboardDetectionResult, RectifiedBoardView,
};
use calib_targets_core::{GrayImageView, GridCoords};
use nalgebra::Point2;
use std::collections::HashMap;

pub(crate) fn decode_markers_from_rectified_view(
    image: &GrayImageView<'_>,
    chessboard: &ChessboardDetectionResult,
    corner_map: &CornerMap,
    px_per_square: f32,
    scan: &ScanDecodeConfig,
    matcher: &Matcher,
) -> (Vec<MarkerDetection>, usize) {
    let Ok(rectified) = rectify_from_chessboard_result(
        image,
        &chessboard.detection.corners,
        &chessboard.inliers,
        px_per_square,
        0.0,
    ) else {
        return (Vec::new(), 0);
    };

    let cells_x = (rectified.max_i - rectified.min_i).max(0) as usize;
    let cells_y = (rectified.max_j - rectified.min_j).max(0) as usize;
    if cells_x == 0 || cells_y == 0 {
        return (Vec::new(), 0);
    }

    let supported_cells = count_rectified_supported_cells(&rectified, corner_map);
    if supported_cells.is_empty() {
        return (Vec::new(), 0);
    }

    let supported_lookup: HashMap<(i32, i32), usize> = supported_cells
        .iter()
        .map(|&(gx, gy, support)| ((gx, gy), support))
        .collect();

    let mut decoded = Vec::new();
    for mut marker in scan_decode_markers(
        &rectified.rect.view(),
        cells_x,
        cells_y,
        rectified.px_per_square,
        scan,
        matcher,
    ) {
        let gx = marker.gc.gx + rectified.min_i;
        let gy = marker.gc.gy + rectified.min_j;
        let Some(&support) = supported_lookup.get(&(gx, gy)) else {
            continue;
        };
        if support != 2 {
            continue;
        }
        marker.gc = GridCell { gx, gy };
        marker.corners_img = Some(rectified_cell_corners_img(
            &rectified,
            marker.gc.gx - rectified.min_i,
            marker.gc.gy - rectified.min_j,
        ));
        decoded.push(marker);
    }

    let extra_supported_cells = supported_cells
        .iter()
        .filter(|(_, _, support)| *support == 2)
        .count();
    (decoded, extra_supported_cells)
}

fn count_rectified_supported_cells(
    rectified: &RectifiedBoardView,
    corner_map: &CornerMap,
) -> Vec<(i32, i32, usize)> {
    let mut out = Vec::new();
    for gy in rectified.min_j..rectified.max_j {
        for gx in rectified.min_i..rectified.max_i {
            let support = cell_support_count(corner_map, gx, gy);
            if support >= 2 {
                out.push((gx, gy, support));
            }
        }
    }
    out
}

fn cell_support_count(corner_map: &CornerMap, gx: i32, gy: i32) -> usize {
    let corners = [
        GridCoords { i: gx, j: gy },
        GridCoords { i: gx + 1, j: gy },
        GridCoords {
            i: gx + 1,
            j: gy + 1,
        },
        GridCoords { i: gx, j: gy + 1 },
    ];
    corners
        .iter()
        .filter(|grid| corner_map.contains_key(grid))
        .count()
}

fn rectified_cell_corners_img(
    rectified: &RectifiedBoardView,
    local_gx: i32,
    local_gy: i32,
) -> [Point2<f32>; 4] {
    let s = rectified.px_per_square;
    let x0 = local_gx as f32 * s;
    let y0 = local_gy as f32 * s;
    [
        rectified.h_img_from_rect.apply(Point2::new(x0, y0)),
        rectified.h_img_from_rect.apply(Point2::new(x0 + s, y0)),
        rectified.h_img_from_rect.apply(Point2::new(x0 + s, y0 + s)),
        rectified.h_img_from_rect.apply(Point2::new(x0, y0 + s)),
    ]
}
