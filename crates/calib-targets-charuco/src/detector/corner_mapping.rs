use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use calib_targets_aruco::MarkerDetection;
use calib_targets_core::{GridCoords, LabeledCorner, TargetDetection, TargetKind};
use std::collections::{HashMap, HashSet};

pub(crate) fn marker_board_cells(
    board: &CharucoBoard,
    markers: &[MarkerDetection],
    alignment: &CharucoAlignment,
) -> Vec<[i32; 2]> {
    markers
        .iter()
        .map(|marker| {
            let mapped = alignment.map(marker.sx, marker.sy);
            if let Some(expected) = board.marker_position(marker.id) {
                debug_assert_eq!(
                    mapped, expected,
                    "marker alignment mismatch for id {}",
                    marker.id
                );
            }
            mapped
        })
        .collect()
}

pub(crate) fn map_charuco_corners_from_markers(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    alignment: &CharucoAlignment,
    marker_board_cells: &[[i32; 2]],
) -> TargetDetection {
    let board_corners = collect_board_corners(board, chessboard, alignment);
    let allowed = marker_corner_set(board, marker_board_cells);

    let mut corners = Vec::new();
    for coord in allowed {
        let Some(obs) = board_corners.get(&coord) else {
            continue;
        };
        let Some(id) = board.charuco_corner_id_from_board_corner(coord.i, coord.j) else {
            continue;
        };
        let Some(grid) = grid_from_charuco_id(board, id) else {
            continue;
        };
        corners.push(LabeledCorner {
            position: obs.position,
            grid: Some(grid),
            id: Some(id),
            confidence: obs.confidence,
        });
    }

    corners.sort_by_key(|c| c.id.unwrap_or(u32::MAX));

    TargetDetection {
        kind: TargetKind::Charuco,
        corners,
    }
}

fn collect_board_corners(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    alignment: &CharucoAlignment,
) -> HashMap<GridCoords, LabeledCorner> {
    let mut out = HashMap::new();
    for corner in &chessboard.corners {
        let Some(grid) = corner.grid else {
            continue;
        };
        let [bi, bj] = alignment.map(grid.i, grid.j);
        if board.charuco_corner_id_from_board_corner(bi, bj).is_none() {
            continue;
        }
        let key = GridCoords { i: bi, j: bj };
        match out.get(&key) {
            None => {
                out.insert(
                    key,
                    LabeledCorner {
                        position: corner.position,
                        grid: Some(grid),
                        id: None,
                        confidence: corner.confidence,
                    },
                );
            }
            Some(prev) if corner.confidence > prev.confidence => {
                out.insert(
                    key,
                    LabeledCorner {
                        position: corner.position,
                        grid: Some(grid),
                        id: None,
                        confidence: corner.confidence,
                    },
                );
            }
            _ => {}
        }
    }
    out
}

fn marker_corner_set(board: &CharucoBoard, marker_board_cells: &[[i32; 2]]) -> HashSet<GridCoords> {
    let cols = i32::try_from(board.spec().cols).unwrap_or(0);
    let rows = i32::try_from(board.spec().rows).unwrap_or(0);
    let mut out = HashSet::new();

    for cell in marker_board_cells {
        let sx = cell[0];
        let sy = cell[1];
        for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let i = sx + dx;
            let j = sy + dy;
            if i <= 0 || j <= 0 || i >= cols || j >= rows {
                continue;
            }
            out.insert(GridCoords { i, j });
        }
    }

    out
}

fn grid_from_charuco_id(board: &CharucoBoard, id: u32) -> Option<GridCoords> {
    let inner_cols = board.expected_inner_cols();
    let inner_rows = board.expected_inner_rows();
    let total = inner_cols.checked_mul(inner_rows)?;
    if id >= total {
        return None;
    }
    let i = (id % inner_cols) as i32;
    let j = (id / inner_cols) as i32;
    Some(GridCoords { i, j })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alignment::{CharucoAlignment, GridTransform};
    use crate::board::{CharucoBoard, CharucoBoardSpec, MarkerLayout};
    use calib_targets_aruco::builtins;
    use nalgebra::Point2;

    fn build_board() -> CharucoBoard {
        let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("dict");
        CharucoBoard::new(CharucoBoardSpec {
            rows: 4,
            cols: 4,
            cell_size: 1.0,
            marker_size_rel: 0.75,
            dictionary: dict,
            marker_layout: MarkerLayout::OpenCvCharuco,
        })
        .expect("board")
    }

    #[test]
    fn marker_corner_set_skips_border_corners() {
        let board = build_board();
        let set = marker_corner_set(&board, &[[0, 0], [1, 1]]);
        assert!(set.contains(&GridCoords { i: 1, j: 1 }));
        assert!(set.contains(&GridCoords { i: 2, j: 2 }));
        assert!(!set.contains(&GridCoords { i: 0, j: 0 }));
    }

    #[test]
    fn grid_from_charuco_id_row_major() {
        let board = build_board();
        assert_eq!(
            grid_from_charuco_id(&board, 0),
            Some(GridCoords { i: 0, j: 0 })
        );
        assert_eq!(
            grid_from_charuco_id(&board, 3),
            Some(GridCoords { i: 0, j: 1 })
        );
    }

    #[test]
    fn collect_board_corners_keeps_best_confidence() {
        let board = build_board();
        let alignment = CharucoAlignment {
            transform: GridTransform {
                a: 1,
                b: 0,
                c: 0,
                d: 1,
            },
            translation: [0, 0],
            marker_inliers: Vec::new(),
        };
        let chessboard = TargetDetection {
            kind: TargetKind::Chessboard,
            corners: vec![
                LabeledCorner {
                    position: Point2::new(1.0, 1.0),
                    grid: Some(GridCoords { i: 1, j: 1 }),
                    id: None,
                    confidence: 0.2,
                },
                LabeledCorner {
                    position: Point2::new(2.0, 2.0),
                    grid: Some(GridCoords { i: 1, j: 1 }),
                    id: None,
                    confidence: 0.9,
                },
            ],
        };

        let map = collect_board_corners(&board, &chessboard, &alignment);
        let corner = map.get(&GridCoords { i: 1, j: 1 }).expect("corner");
        assert_eq!(corner.position, Point2::new(2.0, 2.0));
        assert_eq!(corner.confidence, 0.9);
    }
}
