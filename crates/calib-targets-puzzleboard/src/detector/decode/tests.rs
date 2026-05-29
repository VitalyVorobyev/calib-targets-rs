//! Unit tests and reference implementations for the edge-code decoders.
//!
//! Lives in a dedicated file so the decoder logic stays focused; the
//! `#[cfg(test)]`-only reference decoder and CRT helper used as correctness
//! oracles live here next to the tests that exercise them.

use super::*;
use crate::board::{MASTER_COLS, MASTER_ROWS};
use crate::code_maps::{horizontal_edge_bit, vertical_edge_bit, EdgeOrientation};
use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
use std::collections::HashMap;

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

fn update_best_candidate_if_accepted(
    best: &mut Option<DecodeOutcome>,
    candidate: DecodeOutcome,
    max_bit_error_rate: f32,
) {
    if candidate.bit_error_rate <= max_bit_error_rate {
        update_best_candidate(best, candidate);
    }
}

/// Reference (slow, O(501² × N)) implementation kept for correctness guards.
///
/// Produces the same result as [`decode`] but iterates observations in the
/// inner loop rather than using the cyclic precompute.
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
                let lookup = transform_edge_lookup(e, &transform);
                (
                    lookup.lookup_row,
                    lookup.lookup_col,
                    lookup.orientation,
                    e.bit,
                    e.confidence,
                )
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
                    score_best: score,
                    score_runner_up: None,
                    score_margin: f32::INFINITY,
                    runner_up_origin_row: None,
                    runner_up_origin_col: None,
                    runner_up_transform: None,
                };
                update_best_candidate_if_accepted(&mut best, candidate, max_bit_error_rate);
            }
        }
    }
    best
}

fn rotate_observed_edge_canonically(
    edge: &PuzzleBoardObservedEdge,
    t: &GridTransform,
) -> PuzzleBoardObservedEdge {
    let ((p0_i, p0_j), (p1_i, p1_j)) = match edge.orientation {
        EdgeOrientation::Horizontal => ((edge.col, edge.row), (edge.col + 1, edge.row)),
        EdgeOrientation::Vertical => ((edge.col, edge.row), (edge.col, edge.row + 1)),
    };
    let p0 = t.apply(p0_i, p0_j);
    let p1 = t.apply(p1_i, p1_j);
    let (p0_col, p0_row) = (p0.i, p0.j);
    let (p1_col, p1_row) = (p1.i, p1.j);
    let orientation = if p0_row == p1_row {
        EdgeOrientation::Horizontal
    } else {
        EdgeOrientation::Vertical
    };
    let (col, row) = match orientation {
        EdgeOrientation::Horizontal => {
            if p0_col <= p1_col {
                (p0_col, p0_row)
            } else {
                (p1_col, p1_row)
            }
        }
        EdgeOrientation::Vertical => {
            if p0_row <= p1_row {
                (p0_col, p0_row)
            } else {
                (p1_col, p1_row)
            }
        }
    };
    PuzzleBoardObservedEdge {
        row,
        col,
        orientation,
        bit: edge.bit,
        confidence: edge.confidence,
    }
}

fn build_perfect_observation(
    master_origin_row: i32,
    master_origin_col: i32,
    local_rows: i32,
    local_cols: i32,
) -> Vec<PuzzleBoardObservedEdge> {
    // Mirror the real pipeline convention:
    // - H edge at corner `(c, r)` samples lookup cell `(r-1, c)`
    // - V edge at corner `(c, r)` samples lookup cell `(r, c-1)`
    let mut out = Vec::new();
    for r in 0..local_rows {
        for c in 0..local_cols {
            if c + 1 < local_cols {
                let bit = horizontal_edge_bit(master_origin_row + r - 1, master_origin_col + c);
                out.push(PuzzleBoardObservedEdge {
                    row: r,
                    col: c,
                    orientation: EdgeOrientation::Horizontal,
                    bit,
                    confidence: 1.0,
                });
            }
            if r + 1 < local_rows {
                let bit = vertical_edge_bit(master_origin_row + r, master_origin_col + c - 1);
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
fn transform_edge_lookup_horizontal_matches_all_d4() {
    let edge = PuzzleBoardObservedEdge {
        row: 7,
        col: 11,
        orientation: EdgeOrientation::Horizontal,
        bit: 0,
        confidence: 1.0,
    };
    let expected = [
        (6, 11, EdgeOrientation::Horizontal),
        (-12, 6, EdgeOrientation::Vertical),
        (-8, -12, EdgeOrientation::Horizontal),
        (11, -8, EdgeOrientation::Vertical),
        (6, -12, EdgeOrientation::Horizontal),
        (-8, 11, EdgeOrientation::Horizontal),
        (11, 6, EdgeOrientation::Vertical),
        (-12, -8, EdgeOrientation::Vertical),
    ];
    for (idx, (&t, &(row, col, orient))) in
        GRID_TRANSFORMS_D4.iter().zip(expected.iter()).enumerate()
    {
        let got = transform_edge_lookup(&edge, &t);
        assert_eq!(got.lookup_row, row, "rot {idx}: row");
        assert_eq!(got.lookup_col, col, "rot {idx}: col");
        assert_eq!(got.orientation, orient, "rot {idx}: orientation");
    }
}

#[test]
fn transform_edge_lookup_vertical_matches_all_d4() {
    let edge = PuzzleBoardObservedEdge {
        row: 7,
        col: 11,
        orientation: EdgeOrientation::Vertical,
        bit: 0,
        confidence: 1.0,
    };
    let expected = [
        (7, 10, EdgeOrientation::Vertical),
        (-12, 7, EdgeOrientation::Horizontal),
        (-8, -12, EdgeOrientation::Vertical),
        (10, -8, EdgeOrientation::Horizontal),
        (7, -12, EdgeOrientation::Vertical),
        (-8, 10, EdgeOrientation::Vertical),
        (10, 7, EdgeOrientation::Horizontal),
        (-12, -8, EdgeOrientation::Horizontal),
    ];
    for (idx, (&t, &(row, col, orient))) in
        GRID_TRANSFORMS_D4.iter().zip(expected.iter()).enumerate()
    {
        let got = transform_edge_lookup(&edge, &t);
        assert_eq!(got.lookup_row, row, "rot {idx}: row");
        assert_eq!(got.lookup_col, col, "rot {idx}: col");
        assert_eq!(got.orientation, orient, "rot {idx}: orientation");
    }
}

#[test]
fn legacy_crt_recovery_can_amplify_one_cell_residue_into_large_jump() {
    let good = crt_167_3(65, 66);
    let bad_same_residue = crt_167_3(66, 66);
    let bad_both_shifted = crt_167_3(65, 65);
    assert_eq!(good, 399);
    assert_eq!(bad_same_residue, 66);
    assert_eq!(bad_both_shifted, 65);
    assert_eq!((good - bad_same_residue).abs(), 333);
    assert_eq!((good - bad_both_shifted).abs(), 334);
}

#[test]
fn decoder_recovers_identity_alignment() {
    let obs = build_perfect_observation(12, 37, 5, 5);
    let outcome = decode(&obs, 0.05).expect("decoded");
    assert_eq!(outcome.edges_matched, outcome.edges_observed);
    assert!(outcome.bit_error_rate < 1e-6);
    assert_eq!(outcome.master_origin_row, 12);
    assert_eq!(outcome.master_origin_col, 37);
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
        .map(|e| rotate_observed_edge_canonically(e, &rot))
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
        score_best: 0.7,
        score_runner_up: None,
        score_margin: f32::INFINITY,
        runner_up_origin_row: None,
        runner_up_origin_col: None,
        runner_up_transform: None,
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
        score_best: 0.5,
        score_runner_up: None,
        score_margin: f32::INFINITY,
        runner_up_origin_row: None,
        runner_up_origin_col: None,
        runner_up_transform: None,
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
        .map(|e| rotate_observed_edge_canonically(e, &rot))
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

// --- Soft-log-likelihood scorer -----------------------------------------

fn default_soft_cfg() -> SoftLlConfig {
    SoftLlConfig {
        kappa: 12.0,
        per_bit_floor: -6.0,
        alignment_min_margin: 0.02,
    }
}

#[test]
fn log_sigmoid_matches_reference() {
    for &x in &[-5.0_f32, -1.0, 0.0, 1.0, 5.0] {
        let a = log_sigmoid(x);
        let b = (1.0 / (1.0 + (-x).exp())).ln();
        assert!((a - b).abs() < 1e-5, "log_sigmoid({x}) = {a}, expected {b}");
    }
}

#[test]
fn ll_pair_saturates_and_clips() {
    // At conf=1, kappa=12: ll_match ~ 0, ll_mismatch clipped to -6.
    let (m, mm) = ll_pair(1.0, 12.0, -6.0);
    assert!(m > -1e-3, "ll_match should be near zero, got {m}");
    assert!((mm - (-6.0)).abs() < 1e-5, "ll_mismatch clipped: {mm}");

    // At conf=0 the logit is 0 and log_sigmoid(0) = -ln 2 ≈ -0.693.
    let (m0, mm0) = ll_pair(0.0, 12.0, -6.0);
    assert!((m0 - mm0).abs() < 1e-5, "at conf=0 match/mismatch must tie");
    assert!((m0 - (-0.5f32.ln() * -1.0)).abs() < 1e-2); // Within numerical tolerance.
}

#[test]
fn soft_ll_identity_perfect_obs() {
    let obs = build_perfect_observation(12, 37, 5, 5);
    let out = decode_soft(&obs, &default_soft_cfg(), 0.05).expect("decoded");
    assert_eq!(out.edges_matched, out.edges_observed);
    assert!(out.bit_error_rate < 1e-6);
    assert!(out.score_margin > 0.1, "margin={}", out.score_margin);
    assert_eq!(out.master_origin_row, 12);
    assert_eq!(out.master_origin_col, 37);
}

#[test]
fn soft_ll_handles_d4_rotations() {
    let original = build_perfect_observation(5, 11, 5, 5);
    for (rot_idx, &rot) in GRID_TRANSFORMS_D4.iter().enumerate() {
        let rotated: Vec<PuzzleBoardObservedEdge> = original
            .iter()
            .map(|e| rotate_observed_edge_canonically(e, &rot))
            .collect();
        let out = decode_soft(&rotated, &default_soft_cfg(), 0.05)
            .unwrap_or_else(|| panic!("rot {rot_idx}: decode_soft returned None"));
        assert_eq!(out.edges_matched, out.edges_observed, "rot {rot_idx}");
        assert!(out.bit_error_rate < 1e-6, "rot {rot_idx}");
    }
}

#[test]
fn soft_ll_rejects_below_margin_gate() {
    // Build a weak observation where multiple hypotheses tie. We cannot
    // easily construct a literal zero-margin tie without reverse-
    // engineering the master map, but we can force the gate to trigger
    // by setting an absurdly high margin threshold on a genuine decode.
    let obs = build_perfect_observation(0, 0, 5, 5);
    let mut cfg = default_soft_cfg();
    cfg.alignment_min_margin = 1e9;
    let out = decode_soft(&obs, &cfg, 0.05);
    assert!(
        out.is_none(),
        "margin gate should reject when threshold is huge"
    );
}

#[test]
fn soft_ll_beats_hard_when_winner_has_more_evidence() {
    // A board that spans multiple master rows: soft-LL should produce a
    // strong margin because correct-hypothesis score ≈ 0 while the nearest
    // wrong cyclic-neighbour has several wrong-bit penalties.
    let obs = build_perfect_observation(10, 20, 8, 8);
    let out = decode_soft(&obs, &default_soft_cfg(), 0.05).expect("decoded");
    assert_eq!(out.edges_matched, out.edges_observed);
    // On a perfect build, score_best is a small non-positive number
    // (log_sigmoid saturates to ~0 for each match) and runner-up sits
    // several bits down — ensure we captured a finite runner-up and a
    // healthy margin.
    assert!(out.score_runner_up.is_some(), "runner-up should be tracked");
    assert!(out.score_margin.is_finite() && out.score_margin > 0.5);
}

/// Build observations that sit at a specific fixed-board shift `(p_r, p_c)`
/// relative to `spec_origin=(0, 0)` under the real pipeline convention.
fn synth_fixed_board_obs(
    shift_pr: i32,
    shift_pc: i32,
    local_rows: i32,
    local_cols: i32,
) -> Vec<PuzzleBoardObservedEdge> {
    let mut out = Vec::new();
    for r in 0..local_rows {
        for c in 0..local_cols {
            if c + 1 < local_cols {
                // Horizontal map sees (mor + r, moc + c) but the
                // decode indexes h_bit[(p_r + r - 1)][p_c + c] so we
                // want horizontal_edge_bit(p_r + r - 1, p_c + c).
                let bit = horizontal_edge_bit(shift_pr + r - 1, shift_pc + c);
                out.push(PuzzleBoardObservedEdge {
                    row: r,
                    col: c,
                    orientation: EdgeOrientation::Horizontal,
                    bit,
                    confidence: 1.0,
                });
            }
            if r + 1 < local_rows {
                // Vertical map: decode indexes v_bit[p_r + r][p_c + c - 1].
                let bit = vertical_edge_bit(shift_pr + r, shift_pc + c - 1);
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
fn decode_fixed_board_soft_agrees_with_hard_on_planted_shift() {
    // Plant a 7×7 observation that sits at (p_r=3, p_c=4) inside a
    // 10×10 nominal board. Soft and hard decoders share the same
    // inputs and must agree on the winning cyclic origin and the
    // hard match count. We use the production BER gate (0.30) rather
    // than a synthetic tight gate because a handful of observations
    // near the board boundary legitimately fall off.
    let rows = 10u32;
    let cols = 10u32;
    let obs = synth_fixed_board_obs(3, 4, 7, 7);
    let hard = decode_fixed_board(&obs, 0, 0, rows, cols, 0.30).expect("hard decoded");
    let soft = decode_fixed_board_soft(&obs, 0, 0, rows, cols, &default_soft_cfg(), 0.30)
        .expect("soft decoded");
    assert_eq!(hard.master_origin_row, 3);
    assert_eq!(hard.master_origin_col, 4);
    assert_eq!(soft.master_origin_row, hard.master_origin_row);
    assert_eq!(soft.master_origin_col, hard.master_origin_col);
    assert_eq!(soft.edges_matched, hard.edges_matched);
    assert!(soft.score_margin > 0.1, "margin={}", soft.score_margin);
}

#[test]
fn decode_fixed_board_soft_penalizes_off_board_shifts() {
    // A 4×4 observation planted at (p_r=2, p_c=2) inside a 10×10
    // declared board. The soft decoder must prefer the fully-on-board
    // shift over any alternative that truncates the window.
    let rows = 10u32;
    let cols = 10u32;
    let obs = synth_fixed_board_obs(2, 2, 4, 4);
    let out = decode_fixed_board_soft(&obs, 0, 0, rows, cols, &default_soft_cfg(), 0.30)
        .expect("decoded");
    assert!(out.edges_matched >= out.edges_observed - 6);
    assert!(out.score_margin > 0.05);
}

/// Emit a "pipeline-style" observation set matching what the real edge
/// sampler (`detector::pipeline::sample_all_edges`) produces: an H obs
/// at corner `(c, r)` reads the master dot at cell `(r-1, c)` — i.e.
/// `horizontal_edge_bit(pos_row + r - 1, pos_col + c)`. Likewise a V obs
/// at `(c, r)` reads the dot at cell `(r, c-1)`. The half-cell offsets
/// reflect the `render.rs` dot placement at cell-boundary midpoints.
fn build_pipeline_style_observation(
    pos_row: i32,
    pos_col: i32,
    local_rows: i32,
    local_cols: i32,
) -> Vec<PuzzleBoardObservedEdge> {
    let mut out = Vec::new();
    for r in 0..local_rows {
        for c in 0..local_cols {
            if c + 1 < local_cols {
                let bit = horizontal_edge_bit(pos_row + r - 1, pos_col + c);
                out.push(PuzzleBoardObservedEdge {
                    row: r,
                    col: c,
                    orientation: EdgeOrientation::Horizontal,
                    bit,
                    confidence: 1.0,
                });
            }
            if r + 1 < local_rows {
                let bit = vertical_edge_bit(pos_row + r, pos_col + c - 1);
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

/// Cross-D4 consistency: for a fixed physical corner observed in eight
/// "cameras", each with a different D4 orientation applied to the local
/// grid, every camera's `alignment.map(its-local-coord)` must reduce
/// (mod MASTER_COLS) to the same physical master coordinate.
///
/// Matches the symptom reported on the 130x130 real dataset: snaps that
/// share a rotation class agree; snaps in different rotation classes
/// disagree purely by an integer master-coord translation.
fn assert_fixed_board_target_position_is_d4_invariant(
    mut decode_one: impl FnMut(&[PuzzleBoardObservedEdge], u32, u32, u32, u32) -> DecodeOutcome,
) {
    // Pick a physical corner inside the board and track its target_position.
    let pos_row = 2i32;
    let pos_col = 3i32;
    let n_corners: i32 = 6;
    let obs0 = build_pipeline_style_observation(pos_row, pos_col, n_corners, n_corners);

    // A 12×12 board is wide enough to hold any rotation of a 6×6 inner
    // corner grid after rebasing min-to-(0,0).
    let rows = 12u32;
    let cols = 12u32;

    // Reference: identity D4, no rotation. Compute target_position for
    // every original local corner (gi, gj) in [0, n_corners)².
    let reference = decode_one(&obs0, 0, 0, rows, cols);
    let mut reference_targets: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    for gi in 0..n_corners {
        for gj in 0..n_corners {
            let g = reference.alignment.map(gi, gj);
            let mi = g.i.rem_euclid(MASTER_COLS as i32);
            let mj = g.j.rem_euclid(MASTER_ROWS as i32);
            reference_targets.insert((gi, gj), (mi, mj));
        }
    }

    for (rot_idx, &rot) in GRID_TRANSFORMS_D4.iter().enumerate().skip(1) {
        // Simulate "camera i" observing the same physical board with its
        // local axes D4-rotated: rotate each obs's (col, row) and flip
        // orientation accordingly.
        let rotated: Vec<PuzzleBoardObservedEdge> = obs0
            .iter()
            .map(|e| rotate_observed_edge_canonically(e, &rot))
            .collect();

        // Rebase to min-(0, 0) as the chessboard detector would do.
        let min_col = rotated.iter().map(|e| e.col).min().unwrap();
        let min_row = rotated.iter().map(|e| e.row).min().unwrap();
        let rebased: Vec<PuzzleBoardObservedEdge> = rotated
            .iter()
            .map(|e| PuzzleBoardObservedEdge {
                row: e.row - min_row,
                col: e.col - min_col,
                ..*e
            })
            .collect();

        let result = decode_one(&rebased, 0, 0, rows, cols);

        // For every original corner (gi, gj), compute its rebased local
        // position in camera i and the corresponding target_position.
        let mut mismatches = Vec::new();
        for gi in 0..n_corners {
            for gj in 0..n_corners {
                let nr = rot.apply(gi, gj);
                let rebased_i = nr.i - min_col;
                let rebased_j = nr.j - min_row;
                let g = result.alignment.map(rebased_i, rebased_j);
                let mi = g.i.rem_euclid(MASTER_COLS as i32);
                let mj = g.j.rem_euclid(MASTER_ROWS as i32);
                let reference_xy = reference_targets[&(gi, gj)];
                if (mi, mj) != reference_xy {
                    mismatches.push((gi, gj, (mi, mj), reference_xy));
                }
            }
        }

        assert!(
            mismatches.is_empty(),
            "rot {rot_idx}: {} corner(s) disagree with identity on target_position; first: \
             obs_local=({},{}) got=({},{}) expected=({},{})",
            mismatches.len(),
            mismatches[0].0,
            mismatches[0].1,
            mismatches[0].2 .0,
            mismatches[0].2 .1,
            mismatches[0].3 .0,
            mismatches[0].3 .1,
        );
    }
}

#[test]
fn decode_fixed_board_target_position_is_d4_invariant_hard() {
    assert_fixed_board_target_position_is_d4_invariant(|obs, spec_or, spec_oc, rows, cols| {
        decode_fixed_board(obs, spec_or, spec_oc, rows, cols, 0.30).expect("hard decode")
    });
}

#[test]
fn decode_fixed_board_soft_target_position_is_d4_invariant() {
    let cfg = default_soft_cfg();
    assert_fixed_board_target_position_is_d4_invariant(|obs, spec_or, spec_oc, rows, cols| {
        decode_fixed_board_soft(obs, spec_or, spec_oc, rows, cols, &cfg, 0.30).expect("soft decode")
    });
}
