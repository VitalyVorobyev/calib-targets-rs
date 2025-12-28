use crate::circle_score::CircleCandidate;
use crate::coords::{CellCoords, CellOffset};
use crate::types::{CircleMatch, CircleMatchParams, MarkerCircleSpec};
use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};

use std::collections::HashMap;

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

/// Estimate a dihedral alignment from detected cell coordinates to board cell coordinates.
///
/// The returned alignment maps `(cell_i, cell_j)` from the detected grid coordinate system into
/// the board-anchored coordinate system: `dst = transform(src) + translation`.
pub fn estimate_grid_alignment(
    matches: &[CircleMatch],
    candidates: &[CircleCandidate],
    min_inliers: usize,
) -> Option<(GridAlignment, usize)> {
    #[derive(Clone, Copy)]
    struct Pair {
        sx: i32,
        sy: i32,
        ex: i32,
        ey: i32,
        weight: f32,
    }

    let mut pairs = Vec::new();
    for m in matches {
        let Some(idx) = m.matched_index else {
            continue;
        };
        let cand = candidates.get(idx)?;
        let ex = m.expected.cell.i;
        let ey = m.expected.cell.j;
        pairs.push(Pair {
            sx: cand.cell.i,
            sy: cand.cell.j,
            ex,
            ey,
            weight: cand.contrast.max(0.0),
        });
    }

    if pairs.is_empty() {
        return None;
    }

    type Candidate = (f32, usize, GridTransform, [i32; 2]);
    let mut best: Option<Candidate> = None;

    for transform in GRID_TRANSFORMS_D4 {
        let mut counts: HashMap<[i32; 2], (f32, usize)> = HashMap::new();
        for p in &pairs {
            let [rx, ry] = transform.apply(p.sx, p.sy);
            let translation = [p.ex - rx, p.ey - ry];
            let entry = counts.entry(translation).or_insert((0.0, 0));
            entry.0 += p.weight;
            entry.1 += 1;
        }

        let Some((translation, (weight_sum, count))) =
            counts.into_iter().max_by(|(_, a), (_, b)| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.1.cmp(&b.1))
            })
        else {
            continue;
        };

        let candidate = (weight_sum, count, transform, translation);
        match best {
            None => best = Some(candidate),
            Some((best_w, best_n, _, _)) => {
                if candidate.0 > best_w || (candidate.0 == best_w && candidate.1 > best_n) {
                    best = Some(candidate);
                }
            }
        }
    }

    let (_, inliers, transform, translation) = best?;
    if inliers < min_inliers {
        return None;
    }

    Some((
        GridAlignment {
            transform,
            translation,
        },
        inliers,
    ))
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

    fn candidate_with_contrast(
        cell: CellCoords,
        polarity: CirclePolarity,
        contrast: f32,
    ) -> CircleCandidate {
        CircleCandidate {
            center_img: Point2::new(0.0, 0.0),
            cell,
            polarity,
            score: 0.0,
            contrast,
        }
    }

    #[test]
    fn estimate_grid_alignment_recovers_transform_and_translation() {
        let candidates = vec![
            candidate_with_contrast(CellCoords { i: 2, j: 3 }, CirclePolarity::White, 10.0),
            candidate_with_contrast(CellCoords { i: 5, j: 1 }, CirclePolarity::Black, 10.0),
            candidate_with_contrast(CellCoords { i: -1, j: 4 }, CirclePolarity::White, 10.0),
        ];

        let transform = GridTransform {
            a: 0,
            b: 1,
            c: 1,
            d: 0,
        }; // swap axes: (i, j) -> (j, i)
        let translation = [10, 20];

        let matches = vec![
            CircleMatch {
                expected: MarkerCircleSpec {
                    cell: CellCoords {
                        i: transform.apply(2, 3)[0] + translation[0],
                        j: transform.apply(2, 3)[1] + translation[1],
                    },
                    polarity: CirclePolarity::White,
                },
                matched_index: Some(0),
                distance_cells: None,
                offset_cells: None,
            },
            CircleMatch {
                expected: MarkerCircleSpec {
                    cell: CellCoords {
                        i: transform.apply(5, 1)[0] + translation[0],
                        j: transform.apply(5, 1)[1] + translation[1],
                    },
                    polarity: CirclePolarity::Black,
                },
                matched_index: Some(1),
                distance_cells: None,
                offset_cells: None,
            },
            CircleMatch {
                expected: MarkerCircleSpec {
                    cell: CellCoords {
                        i: transform.apply(-1, 4)[0] + translation[0],
                        j: transform.apply(-1, 4)[1] + translation[1],
                    },
                    polarity: CirclePolarity::White,
                },
                matched_index: Some(2),
                distance_cells: None,
                offset_cells: None,
            },
        ];

        let (est, inliers) = estimate_grid_alignment(&matches, &candidates, 3).expect("align");
        assert_eq!(inliers, 3);
        assert_eq!(est.transform, transform);
        assert_eq!(est.translation, translation);
    }
}
