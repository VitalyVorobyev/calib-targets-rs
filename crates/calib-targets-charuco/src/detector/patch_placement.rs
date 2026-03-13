use super::alignment_select::AlignmentSelection;
use super::marker_decode::{
    cell_has_confident_wrong_decode, dedup_markers_by_id, match_expected_marker_from_hypotheses,
    CellDecodeEvidence,
};
use super::marker_sampling::MarkerCellSource;
use super::result::{MarkerPathDiagnostics, MarkerPathSourceDiagnostics};
use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use calib_targets_aruco::MarkerDetection;
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

pub(crate) fn add_alignment_match_diagnostics(
    board: &CharucoBoard,
    cell_evidence: &[CellDecodeEvidence],
    alignment: GridAlignment,
    diagnostics: &mut MarkerPathDiagnostics,
) {
    diagnostics.expected_id_accounted = true;

    for evidence in cell_evidence {
        let source = source_diagnostics_mut(diagnostics, evidence.candidate.source);
        let [sx, sy] = alignment.map(evidence.candidate.cell.gc.gx, evidence.candidate.cell.gc.gy);
        let expected_id = board.marker_id_at_cell(sx, sy);
        if expected_id.is_some() {
            source.expected_marker_cell_count += 1;
        }

        let Some(marker) = evidence.selected_marker.as_ref() else {
            continue;
        };
        match aligned_marker_cell_id(board, alignment, marker) {
            Some(aligned_expected_id) if aligned_expected_id == marker.id => {
                source.expected_id_match_count += 1;
            }
            Some(_) => {
                source.expected_id_contradiction_count += 1;
            }
            None => {
                source.non_marker_confident_decode_count += 1;
            }
        }
    }
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

fn source_diagnostics_mut(
    diagnostics: &mut MarkerPathDiagnostics,
    source: MarkerCellSource,
) -> &mut MarkerPathSourceDiagnostics {
    match source {
        MarkerCellSource::CompleteQuad => &mut diagnostics.complete,
        MarkerCellSource::InferredThreeCorners { .. } => &mut diagnostics.inferred,
    }
}

fn aligned_marker_cell_id(
    board: &CharucoBoard,
    alignment: GridAlignment,
    marker: &MarkerDetection,
) -> Option<u32> {
    let [sx, sy] = alignment.map(marker.gc.gx, marker.gc.gy);
    board.marker_id_at_cell(sx, sy)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CharucoBoardSpec, MarkerLayout};
    use calib_targets_aruco::{builtins, GridCell, MarkerCell, MarkerDetection};
    use nalgebra::Point2;

    fn marker(
        id: u32,
        gx: i32,
        gy: i32,
        rotation: u8,
        hamming: u8,
        score: f32,
        border_score: f32,
    ) -> MarkerDetection {
        MarkerDetection {
            id,
            gc: GridCell { gx, gy },
            rotation,
            hamming,
            score,
            border_score,
            code: 0,
            inverted: false,
            corners_rect: [Point2::new(0.0, 0.0); 4],
            corners_img: None,
        }
    }

    fn evidence(
        source: MarkerCellSource,
        gx: i32,
        gy: i32,
        selected_marker: Option<MarkerDetection>,
        hypothesis_detections: Vec<(usize, MarkerDetection)>,
    ) -> CellDecodeEvidence {
        CellDecodeEvidence {
            candidate: super::super::marker_sampling::SampledMarkerCell {
                cell: MarkerCell {
                    gc: GridCell { gx, gy },
                    corners_img: [Point2::new(0.0, 0.0); 4],
                },
                source,
            },
            selected_marker,
            hypothesis_detections,
        }
    }

    fn test_board() -> CharucoBoard {
        let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("dict");
        CharucoBoard::new(CharucoBoardSpec {
            rows: 5,
            cols: 7,
            cell_size: 1.0,
            marker_size_rel: 0.75,
            dictionary: dict,
            marker_layout: MarkerLayout::OpenCvCharuco,
        })
        .expect("board")
    }

    fn first_board_cell(board: &CharucoBoard, want_marker: bool) -> (i32, i32, Option<u32>) {
        for sy in 0..(board.spec().rows as i32 - 1) {
            for sx in 0..(board.spec().cols as i32 - 1) {
                let marker_id = board.marker_id_at_cell(sx, sy);
                if marker_id.is_some() == want_marker {
                    return (sx, sy, marker_id);
                }
            }
        }
        panic!("expected board cell");
    }

    #[test]
    fn add_alignment_match_diagnostics_tracks_matches_and_contradictions() {
        let board = test_board();
        let (match_sx, match_sy, expected_match_id) = first_board_cell(&board, true);
        let (wrong_sx, wrong_sy, expected_wrong_id) = first_board_cell(&board, true);
        let (empty_sx, empty_sy, _) = first_board_cell(&board, false);

        let expected_match_id = expected_match_id.expect("marker cell");
        let expected_wrong_id = expected_wrong_id.expect("marker cell");
        let wrong_id = expected_wrong_id + 1;

        let cell_evidence = vec![
            evidence(
                MarkerCellSource::CompleteQuad,
                match_sx,
                match_sy,
                Some(marker(
                    expected_match_id,
                    match_sx,
                    match_sy,
                    0,
                    0,
                    0.96,
                    0.98,
                )),
                vec![(
                    0,
                    marker(expected_match_id, match_sx, match_sy, 0, 0, 0.96, 0.98),
                )],
            ),
            evidence(
                MarkerCellSource::InferredThreeCorners { missing_corner: 1 },
                wrong_sx,
                wrong_sy,
                Some(marker(wrong_id, wrong_sx, wrong_sy, 0, 0, 0.97, 0.98)),
                vec![(0, marker(wrong_id, wrong_sx, wrong_sy, 0, 0, 0.97, 0.98))],
            ),
            evidence(
                MarkerCellSource::CompleteQuad,
                empty_sx,
                empty_sy,
                Some(marker(999, empty_sx, empty_sy, 0, 0, 0.90, 0.91)),
                vec![(0, marker(999, empty_sx, empty_sy, 0, 0, 0.90, 0.91))],
            ),
        ];

        let mut diagnostics = MarkerPathDiagnostics::default();
        add_alignment_match_diagnostics(
            &board,
            &cell_evidence,
            GridAlignment::IDENTITY,
            &mut diagnostics,
        );

        assert!(diagnostics.expected_id_accounted);
        assert_eq!(diagnostics.complete.expected_marker_cell_count, 1);
        assert_eq!(diagnostics.complete.expected_id_match_count, 1);
        assert_eq!(diagnostics.complete.expected_id_contradiction_count, 0);
        assert_eq!(diagnostics.complete.non_marker_confident_decode_count, 1);

        assert_eq!(diagnostics.inferred.expected_marker_cell_count, 1);
        assert_eq!(diagnostics.inferred.expected_id_match_count, 0);
        assert_eq!(diagnostics.inferred.expected_id_contradiction_count, 1);
        assert_eq!(diagnostics.inferred.non_marker_confident_decode_count, 0);
    }

    #[test]
    fn add_alignment_match_diagnostics_uses_selected_marker_grid_frame() {
        let board = test_board();
        let expected_id = board
            .marker_id_at_cell(1, 0)
            .expect("board cell (1,0) should contain a marker");
        assert!(
            board.marker_id_at_cell(0, 0).is_none(),
            "board cell (0,0) should be a non-marker square"
        );

        let cell_evidence = vec![evidence(
            MarkerCellSource::CompleteQuad,
            0,
            0,
            Some(marker(expected_id, 1, 0, 1, 0, 0.97, 0.99)),
            vec![(0, marker(expected_id, 1, 0, 1, 0, 0.97, 0.99))],
        )];

        let mut diagnostics = MarkerPathDiagnostics::default();
        add_alignment_match_diagnostics(
            &board,
            &cell_evidence,
            GridAlignment::IDENTITY,
            &mut diagnostics,
        );

        assert_eq!(diagnostics.complete.expected_marker_cell_count, 0);
        assert_eq!(diagnostics.complete.expected_id_match_count, 1);
        assert_eq!(diagnostics.complete.expected_id_contradiction_count, 0);
        assert_eq!(diagnostics.complete.non_marker_confident_decode_count, 0);
    }
}
