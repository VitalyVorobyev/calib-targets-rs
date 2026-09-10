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

/// The cyclic geometry of one code: each family's long-axis period and the
/// packed short-axis pattern byte it reads at each long index.
///
/// The two families need not share a long period. On the planar master they do
/// — `h_long = v_long = 167` — but that is a property of the master, not of the
/// algorithm: a PuzzlePole of circumference `p` cut at master row `s` wraps the
/// horizontal family at `p` instead, giving `h_long = p` with `h_patterns` the
/// `s`-rotated `p`-row slice, while `v_long` and `v_patterns` stay as they are.
/// Carrying the two periods here rather than in one shared constant is what
/// lets the same builder serve both.
///
/// The *short* axis is 3 in all four positions and stays compile-time: it is
/// the period-3 replication the family fold is built on, and no cut moves it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CodeGeometry<'a> {
    /// Wrap period of the horizontal family's long axis, the master row.
    pub h_long: usize,
    /// Wrap period of the vertical family's long axis, the master column.
    pub v_long: usize,
    /// Packed `map_b` row patterns, one byte per long index; `len == h_long`.
    pub h_patterns: &'a [u8],
    /// Packed `map_a` column patterns, one byte per long index; `len == v_long`.
    pub v_patterns: &'a [u8],
}

impl CodeGeometry<'static> {
    /// The planar 501 × 501 master.
    pub(crate) fn master() -> Self {
        let geometry = Self {
            h_long: H_ROWS,
            v_long: V_COLS,
            h_patterns: h_row_patterns(),
            v_patterns: v_col_patterns(),
        };
        geometry.assert_lengths();
        geometry
    }
}

impl<'a> CodeGeometry<'a> {
    /// A PuzzlePole: the horizontal family wrapped at the pole's circumference,
    /// the vertical family exactly the master's.
    ///
    /// `map_a` is used verbatim rather than sliced, because its row period is 3
    /// and 3 divides every supported circumference — so it is already periodic
    /// at `p`, and a pole coordinate indexes it correctly with no rotation. See
    /// `pole::code` for why the H table has to be keyed by `master_row mod p`
    /// for that to hold.
    pub(crate) fn pole(h_patterns: &'a [u8]) -> Self {
        let geometry = Self {
            h_long: h_patterns.len(),
            v_long: V_COLS,
            h_patterns,
            v_patterns: v_col_patterns(),
        };
        geometry.assert_lengths();
        geometry
    }
}

impl CodeGeometry<'_> {
    /// A pattern table shorter than its period would silently alias two long
    /// indices onto one byte; longer would leave rows unreachable. Neither
    /// shows up as a crash, only as a wrong decode, so the pairing is checked
    /// wherever a geometry is built and wherever one enters the builder.
    #[inline]
    fn assert_lengths(&self) {
        debug_assert_eq!(
            self.h_patterns.len(),
            self.h_long,
            "H pattern table must hold exactly one byte per H long class"
        );
        debug_assert_eq!(
            self.v_patterns.len(),
            self.v_long,
            "V pattern table must hold exactly one byte per V long class"
        );
    }
}

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
    /// Rows of the H table (`mr mod h_long`).
    pub h_rows: ClassInterval,
    /// Columns of the H table (`mc mod 3`).
    pub h_cols: ClassInterval,
    /// Rows of the V table (`mr mod 3`).
    pub v_rows: ClassInterval,
    /// Columns of the V table (`mc mod v_long`).
    pub v_cols: ClassInterval,
}

impl ClassRange {
    /// Every class — what the full-master scan needs.
    pub(crate) fn full(geometry: &CodeGeometry<'_>) -> Self {
        Self {
            h_rows: ClassInterval::full(geometry.h_long),
            h_cols: ClassInterval::full(H_COLS),
            v_rows: ClassInterval::full(V_ROWS),
            v_cols: ClassInterval::full(geometry.v_long),
        }
    }

    /// The classes reached by master origins in
    /// `[first_row, first_row + n_rows) × [first_col, first_col + n_cols)`.
    pub(crate) fn of_origin_rect(
        geometry: &CodeGeometry<'_>,
        first_row: i32,
        n_rows: usize,
        first_col: i32,
        n_cols: usize,
    ) -> Self {
        Self {
            h_rows: ClassInterval::of_consecutive(first_row, n_rows, geometry.h_long),
            h_cols: ClassInterval::of_consecutive(first_col, n_cols, H_COLS),
            v_rows: ClassInterval::of_consecutive(first_row, n_rows, V_ROWS),
            v_cols: ClassInterval::of_consecutive(first_col, n_cols, geometry.v_long),
        }
    }
}

/// Per-class accumulators over one D4 transform's observation set.
///
/// All six tables are full-size for the geometry (`501` entries each on the
/// planar master) regardless of the class restriction; a restricted build
/// simply leaves the unreachable entries at zero, which keeps indexing uniform
/// for the scans.
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
    /// Allocate zeroed tables for `geometry`, which fixes both table extents.
    /// `soft` also allocates the log-likelihood halves.
    pub(crate) fn new(geometry: &CodeGeometry<'_>, soft: bool) -> Self {
        let h_cells = geometry.h_long * H_COLS;
        let v_cells = V_ROWS * geometry.v_long;
        let ll = |n| if soft { vec![0i64; n] } else { Vec::new() };
        Self {
            h_count: vec![0u32; h_cells],
            h_weight: vec![0.0f32; h_cells],
            h_ll: ll(h_cells),
            v_count: vec![0u32; v_cells],
            v_weight: vec![0.0f32; v_cells],
            v_ll: ll(v_cells),
            groups: Groups::new(geometry.h_long, geometry.v_long),
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
    /// In practice it is `3w`, not `6w`: the `bit` in the key can only split a
    /// residue when a dot was *misread*, because every observation sharing a
    /// residue is a period-3 replica of the same code bit. Instrumenting the
    /// public fixtures found no residue carrying both bits at all.
    ///
    /// Passing a [`SoftLlConfig`] additionally accumulates the per-bit
    /// log-likelihood halves; the count and weight tables are identical either
    /// way, so a soft scan gets the hard scan's tables for free.
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip_all))]
    pub(crate) fn build(
        &mut self,
        geometry: &CodeGeometry<'_>,
        transformed: &[TransformedEdge],
        range: &ClassRange,
        soft: Option<&SoftLlConfig>,
    ) {
        geometry.assert_lengths();
        // Every index below is derived from the geometry's periods, so tables
        // allocated for a different one would be silently mis-shaped.
        debug_assert_eq!(
            (self.h_count.len(), self.v_count.len()),
            (geometry.h_long * H_COLS, V_ROWS * geometry.v_long),
            "tables were allocated for a different code geometry"
        );
        self.clear();
        // Dispatched once, not per observation: the scoring mode is a
        // compile-time parameter of the inner loop so the hard path carries no
        // log-likelihood branch at all.
        match soft {
            Some(cfg) => self.accumulate_all::<true>(geometry, transformed, range, cfg),
            None => self.accumulate_all::<false>(geometry, transformed, range, &NO_SOFT),
        }
    }

    fn accumulate_all<const SOFT: bool>(
        &mut self,
        geometry: &CodeGeometry<'_>,
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
        for &family in &self.groups.touched_h {
            let base = family as usize * FAMILY_SLOTS;
            let terms = fold_family::<SOFT>(&self.groups.horizontal[base..base + FAMILY_SLOTS]);
            credit_family::<H_COLS, 1, SOFT>(
                Accumulate {
                    count: &mut self.h_count,
                    weight: &mut self.h_weight,
                    ll_sum: &mut self.h_ll,
                },
                &terms,
                family as usize,
                &range.h_rows,
                &range.h_cols,
                geometry.h_patterns,
                geometry.h_long,
            );
        }
        // The V table is the transpose of the H one — its long axis is the
        // column axis — so the ranges and strides swap with it.
        for &family in &self.groups.touched_v {
            let base = family as usize * FAMILY_SLOTS;
            let terms = fold_family::<SOFT>(&self.groups.vertical[base..base + FAMILY_SLOTS]);
            credit_family::<1, V_COLS, SOFT>(
                Accumulate {
                    count: &mut self.v_count,
                    weight: &mut self.v_weight,
                    ll_sum: &mut self.v_ll,
                },
                &terms,
                family as usize,
                &range.v_cols,
                &range.v_rows,
                geometry.v_patterns,
                geometry.v_long,
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

/// Short axis shared by both tables: the 3-long period. The long axes are
/// per-family and travel at runtime in a [`CodeGeometry`].
const SHORT: usize = H_COLS;
const _: () = assert!(H_COLS == SHORT && V_ROWS == SHORT);

/// Residue slots in one *family*: three short-axis residues × the two bits a
/// dot can carry.
const FAMILY_SLOTS: usize = SHORT * 2;
/// Distinct three-bit map patterns.
const PATTERNS: usize = 1 << SHORT;

/// What one family adds to a class, given the map pattern the class reads and
/// the class's short-axis residue.
///
/// Folding a family into this table is what removes the short axis from the
/// sweep: the three short residues are summed *once* per pattern here instead
/// of walking the whole table once each.
#[derive(Clone, Copy, Debug)]
struct FamilyTerm {
    count: u32,
    weight: f32,
    ll: i64,
}

impl FamilyTerm {
    const ZERO: Self = Self {
        count: 0,
        weight: 0.0,
        ll: 0,
    };
}

/// One family's contribution, indexed `[map pattern][short class]`.
type FamilyTerms = [[FamilyTerm; SHORT]; PATTERNS];

/// Sum a family's members into its per-(pattern, short class) contribution.
///
/// A class whose short residue is `sc` reads map bit `(sc + short) mod 3` of
/// the pattern, so a member either matches or misses depending only on the
/// pattern and `sc` — never on the class's long residue. That independence is
/// the whole reason the fold is possible.
fn fold_family<const SOFT: bool>(members: &[GroupAcc]) -> FamilyTerms {
    let mut terms = [[FamilyTerm::ZERO; SHORT]; PATTERNS];
    for (slot, group) in members.iter().enumerate() {
        if group.count == 0 {
            continue;
        }
        let short = slot / 2;
        let bit = (slot % 2) as u8;
        for (pattern, row) in terms.iter_mut().enumerate() {
            for (sc, term) in row.iter_mut().enumerate() {
                let predicted = ((pattern >> ((sc + short) % SHORT)) & 1) as u8;
                if predicted == bit {
                    term.count += group.count;
                    term.weight += group.weight;
                    if SOFT {
                        term.ll += group.ll_match;
                    }
                } else if SOFT {
                    term.ll += group.ll_mismatch;
                }
            }
        }
    }
    terms
}

/// Per-orientation residue buckets, laid out so a family occupies one
/// contiguous run of [`FAMILY_SLOTS`] slots: `long · 6 + short · 2 + bit`.
///
/// Each index space holds `long · 6` slots — `167 · 6` per orientation on the
/// planar master, so the scratch is 8 KB and clearing it between transforms is
/// cheaper than the work it saves. Only the families actually touched are
/// visited afterwards, tracked in `touched`.
struct Groups {
    horizontal: Vec<GroupAcc>,
    vertical: Vec<GroupAcc>,
    touched_h: Vec<u32>,
    touched_v: Vec<u32>,
    /// The periods the two buckets were sized for. A fill has to reduce with
    /// these exact values or it would index past the bucket it belongs to.
    h_long: usize,
    v_long: usize,
}

impl Groups {
    fn new(h_long: usize, v_long: usize) -> Self {
        Self {
            horizontal: vec![GroupAcc::default(); h_long * FAMILY_SLOTS],
            vertical: vec![GroupAcc::default(); v_long * FAMILY_SLOTS],
            touched_h: Vec::new(),
            touched_v: Vec::new(),
            h_long,
            v_long,
        }
    }

    /// Bucket one transform's observations by residue. `O(N)`.
    #[cfg_attr(feature = "tracing", tracing::instrument(level = "info", skip_all))]
    fn fill<const SOFT: bool>(&mut self, transformed: &[TransformedEdge], cfg: &SoftLlConfig) {
        for &family in &self.touched_h {
            let base = family as usize * FAMILY_SLOTS;
            self.horizontal[base..base + FAMILY_SLOTS].fill(GroupAcc::default());
        }
        for &family in &self.touched_v {
            let base = family as usize * FAMILY_SLOTS;
            self.vertical[base..base + FAMILY_SLOTS].fill(GroupAcc::default());
        }
        self.touched_h.clear();
        self.touched_v.clear();

        let (h_long, v_long) = (self.h_long as i32, self.v_long as i32);
        for e in transformed {
            // The V table transposes the two axes relative to H, so the long
            // residue comes from the column there and from the row here — and
            // it wraps at that family's own period, which the two need not
            // share.
            let (long, short, bucket, touched) = match e.orientation {
                EdgeOrientation::Horizontal => (
                    e.lookup_row.rem_euclid(h_long) as usize,
                    e.lookup_col.rem_euclid(SHORT as i32) as usize,
                    &mut self.horizontal,
                    &mut self.touched_h,
                ),
                EdgeOrientation::Vertical => (
                    e.lookup_col.rem_euclid(v_long) as usize,
                    e.lookup_row.rem_euclid(SHORT as i32) as usize,
                    &mut self.vertical,
                    &mut self.touched_v,
                ),
            };
            debug_assert!(e.bit <= 1, "an edge dot is one bit");
            let slot = long * FAMILY_SLOTS + short * 2 + (e.bit & 1) as usize;
            let acc = &mut bucket[slot];
            if acc.count == 0 {
                touched.push(long as u32);
            }
            acc.count += 1;
            acc.weight += e.confidence;
            if SOFT {
                let (m, mm) = ll_pair(e.confidence, cfg.kappa, cfg.per_bit_floor);
                acc.ll_match += quantize_ll(m);
                acc.ll_mismatch += quantize_ll(mm);
            }
        }
        // Visit families in a fixed order so the table sums are reproducible
        // regardless of observation order. Dedup because a family is pushed
        // once per slot it fills, and it has six.
        for touched in [&mut self.touched_h, &mut self.touched_v] {
            touched.sort_unstable();
            touched.dedup();
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
    ll_sum: &'a mut [i64],
}

/// Credit one residue family into every class it reaches.
///
/// Class `(a, b)` corresponds to origin residues `(mr mod N_ROWS, mc mod
/// N_COLS)`, under which this family's members read master cells sharing a
/// single map pattern — the three bits at the family's long index. So the walk
/// is one pass over the long axis: read the pattern, then add the three
/// pre-folded short-class terms.
///
/// `LONG_STRIDE` / `SHORT_STRIDE` place the two axes in the table's row-major
/// layout: the H table is `[long][short]` (`3`, `1`), the V table `[short][long]`
/// (`1`, `167`). Both stay compile-time even though `long_period` does not: a
/// stride is a *short*-axis row length, and the only long extent among them —
/// the V table's `167` — is the vertical family's period, which no cut of the
/// master moves.
///
/// Both the class index and the pattern index advance by one per step and wrap
/// at most once, so each is reduced at entry rather than once per cell.
#[inline]
fn credit_family<const LONG_STRIDE: usize, const SHORT_STRIDE: usize, const SOFT: bool>(
    acc: Accumulate<'_>,
    terms: &FamilyTerms,
    long_residue: usize,
    long_range: &ClassInterval,
    short_range: &ClassInterval,
    patterns: &[u8],
    long_period: usize,
) {
    debug_assert_eq!(
        patterns.len(),
        long_period,
        "the pattern table indexes the long axis, so it is exactly one period long"
    );
    let Accumulate {
        count,
        weight,
        ll_sum,
    } = acc;
    let mut pattern_idx = (long_range.start + long_residue) % long_period;
    let mut lc = long_range.start;
    for _ in 0..long_range.len() {
        let row = &terms[(patterns[pattern_idx] & (PATTERNS as u8 - 1)) as usize];
        let base = lc * LONG_STRIDE;
        let mut sc = short_range.start;
        for _ in 0..short_range.len() {
            let idx = base + sc * SHORT_STRIDE;
            let term = row[sc];
            count[idx] += term.count;
            weight[idx] += term.weight;
            if SOFT {
                ll_sum[idx] += term.ll;
            }
            sc = if sc + 1 == SHORT { 0 } else { sc + 1 };
        }
        lc = if lc + 1 == long_period { 0 } else { lc + 1 };
        pattern_idx = if pattern_idx + 1 == long_period {
            0
        } else {
            pattern_idx + 1
        };
    }
}

/// The three horizontal-edge bits of master row `r`, packed `bit j = map_b[r][j]`.
///
/// The packed map costs a shift and a mask on every read and the sweep reads it
/// `O(167)` times per family, so the three bits a family needs together are
/// unpacked once into a single byte.
fn h_row_patterns() -> &'static [u8; H_ROWS] {
    static PATS: std::sync::LazyLock<[u8; H_ROWS]> = std::sync::LazyLock::new(|| {
        std::array::from_fn(|r| {
            (0..SHORT).fold(0u8, |acc, j| {
                acc | horizontal_edge_bit(r as i32, j as i32) << j
            })
        })
    });
    &PATS
}

/// The three vertical-edge bits of master column `c`, packed `bit j = map_a[j][c]`.
fn v_col_patterns() -> &'static [u8; V_COLS] {
    static PATS: std::sync::LazyLock<[u8; V_COLS]> = std::sync::LazyLock::new(|| {
        std::array::from_fn(|c| {
            (0..SHORT).fold(0u8, |acc, j| {
                acc | vertical_edge_bit(j as i32, c as i32) << j
            })
        })
    });
    &PATS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_maps::PuzzleBoardObservedEdge;
    use calib_targets_core::GRID_TRANSFORMS_D4;

    /// The six tables, accumulated in `f64` so the weight comparison is against
    /// a more precise sum than the one under test rather than an equally lossy one.
    struct Reference {
        h_count: Vec<u32>,
        h_weight: Vec<f64>,
        h_ll: Vec<i64>,
        v_count: Vec<u32>,
        v_weight: Vec<f64>,
        v_ll: Vec<i64>,
    }

    /// The tables, spelled out from their definition: for every class, walk
    /// every observation and test it against the code bit that class implies.
    ///
    /// `O(501 · N)` and obviously correct — which is the point. The shipped
    /// builder reaches the same numbers by folding residue families and
    /// sweeping each family once, and this is what pins that rearrangement to
    /// the thing it is supposed to compute rather than to its own predecessor.
    ///
    /// Written against a [`CodeGeometry`] rather than against the master maps,
    /// so it keeps pinning the planar path to its definition while remaining
    /// usable as the oracle for any other cut of the code.
    fn reference_tables(
        geometry: &CodeGeometry<'_>,
        transformed: &[TransformedEdge],
        cfg: Option<&SoftLlConfig>,
    ) -> Reference {
        let h_cells = geometry.h_long * H_COLS;
        let v_cells = V_ROWS * geometry.v_long;
        let mut h_count = vec![0u32; h_cells];
        let mut h_weight = vec![0.0f64; h_cells];
        let mut h_ll = vec![0i64; h_cells];
        let mut v_count = vec![0u32; v_cells];
        let mut v_weight = vec![0.0f64; v_cells];
        let mut v_ll = vec![0i64; v_cells];

        for e in transformed {
            let (rows, cols) = match e.orientation {
                EdgeOrientation::Horizontal => (geometry.h_long, H_COLS),
                EdgeOrientation::Vertical => (V_ROWS, geometry.v_long),
            };
            for a in 0..rows {
                for b in 0..cols {
                    let mr = (a as i32 + e.lookup_row).rem_euclid(rows as i32);
                    let mc = (b as i32 + e.lookup_col).rem_euclid(cols as i32);
                    // Unpack the bit from the geometry's own pattern table: H
                    // packs `map_b[mr][·]` at long index `mr`, V packs
                    // `map_a[·][mc]` at long index `mc`. On the master that is
                    // `horizontal_edge_bit` / `vertical_edge_bit` by
                    // construction, and off it the oracle still follows.
                    let expected = match e.orientation {
                        EdgeOrientation::Horizontal => (geometry.h_patterns[mr as usize] >> mc) & 1,
                        EdgeOrientation::Vertical => (geometry.v_patterns[mc as usize] >> mr) & 1,
                    };
                    let idx = a * cols + b;
                    let (count, weight, ll) = match e.orientation {
                        EdgeOrientation::Horizontal => (&mut h_count, &mut h_weight, &mut h_ll),
                        EdgeOrientation::Vertical => (&mut v_count, &mut v_weight, &mut v_ll),
                    };
                    if expected == e.bit {
                        count[idx] += 1;
                        weight[idx] += e.confidence as f64;
                    }
                    if let Some(cfg) = cfg {
                        let (m, mm) = ll_pair(e.confidence, cfg.kappa, cfg.per_bit_floor);
                        ll[idx] += quantize_ll(if expected == e.bit { m } else { mm });
                    }
                }
            }
        }
        Reference {
            h_count,
            h_weight,
            h_ll,
            v_count,
            v_weight,
            v_ll,
        }
    }

    /// A deterministic fragment: every interior edge of a `span × span` corner
    /// window planted at a master origin, with `flip_every`-th dot corrupted so
    /// that some residue families carry both bits.
    fn fragment(span: i32, origin: (i32, i32), flip_every: usize) -> Vec<PuzzleBoardObservedEdge> {
        let mut out = Vec::new();
        let mut seen = 0usize;
        for row in 1..span - 1 {
            for col in 1..span - 1 {
                for orientation in [EdgeOrientation::Horizontal, EdgeOrientation::Vertical] {
                    let (mr, mc) = (origin.0 + row, origin.1 + col);
                    let truth = match orientation {
                        EdgeOrientation::Horizontal => horizontal_edge_bit(mr, mc),
                        EdgeOrientation::Vertical => vertical_edge_bit(mr, mc),
                    };
                    seen += 1;
                    let corrupt = flip_every != 0 && seen.is_multiple_of(flip_every);
                    out.push(PuzzleBoardObservedEdge {
                        row,
                        col,
                        orientation,
                        bit: truth ^ u8::from(corrupt),
                        // Distinct per-dot weights: a uniform confidence would
                        // hide any misrouting of the weight table.
                        confidence: 0.25 + (seen % 7) as f32 / 16.0,
                    });
                }
            }
        }
        out
    }

    fn assert_matches_reference(observed: &[PuzzleBoardObservedEdge], soft: Option<&SoftLlConfig>) {
        let geometry = CodeGeometry::master();
        let mut tables = ClassTables::new(&geometry, soft.is_some());
        for transform in GRID_TRANSFORMS_D4 {
            let transformed = transform_observations(observed, &transform);
            tables.build(&geometry, &transformed, &ClassRange::full(&geometry), soft);
            let want = reference_tables(&geometry, &transformed, soft);

            assert_eq!(tables.h_count, want.h_count, "H count under {transform:?}");
            assert_eq!(tables.v_count, want.v_count, "V count under {transform:?}");
            if soft.is_some() {
                assert_eq!(
                    tables.h_ll, want.h_ll,
                    "H log-likelihood under {transform:?}"
                );
                assert_eq!(
                    tables.v_ll, want.v_ll,
                    "V log-likelihood under {transform:?}"
                );
            }
            // Weight sums in f32 and the reference in f64, so they agree to
            // rounding, not to the bit. Counts and log-likelihoods are integers
            // and must be exact.
            for (got, want) in tables
                .h_weight
                .iter()
                .chain(&tables.v_weight)
                .zip(want.h_weight.iter().chain(&want.v_weight))
            {
                assert!(
                    (*got as f64 - want).abs() <= 1e-4 * want.abs().max(1.0),
                    "weight {got} vs reference {want} under {transform:?}"
                );
            }
        }
    }

    #[test]
    fn tables_match_their_definition_on_a_clean_fragment() {
        assert_matches_reference(&fragment(9, (37, 211), 0), None);
    }

    #[test]
    fn tables_match_their_definition_with_both_bits_at_a_residue() {
        // Corrupting every fifth dot puts both bits in the same residue family,
        // the case the fold has to keep separate.
        assert_matches_reference(&fragment(11, (120, 4), 5), None);
    }

    #[test]
    fn soft_tables_match_their_definition() {
        let cfg = SoftLlConfig {
            kappa: 3.0,
            per_bit_floor: 0.02,
            alignment_min_margin: 0.0,
        };
        assert_matches_reference(&fragment(9, (37, 211), 5), Some(&cfg));
    }

    #[test]
    fn a_restricted_range_leaves_unreachable_classes_at_zero() {
        let observed = fragment(9, (37, 211), 0);
        let transformed = transform_observations(&observed, &GRID_TRANSFORMS_D4[0]);
        let geometry = CodeGeometry::master();
        let mut tables = ClassTables::new(&geometry, false);
        let range = ClassRange::of_origin_rect(&geometry, 37, 12, 211, 12);
        tables.build(&geometry, &transformed, &range, None);

        // A 12-wide origin rectangle reaches 12 of the 167 H rows; the other
        // 155 must be untouched, or the scan could return an origin outside the
        // declared board.
        let reached: Vec<usize> = (0..12).map(|k| (37 + k) % H_ROWS).collect();
        for a in 0..H_ROWS {
            for b in 0..H_COLS {
                let credited = tables.h_count[a * H_COLS + b] > 0;
                assert_eq!(
                    credited,
                    reached.contains(&a),
                    "H class ({a}, {b}) credited={credited} outside the declared rectangle"
                );
            }
        }
    }

    #[test]
    fn a_family_is_visited_once_however_its_dots_are_ordered() {
        let observed = fragment(9, (37, 211), 5);
        let mut shuffled = observed.clone();
        shuffled.reverse();

        let build = |obs: &[PuzzleBoardObservedEdge]| {
            let geometry = CodeGeometry::master();
            let transformed = transform_observations(obs, &GRID_TRANSFORMS_D4[0]);
            let mut tables = ClassTables::new(&geometry, false);
            tables.build(&geometry, &transformed, &ClassRange::full(&geometry), None);
            (tables.h_count.clone(), tables.v_count.clone())
        };
        assert_eq!(
            build(&observed),
            build(&shuffled),
            "table counts must not depend on observation order"
        );
    }
}
