//! Board-frame resolution from scored circle candidates.
//!
//! The three marker circles exist for exactly one reason: to break the board's
//! 4-fold rotational symmetry. Resolving the frame is therefore a
//! hypothesis-and-verify problem, not a matching problem — the same shape the
//! ChArUco and PuzzleBoard anchoring paths use. Each `(rotation, translation)`
//! pair is a complete answer; we enumerate them, count how many expected
//! circles each one explains, and accept only a frame that explains the whole
//! layout and is strictly better than every alternative.
//!
//! Matching the circles *first* and inferring the frame afterwards cannot
//! work: the only cheap similarity available before alignment is the distance
//! between a board cell index and a detected cell index, and comparing those
//! presumes the very translation the alignment is meant to establish.

use crate::circle_score::CircleCandidate;
use crate::coords::CellOffset;
use crate::types::{CircleMatch, CircleMatchParams, MarkerCircleSpec};
use calib_targets_core::{Coord, GridAlignment, GRID_TRANSFORMS_C4};

/// One frame hypothesis: which `C4` relabelling, and the integer translation
/// that carries detected cell coordinates onto board cell coordinates.
///
/// Ordered so the hypothesis sweep is independent of iteration order — a
/// `HashMap` here is how the previous implementation resolved ties
/// non-deterministically.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct FrameKey {
    transform: usize,
    translation: [i32; 2],
}

/// Outcome of the frame sweep.
///
/// `matches` always describes the best hypothesis found, even when that
/// hypothesis is rejected, so the diagnostics channel can show what the
/// detector was looking at.
pub(crate) struct FrameSearch {
    /// One entry per expected circle, in layout order.
    pub matches: Vec<CircleMatch>,
    /// The accepted frame; `None` when the sweep found nothing acceptable.
    pub alignment: Option<GridAlignment>,
    /// Expected circles explained by the best hypothesis.
    pub inliers: usize,
    /// Expected circles explained by the best *other* hypothesis.
    pub runner_up_inliers: usize,
    /// The best hypothesis tied with another distinct frame.
    pub ambiguous: bool,
}

/// Enumerate every board frame consistent with at least one circle, and accept
/// the one that explains the layout outright.
///
/// A frame is accepted when it explains at least `min_offset_inliers` expected
/// circles *and* strictly beats every other distinct frame. The uniqueness
/// requirement is not a tie-break convenience: two frames that explain the
/// layout equally well mean the marker constellation failed at its only job,
/// and picking either one would ship a wrong `(i, j)` label on every corner.
pub(crate) fn resolve_board_frame(
    expected: &[MarkerCircleSpec],
    candidates: &[CircleCandidate],
    params: &CircleMatchParams,
) -> FrameSearch {
    let unmatched = || {
        expected
            .iter()
            .copied()
            .map(CircleMatch::unmatched)
            .collect()
    };

    // Every hypothesis is pinned by one expected-circle-to-candidate seed, so
    // enumerating the seeds enumerates every frame that explains anything at
    // all. Dedup keeps the runner-up honest: the same frame reached from two
    // different seeds is one frame, not two.
    let mut frames: Vec<FrameKey> = Vec::new();
    for (transform_index, transform) in GRID_TRANSFORMS_C4.iter().enumerate() {
        for spec in expected {
            for cand in candidates {
                if cand.polarity != spec.polarity {
                    continue;
                }
                let rotated = transform.apply(Coord::new(cand.cell.i, cand.cell.j));
                frames.push(FrameKey {
                    transform: transform_index,
                    translation: [spec.cell.i - rotated.u, spec.cell.j - rotated.v],
                });
            }
        }
    }
    frames.sort_unstable();
    frames.dedup();

    let mut best: Option<(FrameKey, Vec<Option<usize>>, usize)> = None;
    let mut runner_up_inliers = 0usize;
    let mut ambiguous = false;

    for frame in frames {
        let (assignment, inliers) = verify_frame(expected, candidates, frame);
        let best_inliers = best.as_ref().map(|(_, _, n)| *n).unwrap_or(0);
        if inliers > best_inliers {
            runner_up_inliers = best_inliers;
            ambiguous = false;
            best = Some((frame, assignment, inliers));
        } else if inliers == best_inliers && best.is_some() {
            runner_up_inliers = runner_up_inliers.max(inliers);
            ambiguous = true;
        } else {
            runner_up_inliers = runner_up_inliers.max(inliers);
        }
    }

    let Some((frame, assignment, inliers)) = best else {
        return FrameSearch {
            matches: unmatched(),
            alignment: None,
            inliers: 0,
            runner_up_inliers: 0,
            ambiguous: false,
        };
    };

    let transform = GRID_TRANSFORMS_C4[frame.transform];
    let alignment = transform.with_translation(frame.translation);
    let matches = expected
        .iter()
        .zip(&assignment)
        .map(|(&spec, assigned)| match assigned {
            Some(index) => CircleMatch::unmatched(spec).with_match(
                *index,
                CellOffset {
                    di: frame.translation[0],
                    dj: frame.translation[1],
                },
            ),
            None => CircleMatch::unmatched(spec),
        })
        .collect();

    let accepted = inliers >= params.min_offset_inliers && !ambiguous;
    FrameSearch {
        matches,
        alignment: accepted.then_some(alignment),
        inliers,
        runner_up_inliers,
        ambiguous,
    }
}

/// Count the expected circles one frame explains, and record which candidate
/// explains each.
///
/// Cell coordinates are integers and the frame is a bijection on them, so
/// "explains" is exact coincidence — there is no continuum to threshold, and
/// a candidate can satisfy at most one expected circle.
fn verify_frame(
    expected: &[MarkerCircleSpec],
    candidates: &[CircleCandidate],
    frame: FrameKey,
) -> (Vec<Option<usize>>, usize) {
    let transform = GRID_TRANSFORMS_C4[frame.transform];
    let mut assignment = Vec::with_capacity(expected.len());
    let mut inliers = 0usize;
    for spec in expected {
        let found = candidates.iter().position(|cand| {
            if cand.polarity != spec.polarity {
                return false;
            }
            let rotated = transform.apply(Coord::new(cand.cell.i, cand.cell.j));
            rotated.u + frame.translation[0] == spec.cell.i
                && rotated.v + frame.translation[1] == spec.cell.j
        });
        if found.is_some() {
            inliers += 1;
        }
        assignment.push(found);
    }
    (assignment, inliers)
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
            contrast: 40.0,
            squareness: 0.0,
        }
    }

    /// The layout from issue #96: an L whose polarities alternate with
    /// `(i + j)` parity.
    fn layout() -> [MarkerCircleSpec; 3] {
        [
            MarkerCircleSpec {
                cell: CellCoords { i: 3, j: 3 },
                polarity: CirclePolarity::White,
            },
            MarkerCircleSpec {
                cell: CellCoords { i: 4, j: 3 },
                polarity: CirclePolarity::Black,
            },
            MarkerCircleSpec {
                cell: CellCoords { i: 4, j: 4 },
                polarity: CirclePolarity::White,
            },
        ]
    }

    fn params(min_offset_inliers: usize) -> CircleMatchParams {
        CircleMatchParams {
            max_candidates_per_polarity: 6,
            min_offset_inliers,
        }
    }

    #[test]
    fn resolves_a_translated_layout_to_the_identity_rotation() {
        let candidates = vec![
            candidate(CellCoords { i: 2, j: 2 }, CirclePolarity::White),
            candidate(CellCoords { i: 3, j: 2 }, CirclePolarity::Black),
            candidate(CellCoords { i: 3, j: 3 }, CirclePolarity::White),
        ];
        let search = resolve_board_frame(&layout(), &candidates, &params(3));
        let alignment = search.alignment.expect("frame resolved");
        assert_eq!(alignment.matrix(), [[1, 0], [0, 1]]);
        assert_eq!(alignment.translation(), [1, 1]);
        assert_eq!(search.inliers, 3);
        assert!(!search.ambiguous);
        assert_eq!(
            search
                .matches
                .iter()
                .map(|m| m.matched_index)
                .collect::<Vec<_>>(),
            vec![Some(0), Some(1), Some(2)]
        );
    }

    #[test]
    fn recovers_a_genuinely_rotated_board() {
        // The same L imaged after a quarter turn: (i, j) -> (-j, i).
        let candidates = vec![
            candidate(CellCoords { i: -3, j: 3 }, CirclePolarity::White),
            candidate(CellCoords { i: -3, j: 4 }, CirclePolarity::Black),
            candidate(CellCoords { i: -4, j: 4 }, CirclePolarity::White),
        ];
        let search = resolve_board_frame(&layout(), &candidates, &params(3));
        let alignment = search.alignment.expect("frame resolved");
        assert_eq!(search.inliers, 3);
        for (spec, m) in layout().iter().zip(&search.matches) {
            let index = m.matched_index.expect("matched");
            let cell = candidates[index].cell;
            let mapped = alignment.apply(Coord::new(cell.i, cell.j));
            assert_eq!((mapped.u, mapped.v), (spec.cell.i, spec.cell.j));
        }
    }

    /// The failure issue #96 reported: spurious same-polarity candidates that
    /// happen to complete the layout under a second rotation. Two frames
    /// explain all three circles, so neither may be returned.
    #[test]
    fn rejects_a_frame_a_second_rotation_explains_equally_well() {
        let candidates = vec![
            candidate(CellCoords { i: 3, j: 3 }, CirclePolarity::White),
            candidate(CellCoords { i: 4, j: 3 }, CirclePolarity::Black),
            candidate(CellCoords { i: 4, j: 4 }, CirclePolarity::White),
            // The two cells a half-turn about the black circle needs.
            candidate(CellCoords { i: 5, j: 3 }, CirclePolarity::White),
            candidate(CellCoords { i: 4, j: 2 }, CirclePolarity::White),
        ];
        let search = resolve_board_frame(&layout(), &candidates, &params(3));
        assert!(search.ambiguous, "half-turn twin must be seen");
        assert_eq!(search.inliers, 3);
        assert_eq!(search.runner_up_inliers, 3);
        assert!(
            search.alignment.is_none(),
            "an ambiguous frame is not a frame"
        );
    }

    /// A single circle is consistent with all four rotations, so it can never
    /// determine an orientation however clean it looks.
    #[test]
    fn one_circle_never_determines_a_frame() {
        let candidates = vec![candidate(CellCoords { i: 3, j: 2 }, CirclePolarity::Black)];
        let search = resolve_board_frame(&layout(), &candidates, &params(3));
        assert_eq!(search.inliers, 1);
        assert!(search.ambiguous);
        assert!(search.alignment.is_none());
    }

    /// Why the sweep is restricted to rotations.
    ///
    /// This layout's two white circles are interchangeable, so a mirrored
    /// observation of it is the *same* relabelling as a quarter turn. Over the
    /// full dihedral group both would explain all three circles and the frame
    /// would be reported ambiguous — a legitimate detection lost to a
    /// hypothesis a camera imaging the printed side of an opaque board cannot
    /// produce. Over `C4` there is one answer.
    #[test]
    fn a_mirrored_observation_resolves_as_the_rotation_it_is() {
        let expected = layout();
        let candidates = vec![
            candidate(CellCoords { i: -3, j: 3 }, CirclePolarity::White),
            candidate(CellCoords { i: -4, j: 3 }, CirclePolarity::Black),
            candidate(CellCoords { i: -4, j: 4 }, CirclePolarity::White),
        ];
        let search = resolve_board_frame(&expected, &candidates, &params(3));
        let alignment = search.alignment.expect("frame resolved");
        assert_eq!(search.inliers, 3);
        assert!(!search.ambiguous);
        assert_eq!(
            alignment.determinant(),
            1,
            "the accepted frame is always a rotation"
        );
        for (spec, m) in expected.iter().zip(&search.matches) {
            let cell = candidates[m.matched_index.expect("matched")].cell;
            let mapped = alignment.apply(Coord::new(cell.i, cell.j));
            assert_eq!((mapped.u, mapped.v), (spec.cell.i, spec.cell.j));
        }
    }

    /// Same inputs, same answer, every time — the previous implementation
    /// broke ties by `HashMap` iteration order.
    #[test]
    fn frame_resolution_is_deterministic() {
        let candidates = vec![
            candidate(CellCoords { i: 2, j: 2 }, CirclePolarity::White),
            candidate(CellCoords { i: 3, j: 2 }, CirclePolarity::Black),
            candidate(CellCoords { i: 3, j: 3 }, CirclePolarity::White),
            candidate(CellCoords { i: 7, j: 7 }, CirclePolarity::White),
        ];
        let expected = layout();
        let first = resolve_board_frame(&expected, &candidates, &params(3));
        for _ in 0..64 {
            let again = resolve_board_frame(&expected, &candidates, &params(3));
            assert_eq!(again.alignment, first.alignment);
            assert_eq!(again.inliers, first.inliers);
            assert_eq!(again.runner_up_inliers, first.runner_up_inliers);
        }
    }
}
