//! Hard-weighted (count-and-confidence) decoder over the full master.
//!
//! Ranks hypotheses lexicographically by `(edges_matched, weighted_score)` and
//! rejects anything above `max_bit_error_rate`. [`decode`] sweeps the full
//! 501 × 501 master via the cyclic-period precompute; the declared-board
//! counterpart lives in [`super::fixed`].

use calib_targets_core::{GridTransform, GRID_TRANSFORMS_D4};

use crate::board::{MASTER_COLS, MASTER_ROWS};
use crate::code_maps::PuzzleBoardObservedEdge;

use super::tables::{transform_observations, ClassRange, ClassTables};
use super::{
    crt_master_col, crt_master_row, finalize_hard_winner, update_best_candidate, DecodeOutcome,
    HardRunnerUp, H_COLS, H_ROWS, V_COLS, V_ROWS,
};

/// Hard upper bound on `|optimalH| × |optimalV|` before the separated argmax
/// search falls back to a direct table scan. The master code is a De
/// Bruijn-style torus, so a clean (or near-clean) observation set decodes to a
/// single optimum and this product is `1`. The cap only guards pathological
/// inputs (e.g. an empty or all-tied observation set) where the optimal class
/// set degenerates to many cells; the fallback then costs exactly what the
/// original `O(501²)` scan did for that transform, so worst-case runtime never
/// regresses.
const SEPARATION_PRODUCT_CAP: usize = 1024;

/// Hard-weighted decoder over the full 501 × 501 master.
///
/// For each D4 transform we precompute, per cyclic class, the match count and
/// confidence-weight tables in `O(501 × N)`. The `501²` origin scan is then
/// collapsed to `O(501)` via an exact crossed-CRT separation: because
/// `501 = 3·167` with `gcd(3, 167) = 1`, the per-origin score
/// `(count, weight) = (H[ha,hb] + V[va,vb])` splits into two independent
/// tables whose lexicographic argmax can be combined directly (the count
/// primary key is an integer, so the argmax sets are exact and the separation
/// is byte-safe — unlike the pure-`f32` soft path, see
/// [`super::soft::decode_soft`]). A pathological all-tied input falls back to a
/// direct table scan for the affected transform so worst-case cost never
/// regresses past the original.
pub(crate) fn decode(
    observed: &[PuzzleBoardObservedEdge],
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
    let (winner, best_matched, runner_up) = decode_with_runner_up(observed, max_bit_error_rate)?;
    finalize_hard_winner(winner, best_matched, runner_up)
}

/// Core of [`decode`]: returns the winning hypothesis together with its
/// matched-bit count and the closest competing origin, *before* the uniqueness
/// gate is applied. The public [`decode`] wraps this with
/// [`finalize_hard_winner`]; tests use the pre-gate triple directly to validate
/// the runner-up against an independent brute-force oracle.
pub(crate) fn decode_with_runner_up(
    observed: &[PuzzleBoardObservedEdge],
    max_bit_error_rate: f32,
) -> Option<(DecodeOutcome, u32, Option<HardRunnerUp>)> {
    if observed.is_empty() {
        return None;
    }

    let total_conf: f32 = observed.iter().map(|e| e.confidence).sum();
    if total_conf <= 0.0 {
        return None;
    }
    let total = observed.len();

    let mut scan = HardScan::new();
    let mut tables = ClassTables::new(false);
    let range = ClassRange::full();

    for (transform_idx, transform) in GRID_TRANSFORMS_D4.iter().copied().enumerate() {
        let transformed = transform_observations(observed, &transform);
        tables.build(&transformed, &range, None);

        // Fold this transform's tables into the shared accumulator (steps 1-4:
        // crossed-CRT separation, BER reject, worst-case fallback, winner
        // update). The soft full-master scan runs the identical `fold` over its
        // own byte-identical tables, so both paths produce the same uniqueness
        // top-2 from a single precompute.
        let view = TransformTables {
            h_count: &tables.h_count,
            h_weight: &tables.h_weight,
            v_count: &tables.v_count,
            v_weight: &tables.v_weight,
        };
        scan.fold(
            transform,
            transform_idx,
            &view,
            total,
            total_conf,
            max_bit_error_rate,
        );
    }

    scan.finish()
}

/// Shared accumulator for the *post-precompute* per-transform body of the
/// hard-weighted full-master decode.
///
/// Both the hard path ([`decode_with_runner_up`]) and the soft full-master path
/// ([`super::soft::decode_soft`]) build byte-identical per-transform precompute
/// tables (matched-count + matched-confidence-weight), then run the identical
/// crossed-CRT winner-selection + uniqueness top-2 logic on them. Factoring that
/// logic here lets the soft scan reuse the tables it already builds instead of
/// running a second full precompute pass purely for the uniqueness gate.
///
/// Call [`HardScan::fold`] once per D4 transform (with that transform's tables)
/// and then [`HardScan::finish`] to obtain the pre-gate winner triple.
pub(crate) struct HardScan {
    best: Option<DecodeOutcome>,
    // Index of the transform that currently owns the winner, and the
    // per-transform `(best_count, within_runner)` summaries used to assemble the
    // global runner-up after the scan. `within_runner` is the closest *distinct*
    // competing origin within a single transform (a missing second level is
    // `None`); the cross-transform competitor is the `best_count` of every
    // *other* transform. See [`assemble_global_runner_up`].
    winner_transform_idx: Option<usize>,
    transform_summaries: Vec<TransformSummary>,
}

impl HardScan {
    pub(crate) fn new() -> Self {
        Self {
            best: None,
            winner_transform_idx: None,
            transform_summaries: Vec::with_capacity(8),
        }
    }

    /// Fold one D4 transform's precompute tables into the accumulator.
    ///
    /// `transform_idx` is the transform's position in `GRID_TRANSFORMS_D4` (used
    /// to attribute the global winner for the runner-up assembly). `tables`
    /// carries the matched-count (`h_count`/`v_count`) and matched-weight
    /// (`h_match`/`v_match`) tables; `total = observed.len()` and `total_conf =
    /// Σ confidence`. The body is the exact step-1..4 logic of the original hard
    /// inner loop.
    pub(crate) fn fold(
        &mut self,
        transform: GridTransform,
        transform_idx: usize,
        tables: &TransformTables<'_>,
        total: usize,
        total_conf: f32,
        max_bit_error_rate: f32,
    ) {
        // Collapse the O(501²) origin scan to O(501) via the crossed-CRT
        // separation. The per-origin score is the sum of two *independent*
        // table terms — the H term depends only on `(ha, hb)` and the V term
        // only on `(va, vb)`, and (because `501 = 3·167`, `gcd(3, 167) = 1`)
        // the four residues `(va, ha, hb, vb)` are mutually independent and
        // each ranges over its full domain. The lexicographic argmax of the
        // sum therefore separates: it is exactly the product of the per-table
        // lexicographic argmaxes (proved over the full table-value range — a
        // candidate with strictly more matched bits in one table always wins,
        // and on equal counts the weights add independently).
        //
        // Step 1: lexicographic max `(count, weight)` over each table, the set
        // of classes achieving it, and — for the uniqueness gate — the
        // second-distinct count level (largest count strictly below the max).
        let h_tab = lex_max_classes(tables.h_count, tables.h_weight, H_COLS);
        let v_tab = lex_max_classes(tables.v_count, tables.v_weight, V_COLS);
        let (mc_h, max_h_w, optimal_h) = (h_tab.max_count, h_tab.max_weight, &h_tab.classes);
        let (mc_v, max_v_w, optimal_v) = (v_tab.max_count, v_tab.max_weight, &v_tab.classes);

        let best_matched = (mc_h + mc_v) as usize;
        let best_weighted = max_h_w + max_v_w;

        // The joint optimum is the most-matched origin under this transform, so
        // record `best_count` for the cross-transform runner-up unconditionally
        // (a competing origin threatens uniqueness whether or not it clears the
        // BER gate). The representative origin is the row-major-min over the
        // optimal product, matching the winner-selection tie-break.
        let best_origin = row_major_min_origin(optimal_h, optimal_v);
        let within_runner =
            within_transform_runner_up(transform, &h_tab, &v_tab, mc_h, mc_v, best_origin);
        // Pushed unconditionally, BEFORE the BER early-reject below: every
        // transform's `best_count` competes for the uniqueness runner-up whether
        // or not it clears the gate.
        self.transform_summaries.push(TransformSummary {
            transform,
            best_count: best_matched as u32,
            best_origin,
            within_runner,
        });

        // Step 2: BER early-reject. The joint optimum has the most matched bits
        // of any origin under this transform, so if it fails the gate every
        // other origin (with `matched ≤ best_matched`, hence higher BER) fails
        // too — the transform contributes no winner candidate. (Its `best_count`
        // is still tracked above as a uniqueness competitor.)
        let bit_error_rate = if total == 0 {
            1.0
        } else {
            (total - best_matched) as f32 / total as f32
        };
        if bit_error_rate > max_bit_error_rate {
            return;
        }

        // Step 3: worst-case guard. Real inputs decode uniquely (product == 1);
        // only degenerate all-tied inputs blow this up. Fall back to a direct
        // table scan for this transform only so cost never exceeds the original.
        if optimal_h.len().saturating_mul(optimal_v.len()) > SEPARATION_PRODUCT_CAP {
            if scan_transform_direct(
                transform,
                tables,
                total,
                total_conf,
                max_bit_error_rate,
                &mut self.best,
            ) {
                self.winner_transform_idx = Some(transform_idx);
            }
            return;
        }

        // Step 4: the winning origin is the row-major-min over the joint optimal
        // set (computed above as `best_origin`): the original scan visits origins
        // in (master_row, then master_col) order and keeps the FIRST candidate at
        // the maximum (strict `>`).
        let (master_row, master_col) = best_origin.expect("optimal sets are non-empty");

        // Every entry in the optimal product shares the same `(count, weight)`,
        // so `best_weighted` is byte-identical to the original's
        // `h_match[ha] + v_match[vb]` at the winning origin (same summands,
        // same addition order).
        let score = best_weighted / total_conf;
        let mean_confidence = if best_matched == 0 {
            0.0
        } else {
            best_weighted / best_matched as f32
        };
        let candidate = DecodeOutcome {
            // translation[0] is the i (col) offset, translation[1]
            // is the j (row) offset, so master_col goes first.
            alignment: transform.with_translation([master_col, master_row]),
            edges_matched: best_matched,
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
        if update_best_candidate(&mut self.best, candidate) {
            self.winner_transform_idx = Some(transform_idx);
        }
    }

    /// Consume the accumulator and return the pre-gate winner triple: the
    /// winning hypothesis, its matched-bit count, and the closest competing
    /// origin (`None` if no winner cleared the BER gate).
    pub(crate) fn finish(self) -> Option<(DecodeOutcome, u32, Option<HardRunnerUp>)> {
        let winner = self.best?;
        let runner_up =
            assemble_global_runner_up(&self.transform_summaries, self.winner_transform_idx);
        let best_matched = winner.edges_matched as u32;
        Some((winner, best_matched, runner_up))
    }
}

/// Per-transform summary needed to assemble the global uniqueness runner-up:
/// the transform's most-matched origin count and a representative origin, plus
/// the closest *distinct* competing origin within that single transform.
struct TransformSummary {
    transform: GridTransform,
    best_count: u32,
    best_origin: Option<(i32, i32)>,
    within_runner: Option<HardRunnerUp>,
}

/// Row-major-min master origin over the joint optimal product `optimal_h ×
/// optimal_v`. Matches the original scan's first-seen (strict-`>`) tie-break:
/// the winner is the lexicographic minimum of `(master_row, master_col)`.
fn row_major_min_origin(
    optimal_h: &[(usize, usize)],
    optimal_v: &[(usize, usize)],
) -> Option<(i32, i32)> {
    let mut best_origin: Option<(i32, i32)> = None;
    for &(ha, hb) in optimal_h {
        for &(va, vb) in optimal_v {
            let mr = crt_master_row(va, ha);
            let mc = crt_master_col(hb, vb);
            let better = match best_origin {
                None => true,
                Some((br, bc)) => (mr, mc) < (br, bc),
            };
            if better {
                best_origin = Some((mr, mc));
            }
        }
    }
    best_origin
}

/// Closest *distinct* competing origin within a single transform.
///
/// The per-origin matched count separates as `h_count[a,b] + v_count[c,d]`, so
/// the highest sum is `mc_h + mc_v`, achieved by `n_h · n_v` distinct origins
/// where `n_h` / `n_v` are the counts of cells *at the max count* (weights
/// ignored — two origins at equal count are two distinct competitors regardless
/// of weight). If that product exceeds 1 there are ≥2 distinct origins tied at
/// the maximum — a genuinely ambiguous decode whose runner-up sits at the *same*
/// count (margin 0). Otherwise the second-highest distinct sum keeps one table
/// at its max and drops the other to its second-distinct count level:
/// `max(mc_h + second_v, second_h + mc_v)`. A missing second level (`None`)
/// means that table is constant and offers no competitor on that side. Returns
/// `None` only when neither side offers any competitor (a degenerate
/// single-count-value table pair, e.g. a single observation).
fn within_transform_runner_up(
    transform: GridTransform,
    h_tab: &LexMax,
    v_tab: &LexMax,
    mc_h: u32,
    mc_v: u32,
    winner_origin: Option<(i32, i32)>,
) -> Option<HardRunnerUp> {
    // Case A: ≥2 distinct origins at the joint maximum count → runner-up at the
    // same count as the winner (margin 0, a genuinely ambiguous decode). The
    // gate only consumes the count; for the diagnostic origin we prefer a
    // distinct cell drawn from the weight-max `classes` (the common case), and
    // fall back to the winner's own origin when the count tie is realized only
    // by lower-weight cells (a rare degenerate path).
    if h_tab.n_at_max_count.saturating_mul(v_tab.n_at_max_count) > 1 {
        let (wr, wc) = winner_origin.unwrap_or((i32::MIN, i32::MIN));
        // Prefer a distinct origin from the weight-max classes (cheap, common).
        for &(ha, hb) in &h_tab.classes {
            for &(va, vb) in &v_tab.classes {
                let mr = crt_master_row(va, ha);
                let mc = crt_master_col(hb, vb);
                if (mr, mc) != (wr, wc) {
                    return Some(HardRunnerUp {
                        matched: mc_h + mc_v,
                        master_row: mr,
                        master_col: mc,
                        transform,
                    });
                }
            }
        }
        // All weight-max product cells coincide with the winner but the count
        // tie comes from lower-weight cells: the competing origin still sits at
        // the full max count. Report it with the winner's origin as the
        // representative (the gate only consumes the count; the diagnostic
        // origin is best-effort here, a rare degenerate path).
        return Some(HardRunnerUp {
            matched: mc_h + mc_v,
            master_row: wr,
            master_col: wc,
            transform,
        });
    }

    // Case B: second-highest distinct sum = max-of-one + second-of-other.
    let from_v = v_tab.second_count.map(|sv| {
        // Keep H at its max class, drop V to a second-level class.
        let (ha, hb) = h_tab.classes[0];
        let (va, vb) = v_tab
            .second_class
            .expect("second_count implies second_class");
        HardRunnerUp {
            matched: mc_h + sv,
            master_row: crt_master_row(va, ha),
            master_col: crt_master_col(hb, vb),
            transform,
        }
    });
    let from_h = h_tab.second_count.map(|sh| {
        let (va, vb) = v_tab.classes[0];
        let (ha, hb) = h_tab
            .second_class
            .expect("second_count implies second_class");
        HardRunnerUp {
            matched: sh + mc_v,
            master_row: crt_master_row(va, ha),
            master_col: crt_master_col(hb, vb),
            transform,
        }
    });
    match (from_h, from_v) {
        (Some(h), Some(v)) => Some(if h.matched >= v.matched { h } else { v }),
        (Some(h), None) => Some(h),
        (None, Some(v)) => Some(v),
        (None, None) => None,
    }
}

/// Assemble the global uniqueness runner-up across all transforms.
///
/// The runner-up matched count is `max(within_runner of the winning transform,
/// best_count of every *other* transform)`. A different transform yielding a
/// D4-equivalent labeling legitimately competes (a fragment too small to break
/// D4 symmetry cannot pin an absolute orientation and must be rejected).
fn assemble_global_runner_up(
    summaries: &[TransformSummary],
    winner_idx: Option<usize>,
) -> Option<HardRunnerUp> {
    let winner_idx = winner_idx?;
    let mut runner: Option<HardRunnerUp> = summaries[winner_idx].within_runner;
    for (idx, s) in summaries.iter().enumerate() {
        if idx == winner_idx {
            continue;
        }
        let candidate = HardRunnerUp {
            matched: s.best_count,
            // A degenerate transform with no optimal origin (impossible for a
            // non-empty observation set) contributes count 0 at a placeholder.
            master_row: s.best_origin.map_or(0, |(r, _)| r),
            master_col: s.best_origin.map_or(0, |(_, c)| c),
            transform: s.transform,
        };
        let better = match runner {
            None => true,
            Some(r) => candidate.matched > r.matched,
        };
        if better {
            runner = Some(candidate);
        }
    }
    runner
}

/// Lexicographic-max summary of one precompute table, plus the second-distinct
/// *count* level used by the uniqueness gate.
struct LexMax {
    /// Highest matched count over the table.
    max_count: u32,
    /// Highest weight among cells at `max_count` (the lexicographic-max weight).
    max_weight: f32,
    /// Cells matching exactly `(max_count, max_weight)`. Drives the winner
    /// selection's strict-`>` first-seen tie-break — must stay weight-restricted
    /// for byte-exactness with the original scan.
    classes: Vec<(usize, usize)>,
    /// Number of cells whose *count* equals `max_count` (weight ignored). Two
    /// cells at the same count are two distinct competing origins for the
    /// uniqueness gate, even if their weights differ.
    n_at_max_count: usize,
    /// Largest count strictly below `max_count`, if any.
    second_count: Option<u32>,
    /// A representative `(a, b)` cell at `second_count` (row-major first).
    second_class: Option<(usize, usize)>,
}

/// Find the lexicographic-max `(count, weight)` over a precompute table, collect
/// the `(max_count, max_weight)` classes (for the winner tie-break), and report
/// the second-distinct *count* level (for the uniqueness gate).
///
/// `cols` is the table's column count (both tables happen to be length 501, so
/// it cannot be inferred from the slice length — the H table is `H_ROWS ×
/// H_COLS` and the V table is `V_ROWS × V_COLS`). The weight comparison is exact
/// `==` so the collected `classes` set matches the original scan's strict-`>`
/// tie-break.
fn lex_max_classes(count: &[u32], weight: &[f32], cols: usize) -> LexMax {
    debug_assert_eq!(count.len(), weight.len());
    // First pass: lexicographic max of (count, weight).
    let mut max_count = 0u32;
    let mut max_weight = f32::NEG_INFINITY;
    for (i, &c) in count.iter().enumerate() {
        let w = weight[i];
        if c > max_count || (c == max_count && w > max_weight) {
            max_count = c;
            max_weight = w;
        }
    }
    // Second pass: collect `(max_count, max_weight)` classes, count the cells at
    // `max_count` (weight ignored), and find the second-distinct count level
    // with a row-major-first representative.
    let mut classes = Vec::new();
    let mut n_at_max_count = 0usize;
    let mut second_count: Option<u32> = None;
    let mut second_class: Option<(usize, usize)> = None;
    for (i, &c) in count.iter().enumerate() {
        if c == max_count {
            n_at_max_count += 1;
            if weight[i] == max_weight {
                classes.push((i / cols, i % cols));
            }
        } else {
            // Track the largest count strictly below the max.
            let take = match second_count {
                None => true,
                Some(s) => c > s,
            };
            if take {
                second_count = Some(c);
                second_class = Some((i / cols, i % cols));
            }
        }
    }
    LexMax {
        max_count,
        max_weight,
        classes,
        n_at_max_count,
        second_count,
        second_class,
    }
}

/// Borrowed view over the four per-transform precompute tables, bundled so the
/// direct-scan fallback stays under the workspace argument limit.
///
/// `h_count` / `v_count` are the per-class matched-bit *counts* (`+= 1` on
/// `expected == bit`); `h_match` / `v_match` are the summed confidence *weights*
/// of matched observations (`+= conf`). The soft full-master scan
/// ([`super::soft::decode_soft`]) builds byte-identical tables as a side effect
/// (its `h_match`/`v_match` count tables and `h_match_conf`/`v_match_conf` weight
/// tables — same `transform_edge_lookup`, observation order, and `rem_euclid`
/// loop), so it feeds them straight into [`HardScan::fold`] to drive the
/// matched-count uniqueness gate without a second precompute pass.
pub(crate) struct TransformTables<'a> {
    pub(crate) h_count: &'a [u32],
    pub(crate) h_weight: &'a [f32],
    pub(crate) v_count: &'a [u32],
    pub(crate) v_weight: &'a [f32],
}

/// Fallback direct scan over all 501² origins for a single transform, using
/// the precomputed tables. Byte-identical to the original inner loop; only
/// invoked when the separated optimal set is pathologically large.
///
/// Returns `true` if any of this transform's origins became the reigning best
/// (so the caller can attribute the global winner to this transform for the
/// uniqueness runner-up).
fn scan_transform_direct(
    transform: GridTransform,
    tables: &TransformTables<'_>,
    total: usize,
    total_conf: f32,
    max_bit_error_rate: f32,
    best: &mut Option<DecodeOutcome>,
) -> bool {
    let mut won_any = false;
    for master_row in 0..MASTER_ROWS as i32 {
        let ha = (master_row % H_ROWS as i32) as usize;
        let va = (master_row % V_ROWS as i32) as usize;
        for master_col in 0..MASTER_COLS as i32 {
            let hb = (master_col % H_COLS as i32) as usize;
            let vb = (master_col % V_COLS as i32) as usize;

            let matched =
                (tables.h_count[ha * H_COLS + hb] + tables.v_count[va * V_COLS + vb]) as usize;
            let weighted = tables.h_weight[ha * H_COLS + hb] + tables.v_weight[va * V_COLS + vb];

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
            let candidate = DecodeOutcome {
                alignment: transform.with_translation([master_col, master_row]),
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
            won_any |= update_best_candidate(best, candidate);
        }
    }
    won_any
}
