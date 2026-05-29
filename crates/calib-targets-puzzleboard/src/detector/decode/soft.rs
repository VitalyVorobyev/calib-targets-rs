//! Soft-log-likelihood decoders.
//!
//! Replace the hard-BER ranking used by [`super::hard::decode`] /
//! [`super::hard::decode_fixed_board`] with a ChArUco-style per-bit
//! log-likelihood scorer. Each observation's contribution to a hypothesis
//! is a clipped `log_sigmoid` of a linear logit `sign(expected) × obs_sign ×
//! kappa × confidence` (see `calib-targets-charuco/src/detector/board_match.rs`).
//! Hypotheses are ranked purely on that soft score; the top candidate is
//! returned only if it clears a best-vs-runner-up margin gate.

use calib_targets_core::{GridAlignment, GRID_TRANSFORMS_D4};

use crate::board::{MASTER_COLS, MASTER_ROWS};
use crate::code_maps::{
    horizontal_edge_bit, vertical_edge_bit, EdgeOrientation, PuzzleBoardObservedEdge,
};

use super::{
    ll_pair, transform_edge_lookup, update_best_and_runner_up, DecodeOutcome, SoftLlConfig, H_COLS,
    H_ROWS, V_COLS, V_ROWS,
};

/// Finalize the winning hypothesis: populate `score_runner_up`,
/// `score_margin`, and the runner-up origin/transform fields, then apply
/// the margin and BER rejection gates.
fn finalize_soft_winner(
    best: Option<DecodeOutcome>,
    runner_up: Option<DecodeOutcome>,
    cfg: &SoftLlConfig,
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
    let mut best = best?;
    let edges = best.edges_observed.max(1) as f32;
    match runner_up {
        Some(r) => {
            best.score_runner_up = Some(r.score_best);
            best.score_margin = (best.score_best - r.score_best) / edges;
            best.runner_up_origin_row = Some(r.master_origin_row);
            best.runner_up_origin_col = Some(r.master_origin_col);
            best.runner_up_transform = Some(r.alignment.transform);
        }
        None => {
            best.score_runner_up = None;
            best.score_margin = f32::INFINITY;
            best.runner_up_origin_row = None;
            best.runner_up_origin_col = None;
            best.runner_up_transform = None;
        }
    }
    if best.score_margin < cfg.alignment_min_margin {
        return None;
    }
    if best.bit_error_rate > max_bit_error_rate {
        return None;
    }
    Some(best)
}

/// Soft-log-likelihood decoder over the full 501 × 501 master. Preserves the
/// `O(501² × N)` fast-path: for each D4 transform we precompute per cyclic
/// class `(a, b)` the sum of per-bit LL contributions across observations,
/// then scan origins with a single table lookup per hypothesis.
pub(crate) fn decode_soft(
    observed: &[PuzzleBoardObservedEdge],
    cfg: &SoftLlConfig,
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
    let mut runner_up: Option<DecodeOutcome> = None;

    // Scratch buffers: reused across D4 transforms. `h_ll` / `v_ll` hold the
    // sum of per-bit LL contributions. `h_match` / `v_match` track the hard
    // match count (diagnostic only — feeds `edges_matched` and the BER gate).
    // `h_match_conf` / `v_match_conf` track the summed confidence of matched
    // observations (for the `mean_confidence` diagnostic).
    let mut h_ll = vec![0.0f32; H_ROWS * H_COLS];
    let mut h_match = vec![0u32; H_ROWS * H_COLS];
    let mut h_match_conf = vec![0.0f32; H_ROWS * H_COLS];
    let mut v_ll = vec![0.0f32; V_ROWS * V_COLS];
    let mut v_match = vec![0u32; V_ROWS * V_COLS];
    let mut v_match_conf = vec![0.0f32; V_ROWS * V_COLS];

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

        h_ll.fill(0.0);
        h_match.fill(0);
        h_match_conf.fill(0.0);
        v_ll.fill(0.0);
        v_match.fill(0);
        v_match_conf.fill(0.0);

        for &(tr, tc, orient, bit, conf) in &transformed {
            let (ll_match_val, ll_mismatch_val) = ll_pair(conf, cfg.kappa, cfg.per_bit_floor);
            match orient {
                EdgeOrientation::Horizontal => {
                    for r in 0..H_ROWS {
                        let a = (r as i32 - tr).rem_euclid(H_ROWS as i32) as usize;
                        for c in 0..H_COLS {
                            let b = (c as i32 - tc).rem_euclid(H_COLS as i32) as usize;
                            let expected = horizontal_edge_bit(r as i32, c as i32);
                            let idx = a * H_COLS + b;
                            if expected == bit {
                                h_ll[idx] += ll_match_val;
                                h_match[idx] += 1;
                                h_match_conf[idx] += conf;
                            } else {
                                h_ll[idx] += ll_mismatch_val;
                            }
                        }
                    }
                }
                EdgeOrientation::Vertical => {
                    for r in 0..V_ROWS {
                        let a = (r as i32 - tr).rem_euclid(V_ROWS as i32) as usize;
                        for c in 0..V_COLS {
                            let b = (c as i32 - tc).rem_euclid(V_COLS as i32) as usize;
                            let expected = vertical_edge_bit(r as i32, c as i32);
                            let idx = a * V_COLS + b;
                            if expected == bit {
                                v_ll[idx] += ll_match_val;
                                v_match[idx] += 1;
                                v_match_conf[idx] += conf;
                            } else {
                                v_ll[idx] += ll_mismatch_val;
                            }
                        }
                    }
                }
            }
        }

        for master_row in 0..MASTER_ROWS as i32 {
            let ha = (master_row % H_ROWS as i32) as usize;
            let va = (master_row % V_ROWS as i32) as usize;
            for master_col in 0..MASTER_COLS as i32 {
                let hb = (master_col % H_COLS as i32) as usize;
                let vb = (master_col % V_COLS as i32) as usize;

                let ll_total = h_ll[ha * H_COLS + hb] + v_ll[va * V_COLS + vb];
                let matched = (h_match[ha * H_COLS + hb] + v_match[va * V_COLS + vb]) as usize;
                let match_conf_sum =
                    h_match_conf[ha * H_COLS + hb] + v_match_conf[va * V_COLS + vb];

                let bit_error_rate = (total - matched) as f32 / total as f32;
                let mean_confidence = if matched == 0 {
                    0.0
                } else {
                    match_conf_sum / matched as f32
                };
                let candidate = DecodeOutcome {
                    alignment: GridAlignment {
                        transform,
                        translation: [master_col, master_row],
                    },
                    edges_matched: matched,
                    edges_observed: total,
                    weighted_score: ll_total / total as f32,
                    bit_error_rate,
                    mean_confidence,
                    master_origin_row: master_row,
                    master_origin_col: master_col,
                    score_best: ll_total,
                    // Finalized at the end of the scan.
                    score_runner_up: None,
                    score_margin: 0.0,
                    runner_up_origin_row: None,
                    runner_up_origin_col: None,
                    runner_up_transform: None,
                };
                update_best_and_runner_up(&mut best, &mut runner_up, candidate);
            }
        }
    }

    finalize_soft_winner(best, runner_up, cfg, max_bit_error_rate)
}

/// Soft-log-likelihood decoder constrained to the declared board's bit
/// pattern (FixedBoard mode). Mirrors [`super::hard::decode_fixed_board`] but
/// swaps the hard-BER accumulator for summed `log_sigmoid` contributions and
/// tracks both the winner and the runner-up for margin-gating.
pub(crate) fn decode_fixed_board_soft(
    observed: &[PuzzleBoardObservedEdge],
    spec_origin_row: u32,
    spec_origin_col: u32,
    rows: u32,
    cols: u32,
    cfg: &SoftLlConfig,
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
    let mut runner_up: Option<DecodeOutcome> = None;

    for transform in GRID_TRANSFORMS_D4.iter().copied() {
        let transformed: Vec<(i32, i32, EdgeOrientation, u8, f32, f32, f32)> = observed
            .iter()
            .map(|e| {
                let lookup = transform_edge_lookup(e, &transform);
                let (ll_match_val, ll_mismatch_val) =
                    ll_pair(e.confidence, cfg.kappa, cfg.per_bit_floor);
                (
                    lookup.lookup_row,
                    lookup.lookup_col,
                    lookup.orientation,
                    e.bit,
                    e.confidence,
                    ll_match_val,
                    ll_mismatch_val,
                )
            })
            .collect();

        let (lr_min, lr_max) = transformed
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), &(lr, _, _, _, _, _, _)| {
                (lo.min(lr), hi.max(lr))
            });
        let (lc_min, lc_max) = transformed
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), &(_, lc, _, _, _, _, _)| {
                (lo.min(lc), hi.max(lc))
            });
        let rows_i = rows as i32;
        let cols_i = cols as i32;
        let p_r_lo = (-lr_max).max(0);
        let p_r_hi = (rows_i - lr_min).min(rows_i);
        let p_c_lo = (-lc_max).max(0);
        let p_c_hi = (cols_i - lc_min).min(cols_i);
        if p_r_lo > p_r_hi || p_c_lo > p_c_hi {
            continue;
        }

        for p_r in p_r_lo..=p_r_hi {
            for p_c in p_c_lo..=p_c_hi {
                let mut ll_total = 0.0f32;
                let mut matched = 0usize;
                let mut match_conf_sum = 0.0f32;
                for &(tr, tc, orient, bit, conf, ll_m, ll_mm) in &transformed {
                    let expected_opt = match orient {
                        EdgeOrientation::Horizontal => {
                            let cr = p_r + tr;
                            let cc = p_c + tc;
                            if cr < 0 || cr >= h_rows as i32 || cc < 0 || cc >= h_cols as i32 {
                                None
                            } else {
                                Some(h_bit[cr as usize * h_cols + cc as usize])
                            }
                        }
                        EdgeOrientation::Vertical => {
                            let cr = p_r + tr;
                            let cc = p_c + tc;
                            if cr < 0 || cr >= v_rows as i32 || cc < 0 || cc >= v_cols as i32 {
                                None
                            } else {
                                Some(v_bit[cr as usize * v_cols + cc as usize])
                            }
                        }
                    };
                    match expected_opt {
                        None => {
                            // Off-board observations are penalized as
                            // mismatches so hypotheses that truncate the
                            // board do not artificially tie the correct
                            // full-view hypothesis. Mirrors how the hard-
                            // weighted path counts them in the BER (they
                            // are part of `total` but not `matched`).
                            ll_total += ll_mm;
                        }
                        Some(expected) if expected == bit => {
                            ll_total += ll_m;
                            matched += 1;
                            match_conf_sum += conf;
                        }
                        Some(_) => {
                            ll_total += ll_mm;
                        }
                    }
                }
                let bit_error_rate = (total - matched) as f32 / total as f32;
                let mean_confidence = if matched == 0 {
                    0.0
                } else {
                    match_conf_sum / matched as f32
                };
                let master_row = spec_or + p_r;
                let master_col = spec_oc + p_c;
                let candidate = DecodeOutcome {
                    alignment: GridAlignment {
                        transform,
                        translation: [master_col, master_row],
                    },
                    edges_matched: matched,
                    edges_observed: total,
                    weighted_score: ll_total / total as f32,
                    bit_error_rate,
                    mean_confidence,
                    master_origin_row: master_row,
                    master_origin_col: master_col,
                    score_best: ll_total,
                    score_runner_up: None,
                    score_margin: 0.0,
                    runner_up_origin_row: None,
                    runner_up_origin_col: None,
                    runner_up_transform: None,
                };
                update_best_and_runner_up(&mut best, &mut runner_up, candidate);
            }
        }
    }

    finalize_soft_winner(best, runner_up, cfg, max_bit_error_rate)
}
