//! Marker-to-board alignment and corner ID assignment.

use crate::board::CharucoBoard;
use calib_targets_aruco::{BoardCell, GridCell, MarkerDetection};
use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
#[cfg(test)]
use log::debug;
use serde::{Deserialize, Serialize};

#[cfg(all(test, feature = "tracing"))]
use tracing::instrument;

/// Alignment result between detected markers and a board specification.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoAlignment {
    pub alignment: GridAlignment,
    pub marker_inliers: Vec<usize>,
}

impl CharucoAlignment {
    /// Map grid coordinates `(i, j)` into board coordinates.
    #[inline]
    pub fn map(&self, i: i32, j: i32) -> [i32; 2] {
        self.alignment.map(i, j)
    }
}

#[derive(Clone, Copy)]
struct Pair {
    idx: usize,
    bc: BoardCell,
    gc: GridCell,
    weight: f32,
}

#[derive(Clone)]
pub(crate) struct AlignmentCandidate {
    pub transform: GridTransform,
    pub translation: [i32; 2],
    pub weight_sum: f32,
    pub marker_inliers: Vec<usize>,
}

/// Estimate a grid transform + translation that best aligns marker detections to the board.
#[cfg_attr(feature = "tracing", instrument(level = "info", skip(board, markers)))]
#[cfg(test)]
pub(crate) fn solve_alignment(
    board: &CharucoBoard,
    markers: &[MarkerDetection],
) -> Option<CharucoAlignment> {
    let candidates = solve_alignment_candidates(board, markers);

    let best = select_best_candidate(&candidates)?;
    debug!(
        "Alignment selected {} inliers with transform {:?} and translation {:?}",
        best.marker_inliers.len(),
        best.transform,
        best.translation
    );
    Some(CharucoAlignment {
        alignment: GridAlignment {
            transform: best.transform,
            translation: best.translation,
        },
        marker_inliers: best.marker_inliers.clone(),
    })
}

pub(crate) fn solve_alignment_candidates(
    board: &CharucoBoard,
    markers: &[MarkerDetection],
) -> Vec<AlignmentCandidate> {
    let pairs = marker_pairs(board, markers);
    if pairs.is_empty() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for transform in GRID_TRANSFORMS_D4 {
        let Some((translation, weight_sum, _count)) = best_translation(&pairs, transform) else {
            continue;
        };
        let marker_inliers = inliers_for_transform(&pairs, transform, translation);
        if marker_inliers.is_empty() {
            continue;
        }
        candidates.push(AlignmentCandidate {
            transform,
            translation,
            weight_sum,
            marker_inliers,
        });
    }
    candidates
}

fn marker_pairs(board: &CharucoBoard, markers: &[MarkerDetection]) -> Vec<Pair> {
    markers
        .iter()
        .enumerate()
        .filter_map(|(idx, m)| {
            board.marker_position(m.id).map(|bc| Pair {
                idx,
                gc: m.gc,
                bc,
                weight: m.score.max(0.0),
            })
        })
        .collect()
}

fn best_translation(pairs: &[Pair], transform: GridTransform) -> Option<([i32; 2], f32, usize)> {
    let mut counts: std::collections::HashMap<[i32; 2], (f32, usize)> =
        std::collections::HashMap::new();
    for p in pairs {
        let [rx, ry] = transform.apply(p.gc.gx, p.gc.gy);
        let t = [p.bc.sx - rx, p.bc.sy - ry];
        let entry = counts.entry(t).or_insert((0.0, 0));
        entry.0 += p.weight;
        entry.1 += 1;
    }

    let (translation, (weight_sum, count)) = counts.into_iter().max_by(|(_, a), (_, b)| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.1.cmp(&b.1))
    })?;
    Some((translation, weight_sum, count))
}

fn inliers_for_transform(
    pairs: &[Pair],
    transform: GridTransform,
    translation: [i32; 2],
) -> Vec<usize> {
    let mut inliers = Vec::new();
    for p in pairs {
        let [x, y] = transform.apply(p.gc.gx, p.gc.gy);
        if x + translation[0] == p.bc.sx && y + translation[1] == p.bc.sy {
            inliers.push(p.idx);
        }
    }
    inliers
}

#[cfg(test)]
fn select_best_candidate(candidates: &[AlignmentCandidate]) -> Option<&AlignmentCandidate> {
    let best = candidates.iter().max_by(|a, b| compare_candidates(a, b))?;
    let ambiguous = candidates.iter().any(|candidate| {
        !std::ptr::eq(candidate, best)
            && candidate.marker_inliers.len() == best.marker_inliers.len()
            && (candidate.weight_sum - best.weight_sum).abs() <= 1e-6
            && (candidate.transform != best.transform || candidate.translation != best.translation)
    });
    if ambiguous {
        return None;
    }
    Some(best)
}

#[cfg(test)]
fn compare_candidates(a: &AlignmentCandidate, b: &AlignmentCandidate) -> std::cmp::Ordering {
    a.marker_inliers
        .len()
        .cmp(&b.marker_inliers.len())
        .then_with(|| {
            a.weight_sum
                .partial_cmp(&b.weight_sum)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{CharucoBoard, CharucoBoardSpec, MarkerLayout};
    use calib_targets_aruco::builtins;
    use nalgebra::Point2;

    fn build_board() -> CharucoBoard {
        let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("dict");
        CharucoBoard::new(CharucoBoardSpec {
            rows: 6,
            cols: 6,
            cell_size: 1.0,
            marker_size_rel: 0.75,
            dictionary: dict,
            marker_layout: MarkerLayout::OpenCvCharuco,
        })
        .expect("board")
    }

    #[test]
    fn alignment_identity_transform() {
        let board = build_board();
        let mut markers = Vec::new();

        for id in 0..6u32 {
            let Some(bc) = board.marker_position(id) else {
                continue;
            };
            markers.push(MarkerDetection {
                id,
                gc: GridCell {
                    gx: bc.sx,
                    gy: bc.sy,
                },
                rotation: 0,
                hamming: 0,
                score: 1.0,
                border_score: 1.0,
                code: 0,
                inverted: false,
                corners_rect: [Point2::new(0.0, 0.0); 4],
                corners_img: None,
            });
        }

        let alignment = solve_alignment(&board, &markers).expect("alignment");
        assert_eq!(alignment.alignment.transform, GridTransform::IDENTITY);
        assert_eq!(alignment.alignment.translation, [0, 0]);
        assert!(!alignment.marker_inliers.is_empty());
    }

    #[test]
    fn alignment_searches_all_d4_transforms() {
        let board = build_board();
        let transform = GRID_TRANSFORMS_D4[4];
        let translation = [6, 1];
        let inverse = transform.inverse().expect("inverse");
        let mut markers = Vec::new();

        for id in 0..6u32 {
            let Some(bc) = board.marker_position(id) else {
                continue;
            };
            let [gx, gy] = inverse.apply(bc.sx - translation[0], bc.sy - translation[1]);
            markers.push(MarkerDetection {
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
            });
        }

        let alignment = solve_alignment(&board, &markers).expect("alignment");
        assert_eq!(alignment.alignment.transform, transform);
        assert_eq!(alignment.alignment.translation, translation);
        assert_eq!(alignment.marker_inliers.len(), markers.len());
    }

    #[test]
    fn alignment_rejects_ambiguous_single_marker() {
        let board = build_board();
        let bc = board.marker_position(0).expect("marker");
        let marker = MarkerDetection {
            id: 0,
            gc: GridCell {
                gx: bc.sx,
                gy: bc.sy,
            },
            rotation: 0,
            hamming: 0,
            score: 1.0,
            border_score: 1.0,
            code: 0,
            inverted: false,
            corners_rect: [Point2::new(0.0, 0.0); 4],
            corners_img: None,
        };

        assert!(solve_alignment(&board, &[marker]).is_none());
    }
}
