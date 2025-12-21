use crate::alignment::CharucoAlignment;
use crate::board::CharucoBoard;
use calib_targets_core::{GridCoords, LabeledCorner, TargetDetection, TargetKind};
use std::collections::HashMap;

pub(crate) fn map_charuco_corners(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    alignment: &CharucoAlignment,
) -> TargetDetection {
    let mut by_grid: HashMap<GridCoords, LabeledCorner> = HashMap::new();

    for corner in &chessboard.corners {
        let Some(grid) = corner.grid else {
            continue;
        };
        let [bi, bj] = alignment.map(grid.i, grid.j);
        let Some(id) = board.charuco_corner_id_from_board_corner(bi, bj) else {
            continue;
        };
        let Some(grid) = grid_from_charuco_id(board, id) else {
            continue;
        };

        let candidate = LabeledCorner {
            position: corner.position,
            grid: Some(grid),
            id: Some(id),
            target_position: board.charuco_object_xy(id),
            confidence: corner.confidence,
        };

        match by_grid.get(&grid) {
            None => {
                by_grid.insert(grid, candidate);
            }
            Some(prev) if candidate.confidence > prev.confidence => {
                by_grid.insert(grid, candidate);
            }
            _ => {}
        }
    }

    let mut corners: Vec<LabeledCorner> = by_grid.into_values().collect();
    corners.sort_by_key(|c| c.id.unwrap_or(u32::MAX));

    TargetDetection {
        kind: TargetKind::Charuco,
        corners,
    }
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
    fn map_charuco_corners_keeps_best_confidence() {
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
                    target_position: None,
                    confidence: 0.2,
                },
                LabeledCorner {
                    position: Point2::new(2.0, 2.0),
                    grid: Some(GridCoords { i: 1, j: 1 }),
                    id: None,
                    target_position: None,
                    confidence: 0.9,
                },
            ],
        };

        let detection = map_charuco_corners(&board, &chessboard, &alignment);
        assert_eq!(detection.corners.len(), 1);
        let corner = &detection.corners[0];
        assert_eq!(corner.position, Point2::new(2.0, 2.0));
        assert_eq!(corner.confidence, 0.9);
        assert_eq!(corner.id, Some(0));
        assert_eq!(corner.grid, Some(GridCoords { i: 0, j: 0 }));
        assert_eq!(corner.target_position, board.charuco_object_xy(0));
    }
}
