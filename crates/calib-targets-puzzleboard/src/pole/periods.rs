//! Circumference periods that close seamlessly around a cylinder.
//!
//! A [`PuzzlePole`](crate::PuzzlePoleSpec) is a strip of the master
//! PuzzleBoard wrapped around a cylinder. For the wrap to be *seamless* the
//! pattern must be genuinely periodic along the wrapped axis: the strip's last
//! corner row has to be the same row of puzzle pieces as its first, so that
//! gluing them together introduces no discontinuity a decoder could trip over.
//!
//! # Which axis wraps
//!
//! In this crate's convention (see [`code_maps`](crate::code_maps)) a
//! horizontal-edge bit is `map_b[row % 167][col % 3]` and a vertical-edge bit
//! is `map_a[row % 3][col % 167]`. The long (167) period therefore lives in the
//! **row** index for `map_b` and in the **col** index for `map_a`. Only the row
//! axis can be shortened to a small period, because only there does a long,
//! aperiodic code sit that can repeat early. So:
//!
//! - **row** (`master_row`, `j`) is the **circumference** axis, and
//! - **col** (`master_col`, `i`) is the **axial** (along-the-cylinder) axis.
//!
//! # The condition
//!
//! A period `p` starting at master row `s` is seamless when the *whole* row of
//! puzzle pieces repeats — both edge families and the checkerboard colour,
//! across the full 501-column master width — at `s` and again at `s + 1`:
//!
//! ```text
//! for every col c:  horizontal_edge_bit(r, c) == horizontal_edge_bit(r + p, c)
//!                   vertical_edge_bit(r, c)   == vertical_edge_bit(r + p, c)
//!                   (r + c) % 2               == (r + p + c) % 2
//! for r = s and r = s + 1
//! ```
//!
//! [`is_seamless`] is exactly that predicate, evaluated against the shipped
//! maps. It implies `p % 6 == 0` — `% 3` from `map_a[row % 3]` and `% 2` from
//! the checkerboard colour, which is the paper's "stripe periodicity of 3 times
//! chessboard periodicity of 2".
//!
//! **Two rows, and only two.** The repetition is exactly two rows deep; the
//! third row does *not* repeat, at any of the supported periods. That is the
//! paper's construction working as designed — two rows is what a local 3×3
//! piece code needs, so the seam creates no new local codes — but it is also
//! why a PuzzlePole cannot be decoded against the 501×501 master: the decoder's
//! window spans more than three piece rows, so a seam-crossing window matches
//! no master position at all. A pole is decoded against its own `p`-periodic
//! code stripe instead. `pole_construction_repeats_exactly_two_rows` pins this.
//!
//! # Provenance
//!
//! The table below is the one in Table 1 of Zach & Stelldinger,
//! *PuzzlePoles: Cylindrical Fiducial Markers Based on the PuzzleBoard
//! Pattern* (arXiv:2511.19448), re-derived from the shipped maps rather than
//! transcribed. Six of the paper's seven rows reproduce exactly. The seventh —
//! period 36 at `start y = 325` — does **not** satisfy the condition on these
//! maps, while `327` does; see [`SUPPORTED_PERIODS`].

use crate::code_maps::{horizontal_edge_bit, vertical_edge_bit};

/// Number of master columns a piece row is compared over.
///
/// The master pattern is 501 columns wide and both maps tile cyclically, so
/// agreement across 501 columns is agreement everywhere.
const COMPARE_COLS: i32 = crate::board::MASTER_COLS as i32;

/// A circumference period that closes seamlessly on the shipped code maps.
///
/// `squares` is the number of puzzle pieces around the cylinder, so the
/// circumference is `squares × cell_size` and the radius is
/// `squares × cell_size / 2π`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PuzzlePolePeriod {
    /// Puzzle pieces around the circumference.
    pub squares: u32,
    /// Master row the wrapped strip starts at.
    pub start_row: u32,
}

impl PuzzlePolePeriod {
    /// The piece size that makes this period wrap a cylinder of `diameter_mm`.
    ///
    /// The usual situation is the reverse of the one
    /// [`PuzzlePoleSpec::new`](super::PuzzlePoleSpec::new) assumes: you have a
    /// tube, and you need a target that fits it. Since
    /// `diameter = squares × piece / π`, the piece size follows —
    ///
    /// ```
    /// use calib_targets_puzzleboard::PuzzlePolePeriod;
    ///
    /// // A 75 mm tube, wrapped at each supported circumference.
    /// for period in PuzzlePolePeriod::all_with_squares(18) {
    ///     let piece = period.piece_size_for_diameter(75.0);
    ///     assert!((piece - 13.09).abs() < 0.01);
    /// }
    /// ```
    ///
    /// Every period can wrap every diameter; what changes is how big a piece
    /// is, and therefore how many pixels a code dot gets. Prefer the largest
    /// piece the tube and the sheet allow.
    #[must_use]
    pub fn piece_size_for_diameter(self, diameter_mm: f32) -> f32 {
        core::f32::consts::PI * diameter_mm / self.squares as f32
    }

    /// Are every one of this period's `3p` local 3 x 3 codes distinct?
    ///
    /// This is the decode's actual precondition — a local code has to pin its
    /// position around the circumference, or a fragment cannot be placed. It is
    /// what the paper's "no additional patch has been generated" amounts to,
    /// and it is provable from the seam condition, but it is cheap enough to
    /// check rather than argue.
    ///
    /// Every shipped period satisfies it (`every_supported_period_has_unique_local_codes`).
    /// It is public because it is the test a *candidate* period has to pass:
    /// a seam that closes is necessary and not sufficient.
    #[must_use]
    pub fn local_codes_are_unique(self) -> bool {
        super::code::PoleCode::new(self).windows_are_unique()
    }

    /// Look up a verified period by its `(squares, start_row)` pair.
    ///
    /// Returns `None` for any pair not in [`SUPPORTED_PERIODS`], even if it
    /// happens to satisfy [`is_seamless`] — shipping a period means its decode
    /// uniqueness has been measured too, not just its seam.
    #[must_use]
    pub fn lookup(squares: u32, start_row: u32) -> Option<Self> {
        SUPPORTED_PERIODS
            .iter()
            .copied()
            .find(|p| p.squares == squares && p.start_row == start_row)
    }

    /// The canonical start row for a circumference of `squares` pieces.
    ///
    /// Where the paper names a start row, this is the paper's choice (so a pole
    /// built here is the same physical pattern as one built from the paper),
    /// with the single exception of period 36 — see [`SUPPORTED_PERIODS`].
    /// Several periods admit more than one seamless start row; the alternatives
    /// are distinct patterns and are all listed in [`SUPPORTED_PERIODS`].
    #[must_use]
    pub fn canonical(squares: u32) -> Option<Self> {
        CANONICAL_START_ROWS
            .iter()
            .copied()
            .find(|&(p, _)| p == squares)
            .map(|(squares, start_row)| Self { squares, start_row })
    }

    /// Every verified period with this many pieces around the circumference.
    pub fn all_with_squares(squares: u32) -> impl Iterator<Item = Self> {
        SUPPORTED_PERIODS
            .iter()
            .copied()
            .filter(move |p| p.squares == squares)
    }
}

/// Every `(period, start row)` pair this crate supports.
///
/// Derived from the shipped code maps by [`is_seamless`] and pinned by
/// `supported_periods_are_all_seamless`, so the table cannot drift from the
/// maps it describes.
///
/// # A start row names a pattern modulo 501, not modulo 167
///
/// Seamlessness depends on `map_b` only through `start_row % 167`, so it is
/// tempting to store the reduction. That would be wrong: `map_a` is indexed by
/// `start_row % 3` and the checkerboard colour by `start_row % 2`, and 167 is
/// neither even nor a multiple of 3 — so `s`, `s + 167` and `s + 334` are three
/// **different patterns** that all close. No rotation of the circumference
/// relates them either: undoing the `map_a` shift needs a rotation of
/// `k ≢ 0 (mod 3)`, while leaving the stripe alone needs `k ≡ 0 (mod p)`, and
/// `3 | p` makes those incompatible.
///
/// So each circumference has three phases per seamless residue, all listed
/// here, and [`PuzzlePolePeriod::canonical`] returns the paper's **absolute**
/// `start y` rather than its reduction. A board printed from the paper's
/// parameters and one printed from the reduction are not the same target, and
/// a decoder told the wrong one reads the vertical dots off by a row.
///
/// **The period-36 correction.** The paper gives `start y = 325` for period 36.
/// On the shipped maps — the authors' own `code1` / `code2`, pinned byte for
/// byte by `code_maps::tests::shipped_maps_are_the_authors_code_verbatim` — row
/// 325 does not repeat at period 36, not even one row. Row 327 does, and is
/// used here. The other six rows of Table 1 reproduce exactly, so this reads as
/// a transcription slip rather than a different code revision.
///
/// **One exclusion.** Period 36 at start row 494 is seamless but its `p + 2`
/// piece strip runs off the master's last row, so it cannot be cut. It is left
/// out rather than shipped as a spec that fails at render time; the other five
/// phases of that circumference are unaffected.
pub const SUPPORTED_PERIODS: &[PuzzlePolePeriod] = &[
    PuzzlePolePeriod {
        squares: 12,
        start_row: 73,
    },
    PuzzlePolePeriod {
        squares: 12,
        start_row: 118,
    },
    PuzzlePolePeriod {
        squares: 12,
        start_row: 240,
    },
    PuzzlePolePeriod {
        squares: 12,
        start_row: 285,
    },
    PuzzlePolePeriod {
        squares: 12,
        start_row: 407,
    },
    PuzzlePolePeriod {
        squares: 12,
        start_row: 452,
    },
    PuzzlePolePeriod {
        squares: 18,
        start_row: 7,
    },
    PuzzlePolePeriod {
        squares: 18,
        start_row: 174,
    },
    PuzzlePolePeriod {
        squares: 18,
        start_row: 341,
    },
    PuzzlePolePeriod {
        squares: 24,
        start_row: 75,
    },
    PuzzlePolePeriod {
        squares: 24,
        start_row: 242,
    },
    PuzzlePolePeriod {
        squares: 24,
        start_row: 409,
    },
    PuzzlePolePeriod {
        squares: 30,
        start_row: 9,
    },
    PuzzlePolePeriod {
        squares: 30,
        start_row: 49,
    },
    PuzzlePolePeriod {
        squares: 30,
        start_row: 114,
    },
    PuzzlePolePeriod {
        squares: 30,
        start_row: 176,
    },
    PuzzlePolePeriod {
        squares: 30,
        start_row: 216,
    },
    PuzzlePolePeriod {
        squares: 30,
        start_row: 281,
    },
    PuzzlePolePeriod {
        squares: 30,
        start_row: 343,
    },
    PuzzlePolePeriod {
        squares: 30,
        start_row: 383,
    },
    PuzzlePolePeriod {
        squares: 30,
        start_row: 448,
    },
    PuzzlePolePeriod {
        squares: 36,
        start_row: 41,
    },
    PuzzlePolePeriod {
        squares: 36,
        start_row: 160,
    },
    PuzzlePolePeriod {
        squares: 36,
        start_row: 208,
    },
    PuzzlePolePeriod {
        squares: 36,
        start_row: 327,
    },
    PuzzlePolePeriod {
        squares: 36,
        start_row: 375,
    },
    PuzzlePolePeriod {
        squares: 42,
        start_row: 76,
    },
    PuzzlePolePeriod {
        squares: 42,
        start_row: 123,
    },
    PuzzlePolePeriod {
        squares: 42,
        start_row: 243,
    },
    PuzzlePolePeriod {
        squares: 42,
        start_row: 290,
    },
    PuzzlePolePeriod {
        squares: 42,
        start_row: 410,
    },
    PuzzlePolePeriod {
        squares: 42,
        start_row: 457,
    },
    PuzzlePolePeriod {
        squares: 48,
        start_row: 115,
    },
    PuzzlePolePeriod {
        squares: 48,
        start_row: 282,
    },
    PuzzlePolePeriod {
        squares: 48,
        start_row: 449,
    },
];

/// The paper's choice of start row per circumference, reduced mod 167.
///
/// `(period, start_row)`. Period 36 carries the correction documented on
/// [`SUPPORTED_PERIODS`].
const CANONICAL_START_ROWS: &[(u32, u32)] = &[
    (12, 73),  // paper: start y = 73
    (18, 7),   // paper: start y = 7
    (24, 242), // paper: start y = 242
    (30, 176), // paper: start y = 176
    (36, 327), // paper: start y = 325 -- does not close; 327 does
    (42, 410), // paper: start y = 410
    (48, 115), // paper: start y = 115
];

/// Does the pattern repeat with period `squares` starting at master row `start_row`?
///
/// This is the seam predicate stated in the module docs, evaluated against the
/// shipped maps: two consecutive rows of puzzle pieces — both edge families and
/// the checkerboard colour, across the full master width — must recur `squares`
/// rows later.
///
/// A `true` here means the wrap is geometrically seamless. It does **not** mean
/// the resulting pole decodes uniquely; that is a separate measurement, which
/// is why [`PuzzlePolePeriod::lookup`] consults [`SUPPORTED_PERIODS`] rather
/// than calling this.
#[must_use]
pub fn is_seamless(squares: u32, start_row: u32) -> bool {
    if squares == 0 {
        return false;
    }
    let start = start_row as i32;
    let period = squares as i32;
    (0..2).all(|k| piece_rows_agree(start + k, start + k + period))
}

/// Are the puzzle-piece rows at master rows `a` and `b` identical?
///
/// Compares the horizontal-edge bit, the vertical-edge bit and the checkerboard
/// colour at every one of the 501 master columns.
fn piece_rows_agree(a: i32, b: i32) -> bool {
    (0..COMPARE_COLS).all(|col| {
        horizontal_edge_bit(a, col) == horizontal_edge_bit(b, col)
            && vertical_edge_bit(a, col) == vertical_edge_bit(b, col)
            && (a + col).rem_euclid(2) == (b + col).rem_euclid(2)
    })
}

/// Every seamless start row for `squares`, over the whole master.
///
/// The range is the master's 501 rows, not `map_b`'s 167. Seamlessness depends
/// on `map_b` only modulo 167, but a start row names a *pattern* modulo 501,
/// because `map_a` and the checkerboard colour move with the other two phases.
/// Reducing a start row mod 167 therefore names a different pole — see
/// [`SUPPORTED_PERIODS`].
///
/// Exposed for the design note and the tests: it is what proves
/// [`SUPPORTED_PERIODS`] is complete rather than a hand-picked subset.
#[must_use]
pub fn seamless_start_rows(squares: u32) -> Vec<u32> {
    (0..crate::board::MASTER_ROWS)
        .filter(|&s| is_seamless(squares, s))
        .collect()
}

/// Does a strip at this start row fit inside the master's rows?
///
/// The printable strip is `squares + 2` pieces tall and the renderer cuts it
/// from a contiguous master rectangle, so a start row late enough that the
/// strip runs off the end is seamless but unbuildable. Exactly one shipped
/// circumference has such a phase; it is excluded rather than offered as a spec
/// that fails at render time.
#[must_use]
pub fn strip_fits_master(squares: u32, start_row: u32) -> bool {
    start_row + squares + 2 <= crate::board::MASTER_ROWS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows in `map_b`'s fundamental period — the modulus that is *not* enough
    /// to name a pattern. Only the tests need it; the shipped table works in
    /// absolute master rows precisely so nothing else does.
    const MAP_B_ROWS: u32 = crate::code_maps::EDGE_MAP_B_ROWS as u32;

    /// The shipped table is derived from the maps, not transcribed — so every
    /// entry must satisfy the predicate that defines it.
    #[test]
    fn supported_periods_are_all_seamless() {
        for p in SUPPORTED_PERIODS {
            assert!(
                is_seamless(p.squares, p.start_row),
                "period {} at start row {} is in the table but does not close",
                p.squares,
                p.start_row
            );
        }
    }

    /// ...and the table must be *complete* for the circumferences it covers,
    /// or `SUPPORTED_PERIODS` would be a hand-picked subset masquerading as a
    /// derivation.
    #[test]
    fn supported_periods_are_complete_for_every_circumference_listed() {
        let mut circumferences: Vec<u32> = SUPPORTED_PERIODS.iter().map(|p| p.squares).collect();
        circumferences.sort_unstable();
        circumferences.dedup();

        for squares in circumferences {
            let mut shipped: Vec<u32> = PuzzlePolePeriod::all_with_squares(squares)
                .map(|p| p.start_row)
                .collect();
            shipped.sort_unstable();
            let buildable: Vec<u32> = seamless_start_rows(squares)
                .into_iter()
                .filter(|&s| strip_fits_master(squares, s))
                .collect();
            assert_eq!(
                shipped, buildable,
                "the table's start rows for period {squares} are not every seamless \
                 one whose strip fits the master"
            );
        }
    }

    /// The construction repeats **exactly two** rows of puzzle pieces. The
    /// third does not repeat, which is why a pole must be decoded against its
    /// own `p`-periodic stripe and not against the 501×501 master.
    #[test]
    fn pole_construction_repeats_exactly_two_rows() {
        for p in SUPPORTED_PERIODS {
            let s = p.start_row as i32;
            let period = p.squares as i32;
            assert!(piece_rows_agree(s, s + period), "row 0 of period {p:?}");
            assert!(piece_rows_agree(s + 1, s + 1 + period), "row 1 of {p:?}");
            assert!(
                !piece_rows_agree(s + 2, s + 2 + period),
                "row 2 of {p:?} repeats — the construction is deeper than the \
                 paper describes, and the decode design assumes it is not"
            );
        }
    }

    /// Table 1 of arXiv:2511.19448, verbatim and **unreduced**, against the
    /// authors' own maps. Six rows reproduce; period 36 does not, and 327 is
    /// what closes.
    #[test]
    fn paper_table_1_reproduces_except_the_period_36_row() {
        // (period, the paper's `start y`, exactly as printed)
        const PAPER_TABLE_1: &[(u32, u32)] = &[
            (12, 73),
            (18, 7),
            (24, 242),
            (30, 176),
            (36, 325),
            (42, 410),
            (48, 115),
        ];

        for &(squares, start_y) in PAPER_TABLE_1 {
            let closes = is_seamless(squares, start_y);
            if squares == 36 {
                assert!(!closes, "period 36 at start y = 325 unexpectedly closes");
            } else {
                assert!(
                    closes,
                    "period {squares} at start y = {start_y} does not close"
                );
            }
        }

        assert!(
            is_seamless(36, 327),
            "327 is the documented correction for the period-36 row"
        );
    }

    /// The paper's start rows are shipped **unreduced**. Reducing one mod 167
    /// keeps it seamless but names a different pattern, because `map_a` and the
    /// checkerboard colour move with the other phases — so a board printed from
    /// the reduction would not be the paper's board.
    #[test]
    fn reducing_a_start_row_mod_167_names_a_different_pattern() {
        for squares in [24, 30, 36, 42] {
            let canonical = PuzzlePolePeriod::canonical(squares).expect("supported");
            let reduced = canonical.start_row % MAP_B_ROWS;
            assert_ne!(
                reduced, canonical.start_row,
                "period {squares} should have a canonical row above 167"
            );
            assert!(
                is_seamless(squares, reduced),
                "the reduction still closes -- that is exactly the trap"
            );
            let s = canonical.start_row as i32;
            let r = reduced as i32;
            assert!(
                (0..squares as i32).any(|k| !piece_rows_agree(s + k, r + k)),
                "period {squares}: the reduction is the same pattern after all"
            );
        }
    }

    #[test]
    fn every_canonical_start_row_is_a_supported_period() {
        for &(squares, start_row) in CANONICAL_START_ROWS {
            assert_eq!(
                PuzzlePolePeriod::canonical(squares),
                PuzzlePolePeriod::lookup(squares, start_row),
                "canonical period {squares} is not in the supported table"
            );
        }
    }

    /// The paper derives `p % 6 == 0` from "stripe periodicity of 3 times
    /// chessboard periodicity of 2". We never assert it — it falls out of the
    /// seam predicate — so check that it really does.
    #[test]
    fn a_seamless_period_is_always_a_multiple_of_six() {
        for squares in 1..=MAP_B_ROWS {
            if squares % 6 == 0 {
                continue;
            }
            assert!(
                seamless_start_rows(squares).is_empty(),
                "period {squares} is not a multiple of 6 yet closes somewhere"
            );
        }
    }

    /// A pole is identified by *which* strip it is cut from, so two different
    /// start rows at the same circumference must be different patterns.
    #[test]
    fn distinct_start_rows_are_distinct_patterns() {
        let twelve: Vec<u32> = PuzzlePolePeriod::all_with_squares(12)
            .map(|p| p.start_row)
            .collect();
        assert_eq!(twelve, vec![73, 118, 240, 285, 407, 452]);
        assert!(
            !piece_rows_agree(73, 118),
            "the two period-12 strips would be the same pattern"
        );
    }

    /// The reverse question — "I have this tube, what piece size do I need?" —
    /// must round-trip against the forward one.
    #[test]
    fn the_piece_size_for_a_diameter_reproduces_that_diameter() {
        for p in SUPPORTED_PERIODS {
            let piece = p.piece_size_for_diameter(75.0);
            let back = p.squares as f32 * piece / core::f32::consts::TAU * 2.0;
            assert!(
                (back - 75.0).abs() < 1e-3,
                "period {} round-tripped a 75 mm tube to {back} mm",
                p.squares
            );
        }
    }

    /// The paper describes its pole as "y corner point IDs from 73 to 85" —
    /// `p + 1` labels for `p` distinct rows, because the last is the first.
    /// That is a statement about the *wrapped* pattern and is not the height of
    /// the printed sheet, which carries two extra pieces for the trim and the
    /// glue overlap. Three units, and conflating them is the classic
    /// PuzzleBoard error, so the identity is asserted rather than named.
    #[test]
    fn the_last_labelled_corner_row_is_the_first_one() {
        let p = PuzzlePolePeriod::canonical(12).expect("period 12 is supported");
        let s = p.start_row as i32;
        assert!(piece_rows_agree(s, s + p.squares as i32));
    }
}
