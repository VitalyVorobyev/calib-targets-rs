use crate::alignment::{solve_alignment, CharucoAlignment};
use crate::board::{CharucoBoard, MarkerLayout};
use calib_targets_aruco::MarkerDetection;

pub(crate) fn maybe_refine_alignment(
    board: &CharucoBoard,
    markers: Vec<MarkerDetection>,
    previous_inliers: usize,
) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
    if markers.is_empty() {
        return None;
    }
    let alignment = solve_alignment(board, &markers)?;
    if alignment.marker_inliers.len() >= previous_inliers {
        Some((markers, alignment))
    } else {
        None
    }
}

pub(crate) fn select_alignment(
    board: &CharucoBoard,
    markers: Vec<MarkerDetection>,
) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
    let mut candidates: Vec<(usize, CharucoAlignment, Vec<MarkerDetection>)> = Vec::new();

    if let Some(alignment) = solve_alignment(board, &markers) {
        candidates.push((alignment.marker_inliers.len(), alignment, markers.clone()));
    }

    if board.spec().marker_layout == MarkerLayout::OpenCvCharuco {
        let even = markers
            .iter()
            .filter(|m| ((m.sx + m.sy) & 1) == 0)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(alignment) = solve_alignment(board, &even) {
            candidates.push((alignment.marker_inliers.len(), alignment, even));
        }

        let odd = markers
            .iter()
            .filter(|m| ((m.sx + m.sy) & 1) != 0)
            .cloned()
            .collect::<Vec<_>>();
        if let Some(alignment) = solve_alignment(board, &odd) {
            candidates.push((alignment.marker_inliers.len(), alignment, odd));
        }
    }

    candidates
        .into_iter()
        .max_by_key(|(inliers, _, _)| *inliers)
        .map(|(_, alignment, markers)| (markers, alignment))
}

pub(crate) fn retain_inlier_markers(
    markers: Vec<MarkerDetection>,
    mut alignment: CharucoAlignment,
) -> (Vec<MarkerDetection>, CharucoAlignment) {
    if alignment.marker_inliers.len() == markers.len() {
        return (markers, alignment);
    }

    let mut keep = vec![false; markers.len()];
    for &idx in &alignment.marker_inliers {
        if let Some(slot) = keep.get_mut(idx) {
            *slot = true;
        }
    }

    let mut filtered = Vec::with_capacity(alignment.marker_inliers.len());
    for (idx, marker) in markers.into_iter().enumerate() {
        if keep[idx] {
            filtered.push(marker);
        }
    }

    alignment.marker_inliers = (0..filtered.len()).collect();
    (filtered, alignment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alignment::GridTransform;
    use nalgebra::Point2;

    fn marker(id: u32, sx: i32, sy: i32) -> MarkerDetection {
        MarkerDetection {
            id,
            sx,
            sy,
            rotation: 0,
            hamming: 0,
            score: 1.0,
            border_score: 1.0,
            code: 0,
            inverted: false,
            corners_rect: [Point2::new(0.0, 0.0); 4],
        }
    }

    #[test]
    fn retain_inlier_markers_reindexes() {
        let markers = vec![marker(10, 0, 0), marker(11, 1, 0), marker(12, 2, 0)];
        let alignment = CharucoAlignment {
            transform: GridTransform {
                a: 1,
                b: 0,
                c: 0,
                d: 1,
            },
            translation: [0, 0],
            marker_inliers: vec![2, 0],
        };

        let (filtered, updated) = retain_inlier_markers(markers, alignment);
        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].id, 10);
        assert_eq!(filtered[1].id, 12);
        assert_eq!(updated.marker_inliers, vec![0, 1]);
    }
}
