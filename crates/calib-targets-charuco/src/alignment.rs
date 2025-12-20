//! Marker-to-board alignment and corner ID assignment.

use crate::board::CharucoBoard;
use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{GridCoords, LabeledCorner, TargetDetection, TargetKind};

/// Integer grid transform used for marker alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridTransform {
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
}

impl GridTransform {
    /// Apply the transform to `(i, j)`.
    #[inline]
    pub fn apply(&self, i: i32, j: i32) -> [i32; 2] {
        [self.a * i + self.b * j, self.c * i + self.d * j]
    }
}

/// Alignment result between detected markers and a board specification.
#[derive(Clone, Debug)]
pub struct CharucoAlignment {
    pub transform: GridTransform,
    pub translation: [i32; 2],
    pub marker_inliers: Vec<usize>,
}

impl CharucoAlignment {
    /// Map grid coordinates `(i, j)` into board coordinates.
    #[inline]
    pub fn map(&self, i: i32, j: i32) -> [i32; 2] {
        let [x, y] = self.transform.apply(i, j);
        [x + self.translation[0], y + self.translation[1]]
    }
}

#[derive(Clone, Copy)]
struct Pair {
    idx: usize,
    sx: i32,
    sy: i32,
    ex: i32,
    ey: i32,
}

const TRANSFORMS: [GridTransform; 8] = [
    GridTransform {
        a: 1,
        b: 0,
        c: 0,
        d: 1,
    },
    GridTransform {
        a: 0,
        b: 1,
        c: -1,
        d: 0,
    },
    GridTransform {
        a: -1,
        b: 0,
        c: 0,
        d: -1,
    },
    GridTransform {
        a: 0,
        b: -1,
        c: 1,
        d: 0,
    },
    GridTransform {
        a: -1,
        b: 0,
        c: 0,
        d: 1,
    },
    GridTransform {
        a: 1,
        b: 0,
        c: 0,
        d: -1,
    },
    GridTransform {
        a: 0,
        b: 1,
        c: 1,
        d: 0,
    },
    GridTransform {
        a: 0,
        b: -1,
        c: -1,
        d: 0,
    },
];

/// Estimate a grid transform + translation that best aligns marker detections to the board.
pub(crate) fn solve_alignment(
    board: &CharucoBoard,
    markers: &[MarkerDetection],
) -> Option<CharucoAlignment> {
    let pairs = marker_pairs(board, markers);
    if pairs.is_empty() {
        return None;
    }

    let mut best: Option<(usize, GridTransform, [i32; 2], Vec<usize>)> = None;

    for transform in TRANSFORMS {
        let translation = best_translation(&pairs, transform)?;
        let inliers = inliers_for_transform(&pairs, transform, translation);
        let candidate = (inliers.len(), transform, translation, inliers);
        match best {
            None => best = Some(candidate),
            Some((best_n, _, _, _)) => {
                if candidate.0 > best_n {
                    best = Some(candidate);
                }
            }
        }
    }

    let (_, transform, translation, marker_inliers) = best?;
    Some(CharucoAlignment {
        transform,
        translation,
        marker_inliers,
    })
}

/// Map detected chessboard corners into ChArUco corner IDs using the alignment.
pub(crate) fn map_charuco_corners(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    alignment: &CharucoAlignment,
) -> TargetDetection {
    let mut corners = Vec::new();

    for c in &chessboard.corners {
        let Some(g) = c.grid else {
            continue;
        };

        let [bi, bj] = alignment.map(g.i, g.j);
        let Some(id) = board.charuco_corner_id_from_board_corner(bi, bj) else {
            continue;
        };

        corners.push(LabeledCorner {
            position: c.position,
            grid: Some(GridCoords {
                i: bi - 1,
                j: bj - 1,
            }),
            id: Some(id),
            confidence: c.confidence,
        });
    }

    corners.sort_by_key(|c| c.id.unwrap_or(u32::MAX));

    TargetDetection {
        kind: TargetKind::Charuco,
        corners,
    }
}

fn marker_pairs(board: &CharucoBoard, markers: &[MarkerDetection]) -> Vec<Pair> {
    markers
        .iter()
        .enumerate()
        .filter_map(|(idx, m)| {
            board.marker_position(m.id).map(|[ex, ey]| Pair {
                idx,
                sx: m.sx,
                sy: m.sy,
                ex,
                ey,
            })
        })
        .collect()
}

fn best_translation(pairs: &[Pair], transform: GridTransform) -> Option<[i32; 2]> {
    let mut counts: std::collections::HashMap<[i32; 2], usize> =
        std::collections::HashMap::new();
    for p in pairs {
        let [rx, ry] = transform.apply(p.sx, p.sy);
        let t = [p.ex - rx, p.ey - ry];
        *counts.entry(t).or_insert(0) += 1;
    }

    let (translation, _) = counts.into_iter().max_by_key(|(_, c)| *c)?;
    Some(translation)
}

fn inliers_for_transform(
    pairs: &[Pair],
    transform: GridTransform,
    translation: [i32; 2],
) -> Vec<usize> {
    let mut inliers = Vec::new();
    for p in pairs {
        let [x, y] = transform.apply(p.sx, p.sy);
        if x + translation[0] == p.ex && y + translation[1] == p.ey {
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
            let Some([sx, sy]) = board.marker_position(id) else {
                continue;
            };
            markers.push(MarkerDetection {
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
            });
        }

        let alignment = solve_alignment(&board, &markers).expect("alignment");
        assert_eq!(alignment.transform, TRANSFORMS[0]);
        assert_eq!(alignment.translation, [0, 0]);
        assert!(!alignment.marker_inliers.is_empty());
    }
}
