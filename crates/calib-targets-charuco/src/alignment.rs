//! Marker-to-board alignment and corner ID assignment.

use crate::board::CharucoBoard;
use calib_targets_aruco::{MarkerDetection, GridCell, BoardCell};
use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
use serde::{Deserialize, Serialize};
use log::debug;

#[cfg(feature = "tracing")]
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

fn dominant_rotation(markers: &[MarkerDetection]) -> u8 {
    let mut hist = [0.0f32; 4];
    for m in markers {
        hist[(m.rotation & 3) as usize] += m.score;
    }
    hist.iter()
        .enumerate()
        .max_by(|(_,a),(_,b)| a.partial_cmp(b).unwrap())
        .map(|(i,_)| i as u8)
        .unwrap_or(0)
}

#[derive(Clone, Copy)]
struct Pair {
    idx: usize,
    bc: BoardCell,
    gc: GridCell,
    weight: f32,
}

/// Estimate a grid transform + translation that best aligns marker detections to the board.
#[cfg_attr(feature = "tracing", instrument(level = "info", skip(board, markers)))]
pub(crate) fn solve_alignment(
    board: &CharucoBoard,
    markers: &[MarkerDetection],
) -> Option<CharucoAlignment> {
    let pairs = marker_pairs(board, markers);
        if pairs.is_empty() {
            return None;
        }

    let rot_mode = dominant_rotation(&markers);
    let transform = GRID_TRANSFORMS_D4[rot_mode as usize];
    let (translation, weight_sum, count) = best_translation(&pairs, transform)?;
    let inliers = inliers_for_transform(&pairs, transform, translation);
    debug!("Dominant rotation is {rot_mode}, {} inliers", inliers.len());
    let candidate = (weight_sum, count, transform, translation, inliers);

    let (_, _, transform, translation, marker_inliers) = candidate;
    Some(CharucoAlignment {
        alignment: GridAlignment {
            transform,
            translation,
        },
        marker_inliers,
    })
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
                gc: GridCell { gx: bc.sx, gy: bc.sy },
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
}
