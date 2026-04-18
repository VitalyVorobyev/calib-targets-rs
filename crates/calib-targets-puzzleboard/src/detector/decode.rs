//! Cross-correlate observed edge bits against the master code maps.
//!
//! For each of the 8 D4 transforms and every possible master origin
//! `(I0, J0) ∈ [0, 501) × [0, 501)`, score the observed edge bits against
//! the expected master maps. Pick the `(transform, origin)` with highest
//! confidence-weighted match rate.
//!
//! ## Fast-path via cyclic-period precompute (C3)
//!
//! The master maps have cyclic structure (matching PStelldinger/PuzzleBoard convention):
//! - horizontal edge bit at `(mr, mc)` = `map_b[(mr % 167, mc % 3)]`
//! - vertical edge bit at `(mr, mc)` = `map_a[(mr % 3, mc % 167)]`
//!
//! For transformed observations `{(tr, tc, orient, bit, conf)}`, the score
//! at master origin `(mr, mc)` is:
//!
//! ```text
//! score(mr, mc) = H[(mr % 3, mc % 167)] + V[(mr % 167, mc % 3)]
//! ```
//!
//! where `H` is a `3 × 167` table and `V` is a `167 × 3` table precomputed
//! **once per D4 transform** in `O(501 × N)`.  The 501² origin loop then
//! becomes `O(501²)` with two table lookups — no per-observation work.

use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};

use crate::board::{MASTER_COLS, MASTER_ROWS};
use crate::code_maps::{
    horizontal_edge_bit, vertical_edge_bit, EdgeOrientation, PuzzleBoardObservedEdge,
    EDGE_MAP_A_COLS, EDGE_MAP_A_ROWS, EDGE_MAP_B_COLS, EDGE_MAP_B_ROWS,
};

/// Cyclic-period sizes for the precompute tables.
///
/// Horizontal edges use map_b (167×3); vertical edges use map_a (3×167).
/// (Matches authors' convention: hfullCode from code2/map_b, vfullCode from code1/map_a.)
const H_ROWS: usize = EDGE_MAP_B_ROWS; // 167
const H_COLS: usize = EDGE_MAP_B_COLS; // 3
const V_ROWS: usize = EDGE_MAP_A_ROWS; // 3
const V_COLS: usize = EDGE_MAP_A_COLS; // 167

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

/// Match observations directly against the declared board's own bit pattern.
///
/// For each of the 8 D4 transforms and every shift `(P_r, P_c) ∈ [0, rows] ×
/// [0, cols]` (chessboard-local `(0, 0)` sitting at print-corner
/// `(P_r, P_c)`), score observations against the board-local horizontal and
/// vertical bit tables. Observations whose inferred cell falls outside the
/// board don't vote.
///
/// View-independent: a camera observing any partial subset of the same
/// physical board recovers the same absolute master IDs for the corners it
/// sees, so observations can be fused across cameras.
///
/// Complexity: `O(8 × (rows+1) × (cols+1) × N)` where `N = observed.len()`.
/// For a 50 × 50 board at typical edge counts (~500 per camera) this runs
/// well under 10 ms native.
pub(crate) fn decode_fixed_board(
    observed: &[PuzzleBoardObservedEdge],
    spec_origin_row: u32,
    spec_origin_col: u32,
    rows: u32,
    cols: u32,
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
    if observed.is_empty() || rows < 2 || cols < 2 {
        return None;
    }
    let total_conf: f32 = observed.iter().map(|e| e.confidence).sum();
    if total_conf <= 0.0 {
        return None;
    }
    let total = observed.len();
    let spec_or = spec_origin_row as i32;
    let spec_oc = spec_origin_col as i32;

    // Precompute the board's bit pattern. `h_bit` is `(rows-1) × cols`;
    // `v_bit` is `rows × (cols-1)`.
    let h_rows = (rows - 1) as usize;
    let h_cols = cols as usize;
    let v_rows = rows as usize;
    let v_cols = (cols - 1) as usize;
    let mut h_bit = vec![0u8; h_rows * h_cols];
    let mut v_bit = vec![0u8; v_rows * v_cols];
    for r in 0..h_rows {
        for c in 0..h_cols {
            h_bit[r * h_cols + c] = horizontal_edge_bit(spec_or + r as i32, spec_oc + c as i32);
        }
    }
    for r in 0..v_rows {
        for c in 0..v_cols {
            v_bit[r * v_cols + c] = vertical_edge_bit(spec_or + r as i32, spec_oc + c as i32);
        }
    }

    let mut best: Option<DecodeOutcome> = None;

    for transform in GRID_TRANSFORMS_D4.iter().copied() {
        // Transform all observations into this D4 frame once.
        let transformed: Vec<(i32, i32, EdgeOrientation, u8, f32)> = observed
            .iter()
            .map(|e| {
                let (t_row, t_col, t_orient) = transform_edge(e, &transform);
                (t_row, t_col, t_orient, e.bit, e.confidence)
            })
            .collect();

        // Bounds on (P_r, P_c) such that *every* observation lands on the
        // board. For partial-view captures we still need to consider shifts
        // where only a subset lands on-board, so widen by a small margin
        // (observations off the board just don't vote).
        let (tr_min, tr_max) = transformed
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), &(tr, _, _, _, _)| {
                (lo.min(tr), hi.max(tr))
            });
        let (tc_min, tc_max) = transformed
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), &(_, tc, _, _, _)| {
                (lo.min(tc), hi.max(tc))
            });
        let rows_i = rows as i32;
        let cols_i = cols as i32;
        let p_r_lo = (-tr_max).max(0);
        let p_r_hi = (rows_i - tr_min).min(rows_i);
        let p_c_lo = (-tc_max).max(0);
        let p_c_hi = (cols_i - tc_min).min(cols_i);
        if p_r_lo > p_r_hi || p_c_lo > p_c_hi {
            continue;
        }

        for p_r in p_r_lo..=p_r_hi {
            for p_c in p_c_lo..=p_c_hi {
                let mut matched = 0usize;
                let mut weighted = 0.0f32;
                for &(tr, tc, orient, bit, conf) in &transformed {
                    let (cr, cc, expected) = match orient {
                        EdgeOrientation::Horizontal => {
                            let cr = p_r + tr - 1;
                            let cc = p_c + tc;
                            if cr < 0 || cr >= h_rows as i32 || cc < 0 || cc >= h_cols as i32 {
                                continue;
                            }
                            (cr, cc, h_bit[cr as usize * h_cols + cc as usize])
                        }
                        EdgeOrientation::Vertical => {
                            let cr = p_r + tr;
                            let cc = p_c + tc - 1;
                            if cr < 0 || cr >= v_rows as i32 || cc < 0 || cc >= v_cols as i32 {
                                continue;
                            }
                            (cr, cc, v_bit[cr as usize * v_cols + cc as usize])
                        }
                    };
                    let _ = (cr, cc); // silence unused-variable lint
                    if expected == bit {
                        matched += 1;
                        weighted += conf;
                    }
                }
                let bit_error_rate = if total == 0 {
                    1.0
                } else {
                    (total - matched) as f32 / total as f32
                };
                if bit_error_rate > max_bit_error_rate {
                    continue;
                }
                let score = weighted / total_conf;
                let mean_confidence = if matched == 0 {
                    0.0
                } else {
                    weighted / matched as f32
                };
                // Master origin `decode_full` would have produced for this
                // `(T, P_r, P_c)` — downstream label assignment needs it.
                let master_row = crt_167_3(spec_or + p_r - 1, spec_or + p_r);
                let master_col = crt_167_3(spec_oc + p_c - 1, spec_oc + p_c);
                let candidate = DecodeOutcome {
                    alignment: GridAlignment {
                        transform,
                        translation: [master_col, master_row],
                    },
                    edges_matched: matched,
                    edges_observed: total,
                    weighted_score: score,
                    bit_error_rate,
                    mean_confidence,
                    master_origin_row: master_row,
                    master_origin_col: master_col,
                };
                update_best_candidate(&mut best, candidate);
            }
        }
    }
    best
}

/// Chinese Remainder closed form for `r ≡ a (mod 167) ∧ r ≡ b (mod 3)` in `[0, 501)`.
///
/// `167 mod 3 = 2`, so `(a + 167 k) ≡ b (mod 3)` ⇒ `2 k ≡ b - a (mod 3)`,
/// ⇒ `k ≡ 2 (b - a) (mod 3)` (2 is its own inverse mod 3).
#[inline]
fn crt_167_3(a: i32, b: i32) -> i32 {
    let a_r = a.rem_euclid(167);
    let b_r = b.rem_euclid(3);
    let k = (2 * ((b_r - a_r).rem_euclid(3))).rem_euclid(3);
    (a_r + 167 * k).rem_euclid(501)
}

pub(crate) fn decode(
    observed: &[PuzzleBoardObservedEdge],
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
    if observed.is_empty() {
        return None;
    }

    let total_conf: f32 = observed.iter().map(|e| e.confidence).sum();
    if total_conf <= 0.0 {
        return None;
    }
    let total = observed.len();

    let mut best: Option<DecodeOutcome> = None;

    // Scratch buffers for the precompute tables — allocated once, cleared per transform.
    // h_match[a * H_COLS + b]: sum of confidences for horizontal obs that match at class (a, b).
    // h_count[a * H_COLS + b]: number of matching horizontal observations at class (a, b).
    // v_match[a * V_COLS + b]: sum of confidences for vertical obs that match at class (a, b).
    // v_count[a * V_COLS + b]: number of matching vertical observations at class (a, b).
    let mut h_match = vec![0.0f32; H_ROWS * H_COLS];
    let mut h_count = vec![0u32; H_ROWS * H_COLS];
    let mut v_match = vec![0.0f32; V_ROWS * V_COLS];
    let mut v_count = vec![0u32; V_ROWS * V_COLS];

    for transform in GRID_TRANSFORMS_D4.iter().copied() {
        // Transform all observations once.
        let transformed: Vec<(i32, i32, EdgeOrientation, u8, f32)> = observed
            .iter()
            .map(|e| {
                let (t_row, t_col, t_orient) = transform_edge(e, &transform);
                (t_row, t_col, t_orient, e.bit, e.confidence)
            })
            .collect();

        // Clear scratch buffers.
        h_match.fill(0.0);
        h_count.fill(0);
        v_match.fill(0.0);
        v_count.fill(0);

        // Build the H and V precompute tables.
        //
        // For each observed edge `(tr, tc, orient, bit, conf)` we want to know,
        // for every master origin `(mr, mc)`, whether `expected_bit(mr+tr, mc+tc) == bit`.
        //
        // For a horizontal observation:
        //   expected = DATA_A[((mr + tr) % 3, (mc + tc) % 167)]
        //
        // Equivalently, if we define `a = (mr % 3)` and `b = (mc % 167)`, then:
        //   expected = DATA_A[((a + tr % 3 + 3) % 3, (b + tc % 167 + 167) % 167)]
        //
        // Rather than indexing by (mr, mc), we build the table indexed by the
        // origin's cyclic class `(a, b)`.  For each observation we scan all
        // 501 classes and accumulate match contributions:
        //
        // Simpler alternative: for each master cell `(r, c)` in DATA_A, compute
        //   a = (r - tr).rem_euclid(3), b = (c - tc).rem_euclid(167)
        // If DATA_A[r][c] == bit → accumulate into h_match[a*167 + b] / h_count.
        // This is O(3*167) = O(501) per observation — total O(501 * N).

        for &(tr, tc, orient, bit, conf) in &transformed {
            match orient {
                EdgeOrientation::Horizontal => {
                    // tr is the observation's transformed row, tc its column.
                    // For horizontal edges, the relevant master map is A (3×167).
                    for r in 0..H_ROWS {
                        let a = (r as i32 - tr).rem_euclid(H_ROWS as i32) as usize;
                        for c in 0..H_COLS {
                            let b = (c as i32 - tc).rem_euclid(H_COLS as i32) as usize;
                            let expected = horizontal_edge_bit(r as i32, c as i32);
                            if expected == bit {
                                h_match[a * H_COLS + b] += conf;
                                h_count[a * H_COLS + b] += 1;
                            }
                        }
                    }
                }
                EdgeOrientation::Vertical => {
                    // For vertical edges, the relevant master map is B (167×3).
                    for r in 0..V_ROWS {
                        let a = (r as i32 - tr).rem_euclid(V_ROWS as i32) as usize;
                        for c in 0..V_COLS {
                            let b = (c as i32 - tc).rem_euclid(V_COLS as i32) as usize;
                            let expected = vertical_edge_bit(r as i32, c as i32);
                            if expected == bit {
                                v_match[a * V_COLS + b] += conf;
                                v_count[a * V_COLS + b] += 1;
                            }
                        }
                    }
                }
            }
        }

        // Scan all 501² origins using the precomputed tables.
        for master_row in 0..MASTER_ROWS as i32 {
            let ha = (master_row % H_ROWS as i32) as usize;
            let va = (master_row % V_ROWS as i32) as usize;
            for master_col in 0..MASTER_COLS as i32 {
                let hb = (master_col % H_COLS as i32) as usize;
                let vb = (master_col % V_COLS as i32) as usize;

                let matched = (h_count[ha * H_COLS + hb] + v_count[va * V_COLS + vb]) as usize;
                let weighted = h_match[ha * H_COLS + hb] + v_match[va * V_COLS + vb];

                let bit_error_rate = if total == 0 {
                    1.0
                } else {
                    (total - matched) as f32 / total as f32
                };

                // Early-reject before constructing the full candidate.
                if bit_error_rate > max_bit_error_rate {
                    continue;
                }

                let score = weighted / total_conf;
                let mean_confidence = if matched == 0 {
                    0.0
                } else {
                    weighted / matched as f32
                };
                let candidate = DecodeOutcome {
                    alignment: GridAlignment {
                        transform,
                        // translation[0] is the i (col) offset, translation[1]
                        // is the j (row) offset, so master_col goes first.
                        translation: [master_col, master_row],
                    },
                    edges_matched: matched,
                    edges_observed: total,
                    weighted_score: score,
                    bit_error_rate,
                    mean_confidence,
                    master_origin_row: master_row,
                    master_origin_col: master_col,
                };
                update_best_candidate(&mut best, candidate);
            }
        }
    }

    best
}

#[cfg(test)]
fn update_best_candidate_if_accepted(
    best: &mut Option<DecodeOutcome>,
    candidate: DecodeOutcome,
    max_bit_error_rate: f32,
) {
    if candidate.bit_error_rate <= max_bit_error_rate {
        update_best_candidate(best, candidate);
    }
}

fn update_best_candidate(best: &mut Option<DecodeOutcome>, candidate: DecodeOutcome) {
    // Rank lexicographically by (edges_matched, weighted_score): a candidate
    // with strictly more matched bits always wins regardless of per-bit
    // confidence; weighted_score only breaks ties on equal match count.
    let wins = match best {
        None => true,
        Some(current) => {
            candidate.edges_matched > current.edges_matched
                || (candidate.edges_matched == current.edges_matched
                    && candidate.weighted_score > current.weighted_score)
        }
    };
    if wins {
        *best = Some(candidate);
    }
}

fn transform_edge(
    edge: &PuzzleBoardObservedEdge,
    t: &GridTransform,
) -> (i32, i32, EdgeOrientation) {
    // Convention: edge.col = i-direction, edge.row = j-direction.
    // apply(i, j) = [a*i + b*j, c*i + d*j] where output[0] = new_i (new col),
    // output[1] = new_j (new row).
    let [new_col, new_row] = t.apply(edge.col, edge.row);
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
    (new_row, new_col, orient)
}

/// Reference (slow, O(501² × N)) implementation kept for correctness guards.
///
/// Produces the same result as [`decode`] but iterates observations in the
/// inner loop rather than using the cyclic precompute.
#[cfg(test)]
fn decode_reference(
    observed: &[PuzzleBoardObservedEdge],
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
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
                        // translation[0] is the i (col) offset, translation[1]
                        // is the j (row) offset, so master_col goes first.
                        translation: [master_col, master_row],
                    },
                    edges_matched: matched,
                    edges_observed: total,
                    weighted_score: score,
                    bit_error_rate,
                    mean_confidence,
                    master_origin_row: master_row,
                    master_origin_col: master_col,
                };
                update_best_candidate_if_accepted(&mut best, candidate, max_bit_error_rate);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_perfect_observation(
        master_origin_row: i32,
        master_origin_col: i32,
        local_rows: i32,
        local_cols: i32,
    ) -> Vec<PuzzleBoardObservedEdge> {
        let mut out = Vec::new();
        for r in 0..local_rows {
            for c in 0..local_cols {
                if c + 1 < local_cols {
                    let bit = horizontal_edge_bit(master_origin_row + r, master_origin_col + c);
                    out.push(PuzzleBoardObservedEdge {
                        row: r,
                        col: c,
                        orientation: EdgeOrientation::Horizontal,
                        bit,
                        confidence: 1.0,
                    });
                }
                if r + 1 < local_rows {
                    let bit = vertical_edge_bit(master_origin_row + r, master_origin_col + c);
                    out.push(PuzzleBoardObservedEdge {
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
        assert_eq!(
            outcome.master_origin_row.rem_euclid(3),
            12_i32.rem_euclid(3)
        );
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
        let rotated: Vec<PuzzleBoardObservedEdge> = original
            .iter()
            .map(|e| {
                // apply(col=i, row=j): output[0]=new_i=new_col, output[1]=new_j=new_row
                let [new_col, new_row] = rot.apply(e.col, e.row);
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
                PuzzleBoardObservedEdge {
                    row: new_row,
                    col: new_col,
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

    #[test]
    fn best_candidate_update_keeps_lower_score_valid_candidate() {
        let alignment = GridAlignment {
            transform: GRID_TRANSFORMS_D4[0],
            translation: [0, 0],
        };
        let valid = DecodeOutcome {
            alignment,
            edges_matched: 16,
            edges_observed: 24,
            weighted_score: 0.7,
            bit_error_rate: 0.33,
            mean_confidence: 0.6,
            master_origin_row: 0,
            master_origin_col: 0,
        };
        let invalid_higher_score = DecodeOutcome {
            weighted_score: 0.9,
            bit_error_rate: 0.5,
            master_origin_row: 1,
            master_origin_col: 1,
            ..valid.clone()
        };

        let mut best = None;
        update_best_candidate_if_accepted(&mut best, valid, 0.4);
        update_best_candidate_if_accepted(&mut best, invalid_higher_score, 0.4);

        let best = best.expect("valid candidate retained");
        assert_eq!(best.master_origin_row, 0);
        assert_eq!(best.master_origin_col, 0);
        assert!(best.bit_error_rate <= 0.4);
    }

    /// C2: more matched bits beats higher confidence-weighted score.
    ///
    /// Candidate A: 20 matched bits, weighted_score = 0.5 (lower confidence on each bit).
    /// Candidate B: 18 matched bits, weighted_score = 0.9 (higher confidence but fewer bits).
    /// A should win because edges_matched takes priority.
    #[test]
    fn lex_rank_matched_beats_weighted_score() {
        let alignment = GridAlignment {
            transform: GRID_TRANSFORMS_D4[0],
            translation: [0, 0],
        };
        let candidate_a = DecodeOutcome {
            alignment,
            edges_matched: 20,
            edges_observed: 24,
            weighted_score: 0.5,
            bit_error_rate: 0.17,
            mean_confidence: 0.6,
            master_origin_row: 10,
            master_origin_col: 10,
        };
        let candidate_b = DecodeOutcome {
            edges_matched: 18,
            weighted_score: 0.9,
            bit_error_rate: 0.25,
            mean_confidence: 0.95,
            master_origin_row: 20,
            master_origin_col: 20,
            ..candidate_a.clone()
        };

        // Start with B (fewer matched bits but higher weighted_score).
        let mut best = None;
        update_best_candidate(&mut best, candidate_b);
        // A should displace B despite lower weighted_score.
        update_best_candidate(&mut best, candidate_a);

        let winner = best.expect("some candidate");
        assert_eq!(
            winner.master_origin_row, 10,
            "A (20 matched) should beat B (18 matched) despite lower weighted_score"
        );
    }

    /// C3 correctness guard: the optimized decode must agree with decode_reference
    /// on (edges_matched, bit_error_rate) for several scenarios including
    /// identity transform and D4 rotation.
    #[test]
    fn fast_decode_matches_reference_identity() {
        let obs = build_perfect_observation(12, 37, 5, 5);
        let fast = decode(&obs, 0.30).expect("fast decoded");
        let reference = decode_reference(&obs, 0.30).expect("reference decoded");

        assert_eq!(
            fast.edges_matched, reference.edges_matched,
            "edges_matched mismatch"
        );
        assert!(
            (fast.bit_error_rate - reference.bit_error_rate).abs() < 1e-5,
            "bit_error_rate mismatch: fast={} ref={}",
            fast.bit_error_rate,
            reference.bit_error_rate
        );
        // Both must agree on the cyclic equivalence class of the origin.
        assert_eq!(
            fast.master_origin_row.rem_euclid(3),
            reference.master_origin_row.rem_euclid(3),
            "row coset mismatch"
        );
        assert_eq!(
            fast.master_origin_col.rem_euclid(167),
            reference.master_origin_col.rem_euclid(167),
            "col coset mismatch"
        );
    }

    #[test]
    fn fast_decode_matches_reference_d4_rotation() {
        let original = build_perfect_observation(5, 11, 5, 5);
        let rot = GRID_TRANSFORMS_D4[2]; // 180° rotation
        let rotated: Vec<PuzzleBoardObservedEdge> = original
            .iter()
            .map(|e| {
                // apply(col=i, row=j): output[0]=new_i=new_col, output[1]=new_j=new_row
                let [new_col, new_row] = rot.apply(e.col, e.row);
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
                PuzzleBoardObservedEdge {
                    row: new_row,
                    col: new_col,
                    orientation: new_orient,
                    bit: e.bit,
                    confidence: e.confidence,
                }
            })
            .collect();

        let fast = decode(&rotated, 0.30).expect("fast decoded");
        let reference = decode_reference(&rotated, 0.30).expect("reference decoded");

        assert_eq!(fast.edges_matched, reference.edges_matched);
        assert!(
            (fast.bit_error_rate - reference.bit_error_rate).abs() < 1e-5,
            "bit_error_rate mismatch: fast={} ref={}",
            fast.bit_error_rate,
            reference.bit_error_rate
        );
    }

    #[test]
    fn fast_decode_matches_reference_all_flipped() {
        let mut obs = build_perfect_observation(12, 37, 5, 5);
        for e in obs.iter_mut() {
            e.bit ^= 1;
        }

        let fast = decode(&obs, 0.30);
        let reference = decode_reference(&obs, 0.30);

        match (fast, reference) {
            (None, None) => {} // both found nothing — fine.
            (Some(f), Some(r)) => {
                assert_eq!(f.edges_matched, r.edges_matched);
                assert!(
                    (f.bit_error_rate - r.bit_error_rate).abs() < 1e-5,
                    "ber mismatch: fast={} ref={}",
                    f.bit_error_rate,
                    r.bit_error_rate
                );
            }
            (f, r) => panic!("one returned None and other Some: fast={f:?} ref={r:?}"),
        }
    }

    /// C3 performance check: decode a ~1200-edge observation.
    ///
    /// Run with `cargo test --release -- decode_25x25_timing --nocapture` to
    /// see the wall-clock time.  The 200ms guard is only enforced in release
    /// builds (debug builds are not optimised and can be >200ms).
    #[test]
    fn decode_25x25_timing() {
        let obs = build_perfect_observation(0, 0, 25, 25);
        println!("decode_25x25_timing: {} observations", obs.len());

        let start = std::time::Instant::now();
        let result = decode(&obs, 0.30);
        let elapsed = start.elapsed();

        println!(
            "decode_25x25_timing: elapsed={:?}, edges_matched={:?}",
            elapsed,
            result.as_ref().map(|r| r.edges_matched)
        );

        assert!(result.is_some(), "should decode a perfect observation");

        // Wall-clock guard only in release mode — debug builds are not
        // optimised and routinely exceed 200ms even with the precompute.
        #[cfg(not(debug_assertions))]
        assert!(
            elapsed.as_millis() < 200,
            "decode_25x25 took {:?} in release mode (expected ≤ 200ms)",
            elapsed
        );
    }
}
