//! Soft-log-likelihood decoders.
//!
//! Replace the hard-BER ranking used by [`super::hard::decode`] /
//! [`super::fixed::decode_fixed_board`] with a ChArUco-style per-bit
//! log-likelihood scorer. Each observation's contribution to a hypothesis
//! is a clipped `log_sigmoid` of a linear logit `sign(expected) × obs_sign ×
//! kappa × confidence` (see `calib-targets-charuco/src/detector/board_match.rs`).
//! Hypotheses are ranked purely on that soft score; the top candidate is
//! returned only if it clears a best-vs-runner-up margin gate.

use calib_targets_core::{GridTransform, GRID_TRANSFORMS_D4};

use crate::board::{MASTER_COLS, MASTER_ROWS};
use crate::code_maps::PuzzleBoardObservedEdge;

use super::tables::{transform_observations, ClassRange, ClassTables};
use super::{
    apply_soft_uniqueness_gate, update_best_and_runner_up, DecodeOutcome, HardScan, SoftLlConfig,
    TransformTables, H_COLS, H_ROWS, V_COLS, V_ROWS,
};

/// Finalize the winning hypothesis: populate `score_runner_up`,
/// `score_margin`, and the runner-up origin/transform fields, then apply
/// the margin and BER rejection gates.
pub(super) fn finalize_soft_winner(
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
            best.runner_up_transform = Some(r.alignment.with_translation([0, 0]));
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

/// Soft-log-likelihood decoder over the full 501 × 501 master.
///
/// For each D4 transform we precompute, per cyclic class `(a, b)`, the sum of
/// per-bit LL contributions across observations (`O(501 × N)`), then walk all
/// `501²` origins with a single table lookup per hypothesis. The origin walk
/// keeps the exact serial row-major order — required to reproduce the
/// first-seen tie-break under `f32` rounding (see the inner-loop note for why
/// the integer-keyed crossed-CRT separation used by [`super::hard::decode`] is
/// not byte-safe here) — but defers the cost of materializing a full
/// [`DecodeOutcome`] to the `O(few)` origins that actually enter the
/// winner / runner-up slots.
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

    // Fused matched-count uniqueness scan. The soft precompute produces exactly
    // the count / matched-weight tables the hard path needs, so each transform
    // is folded into a shared `HardScan` here instead of re-running a second
    // full precompute after the soft scan (see the gate call below).
    let mut hard_scan = HardScan::new();

    let mut tables = ClassTables::new(true);
    let range = ClassRange::full();

    for (transform_idx, transform) in GRID_TRANSFORMS_D4.iter().copied().enumerate() {
        let transformed = transform_observations(observed, &transform);
        tables.build(&transformed, &range, Some(cfg));

        // Rank origins in the exact serial row-major order, but defer the
        // expensive full `DecodeOutcome` build to the moments a candidate
        // actually enters the best / runner-up slot.
        //
        // Why not the crossed-CRT argmax separation used by the hard `decode`?
        // The hard path ranks on an *integer* match count as its primary key,
        // so its per-table argmax sets are exact and the separation is
        // byte-safe. The soft path ranks on a single `f32` sum
        // `ll_total = h_ll[ha,hb] + v_ll[va,vb]`, and `f32` rounding can make
        // two origins built from *distinct* table values collapse to an
        // identical sum (`h + v1 == h + v2` with `v1 != v2`). The first-seen
        // tie-break then depends on which of those origins the serial scan
        // visits first — information a per-table level/argmax separation
        // discards. Reproducing the exact tie-break therefore requires visiting
        // origins in the real row-major order. We keep that O(501²) walk but
        // strip its per-origin cost: the original built a 13-field
        // `DecodeOutcome` (two divisions + several table reads) for *every* one
        // of ~2M origins, then discarded all but two. Here the inner loop is
        // two table reads, one add and a single float compare against the
        // weakest retained slot; a `DecodeOutcome` is materialized only on the
        // O(few) occasions a candidate is actually retained, which is
        // byte-identical in ranking (same scan order, same `f32` sums, same
        // strict-`>` first-seen tie-break) and far cheaper.
        for master_row in 0..MASTER_ROWS as i32 {
            let ha = (master_row % H_ROWS as i32) as usize;
            let va = (master_row % V_ROWS as i32) as usize;
            let h_row = &tables.h_ll[ha * H_COLS..ha * H_COLS + H_COLS];
            let v_row = &tables.v_ll[va * V_COLS..va * V_COLS + V_COLS];
            for master_col in 0..MASTER_COLS as i32 {
                let hb = (master_col % H_COLS as i32) as usize;
                let vb = (master_col % V_COLS as i32) as usize;
                let ll_total = h_row[hb] + v_row[vb];

                // Cheap gate replicating `update_best_and_runner_up`'s ranking
                // decision *without* building the candidate. `enters_best` =
                // strictly beats the current best. `enters_runner_up` =
                // does not beat best but strictly beats the current runner-up
                // (or the runner-up slot is empty). Only then do we pay for the
                // full outcome and run the byte-identical two-slot update.
                let enters_best = match &best {
                    None => true,
                    Some(b) => ll_total > b.score_best,
                };
                let enters = enters_best
                    || match (&best, &runner_up) {
                        (None, _) => true,
                        (Some(_), None) => true,
                        (Some(_), Some(r)) => ll_total > r.score_best,
                    };
                if enters {
                    let origin = OriginClass {
                        mr: master_row,
                        mc: master_col,
                        ha,
                        hb,
                        va,
                        vb,
                    };
                    let candidate = build_soft_candidate(transform, &tables, total, &origin);
                    update_best_and_runner_up(&mut best, &mut runner_up, candidate);
                }
            }
        }

        // Fold this transform's count / matched-weight tables into the shared
        // matched-count scan while they are still populated (the `*.fill(...)`
        // The shared precompute already produced exactly the count and
        // matched-weight tables the hard path consumes, so the uniqueness top-2
        // is folded from them rather than from a second precompute pass.
        let hard_tables = TransformTables {
            h_count: &tables.h_count,
            h_weight: &tables.h_weight,
            v_count: &tables.v_count,
            v_weight: &tables.v_weight,
        };
        hard_scan.fold(
            transform,
            transform_idx,
            &hard_tables,
            total,
            total_conf,
            max_bit_error_rate,
        );
    }

    let winner = finalize_soft_winner(best, runner_up, cfg, max_bit_error_rate)?;
    // Re-gate the soft winner by the matched-count uniqueness predicate. The
    // soft-LL `alignment_min_margin` gate does not enforce origin uniqueness;
    // the matched-count top-2 over the full master (the same candidate set the
    // soft winner was drawn from) supplies the competitor.
    //
    // Fused (was a TODO(perf)): the matched-count top-2 is now folded inline
    // from the soft scan's own per-transform count / matched-weight tables (see
    // the `hard_scan.fold(...)` at the end of each transform iteration above),
    // so the gate no longer re-runs a second full O(8·501·N) precompute over the
    // same observations. `HardScan::finish` returns the byte-identical pre-gate
    // triple a fresh `decode_with_runner_up(observed, max_bit_error_rate)` would
    // (same tables, same crossed-CRT top-2 logic).
    let (hard_winner, best_matched, runner) = hard_scan.finish()?;
    apply_soft_uniqueness_gate(
        winner,
        (
            best_matched,
            hard_winner.master_origin_row,
            hard_winner.master_origin_col,
            hard_winner.alignment.with_translation([0, 0]),
            runner,
        ),
    )
}

/// A candidate origin with its precomputed cyclic class indices, used to
/// reconstruct the serial scan's exact per-origin outcome.
struct OriginClass {
    mr: i32,
    mc: i32,
    ha: usize,
    hb: usize,
    va: usize,
    vb: usize,
}

/// Reconstruct the exact [`DecodeOutcome`] the serial scan produces at the
/// given origin and its cyclic classes. All summations preserve the serial
/// scan's operand order so floats are bit-identical.
#[inline]
fn build_soft_candidate(
    transform: GridTransform,
    tables: &ClassTables,
    total: usize,
    origin: &OriginClass,
) -> DecodeOutcome {
    let &OriginClass {
        mr,
        mc,
        ha,
        hb,
        va,
        vb,
    } = origin;
    let ll_total = tables.h_ll[ha * H_COLS + hb] + tables.v_ll[va * V_COLS + vb];
    let matched = (tables.h_count[ha * H_COLS + hb] + tables.v_count[va * V_COLS + vb]) as usize;
    let match_conf_sum = tables.h_weight[ha * H_COLS + hb] + tables.v_weight[va * V_COLS + vb];

    let bit_error_rate = (total - matched) as f32 / total as f32;
    let mean_confidence = if matched == 0 {
        0.0
    } else {
        match_conf_sum / matched as f32
    };
    DecodeOutcome {
        alignment: transform.with_translation([mc, mr]),
        edges_matched: matched,
        edges_observed: total,
        weighted_score: ll_total / total as f32,
        bit_error_rate,
        mean_confidence,
        master_origin_row: mr,
        master_origin_col: mc,
        score_best: ll_total,
        // Finalized at the end of the scan.
        score_runner_up: None,
        score_margin: 0.0,
        runner_up_origin_row: None,
        runner_up_origin_col: None,
        runner_up_transform: None,
    }
}
