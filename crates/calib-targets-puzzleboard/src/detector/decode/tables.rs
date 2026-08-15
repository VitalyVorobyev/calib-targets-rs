//! Cyclic class tables — the precompute every decoder path shares.
//!
//! # Why a table indexed by residue class
//!
//! The master edge code is *cyclic*: the expected bit at master cell
//! `(mr, mc)` depends only on the residues
//!
//! ```text
//! horizontal edge:  map_b[mr mod 167][mc mod 3]
//! vertical edge:    map_a[mr mod  3 ][mc mod 167]
//! ```
//!
//! so an origin's score depends on the observation set only through those
//! residues. There are `167 × 3 = 501` horizontal classes and `3 × 167 = 501`
//! vertical ones, and the per-origin score is the sum of one entry from each
//! table:
//!
//! ```text
//! score(mr, mc) = H[mr mod 167][mc mod 3] + V[mr mod 3][mc mod 167]
//! ```
//!
//! Building both tables costs `O(501 · N)` for `N` observations; every origin
//! afterwards costs two lookups and an add. This is the single precompute
//! behind all four decoder paths (full-master and fixed-board, hard and soft).
//!
//! # Restricting the class range
//!
//! A *declared* board only ever places origins inside a known rectangle of the
//! master, so only the residue classes that rectangle reaches can matter.
//! [`ClassRange`] expresses that restriction and the builder then walks the
//! restricted classes only, making the precompute cost proportional to the
//! declared board rather than to the master. A board spanning the whole master
//! reaches every class and pays the full `O(501 · N)`, which is the floor: at
//! that size the declared board *is* the master.

use crate::code_maps::{
    horizontal_edge_bit, vertical_edge_bit, EdgeOrientation, PuzzleBoardObservedEdge,
};

use super::{ll_pair, transform_edge_lookup, SoftLlConfig, H_COLS, H_ROWS, V_COLS, V_ROWS};
use calib_targets_core::GridTransform;

/// One observation with its lookup cell resolved into a given D4 frame.
///
/// Built once per transform by [`transform_observations`] so the table builder
/// and the origin scans all read the same resolved coordinates.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TransformedEdge {
    /// Lookup-cell row, relative to the hypothesised origin.
    pub lookup_row: i32,
    /// Lookup-cell column, relative to the hypothesised origin.
    pub lookup_col: i32,
    /// Edge orientation *after* the D4 transform (a rotation swaps them).
    pub orientation: EdgeOrientation,
    /// The observed bit.
    pub bit: u8,
    /// Per-bit confidence in `[0, 1]`.
    pub confidence: f32,
}

/// Resolve every observation's lookup cell into `transform`'s frame.
pub(crate) fn transform_observations(
    observed: &[PuzzleBoardObservedEdge],
    transform: &GridTransform,
) -> Vec<TransformedEdge> {
    observed
        .iter()
        .map(|e| {
            let lookup = transform_edge_lookup(e, transform);
            TransformedEdge {
                lookup_row: lookup.lookup_row,
                lookup_col: lookup.lookup_col,
                orientation: lookup.orientation,
                bit: e.bit,
                confidence: e.confidence,
            }
        })
        .collect()
}

/// Inclusive bounds on the lookup cells of a transformed observation set,
/// tracked per orientation because the two tables have different extents.
///
/// `None` for an orientation means no observation of that orientation is
/// present, in which case it constrains nothing.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct LookupExtent {
    /// `(min, max)` lookup row / column over horizontal observations.
    pub horizontal: Option<(i32, i32, i32, i32)>,
    /// `(min, max)` lookup row / column over vertical observations.
    pub vertical: Option<(i32, i32, i32, i32)>,
}

impl LookupExtent {
    /// Measure the extent of a transformed observation set.
    pub(crate) fn of(transformed: &[TransformedEdge]) -> Self {
        let mut out = Self::default();
        for e in transformed {
            let slot = match e.orientation {
                EdgeOrientation::Horizontal => &mut out.horizontal,
                EdgeOrientation::Vertical => &mut out.vertical,
            };
            *slot = Some(match *slot {
                None => (e.lookup_row, e.lookup_row, e.lookup_col, e.lookup_col),
                Some((r_lo, r_hi, c_lo, c_hi)) => (
                    r_lo.min(e.lookup_row),
                    r_hi.max(e.lookup_row),
                    c_lo.min(e.lookup_col),
                    c_hi.max(e.lookup_col),
                ),
            });
        }
        out
    }
}

/// A cyclic interval of residue classes: `len` consecutive values starting at
/// `start`, taken modulo the table's period.
///
/// A rectangle of master origins reaches a *contiguous* run of master rows and
/// columns, and a contiguous run of integers maps to a contiguous cyclic run of
/// residues — which is what makes the restriction expressible as an interval
/// rather than an arbitrary set.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClassInterval {
    start: usize,
    len: usize,
}

impl ClassInterval {
    /// The interval reached by `count` consecutive integers starting at
    /// `first`, modulo `period`. Saturates to the full period once `count`
    /// reaches it.
    pub(crate) fn of_consecutive(first: i32, count: usize, period: usize) -> Self {
        if count >= period {
            return Self {
                start: 0,
                len: period,
            };
        }
        Self {
            start: first.rem_euclid(period as i32) as usize,
            len: count,
        }
    }

    /// The whole period.
    pub(crate) fn full(period: usize) -> Self {
        Self {
            start: 0,
            len: period,
        }
    }

    /// Number of classes in the interval.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

/// The residue classes an origin rectangle can reach, per table.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClassRange {
    /// Rows of the H table (`mr mod 167`).
    pub h_rows: ClassInterval,
    /// Columns of the H table (`mc mod 3`).
    pub h_cols: ClassInterval,
    /// Rows of the V table (`mr mod 3`).
    pub v_rows: ClassInterval,
    /// Columns of the V table (`mc mod 167`).
    pub v_cols: ClassInterval,
}

impl ClassRange {
    /// Every class — what the full-master scan needs.
    pub(crate) fn full() -> Self {
        Self {
            h_rows: ClassInterval::full(H_ROWS),
            h_cols: ClassInterval::full(H_COLS),
            v_rows: ClassInterval::full(V_ROWS),
            v_cols: ClassInterval::full(V_COLS),
        }
    }

    /// The classes reached by master origins in
    /// `[first_row, first_row + n_rows) × [first_col, first_col + n_cols)`.
    pub(crate) fn of_origin_rect(
        first_row: i32,
        n_rows: usize,
        first_col: i32,
        n_cols: usize,
    ) -> Self {
        Self {
            h_rows: ClassInterval::of_consecutive(first_row, n_rows, H_ROWS),
            h_cols: ClassInterval::of_consecutive(first_col, n_cols, H_COLS),
            v_rows: ClassInterval::of_consecutive(first_row, n_rows, V_ROWS),
            v_cols: ClassInterval::of_consecutive(first_col, n_cols, V_COLS),
        }
    }
}

/// Per-class accumulators over one D4 transform's observation set.
///
/// All six tables are full-size (`501` entries each) regardless of the class
/// restriction; a restricted build simply leaves the unreachable entries at
/// zero, which keeps indexing uniform for the scans.
pub(crate) struct ClassTables {
    /// Matched-bit count per H class.
    pub h_count: Vec<u32>,
    /// Summed confidence of matched observations per H class.
    pub h_weight: Vec<f32>,
    /// Summed per-bit log-likelihood per H class. Empty unless a
    /// [`SoftLlConfig`] was supplied.
    pub h_ll: Vec<f32>,
    /// Matched-bit count per V class.
    pub v_count: Vec<u32>,
    /// Summed confidence of matched observations per V class.
    pub v_weight: Vec<f32>,
    /// Summed per-bit log-likelihood per V class. Empty unless a
    /// [`SoftLlConfig`] was supplied.
    pub v_ll: Vec<f32>,
}

impl ClassTables {
    /// Allocate zeroed tables. `soft` also allocates the log-likelihood halves.
    pub(crate) fn new(soft: bool) -> Self {
        let ll = |n| if soft { vec![0.0f32; n] } else { Vec::new() };
        Self {
            h_count: vec![0u32; H_ROWS * H_COLS],
            h_weight: vec![0.0f32; H_ROWS * H_COLS],
            h_ll: ll(H_ROWS * H_COLS),
            v_count: vec![0u32; V_ROWS * V_COLS],
            v_weight: vec![0.0f32; V_ROWS * V_COLS],
            v_ll: ll(V_ROWS * V_COLS),
        }
    }

    fn clear(&mut self) {
        self.h_count.fill(0);
        self.h_weight.fill(0.0);
        self.h_ll.fill(0.0);
        self.v_count.fill(0);
        self.v_weight.fill(0.0);
        self.v_ll.fill(0.0);
    }

    /// Rebuild the tables for one transform's observation set.
    ///
    /// Every observation contributes to exactly one cell per reachable class,
    /// so each cell accumulates its observations in input order — which makes
    /// the `f32` sums reproducible and independent of the class restriction.
    ///
    /// Passing a [`SoftLlConfig`] additionally accumulates the per-bit
    /// log-likelihood halves; the count and weight tables are identical either
    /// way, so a soft scan gets the hard scan's tables for free.
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip_all))]
    pub(crate) fn build(
        &mut self,
        transformed: &[TransformedEdge],
        range: &ClassRange,
        soft: Option<&SoftLlConfig>,
    ) {
        self.clear();
        // Dispatched once, not per observation: the scoring mode is a
        // compile-time parameter of the inner loop so the hard path carries no
        // log-likelihood branch at all.
        match soft {
            Some(cfg) => self.accumulate_all::<true>(transformed, range, cfg),
            None => self.accumulate_all::<false>(transformed, range, &NO_SOFT),
        }
    }

    fn accumulate_all<const SOFT: bool>(
        &mut self,
        transformed: &[TransformedEdge],
        range: &ClassRange,
        cfg: &SoftLlConfig,
    ) {
        for e in transformed {
            let ll = if SOFT {
                ll_pair(e.confidence, cfg.kappa, cfg.per_bit_floor)
            } else {
                (0.0, 0.0)
            };
            match e.orientation {
                EdgeOrientation::Horizontal => accumulate::<H_ROWS, H_COLS, SOFT>(
                    Accumulate {
                        count: &mut self.h_count,
                        weight: &mut self.h_weight,
                        ll_sum: &mut self.h_ll,
                    },
                    e,
                    ll,
                    &range.h_rows,
                    &range.h_cols,
                    flat_map_b(),
                ),
                EdgeOrientation::Vertical => accumulate::<V_ROWS, V_COLS, SOFT>(
                    Accumulate {
                        count: &mut self.v_count,
                        weight: &mut self.v_weight,
                        ll_sum: &mut self.v_ll,
                    },
                    e,
                    ll,
                    &range.v_rows,
                    &range.v_cols,
                    flat_map_a(),
                ),
            }
        }
    }
}

/// Placeholder passed to the hard monomorphisation, which never reads it.
const NO_SOFT: SoftLlConfig = SoftLlConfig {
    kappa: 0.0,
    per_bit_floor: 0.0,
    alignment_min_margin: 0.0,
};

/// The three accumulators one orientation writes into.
struct Accumulate<'a> {
    count: &'a mut [u32],
    weight: &'a mut [f32],
    ll_sum: &'a mut [f32],
}

/// Accumulate one observation into every reachable class of one table.
///
/// Class `(a, b)` corresponds to origin residues `(mr mod N_ROWS, mc mod
/// N_COLS)`, under which this observation reads master cell
/// `(a + lookup_row, b + lookup_col)`. Walking the *classes* rather than the
/// master cells is what lets a restricted range skip work.
///
/// Both indices advance by one per step, so their master coordinates do too and
/// wrap at most once — each axis is reduced once at entry rather than once per
/// cell, leaving the inner body a table read, a compare and an add. The table
/// shape and the scoring mode are const parameters so the strides, the wrap
/// points and the presence of the log-likelihood term are all fixed at compile
/// time.
#[inline]
fn accumulate<const N_ROWS: usize, const N_COLS: usize, const SOFT: bool>(
    acc: Accumulate<'_>,
    e: &TransformedEdge,
    ll: (f32, f32),
    rows: &ClassInterval,
    cols: &ClassInterval,
    map: &'static [u8],
) {
    let (ll_match, ll_mismatch) = ll;
    let Accumulate {
        count,
        weight,
        ll_sum,
    } = acc;
    // One cell: read the master bit, credit the class it belongs to.
    let mut hit = |class_idx: usize, map_idx: usize| {
        if map[map_idx] == e.bit {
            count[class_idx] += 1;
            weight[class_idx] += e.confidence;
            if SOFT {
                ll_sum[class_idx] += ll_match;
            }
        } else if SOFT {
            ll_sum[class_idx] += ll_mismatch;
        }
    };

    if rows.len() == N_ROWS && cols.len() == N_COLS {
        // Unrestricted: walk the master cells, whose bounds are compile-time
        // constants, and map each to the class it credits.
        let first_b = (-e.lookup_col).rem_euclid(N_COLS as i32) as usize;
        for r in 0..N_ROWS {
            let class_base =
                ((r as i32 - e.lookup_row).rem_euclid(N_ROWS as i32) as usize) * N_COLS;
            let row_base = r * N_COLS;
            let mut b = first_b;
            for c in 0..N_COLS {
                hit(class_base + b, row_base + c);
                b = if b + 1 == N_COLS { 0 } else { b + 1 };
            }
        }
        return;
    }

    // Restricted: walk the reachable classes instead. Both indices advance by
    // one per step, so their master coordinates do too and wrap at most once —
    // each axis is reduced once at entry rather than once per cell.
    let mut master_row = (rows.start as i32 + e.lookup_row).rem_euclid(N_ROWS as i32) as usize;
    let first_col = (cols.start as i32 + e.lookup_col).rem_euclid(N_COLS as i32) as usize;
    let mut a = rows.start;
    for _ in 0..rows.len() {
        let row_base = master_row * N_COLS;
        let class_base = a * N_COLS;
        let mut master_col = first_col;
        let mut b = cols.start;
        for _ in 0..cols.len() {
            hit(class_base + b, row_base + master_col);
            b = if b + 1 == N_COLS { 0 } else { b + 1 };
            master_col = if master_col + 1 == N_COLS {
                0
            } else {
                master_col + 1
            };
        }
        a = if a + 1 == N_ROWS { 0 } else { a + 1 };
        master_row = if master_row + 1 == N_ROWS {
            0
        } else {
            master_row + 1
        };
    }
}

/// The A map (`3 × 167`, vertical-edge bits) unpacked to one byte per cell.
///
/// The packed representation costs a shift and a mask on every read; the
/// precompute reads it `O(501 · N)` times per transform, so it is unpacked once
/// and indexed directly.
fn flat_map_a() -> &'static [u8] {
    static FLAT: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
        (0..V_ROWS * V_COLS)
            .map(|i| vertical_edge_bit((i / V_COLS) as i32, (i % V_COLS) as i32))
            .collect()
    });
    &FLAT
}

/// The B map (`167 × 3`, horizontal-edge bits) unpacked to one byte per cell.
fn flat_map_b() -> &'static [u8] {
    static FLAT: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| {
        (0..H_ROWS * H_COLS)
            .map(|i| horizontal_edge_bit((i / H_COLS) as i32, (i % H_COLS) as i32))
            .collect()
    });
    &FLAT
}
