//! Decoders for a *declared* board — a known sub-rectangle of the master.
//!
//! # The hypothesis space
//!
//! When the caller knows which board they printed, the origin is no longer
//! anywhere on the 501 × 501 master: it lies in the rectangle that board
//! occupies. Each hypothesis is a searched grid transform (see
//! [`PuzzleBoardSymmetryMode`](crate::PuzzleBoardSymmetryMode)) plus a shift
//! `(P_r, P_c)` placing the observed fragment's local `(0, 0)` at board corner
//! `(P_r, P_c)`, which is master origin `(spec_origin + P)`.
//!
//! Because a declared board is *cut from* the master, its bit at board cell
//! `(r, c)` is the master bit at `(spec_origin_row + r, spec_origin_col + c)`.
//! A fixed-board hypothesis is therefore the *same* scoring problem as a
//! full-master one, restricted to a rectangle of origins — so it reuses the
//! cyclic class tables (see [`super::tables`]) instead of correlating against a
//! separately materialised copy of the board's bits.
//!
//! # Only physically realisable shifts are scored
//!
//! An observation exists only where the detector saw a printed dot, and a dot
//! is only sampled when the full corner neighbourhood around it was detected
//! (`PuzzleBoardDetector::sample_all_edges`). Every observation therefore
//! *does* lie on the printed board, and any shift that places one of them
//! outside the board is a hypothesis that cannot describe the physical scene.
//! The scan is restricted to shifts under which every observation lands on the
//! board, which
//!
//! - makes each hypothesis cost two table lookups instead of `O(N)`, since
//!   with nothing off-board the score is exactly the class-table sum, and
//! - stops impossible placements from competing in the uniqueness gate, where
//!   they could only ever suppress a correct decode.
//!
//! # Cost
//!
//! Let `N` be the observation count, `T` the number of searched transforms,
//! and `L_r × L_c` the surviving shift rectangle. Per transform the precompute
//! walks `min(167, L_r) · min(3, L_c)` classes per horizontal observation (and
//! the transpose for vertical ones), then each shift costs `O(1)`:
//!
//! ```text
//! O( T · ( min(501, class cells) · N  +  L_r · L_c ) )
//! ```
//!
//! which is bounded by the full-master `O(T · 501 · N)` and falls strictly
//! below it as the declared board shrinks. A board spanning the whole master
//! reaches every class and converges to the full-master cost, as it must.

use calib_targets_core::GridTransform;

use crate::code_maps::PuzzleBoardObservedEdge;

use super::tables::{transform_observations, ClassRange, ClassTables, LookupExtent};
use super::{
    apply_soft_uniqueness_gate, dequantize_ll, finalize_hard_winner, update_best_and_runner_up,
    DecodeOutcome, HardRunnerUp, SoftLlConfig, H_COLS, V_COLS,
};

/// The declared board, as the decoders need it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BoardRect {
    /// Master row the board is cut from.
    pub origin_row: i32,
    /// Master column the board is cut from.
    pub origin_col: i32,
    /// Board height in squares.
    pub rows: i32,
    /// Board width in squares.
    pub cols: i32,
}

impl BoardRect {
    pub(crate) fn new(origin_row: u32, origin_col: u32, rows: u32, cols: u32) -> Self {
        Self {
            origin_row: origin_row as i32,
            origin_col: origin_col as i32,
            rows: rows as i32,
            cols: cols as i32,
        }
    }

    /// Inclusive shift range under which *every* observation lands on the
    /// board, or `None` when no such shift exists.
    ///
    /// A board of `rows × cols` squares carries `(rows - 1) × cols` horizontal
    /// edge cells and `rows × (cols - 1)` vertical ones; a lookup cell must
    /// fall inside the table its orientation reads. Shifts stay non-negative:
    /// the fragment's local `(0, 0)` is itself a detected corner, so it cannot
    /// sit off the board.
    fn shift_range(&self, extent: &LookupExtent) -> Option<((i32, i32), (i32, i32))> {
        let (mut r_lo, mut r_hi) = (0, i32::MAX);
        let (mut c_lo, mut c_hi) = (0, i32::MAX);
        if let Some((lr_lo, lr_hi, lc_lo, lc_hi)) = extent.horizontal {
            r_lo = r_lo.max(-lr_lo);
            r_hi = r_hi.min(self.rows - 2 - lr_hi);
            c_lo = c_lo.max(-lc_lo);
            c_hi = c_hi.min(self.cols - 1 - lc_hi);
        }
        if let Some((lr_lo, lr_hi, lc_lo, lc_hi)) = extent.vertical {
            r_lo = r_lo.max(-lr_lo);
            r_hi = r_hi.min(self.rows - 1 - lr_hi);
            c_lo = c_lo.max(-lc_lo);
            c_hi = c_hi.min(self.cols - 2 - lc_hi);
        }
        if r_lo > r_hi || c_lo > c_hi {
            return None;
        }
        Some(((r_lo, r_hi), (c_lo, c_hi)))
    }
}

/// One hypothesis' score, read straight out of the class tables.
struct ShiftScore {
    matched: usize,
    weight: f32,
    /// Fixed-point log-likelihood (see [`super::LL_SCALE`]); zero when the
    /// scan is not running the soft scorer.
    ll: i64,
}

/// Read the class-table entries for master origin `(master_row, master_col)`.
#[inline]
fn score_at(tables: &ClassTables, master_row: i32, master_col: i32, soft: bool) -> ShiftScore {
    let h = (master_row.rem_euclid(super::H_ROWS as i32) as usize) * H_COLS
        + master_col.rem_euclid(H_COLS as i32) as usize;
    let v = (master_row.rem_euclid(super::V_ROWS as i32) as usize) * V_COLS
        + master_col.rem_euclid(V_COLS as i32) as usize;
    ShiftScore {
        matched: (tables.h_count[h] + tables.v_count[v]) as usize,
        weight: tables.h_weight[h] + tables.v_weight[v],
        ll: if soft {
            tables.h_ll[h] + tables.v_ll[v]
        } else {
            0
        },
    }
}

/// Winner / runner-up state accumulated over the whole shift scan.
///
/// The hard halves are always tracked: the soft scorer's uniqueness gate is a
/// *matched-count* predicate, so a soft decode needs them too and gets them
/// from this one pass rather than from a second scan.
struct FixedScan {
    hard_best: Option<DecodeOutcome>,
    hard_runner: Option<HardRunnerUp>,
    soft_best: Option<DecodeOutcome>,
    soft_runner: Option<DecodeOutcome>,
}

/// Everything the per-shift ranking needs that does not vary with the shift.
///
/// The two rankings run in different alphabets and each carries its own
/// denominator. The matched-count / BER ranking is evaluated over **logical
/// bits** — one entry per distinct master bit, after the period-3 replicas have
/// been voted — because both the BER budget and the uniqueness predicate are
/// statements about the code, not about how many times it was sampled. The
/// soft-LL ranking is evaluated over the **physical dots**, whose per-dot
/// likelihoods are exactly what it exists to sum.
struct ScanContext {
    /// Logical-bit count: denominator of the matched-count ranking and the BER.
    logical_total: usize,
    /// Summed confidence over the logical bits.
    logical_conf: f32,
    /// Physical dot count: denominator of the soft-LL normalisation.
    physical_total: usize,
    max_bit_error_rate: f32,
    soft: bool,
}

impl FixedScan {
    fn new() -> Self {
        Self {
            hard_best: None,
            hard_runner: None,
            soft_best: None,
            soft_runner: None,
        }
    }

    /// Rank one hypothesis into the hard (and, when enabled, soft) slots.
    ///
    /// `logical` scores the voted class set; `physical` scores the raw dots and
    /// is present only when the soft scorer is running.
    fn offer(
        &mut self,
        transform: GridTransform,
        master_row: i32,
        master_col: i32,
        logical: &ShiftScore,
        physical: Option<&ShiftScore>,
        ctx: &ScanContext,
    ) {
        let matched = logical.matched;
        let bit_error_rate = (ctx.logical_total - matched) as f32 / ctx.logical_total as f32;
        let mean_confidence = if matched == 0 {
            0.0
        } else {
            logical.weight / matched as f32
        };
        let weighted = logical.weight / ctx.logical_conf;

        // Hard ranking: BER-gated, then lexicographic (matched, weighted) with a
        // first-seen tie-break. Every shift that does not take the crown — and
        // every crown it displaces — competes for the uniqueness runner-up,
        // whether or not it passed the BER gate, because a high-matching
        // competitor threatens uniqueness regardless.
        let becomes_best = bit_error_rate <= ctx.max_bit_error_rate
            && match &self.hard_best {
                None => true,
                Some(cur) => {
                    matched > cur.edges_matched
                        || (matched == cur.edges_matched && weighted > cur.weighted_score)
                }
            };
        if becomes_best {
            if let Some(prev) = &self.hard_best {
                demote_into_runner(
                    &mut self.hard_runner,
                    prev.edges_matched as u32,
                    prev.master_origin_row,
                    prev.master_origin_col,
                    prev.alignment.with_translation([0, 0]),
                );
            }
            self.hard_best = Some(DecodeOutcome {
                alignment: transform.with_translation([master_col, master_row]),
                edges_matched: matched,
                edges_observed: ctx.logical_total,
                weighted_score: weighted,
                bit_error_rate,
                mean_confidence,
                master_origin_row: master_row,
                master_origin_col: master_col,
                score_best: weighted,
                score_runner_up: None,
                score_margin: f32::INFINITY,
                runner_up_origin_row: None,
                runner_up_origin_col: None,
                runner_up_transform: None,
            });
        } else {
            demote_into_runner(
                &mut self.hard_runner,
                matched as u32,
                master_row,
                master_col,
                transform,
            );
        }

        let Some(physical) = physical else {
            return;
        };
        // Soft ranking: a plain two-slot update on the summed log-likelihood
        // over the raw dots. There is no BER pre-gate here — the soft winner is
        // BER-checked once, against the logical bits, after the scan.
        let candidate = DecodeOutcome {
            alignment: transform.with_translation([master_col, master_row]),
            edges_matched: physical.matched,
            edges_observed: ctx.physical_total,
            weighted_score: dequantize_ll(physical.ll) / ctx.physical_total as f32,
            bit_error_rate: (ctx.physical_total - physical.matched) as f32
                / ctx.physical_total as f32,
            mean_confidence: if physical.matched == 0 {
                0.0
            } else {
                physical.weight / physical.matched as f32
            },
            master_origin_row: master_row,
            master_origin_col: master_col,
            score_best: dequantize_ll(physical.ll),
            score_runner_up: None,
            score_margin: 0.0,
            runner_up_origin_row: None,
            runner_up_origin_col: None,
            runner_up_transform: None,
        };
        update_best_and_runner_up(&mut self.soft_best, &mut self.soft_runner, candidate);
    }
}

/// Update the uniqueness runner-up slot if `matched` exceeds the count it
/// currently holds.
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

/// Scan every realisable `(transform, shift)` hypothesis for a declared board.
///
/// `transforms` is the orientation hypothesis set to search — see
/// [`PuzzleBoardSymmetryMode`](crate::PuzzleBoardSymmetryMode).
///
/// Returns `None` when the observation set is empty, carries no confidence, or
/// admits no shift that keeps all of it on the board.
#[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip_all))]
/// `logical` is the voted class set the matched-count ranking is evaluated
/// over; `physical` is the raw dot set, supplied together with the soft config
/// when the soft scorer is running.
///
/// Both sets are scored in a **single** traversal of the shift rectangle, from
/// two per-transform table builds, rather than by scanning the rectangle twice.
///
/// The shift rectangle itself is always derived from the *physical* extent when
/// there is one. That constraint is geometric — "no observed dot may fall off
/// the declared board" — and the class representatives are a thinned subset of
/// the dots, so deriving it from them would admit placements that push real
/// observations off the board.
fn scan(
    logical: &[PuzzleBoardObservedEdge],
    physical: Option<(&[PuzzleBoardObservedEdge], &SoftLlConfig)>,
    board: BoardRect,
    transforms: &[GridTransform],
    max_bit_error_rate: f32,
) -> Option<FixedScan> {
    if logical.is_empty() || board.rows < 2 || board.cols < 2 {
        return None;
    }
    let logical_conf: f32 = logical.iter().map(|e| e.confidence).sum();
    if logical_conf <= 0.0 {
        return None;
    }
    let soft_cfg = physical.map(|(_, cfg)| cfg);
    let physical_edges = physical.map(|(edges, _)| edges);
    if let Some(edges) = physical_edges {
        if edges.is_empty() || edges.iter().map(|e| e.confidence).sum::<f32>() <= 0.0 {
            return None;
        }
    }
    let ctx = ScanContext {
        logical_total: logical.len(),
        logical_conf,
        physical_total: physical_edges.map_or(logical.len(), <[_]>::len),
        max_bit_error_rate,
        soft: soft_cfg.is_some(),
    };

    let mut scan = FixedScan::new();
    let mut logical_tables = ClassTables::new(false);
    let mut physical_tables = ClassTables::new(true);
    let mut any_shift = false;

    for transform in transforms.iter().copied() {
        let transformed = transform_observations(logical, &transform);
        // Geometry comes from the dots when we have them (see fn docs).
        let extent = match physical_edges {
            Some(edges) => LookupExtent::of(&transform_observations(edges, &transform)),
            None => LookupExtent::of(&transformed),
        };
        let Some(((r_lo, r_hi), (c_lo, c_hi))) = board.shift_range(&extent) else {
            continue;
        };
        any_shift = true;

        // Only the residue classes this transform's shift rectangle reaches can
        // ever be read, so the precompute skips the rest.
        let first_row = board.origin_row + r_lo;
        let first_col = board.origin_col + c_lo;
        let range = ClassRange::of_origin_rect(
            first_row,
            (r_hi - r_lo + 1) as usize,
            first_col,
            (c_hi - c_lo + 1) as usize,
        );
        logical_tables.build(&transformed, &range, None);
        if let (Some(edges), Some(cfg)) = (physical_edges, soft_cfg) {
            let transformed_physical = transform_observations(edges, &transform);
            physical_tables.build(&transformed_physical, &range, Some(cfg));
        }

        #[cfg(feature = "tracing")]
        let _origin_span = tracing::info_span!("origin_scan").entered();
        for p_r in r_lo..=r_hi {
            let master_row = board.origin_row + p_r;
            for p_c in c_lo..=c_hi {
                let master_col = board.origin_col + p_c;
                let logical_score = score_at(&logical_tables, master_row, master_col, false);
                let physical_score = ctx
                    .soft
                    .then(|| score_at(&physical_tables, master_row, master_col, true));
                scan.offer(
                    transform,
                    master_row,
                    master_col,
                    &logical_score,
                    physical_score.as_ref(),
                    &ctx,
                );
            }
        }
    }

    if !any_shift {
        return None;
    }
    Some(scan)
}

/// Match observations against a declared board's own bit pattern, ranking by
/// hard bit-match count with a confidence-weighted tie-break.
///
/// View-independent: a camera observing any partial subset of the same physical
/// board recovers the same absolute master IDs for the corners it sees, so
/// observations can be fused across cameras.
pub(crate) fn decode_fixed_board(
    logical: &[PuzzleBoardObservedEdge],
    observed: &[PuzzleBoardObservedEdge],
    board: BoardRect,
    transforms: &[GridTransform],
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
    let voted = scan(logical, None, board, transforms, max_bit_error_rate)?;
    let winner = voted.hard_best?;
    // Cross-view sanity: the winner must also be the best-supported origin on
    // the raw dots, so voting cannot silently *move* the answer.
    let best = scan(observed, None, board, transforms, 1.0)?.hard_best?;
    if (winner.master_origin_row, winner.master_origin_col)
        != (best.master_origin_row, best.master_origin_col)
        || winner.alignment.matrix() != best.alignment.matrix()
    {
        return None;
    }
    let best_matched = winner.edges_matched as u32;
    finalize_hard_winner(winner, logical.len(), best_matched, voted.hard_runner)
}

/// Soft-log-likelihood decoder over a declared board.
///
/// One pass produces both rankings: the log-likelihood top-2 over the raw dots,
/// which selects the winner and applies the margin gate, and the matched-count
/// top-2 over the voted `logical` bits, which the BER and uniqueness gates
/// consume.
pub(crate) fn decode_fixed_board_soft(
    observed: &[PuzzleBoardObservedEdge],
    logical: &[PuzzleBoardObservedEdge],
    board: BoardRect,
    transforms: &[GridTransform],
    cfg: &SoftLlConfig,
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
    let voted = scan(
        logical,
        Some((observed, cfg)),
        board,
        transforms,
        max_bit_error_rate,
    )?;
    let winner = super::soft::finalize_soft_winner(voted.soft_best, voted.soft_runner, cfg)?;
    // Budget and uniqueness both come from the hard half of the same scan,
    // which ran over the voted bits — see `super::hard::decode` for why that is
    // the right domain for both.
    let hard_winner = voted.hard_best?;
    apply_soft_uniqueness_gate(
        winner,
        logical.len(),
        (
            hard_winner.edges_matched as u32,
            hard_winner.master_origin_row,
            hard_winner.master_origin_col,
            hard_winner.alignment.with_translation([0, 0]),
            voted.hard_runner,
        ),
    )
}
