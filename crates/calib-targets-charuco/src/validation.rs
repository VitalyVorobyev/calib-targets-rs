//! Marker-to-corner linkage validation for ChArUco detections.

use crate::board::{charuco_corner_id, CharucoBoard};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// How strictly to validate marker-to-corner links.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkCheckMode {
    /// Reported corners must be a unique subset of the expected 4 corners.
    #[default]
    SubsetOk,
    /// Reported corners must be exactly the expected 4 corners.
    MustMatchAll4,
}

/// One reported marker-corner link.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkerCornerLink {
    pub marker_id: u32,
    pub corner_id: u32,
}

/// Collection of reported marker-corner links plus validation mode.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CharucoMarkerCornerLinks {
    pub links: Vec<MarkerCornerLink>,
    #[serde(default)]
    pub mode: LinkCheckMode,
}

/// Specific violation encountered while validating marker-corner links.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkViolationKind {
    UnknownMarker,
    MarkerHasNoFourCorners,
    CornerNotInNeighborhood,
    DuplicateCorner,
    MissingCorners { missing: Vec<u32> },
}

/// One validation error with context.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LinkViolation {
    pub marker_id: u32,
    pub reported_corner_id: Option<u32>,
    pub expected: Option<[u32; 4]>,
    pub kind: LinkViolationKind,
}

#[derive(Clone, Copy, Debug)]
enum MarkerExpectation {
    Unknown,
    NoFourCorners,
    Expected { corners: [u32; 4] },
}

/// Validate marker-corner links against the board definition.
pub fn validate_marker_corner_links(
    board: &CharucoBoard,
    det: &CharucoMarkerCornerLinks,
) -> Result<(), Vec<LinkViolation>> {
    let mut violations = Vec::new();
    let mut by_marker: HashMap<u32, Vec<u32>> = HashMap::new();
    let mut expected_cache: HashMap<u32, MarkerExpectation> = HashMap::new();

    for link in &det.links {
        by_marker
            .entry(link.marker_id)
            .or_default()
            .push(link.corner_id);
    }

    let squares_x = board.spec().cols as usize;
    let squares_y = board.spec().rows as usize;

    for link in &det.links {
        let entry = expected_cache.entry(link.marker_id).or_insert_with(|| {
            let Some((sx, sy)) = board.marker_cell(link.marker_id as i32) else {
                return MarkerExpectation::Unknown;
            };
            if !is_internal_cell(squares_x, squares_y, sx, sy) {
                return MarkerExpectation::NoFourCorners;
            }
            let expected = expected_corners_for_cell(squares_x, squares_y, sx, sy)
                .expect("cell precondition ensures expected corners exist");
            MarkerExpectation::Expected {
                corners: expected.map(|v| v as u32),
            }
        });

        match entry {
            MarkerExpectation::Unknown => {
                violations.push(LinkViolation {
                    marker_id: link.marker_id,
                    reported_corner_id: Some(link.corner_id),
                    expected: None,
                    kind: LinkViolationKind::UnknownMarker,
                });
            }
            MarkerExpectation::NoFourCorners => {
                violations.push(LinkViolation {
                    marker_id: link.marker_id,
                    reported_corner_id: Some(link.corner_id),
                    expected: None,
                    kind: LinkViolationKind::MarkerHasNoFourCorners,
                });
            }
            MarkerExpectation::Expected { corners } => {
                let expected_set: HashSet<u32> = corners.iter().copied().collect();
                if !expected_set.contains(&link.corner_id) {
                    violations.push(LinkViolation {
                        marker_id: link.marker_id,
                        reported_corner_id: Some(link.corner_id),
                        expected: Some(*corners),
                        kind: LinkViolationKind::CornerNotInNeighborhood,
                    });
                }
            }
        }
    }

    for (marker_id, reported) in by_marker {
        let MarkerExpectation::Expected { corners } = expected_cache
            .get(&marker_id)
            .copied()
            .unwrap_or(MarkerExpectation::Unknown)
        else {
            continue;
        };

        let expected_set: HashSet<u32> = corners.iter().copied().collect();
        let mut seen = HashSet::new();
        for &corner_id in &reported {
            if !seen.insert(corner_id) {
                violations.push(LinkViolation {
                    marker_id,
                    reported_corner_id: Some(corner_id),
                    expected: Some(corners),
                    kind: LinkViolationKind::DuplicateCorner,
                });
            }
        }

        if det.mode == LinkCheckMode::MustMatchAll4 {
            let missing: Vec<u32> = expected_set.difference(&seen).copied().collect();
            if !missing.is_empty() {
                violations.push(LinkViolation {
                    marker_id,
                    reported_corner_id: None,
                    expected: Some(corners),
                    kind: LinkViolationKind::MissingCorners { missing },
                });
            }
        }
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn is_internal_cell(squares_x: usize, squares_y: usize, sx: usize, sy: usize) -> bool {
    squares_x >= 2
        && squares_y >= 2
        && sx >= 1
        && sy >= 1
        && sx + 1 < squares_x
        && sy + 1 < squares_y
}

fn expected_corners_for_cell(
    squares_x: usize,
    squares_y: usize,
    sx: usize,
    sy: usize,
) -> Option<[usize; 4]> {
    let tl = charuco_corner_id(squares_x, squares_y, sx, sy)?;
    let tr = charuco_corner_id(squares_x, squares_y, sx + 1, sy)?;
    let br = charuco_corner_id(squares_x, squares_y, sx + 1, sy + 1)?;
    let bl = charuco_corner_id(squares_x, squares_y, sx, sy + 1)?;
    Some([tl, tr, br, bl])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{CharucoBoard, CharucoBoardSpec, MarkerLayout};
    use calib_targets_aruco::builtins;

    fn build_board() -> CharucoBoard {
        let dict = builtins::builtin_dictionary("DICT_4X4_50").expect("dict");
        CharucoBoard::new(CharucoBoardSpec {
            rows: 5,
            cols: 6,
            cell_size: 1.0,
            marker_size_rel: 0.75,
            dictionary: dict,
            marker_layout: MarkerLayout::OpenCvCharuco,
        })
        .expect("board")
    }

    #[test]
    fn validate_links_ok() {
        let board = build_board();
        let marker_id = 4u32;
        let expected = board
            .marker_surrounding_charuco_corners(marker_id as i32)
            .expect("expected corners");
        let links = CharucoMarkerCornerLinks {
            links: expected
                .iter()
                .map(|&corner_id| MarkerCornerLink {
                    marker_id,
                    corner_id: corner_id as u32,
                })
                .collect(),
            mode: LinkCheckMode::MustMatchAll4,
        };

        assert!(validate_marker_corner_links(&board, &links).is_ok());
    }

    #[test]
    fn validate_links_fails_on_wrong_corner() {
        let board = build_board();
        let marker_id = 4u32;
        let expected = board
            .marker_surrounding_charuco_corners(marker_id as i32)
            .expect("expected corners");

        let mut links: Vec<MarkerCornerLink> = expected
            .iter()
            .map(|&corner_id| MarkerCornerLink {
                marker_id,
                corner_id: corner_id as u32,
            })
            .collect();
        links[0].corner_id = 0;

        let det = CharucoMarkerCornerLinks {
            links,
            mode: LinkCheckMode::MustMatchAll4,
        };

        let err = validate_marker_corner_links(&board, &det).expect_err("should fail");
        assert!(err.iter().any(|v| {
            matches!(v.kind, LinkViolationKind::CornerNotInNeighborhood)
                && v.expected == Some(expected.map(|v| v as u32))
        }));
    }
}
