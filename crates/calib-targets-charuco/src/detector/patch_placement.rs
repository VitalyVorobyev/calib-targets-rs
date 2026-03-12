use super::alignment_select::AlignmentSelection;
use super::marker_decode::{
    cell_has_confident_wrong_decode, dedup_markers_by_id, match_expected_marker_from_hypotheses,
    CellDecodeEvidence,
};
use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use calib_targets_aruco::ScanDecodeConfig;
use calib_targets_core::{GridAlignment, TargetDetection, GRID_TRANSFORMS_D4};
use std::cmp::Ordering;

#[derive(Clone)]
struct PlacementSelectionCandidate {
    alignment: GridAlignment,
    markers: Vec<calib_targets_aruco::MarkerDetection>,
    matched_count: usize,
    contradiction_count: usize,
    score_sum: f32,
    corner_in_bounds_count: usize,
    corner_in_bounds_ratio: f32,
}

pub(crate) fn select_patch_alignment(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    cell_evidence: &[CellDecodeEvidence],
    scan: &ScanDecodeConfig,
) -> Option<AlignmentSelection> {
    let mut candidates = enumerate_legal_patch_alignments(board, chessboard)
        .into_iter()
        .filter_map(|alignment| {
            evaluate_patch_alignment_candidate(board, chessboard, cell_evidence, scan, alignment)
        })
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return None;
    }

    candidates.sort_by(|a, b| compare_patch_selection_candidates(b, a));
    let best = candidates.first()?.clone();
    let runner_up = candidates.get(1);
    if runner_up.is_some_and(|runner_up| {
        compare_patch_selection_candidates(&best, runner_up) == Ordering::Equal
            && runner_up.alignment != best.alignment
    }) {
        return None;
    }

    let marker_inliers = (0..best.markers.len()).collect();
    Some(AlignmentSelection {
        markers: best.markers.clone(),
        alignment: CharucoAlignment {
            alignment: best.alignment,
            marker_inliers,
        },
        candidate_count: candidates.len(),
        corner_in_bounds_count: best.corner_in_bounds_count,
        corner_in_bounds_ratio: best.corner_in_bounds_ratio,
        runner_up_inlier_count: runner_up
            .map(|candidate| candidate.matched_count)
            .unwrap_or(0),
        runner_up_corner_in_bounds_ratio: runner_up
            .map(|candidate| candidate.corner_in_bounds_ratio)
            .unwrap_or(0.0),
    })
}

fn evaluate_patch_alignment_candidate(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    cell_evidence: &[CellDecodeEvidence],
    scan: &ScanDecodeConfig,
    alignment: GridAlignment,
) -> Option<PlacementSelectionCandidate> {
    let (corner_in_bounds_count, corner_in_bounds_ratio) =
        alignment_corner_fit(board, chessboard, alignment);
    if corner_in_bounds_count == 0 {
        return None;
    }

    let mut matched_markers = Vec::new();
    let mut contradiction_count = 0usize;

    for evidence in cell_evidence {
        let [sx, sy] = alignment.map(evidence.candidate.cell.gc.gx, evidence.candidate.cell.gc.gy);
        let expected_id = board.marker_id_at_cell(sx, sy);
        match expected_id {
            Some(expected_id) => {
                if let Some(marker) = match_expected_marker_from_hypotheses(
                    evidence.candidate.source,
                    expected_id,
                    &evidence.hypothesis_detections,
                    scan,
                ) {
                    matched_markers.push(marker);
                } else if cell_has_confident_wrong_decode(evidence, Some(expected_id), scan) {
                    contradiction_count += 1;
                }
            }
            None => {
                if cell_has_confident_wrong_decode(evidence, None, scan) {
                    contradiction_count += 1;
                }
            }
        }
    }

    let matched_markers = dedup_markers_by_id(matched_markers);
    if matched_markers.is_empty() {
        return None;
    }

    let score_sum = matched_markers.iter().map(|marker| marker.score).sum();
    Some(PlacementSelectionCandidate {
        alignment,
        matched_count: matched_markers.len(),
        contradiction_count,
        score_sum,
        markers: matched_markers,
        corner_in_bounds_count,
        corner_in_bounds_ratio,
    })
}

fn compare_patch_selection_candidates(
    a: &PlacementSelectionCandidate,
    b: &PlacementSelectionCandidate,
) -> Ordering {
    a.matched_count
        .cmp(&b.matched_count)
        .then_with(|| b.contradiction_count.cmp(&a.contradiction_count))
        .then_with(|| {
            a.score_sum
                .partial_cmp(&b.score_sum)
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| {
            a.corner_in_bounds_ratio
                .partial_cmp(&b.corner_in_bounds_ratio)
                .unwrap_or(Ordering::Equal)
        })
}

fn alignment_corner_fit(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    alignment: GridAlignment,
) -> (usize, f32) {
    let mut total = 0usize;
    let mut in_bounds = 0usize;
    for corner in &chessboard.corners {
        let Some(grid) = corner.grid else {
            continue;
        };
        total += 1;
        let [bi, bj] = alignment.map(grid.i, grid.j);
        if board.charuco_corner_id_from_board_corner(bi, bj).is_some() {
            in_bounds += 1;
        }
    }
    let ratio = if total == 0 {
        0.0
    } else {
        in_bounds as f32 / total as f32
    };
    (in_bounds, ratio)
}

fn enumerate_legal_patch_alignments(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
) -> Vec<GridAlignment> {
    let Some((min_i, max_i, min_j, max_j)) = chessboard_grid_bounds(chessboard) else {
        return Vec::new();
    };
    let inner_cols = board.expected_inner_cols() as i32;
    let inner_rows = board.expected_inner_rows() as i32;
    let bbox = [
        (min_i, min_j),
        (max_i, min_j),
        (max_i, max_j),
        (min_i, max_j),
    ];

    let mut alignments = Vec::new();
    for transform in GRID_TRANSFORMS_D4 {
        let transformed = bbox.map(|(i, j)| transform.apply(i, j));
        let min_x = transformed.iter().map(|p| p[0]).min().unwrap_or(0);
        let max_x = transformed.iter().map(|p| p[0]).max().unwrap_or(0);
        let min_y = transformed.iter().map(|p| p[1]).min().unwrap_or(0);
        let max_y = transformed.iter().map(|p| p[1]).max().unwrap_or(0);

        let tx_min = 1 - min_x;
        let tx_max = inner_cols - max_x;
        let ty_min = 1 - min_y;
        let ty_max = inner_rows - max_y;
        if tx_min > tx_max || ty_min > ty_max {
            continue;
        }

        for tx in tx_min..=tx_max {
            for ty in ty_min..=ty_max {
                alignments.push(GridAlignment {
                    transform,
                    translation: [tx, ty],
                });
            }
        }
    }
    alignments
}

fn chessboard_grid_bounds(chessboard: &TargetDetection) -> Option<(i32, i32, i32, i32)> {
    let mut min_i = i32::MAX;
    let mut max_i = i32::MIN;
    let mut min_j = i32::MAX;
    let mut max_j = i32::MIN;

    for corner in &chessboard.corners {
        let Some(grid) = corner.grid else {
            continue;
        };
        min_i = min_i.min(grid.i);
        max_i = max_i.max(grid.i);
        min_j = min_j.min(grid.j);
        max_j = max_j.max(grid.j);
    }

    (min_i != i32::MAX).then_some((min_i, max_i, min_j, max_j))
}
