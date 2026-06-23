//! Cross-correlate observed edge bits against the master code maps.
//!
//! For each of the 8 D4 transforms and every possible master origin
//! `(I0, J0) ∈ [0, 501) × [0, 501)`, score the observed edge bits against
//! the expected master maps. Pick the `(transform, origin)` with highest
//! confidence-weighted match rate.
//!
//! ## Module layout
//!
//! - [`hard`] — the hard-weighted (count + confidence) decoders [`decode`] and
//!   [`decode_fixed_board`].
//! - [`soft`] — the soft-log-likelihood decoders [`decode_soft`] and
//!   [`decode_fixed_board_soft`].
//! - This module owns the shared scaffolding both halves depend on: the
//!   [`DecodeOutcome`] carrier, the [`SoftLlConfig`] knobs, the D4 lookup
//!   transform, the candidate-ranking helpers, and the per-bit
//!   log-likelihood pair.
//!
//! ## Fast-path via cyclic-period precompute (C3)
//!
//! The master maps have cyclic structure (matching PStelldinger/PuzzleBoard convention):
//! - horizontal edge bit at `(mr, mc)` = `map_b[(mr % 167, mc % 3)]`
//! - vertical edge bit at `(mr, mc)` = `map_a[(mr % 3, mc % 167)]`
//!
//! For transformed lookup coordinates `{(lr, lc, orient, bit, conf)}`, the score
//! at master origin `(mr, mc)` is:
//!
//! ```text
//! score(mr, mc) = H[(mr % 3, mc % 167)] + V[(mr % 167, mc % 3)]
//! ```
//!
//! where `H` is a `3 × 167` table and `V` is a `167 × 3` table precomputed
//! **once per D4 transform** in `O(501 × N)`.  The 501² origin loop then
//! becomes `O(501²)` with two table lookups — no per-observation work.

use calib_targets_core::{log_sigmoid, GridAlignment, GridTransform};

use crate::code_maps::{
    EdgeOrientation, PuzzleBoardObservedEdge, EDGE_MAP_A_COLS, EDGE_MAP_A_ROWS, EDGE_MAP_B_COLS,
    EDGE_MAP_B_ROWS,
};

mod hard;
mod soft;

#[cfg(test)]
mod tests;

pub(crate) use hard::{
    decode, decode_fixed_board, decode_fixed_board_with_runner_up, decode_with_runner_up,
};
pub(crate) use soft::{decode_fixed_board_soft, decode_soft};

/// Cyclic-period sizes for the precompute tables.
///
/// Horizontal edges use map_b (167×3); vertical edges use map_a (3×167).
/// (Matches authors' convention: hfullCode from code2/map_b, vfullCode from code1/map_a.)
const H_ROWS: usize = EDGE_MAP_B_ROWS; // 167
const H_COLS: usize = EDGE_MAP_B_COLS; // 3
const V_ROWS: usize = EDGE_MAP_A_ROWS; // 3
const V_COLS: usize = EDGE_MAP_A_COLS; // 167

/// Recover a master row from its two CRT residues.
///
/// `mr = (334·va + 168·ha) mod 501` is the Chinese-Remainder inverse for the
/// pair `(va, ha) = (mr % 3, mr % 167)`: it satisfies `mr % 3 == va` and
/// `mr % 167 == ha`. Because `501 = 3·167` with `gcd(3, 167) = 1`, the map
/// `mr ↔ (va, ha)` is a bijection over `[0, 501)`.
#[inline]
fn crt_master_row(va: usize, ha: usize) -> i32 {
    (334 * va as i32 + 168 * ha as i32).rem_euclid(MASTER_ROWS_I32)
}

/// Recover a master col from its two CRT residues.
///
/// `mc = (334·hb + 168·vb) mod 501` is the Chinese-Remainder inverse for the
/// pair `(hb, vb) = (mc % 3, mc % 167)`: it satisfies `mc % 3 == hb` and
/// `mc % 167 == vb`. Bijective over `[0, 501)` by the same CRT argument.
#[inline]
fn crt_master_col(hb: usize, vb: usize) -> i32 {
    (334 * hb as i32 + 168 * vb as i32).rem_euclid(MASTER_COLS_I32)
}

const MASTER_ROWS_I32: i32 = crate::board::MASTER_ROWS as i32;
const MASTER_COLS_I32: i32 = crate::board::MASTER_COLS as i32;

/// The parameter-free uniqueness predicate, shared by the hard and soft paths.
///
/// Accept iff `margin > k_winner`, where `margin = best_matched −
/// runner_up_matched` and `k_winner = edges_observed − best_matched` (the
/// winner's own mismatch count). Equivalently, the winner's net score
/// (`matched − mismatched`) must strictly exceed the runner-up's matched count:
/// `2·best_matched − runner_up_matched > edges_observed`.
///
/// `best_matched` is the winning origin's matched-bit count; `runner_up_matched`
/// is the highest matched-bit count of any *distinct* competing origin (across
/// all D4 transforms). `margin` is computed with saturating subtraction, so when
/// a distinct origin out-matches the winner (`runner_up_matched ≥ best_matched`,
/// which can happen when the soft-LL winner is not the maximum-matched origin)
/// the margin is `0` and the decode is correctly rejected.
///
/// **Why this and not a `C·√N` magnitude threshold:** the master edge code has
/// minimum Hamming distance `d(w)` that grows ~quadratically with window size
/// but is only `1` at the `4×4` minimum window (zero error-correction). A
/// magnitude threshold either rejects clean small *unique* patches (margin `1`,
/// `k_winner = 0`) or accepts corrupted small *alias* patches (a 1-bit-corrupted
/// `4×4` is frequently a perfect read of a *different* master location:
/// `best_matched = N`, `k_winner = 0`, `margin = 1`, but wrong). `margin >
/// k_winner` rejects the alias case (the perfect alias still only beats the true
/// origin by `1`, while `k_winner = 0` demands the winner be *strictly* perfect
/// *and* uniquely so) — but because `d(4) = 1`, no acceptance test can make the
/// `4×4` window safe under noise, so the pipeline pairs this gate with a
/// `min_window` sized to the code's distance for the BER budget (bounded-distance
/// decoding). At those window sizes a worst-case guarantee is unachievable
/// (`d` would need to exceed `2·⌊BER·N⌋ ≈ 0.8N`, far above the ~`N/4` the code
/// provides), so safety is empirical-with-defense-in-depth: `min_window` keeps
/// random corruption below the aliasing regime, and this gate catches the
/// residual near-aliases.
#[inline]
fn passes_uniqueness_gate(
    edges_observed: usize,
    best_matched: u32,
    runner_up_matched: u32,
) -> bool {
    let margin = best_matched.saturating_sub(runner_up_matched);
    let k_winner = (edges_observed as u32).saturating_sub(best_matched);
    margin > k_winner
}

/// Apply the parameter-free uniqueness gate to a hard-path winner.
///
/// `best_matched` is the winning origin's matched-bit count; `runner_up_matched`
/// is the highest matched-bit count of any *distinct* competing origin (across
/// all D4 transforms — D4-equivalent labelings of a fragment too small to break
/// the symmetry legitimately compete here).
///
/// **Criterion (no magic constant):** accept iff
///
/// ```text
///   margin > k_winner
/// ```
///
/// where `margin = best_matched − runner_up_matched` and `k_winner =
/// edges_observed − best_matched` is the winner's own mismatch count.
/// Equivalently `2·best_matched − runner_up_matched > edges_observed`: the
/// winner's *net* score (matched − mismatched) must strictly beat the
/// runner-up's matched count.
///
/// The decode is trustworthy only when the best hypothesis is strictly closer
/// to a *perfect* read than to its nearest competitor. This separates two
/// distinct failure modes that a single-magnitude margin threshold conflates:
///
/// - A **clean exact** read (`k_winner = 0`) passes at any `margin ≥ 1`, so the
///   board's exact-uniqueness design is honored at *any* fragment size down to
///   the pipeline's `min_window` — even a 4×4 patch, whose true origin out-votes
///   every alias by ≥1 bit, decodes.
/// - A **noisy-ambiguous** read fails: when the observation is corrupted enough
///   that a wrong origin matches nearly as many bits (`margin` small) *and* the
///   winner itself mismatches many bits (`k_winner` large), the winner is not
///   meaningfully closer to a perfect read than the competitor, and the decode
///   declines (a miss). The witnessed false positive — a heavily distorted board
///   where `margin = 0` while `k_winner` is large — is rejected here.
///
/// Rejection returns `None`, exactly like a BER-gate failure — a miss, never a
/// wrong label; the detection contract forbids wrong labels far more strongly
/// than misses. On acceptance the runner-up diagnostic fields are populated and
/// `score_margin` carries the integer bit margin (`best_matched −
/// runner_up_matched`) as `f32`; the winner's own origin / weighted score are
/// left untouched.
///
/// The gate is mode-independent — it is a property of `(observed bits, winning
/// origin)`, not of how the winner was scored — so the soft-LL path applies the
/// identical predicate via [`passes_uniqueness_gate`] over its own
/// matched-count runner-up (the soft `alignment_min_margin` score-gap does *not*
/// enforce origin uniqueness; measured to false-accept at every window size).
fn finalize_hard_winner(
    mut winner: DecodeOutcome,
    best_matched: u32,
    runner_up: Option<HardRunnerUp>,
) -> Option<DecodeOutcome> {
    let runner_up_matched = runner_up.as_ref().map_or(0, |r| r.matched);
    if !passes_uniqueness_gate(winner.edges_observed, best_matched, runner_up_matched) {
        return None;
    }
    let margin = best_matched.saturating_sub(runner_up_matched);
    match runner_up {
        Some(r) => {
            winner.score_runner_up = Some(r.matched as f32);
            winner.score_margin = margin as f32;
            winner.runner_up_origin_row = Some(r.master_row);
            winner.runner_up_origin_col = Some(r.master_col);
            winner.runner_up_transform = Some(r.transform);
        }
        None => {
            // No competing origin at all (degenerate: a single observation, or
            // a master with a unique total match). Margin is the full count.
            winner.score_runner_up = None;
            winner.score_margin = margin as f32;
            winner.runner_up_origin_row = None;
            winner.runner_up_origin_col = None;
            winner.runner_up_transform = None;
        }
    }
    Some(winner)
}

/// The closest competing origin to a hard-path winner: its matched-bit count
/// and the master origin / transform that realizes it. Used to populate the
/// winner's runner-up diagnostic fields and drive the uniqueness gate.
#[derive(Clone, Copy, Debug)]
pub(crate) struct HardRunnerUp {
    pub matched: u32,
    pub master_row: i32,
    pub master_col: i32,
    pub transform: GridTransform,
}

/// Apply the matched-count uniqueness gate to a *soft-LL* winner.
///
/// The soft scorer ranks by summed log-likelihood, whose `alignment_min_margin`
/// gate does **not** enforce origin uniqueness — measured to false-accept a
/// wrong physical origin at every window size. This re-gates the soft winner by
/// the same matched-count predicate the hard path uses ([`passes_uniqueness_gate`]).
///
/// `matched_top2` is the global matched-count top-2 over the *same* candidate
/// set the soft winner was drawn from (full master or fixed-board shifts): the
/// maximum-matched origin (with its identity and matched count) and the closest
/// distinct competitor. The competitor count for the soft winner is:
///
/// - `runner.matched` when the soft winner *is* the maximum-matched origin, or
/// - `best_matched` (the maximum) otherwise — a distinct origin out-matches the
///   soft winner, which saturates the margin to `0` and rejects (correct: the
///   soft winner is not even the best-supported origin).
///
/// On acceptance, the soft winner's runner-up diagnostic fields are overwritten
/// with the matched-count competitor (so consumers see the uniqueness margin,
/// not the soft-score gap), and `score_best` / `weighted_score` (the soft score)
/// are preserved.
fn apply_soft_uniqueness_gate(
    mut winner: DecodeOutcome,
    matched_top2: (u32, i32, i32, GridTransform, Option<HardRunnerUp>),
) -> Option<DecodeOutcome> {
    let (best_matched, best_row, best_col, best_transform, runner) = matched_top2;
    let soft_matched = winner.edges_matched as u32;
    let soft_is_global_best = winner.master_origin_row == best_row
        && winner.master_origin_col == best_col
        && winner.alignment.transform == best_transform;
    let (competitor_matched, competitor) = if soft_is_global_best {
        (runner.map_or(0, |r| r.matched), runner)
    } else {
        (
            best_matched,
            Some(HardRunnerUp {
                matched: best_matched,
                master_row: best_row,
                master_col: best_col,
                transform: best_transform,
            }),
        )
    };
    if !passes_uniqueness_gate(winner.edges_observed, soft_matched, competitor_matched) {
        return None;
    }
    // Surface the matched-count uniqueness margin in the runner-up diagnostics
    // (overwriting the soft-score-gap runner-up populated by the LL finalizer).
    match competitor {
        Some(c) => {
            winner.score_runner_up = Some(c.matched as f32);
            winner.runner_up_origin_row = Some(c.master_row);
            winner.runner_up_origin_col = Some(c.master_col);
            winner.runner_up_transform = Some(c.transform);
        }
        None => {
            winner.score_runner_up = None;
            winner.runner_up_origin_row = None;
            winner.runner_up_origin_col = None;
            winner.runner_up_transform = None;
        }
    }
    Some(winner)
}

/// Tuning knobs for the soft-log-likelihood scorer. See [`soft::decode_soft`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct SoftLlConfig {
    /// Per-bit logit slope. `logit = kappa * confidence` at a clean match.
    pub kappa: f32,
    /// Lower bound applied to each per-bit `log_sigmoid` contribution so a
    /// single catastrophically wrong bit cannot dominate the hypothesis score.
    pub per_bit_floor: f32,
    /// Minimum per-observation score gap between winner and runner-up.
    /// Hypotheses that do not clear this gate are rejected.
    pub alignment_min_margin: f32,
}

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
    /// Soft-LL raw score for the winning hypothesis; under hard-weighted
    /// scoring this mirrors `weighted_score` so downstream consumers see a
    /// single "best score" field regardless of mode.
    pub score_best: f32,
    /// Runner-up hypothesis score (soft-LL only; `None` under hard-weighted).
    pub score_runner_up: Option<f32>,
    /// Normalized per-observation score gap between winner and runner-up.
    /// Under hard-weighted scoring this is `f32::INFINITY`.
    pub score_margin: f32,
    pub runner_up_origin_row: Option<i32>,
    pub runner_up_origin_col: Option<i32>,
    pub runner_up_transform: Option<GridTransform>,
}

/// Rank `candidate` against the current `best`, replacing it on a win.
///
/// Returns `true` when `candidate` became the new best (caller uses this to
/// track which transform owns the winner for the uniqueness runner-up).
fn update_best_candidate(best: &mut Option<DecodeOutcome>, candidate: DecodeOutcome) -> bool {
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
    wins
}

/// Per-observation `(ll_match, ll_mismatch)` contributions under the
/// soft-log-likelihood scorer. `conf` is the per-bit confidence in `[0, 1]`.
///
/// Shares the numerically-stable transfer function with the ChArUco board
/// matcher via [`calib_targets_core::log_sigmoid`]; the symmetric
/// `kappa * confidence` logit and the per-bit floor are PuzzleBoard-specific
/// and stay here.
#[inline]
pub(crate) fn ll_pair(conf: f32, kappa: f32, floor: f32) -> (f32, f32) {
    let k = kappa * conf;
    (log_sigmoid(k).max(floor), log_sigmoid(-k).max(floor))
}

/// Rank candidates by `score_best`, maintaining the current winner and
/// runner-up in lock-step. Mirrors the two-slot update in
/// `calib-targets-charuco/src/detector/board_match.rs`.
fn update_best_and_runner_up(
    best: &mut Option<DecodeOutcome>,
    runner_up: &mut Option<DecodeOutcome>,
    candidate: DecodeOutcome,
) {
    match best {
        None => {
            *best = Some(candidate);
        }
        Some(current) => {
            if candidate.score_best > current.score_best {
                let old = best.take();
                *best = Some(candidate);
                *runner_up = old;
            } else {
                let beats_runner_up = match runner_up {
                    None => true,
                    Some(r) => candidate.score_best > r.score_best,
                };
                if beats_runner_up {
                    *runner_up = Some(candidate);
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TransformedEdgeLookup {
    lookup_row: i32,
    lookup_col: i32,
    orientation: EdgeOrientation,
}

fn transform_edge_lookup(
    edge: &PuzzleBoardObservedEdge,
    t: &GridTransform,
) -> TransformedEdgeLookup {
    // Convention: `edge.col = i`, `edge.row = j`.
    //
    // `PuzzleBoardObservedEdge` stores the canonical anchor for each edge in
    // its local frame:
    // - Horizontal edge: left endpoint `(c, r)` of segment `[(c, r), (c+1, r)]`
    // - Vertical edge:   top endpoint  `(c, r)` of segment `[(c, r), (c, r+1)]`
    //
    // After a D4 transform, sign-negating classes can swap which transformed
    // endpoint is the canonical left/top anchor. So:
    // 1. transform both endpoints
    // 2. pick the canonical anchor in the transformed frame
    // 3. apply the standard lookup offset there:
    //    - H -> cell above `(row-1, col)`
    //    - V -> cell left  `(row, col-1)`
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
        debug_assert_eq!(p0_col, p1_col);
        EdgeOrientation::Vertical
    };
    let (anchor_col, anchor_row) = match orientation {
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
    let (lookup_row, lookup_col) = match orientation {
        EdgeOrientation::Horizontal => (anchor_row - 1, anchor_col),
        EdgeOrientation::Vertical => (anchor_row, anchor_col - 1),
    };
    TransformedEdgeLookup {
        lookup_row,
        lookup_col,
        orientation,
    }
}
