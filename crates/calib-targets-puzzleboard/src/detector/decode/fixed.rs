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
//! (`edge_sampling::sample_all_edges`). Every observation therefore *does* lie
//! on the printed board, and any shift that places one of them outside the
//! board is a hypothesis that cannot describe the physical scene. The scan is
//! restricted to shifts under which every observation lands on the board, which
//!
//! - makes each hypothesis cost two table lookups instead of `O(N)`, since
//!   with nothing off-board the score is exactly the class-table sum, and
//! - stops impossible placements from competing in the uniqueness gate, where
//!   they could only ever suppress a correct decode.
//!
//! "Outside the board" is per axis, not global: a wrapped axis has no outside
//! at all, so it contributes no restriction. See [`AxisExtent`].
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

use super::tables::{transform_observations, ClassRange, ClassTables, CodeGeometry, LookupExtent};
use super::{
    apply_soft_uniqueness_gate, dequantize_ll, finalize_hard_winner, update_best_and_runner_up,
    DecodeOutcome, HardRunnerUp, SoftLlConfig, H_COLS, V_COLS,
};

/// One axis of the origin space.
///
/// A declared planar board is a window *cut from* the master on both axes, so
/// both are clamped: a shift that pushes an observation past an end names a
/// placement that cannot physically exist. A PuzzlePole's circumference axis is
/// not a window but a **ring** — the pattern closes on itself at the seam — and
/// has no end to fall off. Spelling that as a variant rather than as a flag on
/// a rectangle is what keeps the two apart: a cyclic axis carries no origin and
/// no end, so there is nothing left to clamp against by accident.
#[derive(Clone, Copy, Debug)]
pub(crate) enum AxisExtent {
    /// A finite run of `cells` master indices from `origin`. Shifts are
    /// constrained so no observation falls off the end.
    Clamped {
        /// Master index the window starts at.
        origin: i32,
        /// Window length, in squares.
        cells: i32,
    },
    /// A ring of `period` origins. Every shift is realisable; shifts outside
    /// `[0, period)` are congruent to one inside it.
    Cyclic {
        /// Ring length, in squares.
        period: i32,
    },
}

/// What one edge family demands of one axis.
///
/// `lo` and `hi` are the inclusive bounds of that family's lookup indices along
/// the axis, relative to the shift. `deficit` is how far short of the axis'
/// square count the last lookup index the family may use falls; see
/// [`BoardRect::shift_range`] for why it differs between the families and
/// between the axes.
#[derive(Clone, Copy, Debug)]
struct AxisDemand {
    lo: i32,
    hi: i32,
    deficit: i32,
}

impl AxisExtent {
    /// The axis' length in squares — the window length when clamped, the ring
    /// length when cyclic.
    fn squares(&self) -> i32 {
        match *self {
            Self::Clamped { cells, .. } => cells,
            Self::Cyclic { period } => period,
        }
    }

    /// The master index a shift of `p` names on this axis.
    ///
    /// A clamped axis is a window cut from the master, so a shift is measured
    /// from where that window starts. A cyclic axis is scored against its own
    /// `period`-long code and *is* its own coordinate — the shift already is
    /// the index — so there is nothing to add.
    #[inline]
    fn master_index(&self, p: i32) -> i32 {
        match *self {
            Self::Clamped { origin, .. } => origin + p,
            Self::Cyclic { .. } => p,
        }
    }

    /// Inclusive shift range on this axis, or `None` when the families' demands
    /// cannot be met at once.
    fn shift_range(&self, demands: [Option<AxisDemand>; 2]) -> Option<(i32, i32)> {
        let cells = match *self {
            // Nothing can fall off a ring: every index in `[0, period)` is a
            // realisable origin, and one outside it is congruent to one inside,
            // so the whole ring is scanned and the demands constrain nothing.
            Self::Cyclic { period } => return Some((0, period - 1)),
            Self::Clamped { cells, .. } => cells,
        };
        // Shifts stay non-negative: the fragment's local `(0, 0)` is itself a
        // detected corner, so it cannot sit off the start of the window.
        let (mut lo, mut hi) = (0, i32::MAX);
        for demand in demands.into_iter().flatten() {
            lo = lo.max(-demand.lo);
            hi = hi.min(cells - demand.deficit - demand.hi);
        }
        (lo <= hi).then_some((lo, hi))
    }
}

/// The declared board, as the decoders need it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BoardRect {
    /// The master-row axis.
    pub rows: AxisExtent,
    /// The master-column axis.
    pub cols: AxisExtent,
}

impl BoardRect {
    /// A planar board of `rows` x `cols` squares cut from master
    /// `(origin_row, origin_col)` — both axes clamped.
    pub(crate) fn new(origin_row: u32, origin_col: u32, rows: u32, cols: u32) -> Self {
        Self {
            rows: AxisExtent::Clamped {
                origin: origin_row as i32,
                cells: rows as i32,
            },
            cols: AxisExtent::Clamped {
                origin: origin_col as i32,
                cells: cols as i32,
            },
        }
    }

    /// A pole: `period` origins around the circumference, `axial_cells` squares
    /// from `axial_origin` along the cylinder.
    ///
    /// The circumference is the master **row** axis — the one a pole's
    /// shortened code wraps — so that is the axis that becomes cyclic; the
    /// axial axis stays an ordinary window cut from the master columns.
    pub(crate) fn pole(period: u32, axial_origin: u32, axial_cells: u32) -> Self {
        Self {
            rows: AxisExtent::Cyclic {
                period: period as i32,
            },
            cols: AxisExtent::Clamped {
                origin: axial_origin as i32,
                cells: axial_cells as i32,
            },
        }
    }

    /// Inclusive shift range under which *every* observation lands on the
    /// board, or `None` when no such shift exists.
    ///
    /// A board of `rows` x `cols` squares carries `(rows - 1) * cols`
    /// horizontal edge cells and `rows * (cols - 1)` vertical ones: the
    /// horizontal family spans one row fewer than the board has and all of its
    /// columns, and the vertical family is its mirror image. A lookup cell must
    /// fall inside the table its orientation reads, and an inclusive last index
    /// is one below a count — so a family's deficit against the axis' square
    /// count is `2` where that family is one short and `1` where it is not.
    /// That is where the `- 2` and the `- 1` come from, and why they land on
    /// opposite orientations on the two axes.
    ///
    /// Each axis then folds its own two demands. A cyclic axis discards them:
    /// a ring has no end for an observation to fall off.
    fn shift_range(&self, extent: &LookupExtent) -> Option<((i32, i32), (i32, i32))> {
        let rows = self.rows.shift_range([
            extent
                .horizontal
                .map(|(lo, hi, _, _)| AxisDemand { lo, hi, deficit: 2 }),
            extent
                .vertical
                .map(|(lo, hi, _, _)| AxisDemand { lo, hi, deficit: 1 }),
        ])?;
        let cols = self.cols.shift_range([
            extent
                .horizontal
                .map(|(_, _, lo, hi)| AxisDemand { lo, hi, deficit: 1 }),
            extent
                .vertical
                .map(|(_, _, lo, hi)| AxisDemand { lo, hi, deficit: 2 }),
        ])?;
        Some((rows, cols))
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
fn score_at(
    geometry: &CodeGeometry<'_>,
    tables: &ClassTables,
    master_row: i32,
    master_col: i32,
    soft: bool,
) -> ShiftScore {
    let h = (master_row.rem_euclid(geometry.h_long as i32) as usize) * H_COLS
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
    geometry: CodeGeometry<'_>,
    board: BoardRect,
    transforms: &[GridTransform],
    max_bit_error_rate: f32,
) -> Option<FixedScan> {
    if logical.is_empty() || board.rows.squares() < 2 || board.cols.squares() < 2 {
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
    let mut logical_tables = ClassTables::new(&geometry, false);
    let mut physical_tables = ClassTables::new(&geometry, true);
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
        let first_row = board.rows.master_index(r_lo);
        let first_col = board.cols.master_index(c_lo);
        let range = ClassRange::of_origin_rect(
            &geometry,
            first_row,
            (r_hi - r_lo + 1) as usize,
            first_col,
            (c_hi - c_lo + 1) as usize,
        );
        logical_tables.build(&geometry, &transformed, &range, None);
        if let (Some(edges), Some(cfg)) = (physical_edges, soft_cfg) {
            let transformed_physical = transform_observations(edges, &transform);
            physical_tables.build(&geometry, &transformed_physical, &range, Some(cfg));
        }

        #[cfg(feature = "tracing")]
        let _origin_span = tracing::info_span!("origin_scan").entered();
        for p_r in r_lo..=r_hi {
            let master_row = board.rows.master_index(p_r);
            for p_c in c_lo..=c_hi {
                let master_col = board.cols.master_index(p_c);
                let logical_score =
                    score_at(&geometry, &logical_tables, master_row, master_col, false);
                let physical_score = ctx
                    .soft
                    .then(|| score_at(&geometry, &physical_tables, master_row, master_col, true));
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
    decode_fixed_hard(
        logical,
        observed,
        CodeGeometry::master(),
        board,
        transforms,
        max_bit_error_rate,
    )
}

/// [`decode_fixed_board`] against an arbitrary code geometry.
///
/// A PuzzlePole is decoded through here: its origin space is `p × axial_cells`,
/// a few thousand hypotheses against the master's 501² = 251 001, so direct
/// enumeration is cheap and — unlike the full-master path — needs no coprimality
/// between the two row moduli. On a pole those are 3 and `p` with `3 | p`, so
/// the CRT collapse `hard.rs` relies on is simply not available.
pub(crate) fn decode_fixed_hard(
    logical: &[PuzzleBoardObservedEdge],
    observed: &[PuzzleBoardObservedEdge],
    geometry: CodeGeometry<'_>,
    board: BoardRect,
    transforms: &[GridTransform],
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
    let voted = scan(
        logical,
        None,
        geometry,
        board,
        transforms,
        max_bit_error_rate,
    )?;
    let winner = voted.hard_best?;
    // Cross-view sanity: the winner must also be the best-supported origin on
    // the raw dots, so voting cannot silently *move* the answer.
    let best = scan(observed, None, geometry, board, transforms, 1.0)?.hard_best?;
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
    decode_fixed_soft(
        observed,
        logical,
        CodeGeometry::master(),
        board,
        transforms,
        cfg,
        max_bit_error_rate,
    )
}

/// [`decode_fixed_board_soft`] against an arbitrary code geometry.
pub(crate) fn decode_fixed_soft(
    observed: &[PuzzleBoardObservedEdge],
    logical: &[PuzzleBoardObservedEdge],
    geometry: CodeGeometry<'_>,
    board: BoardRect,
    transforms: &[GridTransform],
    cfg: &SoftLlConfig,
    max_bit_error_rate: f32,
) -> Option<DecodeOutcome> {
    let voted = scan(
        logical,
        Some((observed, cfg)),
        geometry,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A `LookupExtent` carrying only horizontal observations, spanning
    /// `rows.0..=rows.1` lookup rows and `cols.0..=cols.1` lookup columns.
    ///
    /// One orientation at a time is deliberate: with both present the two
    /// deficits meet under a `min` and the asymmetry between them disappears
    /// from the result, which is exactly what these tests are checking.
    fn horizontal_only(rows: (i32, i32), cols: (i32, i32)) -> LookupExtent {
        LookupExtent {
            horizontal: Some((rows.0, rows.1, cols.0, cols.1)),
            vertical: None,
        }
    }

    /// The two families lose a *different* index on the two axes: a board of
    /// `rows` x `cols` squares carries `(rows - 1) * cols` horizontal edge cells
    /// and `rows * (cols - 1)` vertical ones. Per-axis folding must not have
    /// smoothed that away.
    #[test]
    fn the_two_families_clamp_the_two_axes_differently() {
        let board = BoardRect::new(0, 0, 10, 10);
        // Horizontal cells reach row 8 and column 9, so a span of `0..=3`
        // leaves shifts `0..=5` on the rows and `0..=6` on the columns.
        assert_eq!(
            board.shift_range(&horizontal_only((0, 3), (0, 3))),
            Some(((0, 5), (0, 6)))
        );
        // Vertical cells are the mirror image: row 9, column 8.
        let vertical_only = LookupExtent {
            horizontal: None,
            vertical: Some((0, 3, 0, 3)),
        };
        assert_eq!(board.shift_range(&vertical_only), Some(((0, 6), (0, 5))));
    }

    /// A transform can push lookup cells negative; the shift then has to start
    /// high enough to bring them back onto the board.
    #[test]
    fn a_negative_lookup_span_raises_the_lower_shift() {
        assert_eq!(
            BoardRect::new(0, 0, 10, 10).shift_range(&horizontal_only((-2, 3), (-1, 3))),
            Some(((2, 5), (1, 6)))
        );
    }

    /// The circumference axis is a ring, so every origin on it is realisable no
    /// matter how far the observations reach — including past the period, where
    /// they simply wrap. A clamped axis of the *same length* narrows instead,
    /// which is the whole difference between the two variants.
    #[test]
    fn a_cyclic_axis_admits_every_origin_on_the_ring() {
        let pole = BoardRect::pole(12, 4, 8);
        let planar = BoardRect::new(0, 4, 12, 8);
        let fragment = horizontal_only((0, 3), (0, 3));
        assert_eq!(pole.shift_range(&fragment), Some(((0, 11), (0, 4))));
        assert_eq!(planar.shift_range(&fragment), Some(((0, 7), (0, 4))));
        // Three periods' worth of lookup rows: nothing falls off a ring.
        assert_eq!(
            pole.shift_range(&horizontal_only((0, 40), (0, 3))),
            Some(((0, 11), (0, 4)))
        );
        assert!(planar
            .shift_range(&horizontal_only((0, 40), (0, 3)))
            .is_none());
    }

    /// Making one axis cyclic must not disarm the other: a fragment wider than
    /// the axial window still admits no shift at all.
    #[test]
    fn a_cyclic_row_axis_leaves_the_axial_guard_armed() {
        let pole = BoardRect::pole(12, 4, 8);
        assert!(pole
            .shift_range(&horizontal_only((0, 3), (0, 20)))
            .is_none());
    }

    /// A pole's circumference coordinate *is* the shift — its code is indexed
    /// from the seam — while the axial axis is still measured from where its
    /// window was cut out of the master.
    #[test]
    fn only_a_clamped_axis_offsets_the_shift() {
        let pole = BoardRect::pole(12, 60, 8);
        assert_eq!(pole.rows.master_index(5), 5);
        assert_eq!(pole.cols.master_index(5), 65);
        assert_eq!(pole.rows.squares(), 12);
        assert_eq!(pole.cols.squares(), 8);
    }
}
