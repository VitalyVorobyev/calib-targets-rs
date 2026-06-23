//! Hard-weighted (count-and-confidence) decoders.
//!
//! Rank hypotheses lexicographically by `(edges_matched, weighted_score)`
//! and reject anything above `max_bit_error_rate`. [`decode`] sweeps the
//! full 501 × 501 master via the cyclic-period precompute; [`decode_fixed_board`]
//! constrains the sweep to a declared board's own bit pattern.

use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};

use crate::board::{MASTER_COLS, MASTER_ROWS};
use crate::code_maps::{
    horizontal_edge_bit, vertical_edge_bit, EdgeOrientation, PuzzleBoardObservedEdge,
};

use super::{
    crt_master_col, crt_master_row, finalize_hard_winner, transform_edge_lookup,
    update_best_candidate, DecodeOutcome, HardRunnerUp, H_COLS, H_ROWS, V_COLS, V_ROWS,
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

/// Match observations directly against the declared board's own bit pattern.
///
/// For each of the 8 D4 transforms and every shift `(P_r, P_c) ∈ [0, rows] ×
/// [0, cols]` (chessboard-local `(0, 0)` sitting at print-corner
/// `(P_r, P_c)`), score observations against the board-local horizontal and
/// vertical bit tables. Observations whose inferred cell falls outside the
/// board don't vote.
///
/// Observation convention:
/// - a horizontal edge anchored at local corner `(c, r)` samples lookup cell `(r-1, c)`
/// - a vertical edge anchored at local corner `(c, r)` samples lookup cell `(r, c-1)`
///
/// Those lookup offsets live in the original observation frame and must be
/// transformed together with the edge under D4.
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
    let (winner, best_matched, runner_up) = decode_fixed_board_with_runner_up(
        observed,
        spec_origin_row,
        spec_origin_col,
        rows,
        cols,
        max_bit_error_rate,
    )?;
    finalize_hard_winner(winner, best_matched, runner_up)
}

/// Core of [`decode_fixed_board`]: the winning shift plus its matched-bit count
/// and closest competitor, *before* the uniqueness gate. See
/// [`decode_with_runner_up`] for the rationale (test access to the pre-gate
/// runner-up).
pub(crate) fn decode_fixed_board_with_runner_up(
    observed: &[PuzzleBoardObservedEdge],
    spec_origin_row: u32,
    spec_origin_col: u32,
    rows: u32,
    cols: u32,
    max_bit_error_rate: f32,
) -> Option<(DecodeOutcome, u32, Option<HardRunnerUp>)> {
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
    // Uniqueness runner-up: the highest matched-bit count over any shift /
    // transform *other than the winning one* (tracked regardless of the BER
    // gate, since a high-matching competitor threatens uniqueness even when it
    // would itself be BER-rejected). The winner's exact identity moves during
    // the scan, so on each displacement the old best demotes into the runner.
    let mut runner_up: Option<HardRunnerUp> = None;

    for transform in GRID_TRANSFORMS_D4.iter().copied() {
        // Transform all lookup coordinates into this D4 frame once.
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

        // Bounds on (P_r, P_c) such that *every* observation lands on the
        // board. For partial-view captures we still need to consider shifts
        // where only a subset lands on-board, so widen by a small margin
        // (observations off the board just don't vote).
        let (lr_min, lr_max) = transformed
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), &(lr, _, _, _, _)| {
                (lo.min(lr), hi.max(lr))
            });
        let (lc_min, lc_max) = transformed
            .iter()
            .fold((i32::MAX, i32::MIN), |(lo, hi), &(_, lc, _, _, _)| {
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
                let mut matched = 0usize;
                let mut weighted = 0.0f32;
                for &(lookup_row, lookup_col, orient, bit, conf) in &transformed {
                    let expected = match orient {
                        EdgeOrientation::Horizontal => {
                            let cr = p_r + lookup_row;
                            let cc = p_c + lookup_col;
                            if cr < 0 || cr >= h_rows as i32 || cc < 0 || cc >= h_cols as i32 {
                                continue;
                            }
                            h_bit[cr as usize * h_cols + cc as usize]
                        }
                        EdgeOrientation::Vertical => {
                            let cr = p_r + lookup_row;
                            let cc = p_c + lookup_col;
                            if cr < 0 || cr >= v_rows as i32 || cc < 0 || cc >= v_cols as i32 {
                                continue;
                            }
                            v_bit[cr as usize * v_cols + cc as usize]
                        }
                    };
                    if expected == bit {
                        matched += 1;
                        weighted += conf;
                    }
                }
                let master_row = spec_or + p_r;
                let master_col = spec_oc + p_c;
                let bit_error_rate = if total == 0 {
                    1.0
                } else {
                    (total - matched) as f32 / total as f32
                };

                // Would this shift become the reigning best? (BER-gated, then
                // ranked by the same `(edges_matched, weighted_score)` rule as
                // the full-board path.) The displaced old best, or any
                // non-winning shift, competes for the uniqueness runner-up.
                let score = weighted / total_conf;
                let becomes_best = bit_error_rate <= max_bit_error_rate
                    && match &best {
                        None => true,
                        Some(cur) => {
                            matched > cur.edges_matched
                                || (matched == cur.edges_matched && score > cur.weighted_score)
                        }
                    };

                if becomes_best {
                    // Demote the outgoing winner into the runner-up race.
                    if let Some(prev) = &best {
                        demote_into_runner(
                            &mut runner_up,
                            prev.edges_matched as u32,
                            prev.master_origin_row,
                            prev.master_origin_col,
                            prev.alignment.transform,
                        );
                    }
                    let mean_confidence = if matched == 0 {
                        0.0
                    } else {
                        weighted / matched as f32
                    };
                    best = Some(DecodeOutcome {
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
                        score_best: score,
                        score_runner_up: None,
                        score_margin: f32::INFINITY,
                        runner_up_origin_row: None,
                        runner_up_origin_col: None,
                        runner_up_transform: None,
                    });
                } else {
                    demote_into_runner(
                        &mut runner_up,
                        matched as u32,
                        master_row,
                        master_col,
                        transform,
                    );
                }
            }
        }
    }

    let winner = best?;
    let best_matched = winner.edges_matched as u32;
    Some((winner, best_matched, runner_up))
}

/// Update the uniqueness runner-up slot if `matched` exceeds the count it
/// currently holds. Shared by the fixed-board shift scan (both the demoted
/// outgoing winner and every non-winning shift route through here).
fn demote_into_runner(
    runner_up: &mut Option<HardRunnerUp>,
    matched: u32,
    master_row: i32,
    master_col: i32,
    transform: GridTransform,
) {
    let better = match runner_up {
        None => true,
        Some(r) => matched > r.matched,
    };
    if better {
        *runner_up = Some(HardRunnerUp {
            matched,
            master_row,
            master_col,
            transform,
        });
    }
}

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

    let mut best: Option<DecodeOutcome> = None;
    // Index into `best` of the transform that currently owns the winner, and
    // the per-transform `(best_count, within_runner)` summaries used to assemble
    // the global runner-up after the scan. `within_runner` is the closest
    // *distinct* competing origin within a single transform (a missing second
    // level is `None`); the cross-transform competitor is the `best_count` of
    // every *other* transform. See the runner-up assembly below the loop.
    let mut winner_transform_idx: Option<usize> = None;
    let mut transform_summaries: Vec<TransformSummary> = Vec::with_capacity(8);

    // Scratch buffers for the precompute tables — allocated once, cleared per transform.
    // h_match[a * H_COLS + b]: sum of confidences for horizontal lookups that match at class (a, b).
    // h_count[a * H_COLS + b]: number of matching horizontal lookups at class (a, b).
    // v_match[a * V_COLS + b]: sum of confidences for vertical lookups that match at class (a, b).
    // v_count[a * V_COLS + b]: number of matching vertical lookups at class (a, b).
    let mut h_match = vec![0.0f32; H_ROWS * H_COLS];
    let mut h_count = vec![0u32; H_ROWS * H_COLS];
    let mut v_match = vec![0.0f32; V_ROWS * V_COLS];
    let mut v_count = vec![0u32; V_ROWS * V_COLS];

    for (transform_idx, transform) in GRID_TRANSFORMS_D4.iter().copied().enumerate() {
        // Transform all lookup coordinates once.
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

        // Clear scratch buffers.
        h_match.fill(0.0);
        h_count.fill(0);
        v_match.fill(0.0);
        v_count.fill(0);

        // Build the H and V precompute tables.
        //
        // For each transformed lookup `(lr, lc, orient, bit, conf)` we want to know,
        // for every master origin `(mr, mc)`, whether `expected_bit(mr+lr, mc+lc) == bit`.
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

        for &(lookup_row, lookup_col, orient, bit, conf) in &transformed {
            match orient {
                EdgeOrientation::Horizontal => {
                    // For horizontal lookups, the relevant master map is B (167×3).
                    for r in 0..H_ROWS {
                        let a = (r as i32 - lookup_row).rem_euclid(H_ROWS as i32) as usize;
                        for c in 0..H_COLS {
                            let b = (c as i32 - lookup_col).rem_euclid(H_COLS as i32) as usize;
                            let expected = horizontal_edge_bit(r as i32, c as i32);
                            if expected == bit {
                                h_match[a * H_COLS + b] += conf;
                                h_count[a * H_COLS + b] += 1;
                            }
                        }
                    }
                }
                EdgeOrientation::Vertical => {
                    // For vertical lookups, the relevant master map is A (3×167).
                    for r in 0..V_ROWS {
                        let a = (r as i32 - lookup_row).rem_euclid(V_ROWS as i32) as usize;
                        for c in 0..V_COLS {
                            let b = (c as i32 - lookup_col).rem_euclid(V_COLS as i32) as usize;
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
        let h_tab = lex_max_classes(&h_count, &h_match, H_COLS);
        let v_tab = lex_max_classes(&v_count, &v_match, V_COLS);
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
        transform_summaries.push(TransformSummary {
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
            continue;
        }

        // Step 3: worst-case guard. Real inputs decode uniquely (product == 1);
        // only degenerate all-tied inputs blow this up. Fall back to a direct
        // table scan for this transform only so cost never exceeds the original.
        if optimal_h.len().saturating_mul(optimal_v.len()) > SEPARATION_PRODUCT_CAP {
            let tables = TransformTables {
                h_count: &h_count,
                h_match: &h_match,
                v_count: &v_count,
                v_match: &v_match,
            };
            if scan_transform_direct(
                transform,
                &tables,
                total,
                total_conf,
                max_bit_error_rate,
                &mut best,
            ) {
                winner_transform_idx = Some(transform_idx);
            }
            continue;
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
            alignment: GridAlignment {
                transform,
                // translation[0] is the i (col) offset, translation[1]
                // is the j (row) offset, so master_col goes first.
                translation: [master_col, master_row],
            },
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
        if update_best_candidate(&mut best, candidate) {
            winner_transform_idx = Some(transform_idx);
        }
    }

    let winner = best?;
    let runner_up = assemble_global_runner_up(&transform_summaries, winner_transform_idx);
    let best_matched = winner.edges_matched as u32;
    Some((winner, best_matched, runner_up))
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
struct TransformTables<'a> {
    h_count: &'a [u32],
    h_match: &'a [f32],
    v_count: &'a [u32],
    v_match: &'a [f32],
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
            let weighted = tables.h_match[ha * H_COLS + hb] + tables.v_match[va * V_COLS + vb];

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
