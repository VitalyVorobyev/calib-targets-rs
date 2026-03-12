use crate::alignment::{solve_alignment_candidates, AlignmentCandidate, CharucoAlignment};
use crate::board::CharucoBoard;
use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{GridAlignment, TargetDetection};

const MIN_ALIGNMENT_PATCH_RATIO: f32 = 0.75;
const MIN_ALIGNMENT_PATCH_CORNERS: usize = 12;

#[derive(Debug)]
pub(crate) struct AlignmentSelection {
    pub markers: Vec<MarkerDetection>,
    pub alignment: CharucoAlignment,
    pub candidate_count: usize,
    pub corner_in_bounds_count: usize,
    pub corner_in_bounds_ratio: f32,
    pub runner_up_inlier_count: usize,
    pub runner_up_corner_in_bounds_ratio: f32,
}

#[derive(Debug)]
pub(crate) struct AlignmentAttempt {
    pub selection: Option<AlignmentSelection>,
    pub candidate_count: usize,
}

#[derive(Clone, Copy, Debug)]
struct PatchFit {
    corner_in_bounds_count: usize,
    total_corners: usize,
}

impl PatchFit {
    fn ratio(self) -> f32 {
        if self.total_corners == 0 {
            0.0
        } else {
            self.corner_in_bounds_count as f32 / self.total_corners as f32
        }
    }
}

pub(crate) fn select_alignment(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    markers: Vec<MarkerDetection>,
) -> AlignmentAttempt {
    let candidates = solve_alignment_candidates(board, &markers);
    let Some((candidate, fit, runner_up_fit, runner_up_inliers)) =
        select_best_candidate(board, chessboard, &candidates)
    else {
        return AlignmentAttempt {
            selection: None,
            candidate_count: candidates.len(),
        };
    };
    if fit.corner_in_bounds_count < MIN_ALIGNMENT_PATCH_CORNERS
        || fit.ratio() < MIN_ALIGNMENT_PATCH_RATIO
    {
        return AlignmentAttempt {
            selection: None,
            candidate_count: candidates.len(),
        };
    }

    let alignment = CharucoAlignment {
        alignment: GridAlignment {
            transform: candidate.transform,
            translation: candidate.translation,
        },
        marker_inliers: candidate.marker_inliers.clone(),
    };

    let markers = if alignment.marker_inliers.len() == markers.len() {
        markers
    } else {
        retain_inlier_markers(&markers, &alignment)
    };

    AlignmentAttempt {
        selection: Some(AlignmentSelection {
            markers,
            alignment,
            candidate_count: candidates.len(),
            corner_in_bounds_count: fit.corner_in_bounds_count,
            corner_in_bounds_ratio: fit.ratio(),
            runner_up_inlier_count: runner_up_inliers,
            runner_up_corner_in_bounds_ratio: runner_up_fit.map(PatchFit::ratio).unwrap_or(0.0),
        }),
        candidate_count: candidates.len(),
    }
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

fn select_best_candidate<'a>(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    candidates: &'a [AlignmentCandidate],
) -> Option<(&'a AlignmentCandidate, PatchFit, Option<PatchFit>, usize)> {
    let mut scored: Vec<(&AlignmentCandidate, PatchFit)> = candidates
        .iter()
        .map(|candidate| (candidate, evaluate_patch_fit(board, chessboard, candidate)))
        .collect();
    scored.sort_by(|(a_candidate, a_fit), (b_candidate, b_fit)| {
        compare_candidates(b_candidate, b_fit, a_candidate, a_fit)
    });

    let (best_candidate, best_fit) = *scored.first()?;
    let runner_up = scored.get(1).copied();

    let ambiguous = runner_up.is_some_and(|(candidate, fit)| {
        compare_candidates(candidate, &fit, best_candidate, &best_fit) == std::cmp::Ordering::Equal
            && (candidate.transform != best_candidate.transform
                || candidate.translation != best_candidate.translation)
    });
    if ambiguous {
        return None;
    }

    Some((
        best_candidate,
        best_fit,
        runner_up.map(|(_, fit)| fit),
        runner_up
            .map(|(candidate, _)| candidate.marker_inliers.len())
            .unwrap_or(0),
    ))
}

fn compare_candidates(
    a_candidate: &AlignmentCandidate,
    a_fit: &PatchFit,
    b_candidate: &AlignmentCandidate,
    b_fit: &PatchFit,
) -> std::cmp::Ordering {
    a_candidate
        .marker_inliers
        .len()
        .cmp(&b_candidate.marker_inliers.len())
        .then_with(|| {
            a_fit
                .corner_in_bounds_count
                .cmp(&b_fit.corner_in_bounds_count)
        })
        .then_with(|| {
            a_fit
                .ratio()
                .partial_cmp(&b_fit.ratio())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| {
            a_candidate
                .weight_sum
                .partial_cmp(&b_candidate.weight_sum)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

fn evaluate_patch_fit(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    candidate: &AlignmentCandidate,
) -> PatchFit {
    let mut total_corners = 0usize;
    let mut corner_in_bounds_count = 0usize;
    let alignment = GridAlignment {
        transform: candidate.transform,
        translation: candidate.translation,
    };

    for corner in &chessboard.corners {
        let Some(grid) = corner.grid else {
            continue;
        };
        total_corners += 1;
        let [bi, bj] = alignment.map(grid.i, grid.j);
        if board.charuco_corner_id_from_board_corner(bi, bj).is_some() {
            corner_in_bounds_count += 1;
        }
    }

    PatchFit {
        corner_in_bounds_count,
        total_corners,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_aruco::GridCell;
    use calib_targets_core::{GridAlignment, GridCoords, LabeledCorner, TargetKind};
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

    #[test]
    fn patch_fit_breaks_marker_ties() {
        let board = crate::board::CharucoBoard::new(crate::board::CharucoBoardSpec {
            rows: 6,
            cols: 6,
            cell_size: 1.0,
            marker_size_rel: 0.75,
            dictionary: calib_targets_aruco::builtins::builtin_dictionary("DICT_4X4_50")
                .expect("dict"),
            marker_layout: crate::board::MarkerLayout::OpenCvCharuco,
        })
        .expect("board");

        let mut corners = Vec::new();
        for j in 0..4 {
            for i in 0..4 {
                corners.push(LabeledCorner {
                    position: Point2::new(i as f32, j as f32),
                    grid: Some(GridCoords { i, j }),
                    id: None,
                    target_position: None,
                    score: 1.0,
                });
            }
        }
        let chessboard = TargetDetection {
            kind: TargetKind::Chessboard,
            corners,
        };

        let good = AlignmentCandidate {
            transform: calib_targets_core::GridTransform::IDENTITY,
            translation: [1, 2],
            weight_sum: 1.0,
            marker_inliers: vec![0],
        };
        let bad = AlignmentCandidate {
            transform: calib_targets_core::GridTransform::IDENTITY,
            translation: [4, 2],
            weight_sum: 1.0,
            marker_inliers: vec![0],
        };

        let good_fit = evaluate_patch_fit(&board, &chessboard, &good);
        let bad_fit = evaluate_patch_fit(&board, &chessboard, &bad);
        assert!(
            compare_candidates(&good, &good_fit, &bad, &bad_fit).is_gt(),
            "{good_fit:?} vs {bad_fit:?}"
        );
        assert!(good_fit.ratio() >= MIN_ALIGNMENT_PATCH_RATIO);
    }
}
