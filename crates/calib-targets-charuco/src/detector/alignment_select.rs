use crate::alignment::{solve_alignment, CharucoAlignment};
use crate::board::CharucoBoard;
use calib_targets_aruco::MarkerDetection;

pub(crate) fn select_alignment(
    board: &CharucoBoard,
    markers: Vec<MarkerDetection>,
) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
    if let Some(alignment) = solve_alignment(board, &markers) {
        if alignment.marker_inliers.len() == markers.len() {
            return Some((markers, alignment));
        }
        return Some((retain_inlier_markers(&markers, &alignment), alignment));
    }
    None
}

pub(crate) fn retain_inlier_markers(
    markers: &[MarkerDetection],
    alignment: &CharucoAlignment,
) -> Vec<MarkerDetection> {
    if alignment.marker_inliers.len() == markers.len() {
        return markers.to_vec();
    }

    let mut keep = vec![false; markers.len()];
    for &idx in &alignment.marker_inliers {
        if let Some(slot) = keep.get_mut(idx) {
            *slot = true;
        }
    }

    let mut filtered = Vec::with_capacity(alignment.marker_inliers.len());
    for (idx, marker) in markers.iter().enumerate() {
        if keep[idx] {
            filtered.push(marker.clone());
        }
    }

    filtered
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_aruco::GridCell;
    use calib_targets_core::GridAlignment;
    use nalgebra::Point2;

    fn marker(id: u32, gx: i32, gy: i32) -> MarkerDetection {
        MarkerDetection {
            id,
            gc: GridCell { gx, gy },
            rotation: 0,
            hamming: 0,
            score: 1.0,
            border_score: 1.0,
            code: 0,
            inverted: false,
            corners_rect: [Point2::new(0.0, 0.0); 4],
            corners_img: None,
        }
    }

    #[test]
    fn retain_inlier_markers_reindexes() {
        let markers = vec![marker(10, 0, 0), marker(11, 1, 0), marker(12, 2, 0)];
        let alignment = CharucoAlignment {
            alignment: GridAlignment::IDENTITY,
            marker_inliers: vec![2, 0],
        };

        let filtered = retain_inlier_markers(&markers, &alignment);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].id, 10);
        assert_eq!(filtered[1].id, 12);
    }
}
