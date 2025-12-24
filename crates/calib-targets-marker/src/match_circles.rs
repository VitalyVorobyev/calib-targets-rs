use std::collections::HashMap;

use crate::circle_score::CircleCandidate;
use crate::coords::{CellCoords, CellOffset};
use crate::types::{CircleMatch, CircleMatchParams, MarkerCircleSpec};

#[derive(Clone, Copy, Debug)]
struct MatchOption {
    index: usize,
    distance: f32,
    offset: CellOffset,
}

fn distance_cells(a: CellCoords, b: CellCoords) -> f32 {
    let di = (a.i - b.i) as f32;
    let dj = (a.j - b.j) as f32;
    (di * di + dj * dj).sqrt()
}

fn build_match_options(
    expected: MarkerCircleSpec,
    candidates: &[CircleCandidate],
    params: &CircleMatchParams,
) -> Vec<MatchOption> {
    let mut out = Vec::new();
    for (idx, cand) in candidates.iter().enumerate() {
        if cand.polarity != expected.polarity {
            continue;
        }
        let dist = distance_cells(expected.cell, cand.cell);
        if let Some(max_dist) = params.max_distance_cells {
            if dist > max_dist {
                continue;
            }
        }
        let offset = CellOffset {
            di: expected.cell.i - cand.cell.i,
            dj: expected.cell.j - cand.cell.j,
        };
        out.push(MatchOption {
            index: idx,
            distance: dist,
            offset,
        });
    }
    out
}

/// Match expected circles to detected candidates, enforcing polarity.
pub fn match_expected_circles(
    expected: &[MarkerCircleSpec],
    candidates: &[CircleCandidate],
    params: &CircleMatchParams,
) -> Vec<CircleMatch> {
    let options: Vec<Vec<MatchOption>> = expected
        .iter()
        .map(|&spec| build_match_options(spec, candidates, params))
        .collect();

    let mut best: Option<(usize, f32, Vec<Option<MatchOption>>)> = None;
    let mut current: Vec<Option<MatchOption>> = vec![None; expected.len()];
    let mut used: Vec<bool> = vec![false; candidates.len()];

    fn search(
        idx: usize,
        options: &[Vec<MatchOption>],
        used: &mut [bool],
        current: &mut [Option<MatchOption>],
        best: &mut Option<(usize, f32, Vec<Option<MatchOption>>)>,
    ) {
        if idx == options.len() {
            let matches = current.iter().filter(|m| m.is_some()).count();
            let total_dist: f32 = current
                .iter()
                .filter_map(|m| m.map(|opt| opt.distance))
                .sum();
            let should_take = match best {
                None => true,
                Some((best_matches, best_dist, _)) => {
                    matches > *best_matches || (matches == *best_matches && total_dist < *best_dist)
                }
            };
            if should_take {
                *best = Some((matches, total_dist, current.to_vec()));
            }
            return;
        }

        current[idx] = None;
        search(idx + 1, options, used, current, best);

        for opt in &options[idx] {
            if used[opt.index] {
                continue;
            }
            used[opt.index] = true;
            current[idx] = Some(*opt);
            search(idx + 1, options, used, current, best);
            current[idx] = None;
            used[opt.index] = false;
        }
    }

    search(0, &options, &mut used, &mut current, &mut best);

    let assignments = best
        .map(|(_, _, assign)| assign)
        .unwrap_or_else(|| vec![None; expected.len()]);

    expected
        .iter()
        .zip(assignments)
        .map(|(&spec, assigned)| CircleMatch {
            expected: spec,
            matched_index: assigned.map(|opt| opt.index),
            distance_cells: assigned.map(|opt| opt.distance),
            offset_cells: assigned.map(|opt| opt.offset),
        })
        .collect()
}

/// Estimate the grid offset from matched circles.
pub fn estimate_grid_offset(
    matches: &[CircleMatch],
    min_inliers: usize,
) -> Option<(CellOffset, usize)> {
    let mut counts: HashMap<CellOffset, usize> = HashMap::new();
    for m in matches {
        if let Some(offset) = m.offset_cells {
            *counts.entry(offset).or_insert(0) += 1;
        }
    }

    let (best_offset, best_count) = counts.into_iter().max_by_key(|(_, count)| *count)?;
    if best_count < min_inliers {
        return None;
    }
    Some((best_offset, best_count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circle_score::{CircleCandidate, CirclePolarity};
    use crate::coords::CellCoords;
    use nalgebra::Point2;

    fn candidate(cell: CellCoords, polarity: CirclePolarity) -> CircleCandidate {
        CircleCandidate {
            center_img: Point2::new(0.0, 0.0),
            cell,
            polarity,
            score: 0.0,
            contrast: 0.0,
        }
    }

    #[test]
    fn match_expected_circles_prefers_complete_assignment() {
        let expected = [
            MarkerCircleSpec {
                cell: CellCoords { i: 5, j: 5 },
                polarity: CirclePolarity::White,
            },
            MarkerCircleSpec {
                cell: CellCoords { i: 6, j: 5 },
                polarity: CirclePolarity::Black,
            },
            MarkerCircleSpec {
                cell: CellCoords { i: 6, j: 6 },
                polarity: CirclePolarity::White,
            },
        ];

        let candidates = vec![
            candidate(CellCoords { i: 5, j: 5 }, CirclePolarity::White),
            candidate(CellCoords { i: 6, j: 5 }, CirclePolarity::Black),
            candidate(CellCoords { i: 6, j: 6 }, CirclePolarity::White),
        ];

        let params = CircleMatchParams {
            max_candidates_per_polarity: 6,
            max_distance_cells: Some(0.1),
            min_offset_inliers: 1,
        };

        let matches = match_expected_circles(&expected, &candidates, &params);
        let matched: Vec<Option<usize>> = matches.iter().map(|m| m.matched_index).collect();
        assert_eq!(matched, vec![Some(0), Some(1), Some(2)]);
    }

    #[test]
    fn estimate_grid_offset_uses_majority_vote() {
        let matches = vec![
            CircleMatch {
                expected: MarkerCircleSpec {
                    cell: CellCoords { i: 10, j: 10 },
                    polarity: CirclePolarity::White,
                },
                matched_index: Some(0),
                distance_cells: Some(0.0),
                offset_cells: Some(CellOffset { di: 3, dj: 4 }),
            },
            CircleMatch {
                expected: MarkerCircleSpec {
                    cell: CellCoords { i: 11, j: 10 },
                    polarity: CirclePolarity::Black,
                },
                matched_index: Some(1),
                distance_cells: Some(0.0),
                offset_cells: Some(CellOffset { di: 3, dj: 4 }),
            },
            CircleMatch {
                expected: MarkerCircleSpec {
                    cell: CellCoords { i: 11, j: 11 },
                    polarity: CirclePolarity::White,
                },
                matched_index: Some(2),
                distance_cells: Some(0.0),
                offset_cells: Some(CellOffset { di: 2, dj: 4 }),
            },
        ];

        let (offset, count) = estimate_grid_offset(&matches, 2).expect("offset");
        assert_eq!(offset, CellOffset { di: 3, dj: 4 });
        assert_eq!(count, 2);
    }
}
