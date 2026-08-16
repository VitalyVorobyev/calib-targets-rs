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

use super::{
    ll_pair, quantize_ll, transform_edge_lookup, SoftLlConfig, H_COLS, H_ROWS, V_COLS, V_ROWS,
};
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
    /// Summed per-bit log-likelihood per H class, in the fixed-point units of
    /// [`super::LL_SCALE`]. Empty unless a [`SoftLlConfig`] was supplied.
    pub h_ll: Vec<i64>,
    /// Matched-bit count per V class.
    pub v_count: Vec<u32>,
    /// Summed confidence of matched observations per V class.
    pub v_weight: Vec<f32>,
    /// Summed per-bit log-likelihood per V class, in the fixed-point units of
    /// [`super::LL_SCALE`]. Empty unless a [`SoftLlConfig`] was supplied.
    pub v_ll: Vec<i64>,
    /// Scratch for the residue grouping, reused across transforms.
    groups: Groups,
}

impl ClassTables {
    /// Allocate zeroed tables. `soft` also allocates the log-likelihood halves.
    pub(crate) fn new(soft: bool) -> Self {
        let ll = |n| if soft { vec![0i64; n] } else { Vec::new() };
        Self {
            h_count: vec![0u32; H_ROWS * H_COLS],
            h_weight: vec![0.0f32; H_ROWS * H_COLS],
            h_ll: ll(H_ROWS * H_COLS),
            v_count: vec![0u32; V_ROWS * V_COLS],
            v_weight: vec![0.0f32; V_ROWS * V_COLS],
            v_ll: ll(V_ROWS * V_COLS),
            groups: Groups::new(),
        }
    }

    fn clear(&mut self) {
        self.h_count.fill(0);
        self.h_weight.fill(0.0);
        self.h_ll.fill(0);
        self.v_count.fill(0);
        self.v_weight.fill(0.0);
        self.v_ll.fill(0);
    }

    /// Rebuild the tables for one transform's observation set.
    ///
    /// # Cost is bounded by residue classes, not by observations
    ///
    /// An observation reaches the tables only through its *residues*: which
    /// cells it credits depends on `(bit, lookup_row mod period, lookup_col mod
    /// period)` and nothing else. Two observations agreeing on those three
    /// credit exactly the same cells with exactly the same shape of
    /// contribution, so they can be summed once and applied once.
    ///
    /// There are at most `2 · 167 · 3 = 1002` such residue groups per
    /// orientation, and for a window spanning `w` squares at most `6w` of them
    /// are non-empty — while the window holds on the order of `w²`
    /// observations. So the precompute costs
    /// `O(N + min(N, 6w) · 501)` rather than `O(N · 501)`, and the saving grows
    /// linearly with window size.
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
        self.groups.fill::<SOFT>(transformed, cfg);
        // The two halves of `build` scale differently — bucketing is `O(N)` in
        // observations, crediting is `O(touched · 501)` in residue groups — so
        // they are timed apart. One span for the whole loop pair, not one per
        // group: a per-group span would cost more than the group.
        #[cfg(feature = "tracing")]
        let _credit = tracing::info_span!("class_credit").entered();
        for (key, acc) in self.groups.horizontal() {
            accumulate::<H_ROWS, H_COLS, SOFT>(
                Accumulate {
                    count: &mut self.h_count,
                    weight: &mut self.h_weight,
                    ll_sum: &mut self.h_ll,
                },
                key,
                acc,
                &range.h_rows,
                &range.h_cols,
                flat_map_b(),
            );
        }
        for (key, acc) in self.groups.vertical() {
            accumulate::<V_ROWS, V_COLS, SOFT>(
                Accumulate {
                    count: &mut self.v_count,
                    weight: &mut self.v_weight,
                    ll_sum: &mut self.v_ll,
                },
                key,
                acc,
                &range.v_rows,
                &range.v_cols,
                flat_map_a(),
            );
        }
    }
}

/// One residue group's summed contribution.
#[derive(Clone, Copy, Debug, Default)]
struct GroupAcc {
    /// How many observations fell into this group.
    count: u32,
    /// Their summed confidence.
    weight: f32,
    /// Their summed match / mismatch log-likelihood, in fixed-point units
    /// (zero unless soft). Integer accumulation makes the group sums exact.
    ll_match: i64,
    ll_mismatch: i64,
}

/// The residues that decide which cells a group credits.
#[derive(Clone, Copy, Debug)]
struct GroupKey {
    row: usize,
    col: usize,
    bit: u8,
}

/// Per-orientation residue buckets, indexed by `bit · 501 + row · 3 + col` for
/// horizontal observations and `bit · 501 + row · 167 + col` for vertical ones.
///
/// Both index spaces hold `2 · 501` slots, so the scratch is 8 KB and clearing
/// it between transforms is cheaper than the work it saves. Only the slots
/// actually touched are visited afterwards, tracked in `touched`.
struct Groups {
    horizontal: Vec<GroupAcc>,
    vertical: Vec<GroupAcc>,
    touched_h: Vec<u32>,
    touched_v: Vec<u32>,
}

const GROUPS_PER_ORIENTATION: usize = 2 * H_ROWS * H_COLS;

impl Groups {
    fn new() -> Self {
        Self {
            horizontal: vec![GroupAcc::default(); GROUPS_PER_ORIENTATION],
            vertical: vec![GroupAcc::default(); GROUPS_PER_ORIENTATION],
            touched_h: Vec::new(),
            touched_v: Vec::new(),
        }
    }

    /// Bucket one transform's observations by residue. `O(N)`.
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip_all))]
    fn fill<const SOFT: bool>(&mut self, transformed: &[TransformedEdge], cfg: &SoftLlConfig) {
        for &slot in &self.touched_h {
            self.horizontal[slot as usize] = GroupAcc::default();
        }
        for &slot in &self.touched_v {
            self.vertical[slot as usize] = GroupAcc::default();
        }
        self.touched_h.clear();
        self.touched_v.clear();

        for e in transformed {
            let (bucket, touched, n_rows, n_cols) = match e.orientation {
                EdgeOrientation::Horizontal => {
                    (&mut self.horizontal, &mut self.touched_h, H_ROWS, H_COLS)
                }
                EdgeOrientation::Vertical => {
                    (&mut self.vertical, &mut self.touched_v, V_ROWS, V_COLS)
                }
            };
            let row = e.lookup_row.rem_euclid(n_rows as i32) as usize;
            let col = e.lookup_col.rem_euclid(n_cols as i32) as usize;
            let slot = (e.bit as usize) * (n_rows * n_cols) + row * n_cols + col;
            let acc = &mut bucket[slot];
            if acc.count == 0 {
                touched.push(slot as u32);
            }
            acc.count += 1;
            acc.weight += e.confidence;
            if SOFT {
                let (m, mm) = ll_pair(e.confidence, cfg.kappa, cfg.per_bit_floor);
                acc.ll_match += quantize_ll(m);
                acc.ll_mismatch += quantize_ll(mm);
            }
        }
        // Visit groups in a fixed order so the table sums are reproducible
        // regardless of observation order.
        self.touched_h.sort_unstable();
        self.touched_v.sort_unstable();
    }

    fn horizontal(&self) -> impl Iterator<Item = (GroupKey, GroupAcc)> + '_ {
        Self::iter(&self.touched_h, &self.horizontal, H_ROWS, H_COLS)
    }

    fn vertical(&self) -> impl Iterator<Item = (GroupKey, GroupAcc)> + '_ {
        Self::iter(&self.touched_v, &self.vertical, V_ROWS, V_COLS)
    }

    fn iter<'a>(
        touched: &'a [u32],
        bucket: &'a [GroupAcc],
        n_rows: usize,
        n_cols: usize,
    ) -> impl Iterator<Item = (GroupKey, GroupAcc)> + 'a {
        let per_bit = n_rows * n_cols;
        touched.iter().map(move |&slot| {
            let slot = slot as usize;
            let bit = (slot / per_bit) as u8;
            let within = slot % per_bit;
            (
                GroupKey {
                    row: within / n_cols,
                    col: within % n_cols,
                    bit,
                },
                bucket[slot],
            )
        })
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
    ll_sum: &'a mut [i64],
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
    key: GroupKey,
    group: GroupAcc,
    rows: &ClassInterval,
    cols: &ClassInterval,
    map: &'static [u8],
) {
    let Accumulate {
        count,
        weight,
        ll_sum,
    } = acc;
    // One cell: read the master bit, credit the class it belongs to with the
    // whole group's contribution at once.
    let mut hit = |class_idx: usize, map_idx: usize| {
        if map[map_idx] == key.bit {
            count[class_idx] += group.count;
            weight[class_idx] += group.weight;
            if SOFT {
                ll_sum[class_idx] += group.ll_match;
            }
        } else if SOFT {
            ll_sum[class_idx] += group.ll_mismatch;
        }
    };

    if rows.len() == N_ROWS && cols.len() == N_COLS {
        // Unrestricted: walk the master cells, whose bounds are compile-time
        // constants, and map each to the class it credits.
        let first_b = (N_COLS - key.col) % N_COLS;
        for r in 0..N_ROWS {
            let class_base = ((r + N_ROWS - key.row) % N_ROWS) * N_COLS;
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
    let mut master_row = (rows.start + key.row) % N_ROWS;
    let first_col = (cols.start + key.col) % N_COLS;
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
