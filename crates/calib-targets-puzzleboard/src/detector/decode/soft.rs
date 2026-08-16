//! Soft-log-likelihood decoders.
//!
//! Replace the hard-BER ranking used by [`super::hard::decode`] /
//! [`super::fixed::decode_fixed_board`] with a ChArUco-style per-bit
//! log-likelihood scorer. Each observation's contribution to a hypothesis
//! is a clipped `log_sigmoid` of a linear logit `sign(expected) × obs_sign ×
//! kappa × confidence` (see `calib-targets-charuco/src/detector/board_match.rs`).
//! Hypotheses are ranked purely on that soft score; the top candidate is
//! returned only if it clears a best-vs-runner-up margin gate.

use calib_targets_core::GridTransform;

use crate::code_maps::PuzzleBoardObservedEdge;

use super::hard::row_major_min_origin;
use super::tables::{transform_observations, ClassRange, ClassTables};
use super::{
    apply_soft_uniqueness_gate, crt_master_col, crt_master_row, dequantize_ll,
    update_best_and_runner_up, DecodeOutcome, SoftLlConfig, H_COLS, H_ROWS, V_COLS, V_ROWS,
};

/// Finalize the winning hypothesis: populate `score_runner_up`,
/// `score_margin`, and the runner-up origin/transform fields, then apply the
/// margin gate.
///
/// The BER gate is **not** applied here. It belongs on the logical-bit view of
/// the observation set, which this function does not have; the callers apply it
/// via the class-set scan that also supplies the uniqueness top-2.
///
/// `score_margin` stays normalised by the *physical* edge count. It is a tuned
/// threshold (`alignment_min_margin`) whose value was swept in that domain, so
/// re-denominating it over logical bits would silently change what the
/// configured number means. The two gates therefore live in different units on
/// purpose: this one is a soft heuristic, the uniqueness gate is an exact
/// bounded-distance argument that has to be in the code's own alphabet.
pub(super) fn finalize_soft_winner(
    best: Option<DecodeOutcome>,
    runner_up: Option<DecodeOutcome>,
    cfg: &SoftLlConfig,
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
    Some(best)
}

/// Soft-log-likelihood decoder over the full 501 × 501 master.
///
/// `transforms` is the orientation hypothesis set to search — see
/// [`PuzzleBoardSymmetryMode`](crate::PuzzleBoardSymmetryMode). The soft scan
/// and the fused matched-count uniqueness scan below walk the *same* list, so
/// the gate's competitor set is always the set the winner was drawn from.
///
/// For each transform the shared precompute fills the per-class
/// log-likelihood tables, and the winning origin is then recovered by the same
/// crossed-CRT separation the hard path uses: because `501 = 3 · 167` with
/// `gcd(3, 167) = 1`, the argmax of `h_ll[ha, hb] + v_ll[va, vb]` over all
/// origins is exactly the pair of per-table argmaxes, so the `501²` scan
/// collapses to `O(501)`.
///
/// That separation is only valid for an **integer** key — a table entry below
/// the maximum must be at least one below it, so that it provably cannot reach
/// the maximum sum — which is why the log-likelihood is accumulated in the
/// fixed-point units of [`super::LL_SCALE`] rather than as `f32`.
///
/// The runner-up needed by the margin gate comes from the same separation: if
/// the joint maximum is attained by more than one origin the decode is
/// ambiguous and the runner-up sits at the same score (margin `0`); otherwise
/// the second-highest achievable sum keeps one table at its maximum and drops
/// the other to its second-distinct level.
pub(crate) fn decode_soft(
    observed: &[PuzzleBoardObservedEdge],
    logical: &[PuzzleBoardObservedEdge],
    transforms: &[GridTransform],
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

    let mut tables = ClassTables::new(true);
    let range = ClassRange::full();

    for transform in transforms.iter().copied() {
        let transformed = transform_observations(observed, &transform);
        tables.build(&transformed, &range, Some(cfg));

        // Collapse the origin scan by crossed-CRT separation (see the
        // function docs). `level_max` plays the role `lex_max_classes` plays on
        // the hard path, on a single integer key instead of a lexicographic
        // (count, weight) pair.
        #[cfg(feature = "tracing")]
        let _origin_span = tracing::info_span!("origin_scan").entered();
        let h = level_max(&tables.h_ll, H_COLS);
        let v = level_max(&tables.v_ll, V_COLS);
        if let Some((winner, runner)) = separated_top2(transform, &tables, total, &h, &v) {
            update_best_and_runner_up(&mut best, &mut runner_up, winner);
            if let Some(runner) = runner {
                update_best_and_runner_up(&mut best, &mut runner_up, runner);
            }
        }
        #[cfg(feature = "tracing")]
        drop(_origin_span);
    }

    let winner = finalize_soft_winner(best, runner_up, cfg)?;

    // Budget and uniqueness, both on the voted bits — the domain the paper
    // quotes the budget in ("after averaging over all repetitions") and, per
    // `runner_up_floor_report`, a domain where the uniqueness predicate is no
    // weaker: the structural alias floor is identical in both views, so the
    // same ceiling now applies to the post-vote error rate rather than the raw
    // dot rate.
    //
    // This is where essentially all of the range comes from, on *both* scoring
    // paths: with the gate moved, the soft and hard columns of
    // `super::tests::consensus_noise_tolerance_report` agree cell for cell. The
    // soft scorer's own ranking is unchanged — it still sums per-dot
    // log-likelihoods, which is already the optimal way to combine replicas
    // that predict the same bit under every hypothesis.
    let (hard_winner, best_matched, runner) =
        super::hard::decode_with_runner_up(logical, transforms, max_bit_error_rate)?;
    apply_soft_uniqueness_gate(
        winner,
        logical.len(),
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
    let ll_fixed = tables.h_ll[ha * H_COLS + hb] + tables.v_ll[va * V_COLS + vb];
    let ll_total = dequantize_ll(ll_fixed);
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

/// Highest and second-highest **distinct** values of one class table, with the
/// cells attaining the maximum.
///
/// The integer key is what makes this usable: every cell not in `classes` is at
/// least one below `max`, so no combination involving one can reach the joint
/// maximum. `second` is the largest value strictly below `max`, which is what
/// the runner-up needs.
struct LevelMax {
    max: i64,
    classes: Vec<(usize, usize)>,
    second: Option<i64>,
    second_class: Option<(usize, usize)>,
}

fn level_max(table: &[i64], cols: usize) -> LevelMax {
    let mut max = i64::MIN;
    for &value in table {
        if value > max {
            max = value;
        }
    }
    let mut classes = Vec::new();
    let mut second: Option<i64> = None;
    let mut second_class: Option<(usize, usize)> = None;
    for (i, &value) in table.iter().enumerate() {
        if value == max {
            classes.push((i / cols, i % cols));
        } else if second.is_none_or(|s| value > s) {
            second = Some(value);
            second_class = Some((i / cols, i % cols));
        }
    }
    LevelMax {
        max,
        classes,
        second,
        second_class,
    }
}

/// Build this transform's best origin and its closest competitor from the two
/// per-table optima.
///
/// The winner is the row-major-minimum over the product of the two argmax sets,
/// matching the serial scan's first-seen tie-break. The competitor is either
/// another origin at the *same* joint maximum — an ambiguous decode, margin `0`
/// — or the second-highest achievable sum, which keeps one table at its maximum
/// and drops the other by one level.
fn separated_top2(
    transform: GridTransform,
    tables: &ClassTables,
    total: usize,
    h: &LevelMax,
    v: &LevelMax,
) -> Option<(DecodeOutcome, Option<DecodeOutcome>)> {
    let (winner_row, winner_col) = row_major_min_origin(&h.classes, &v.classes)?;
    let build = |mr: i32, mc: i32| {
        build_soft_candidate(
            transform,
            tables,
            total,
            &OriginClass {
                mr,
                mc,
                ha: mr.rem_euclid(H_ROWS as i32) as usize,
                hb: mc.rem_euclid(H_COLS as i32) as usize,
                va: mr.rem_euclid(V_ROWS as i32) as usize,
                vb: mc.rem_euclid(V_COLS as i32) as usize,
            },
        )
    };
    let winner = build(winner_row, winner_col);

    // Case A: more than one origin attains the joint maximum — genuinely
    // ambiguous, and the runner-up sits at the winner's own score.
    if h.classes.len().saturating_mul(v.classes.len()) > 1 {
        for &(ha, hb) in &h.classes {
            for &(va, vb) in &v.classes {
                let mr = crt_master_row(va, ha);
                let mc = crt_master_col(hb, vb);
                if (mr, mc) != (winner_row, winner_col) {
                    return Some((winner, Some(build(mr, mc))));
                }
            }
        }
    }

    // Case B: the second-highest sum drops exactly one table one level.
    let from_v = v
        .second
        .map(|sv| (h.max + sv, Some(h.classes[0]), v.second_class));
    let from_h = h
        .second
        .map(|sh| (sh + v.max, h.second_class, Some(v.classes[0])));
    let pick = match (from_h, from_v) {
        (Some(a), Some(b)) => Some(if a.0 >= b.0 { a } else { b }),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };
    let runner = pick.and_then(|(_, h_cell, v_cell)| {
        let (ha, hb) = h_cell?;
        let (va, vb) = v_cell?;
        Some(build(crt_master_row(va, ha), crt_master_col(hb, vb)))
    });
    Some((winner, runner))
}
