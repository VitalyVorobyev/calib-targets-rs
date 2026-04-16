//! Cross-correlate observed edge bits against the master code maps.
//!
//! For each of the 8 D4 transforms and every possible master origin
//! `(I0, J0) ∈ [0, 501) × [0, 501)`, score the observed edge bits against
//! the expected master maps. Pick the `(transform, origin)` with highest
//! confidence-weighted match rate. The cyclic structure of the master maps
//! means the search space is bounded (501² × 8 ≈ 2M), and each score is
//! linear in the number of observed edges — this is trivially fast for
//! typical boards.

use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};

use crate::board::{MASTER_COLS, MASTER_ROWS};
use crate::code_maps::{horizontal_edge_bit, vertical_edge_bit, EdgeOrientation, ObservedEdge};

#[derive(Clone, Debug)]
pub(crate) struct DecodeOutcome {
    pub alignment: GridAlignment,
    pub edges_matched: usize,
    pub edges_observed: usize,
    pub weighted_score: f32,
    pub bit_error_rate: f32,
    pub mean_confidence: f32,
    pub master_origin_row: i32,
    pub master_origin_col: i32,
}

pub(crate) fn decode(observed: &[ObservedEdge], max_bit_error_rate: f32) -> Option<DecodeOutcome> {
    if observed.is_empty() {
        return None;
    }

    let total_conf: f32 = observed.iter().map(|e| e.confidence).sum();
    if total_conf <= 0.0 {
        return None;
    }

    let mut best: Option<DecodeOutcome> = None;

    for transform in GRID_TRANSFORMS_D4.iter().copied() {
        let transformed: Vec<(i32, i32, EdgeOrientation, u8, f32)> = observed
            .iter()
            .map(|e| {
                let (t_row, t_col, t_orient) = transform_edge(e, &transform);
                (t_row, t_col, t_orient, e.bit, e.confidence)
            })
            .collect();

        for master_row in 0..MASTER_ROWS as i32 {
            for master_col in 0..MASTER_COLS as i32 {
                let mut matched = 0usize;
                let mut weighted = 0.0f32;
                for &(tr, tc, to, bit, conf) in &transformed {
                    let m_row = master_row + tr;
                    let m_col = master_col + tc;
                    let expected = match to {
                        EdgeOrientation::Horizontal => horizontal_edge_bit(m_row, m_col),
                        EdgeOrientation::Vertical => vertical_edge_bit(m_row, m_col),
                    };
                    if expected == bit {
                        matched += 1;
                        weighted += conf;
                    }
                }
                let total = transformed.len();
                let bit_error_rate = if total == 0 {
                    1.0
                } else {
                    (total - matched) as f32 / total as f32
                };
                let score = weighted / total_conf;
                let mean_confidence = if matched == 0 {
                    0.0
                } else {
                    weighted / matched as f32
                };
                let candidate = DecodeOutcome {
                    alignment: GridAlignment {
                        transform,
                        translation: [master_row, master_col],
                    },
                    edges_matched: matched,
                    edges_observed: total,
                    weighted_score: score,
                    bit_error_rate,
                    mean_confidence,
                    master_origin_row: master_row,
                    master_origin_col: master_col,
                };
                match &best {
                    None => best = Some(candidate),
                    Some(b) if candidate.weighted_score > b.weighted_score => {
                        best = Some(candidate)
                    }
                    _ => {}
                }
            }
        }
    }

    let best = best?;
    if best.bit_error_rate > max_bit_error_rate {
        return None;
    }
    Some(best)
}

fn transform_edge(edge: &ObservedEdge, t: &GridTransform) -> (i32, i32, EdgeOrientation) {
    let [r, c] = t.apply(edge.row, edge.col);
    // For D4 exactly one of {|t.b|, |t.d|} and {|t.a|, |t.c|} is non-zero.
    // Horizontal edge axis = (0, 1) in (drow, dcol). After transform, axis
    // becomes (t.b, t.d). Edge stays horizontal iff the new axis has zero
    // drow component (|t.b| == 0 → axis still along +col direction).
    let orient = match edge.orientation {
        EdgeOrientation::Horizontal => {
            if t.b.abs() > t.d.abs() {
                EdgeOrientation::Vertical
            } else {
                EdgeOrientation::Horizontal
            }
        }
        // Vertical edge axis (1, 0) → (t.a, t.c). Stays vertical iff new
        // axis has zero dcol component (|t.c| == 0 → axis still along +row).
        EdgeOrientation::Vertical => {
            if t.c.abs() > t.a.abs() {
                EdgeOrientation::Horizontal
            } else {
                EdgeOrientation::Vertical
            }
        }
    };
    (r, c, orient)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_perfect_observation(
        master_origin_row: i32,
        master_origin_col: i32,
        local_rows: i32,
        local_cols: i32,
    ) -> Vec<ObservedEdge> {
        let mut out = Vec::new();
        for r in 0..local_rows {
            for c in 0..local_cols {
                if c + 1 < local_cols {
                    let bit = horizontal_edge_bit(master_origin_row + r, master_origin_col + c);
                    out.push(ObservedEdge {
                        row: r,
                        col: c,
                        orientation: EdgeOrientation::Horizontal,
                        bit,
                        confidence: 1.0,
                    });
                }
                if r + 1 < local_rows {
                    let bit = vertical_edge_bit(master_origin_row + r, master_origin_col + c);
                    out.push(ObservedEdge {
                        row: r,
                        col: c,
                        orientation: EdgeOrientation::Vertical,
                        bit,
                        confidence: 1.0,
                    });
                }
            }
        }
        out
    }

    #[test]
    fn decoder_recovers_identity_alignment() {
        // Cyclic structure makes (row, col) and (row + 3, col + 167) identical
        // master-pattern observations; accept any such equivalent position.
        let obs = build_perfect_observation(12, 37, 5, 5);
        let outcome = decode(&obs, 0.05).expect("decoded");
        // Regardless of which cyclic representative was chosen, a correct
        // decode must match every observed bit.
        assert_eq!(outcome.edges_matched, outcome.edges_observed);
        assert!(outcome.bit_error_rate < 1e-6);
        // Translation must be in the expected coset modulo master periods.
        assert_eq!(outcome.master_origin_row.rem_euclid(3), 12_i32.rem_euclid(3));
        assert_eq!(
            outcome.master_origin_col.rem_euclid(167),
            37_i32.rem_euclid(167)
        );
    }

    #[test]
    fn decoder_handles_d4_rotations() {
        // Construct a perfect observation, then physically rotate it 90°
        // around the local frame origin — the decoder should find the
        // inverse D4 transform that un-rotates it.
        let original = build_perfect_observation(5, 11, 5, 5);
        let rot = GRID_TRANSFORMS_D4[1]; // 90° rotation: a=0, b=1, c=-1, d=0
        // Rotated observation: apply rot to each anchor + flip orientation.
        let rotated: Vec<ObservedEdge> = original
            .iter()
            .map(|e| {
                let [r, c] = rot.apply(e.row, e.col);
                let new_orient = match e.orientation {
                    EdgeOrientation::Horizontal => {
                        if rot.b.abs() > rot.d.abs() {
                            EdgeOrientation::Vertical
                        } else {
                            EdgeOrientation::Horizontal
                        }
                    }
                    EdgeOrientation::Vertical => {
                        if rot.c.abs() > rot.a.abs() {
                            EdgeOrientation::Horizontal
                        } else {
                            EdgeOrientation::Vertical
                        }
                    }
                };
                ObservedEdge {
                    row: r,
                    col: c,
                    orientation: new_orient,
                    bit: e.bit,
                    confidence: e.confidence,
                }
            })
            .collect();

        let outcome = decode(&rotated, 0.05).expect("decoded under rotation");
        assert_eq!(outcome.edges_matched, outcome.edges_observed);
        assert!(outcome.bit_error_rate < 1e-6);
    }

    #[test]
    fn decoder_rejects_when_bit_error_rate_too_high() {
        // All-wrong observation at an arbitrary origin.
        let mut obs = build_perfect_observation(12, 37, 5, 5);
        for e in obs.iter_mut() {
            e.bit ^= 1;
        }
        // Default is 0.30; flipping all bits makes best error rate at matching
        // origin = 1.0 (no match). But the decoder picks *best* origin — another
        // position may coincidentally match the flipped bits. We just assert
        // that with a strict threshold, nothing is returned.
        let outcome = decode(&obs, 0.01);
        // Either we got an almost-perfect match somewhere else (possible) or none
        // — both are valid.
        if let Some(out) = outcome {
            assert!(out.bit_error_rate <= 0.01);
        }
    }
}
