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

pub(crate) use hard::{decode, decode_fixed_board};
pub(crate) use soft::{decode_fixed_board_soft, decode_soft};

/// Cyclic-period sizes for the precompute tables.
///
/// Horizontal edges use map_b (167×3); vertical edges use map_a (3×167).
/// (Matches authors' convention: hfullCode from code2/map_b, vfullCode from code1/map_a.)
const H_ROWS: usize = EDGE_MAP_B_ROWS; // 167
const H_COLS: usize = EDGE_MAP_B_COLS; // 3
const V_ROWS: usize = EDGE_MAP_A_ROWS; // 3
const V_COLS: usize = EDGE_MAP_A_COLS; // 167

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
