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

/// Master rows in the code map's fundamental period along the wrapped axis.
const MAP_B_ROWS: u32 = crate::code_maps::EDGE_MAP_B_ROWS as u32;

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
    /// The corner rows a printable wrap strip spans: `squares + 1`.
    ///
    /// The first and last are the *same* row of the pattern — they land on each
    /// other when the strip is wrapped — which is the one-piece overlap the
    /// paper describes.
    #[must_use]
    pub const fn strip_corner_rows(self) -> u32 {
        self.squares + 1
    }

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
/// The circumferences are the seven in the paper's Table 1. Where the paper
/// lists one start row and the maps admit several, all are listed — they are
/// different patterns, and a pole's pattern is part of its identity.
///
/// **The period-36 correction.** The paper gives `start y = 325` for period 36.
/// On the shipped maps (which are the authors' own `code1` / `code2`, pinned
/// byte-for-byte by `code_maps::tests::shipped_maps_are_the_authors_code_verbatim`)
/// row `325 % 167 = 158` does not repeat at period 36 — not even one row. Row
/// `327 % 167 = 160` does, and is used here. The other six rows of Table 1
/// reproduce exactly, so this is almost certainly a transcription slip in the
/// paper rather than a different code revision.
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
        squares: 18,
        start_row: 7,
    },
    PuzzlePolePeriod {
        squares: 24,
        start_row: 75,
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
        squares: 36,
        start_row: 41,
    },
    PuzzlePolePeriod {
        squares: 36,
        start_row: 160,
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
        squares: 48,
        start_row: 115,
    },
];

/// The paper's choice of start row per circumference, reduced mod 167.
///
/// `(period, start_row)`. Period 36 carries the correction documented on
/// [`SUPPORTED_PERIODS`].
const CANONICAL_START_ROWS: &[(u32, u32)] = &[
    (12, 73),  // paper: start y = 73
    (18, 7),   // paper: start y = 7
    (24, 75),  // paper: start y = 242, 242 % 167 = 75
    (30, 9),   // paper: start y = 176, 176 % 167 = 9
    (36, 160), // paper: start y = 325 — does not close; 327 % 167 = 160 does
    (42, 76),  // paper: start y = 410, 410 % 167 = 76
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

/// Every seamless start row for `squares`, over one fundamental period.
///
/// Exposed for the design note and the tests: it is what proves
/// [`SUPPORTED_PERIODS`] is complete rather than a hand-picked subset.
#[must_use]
pub fn seamless_start_rows(squares: u32) -> Vec<u32> {
    (0..MAP_B_ROWS)
        .filter(|&s| is_seamless(squares, s))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(
                shipped,
                seamless_start_rows(squares),
                "the table's start rows for period {squares} are not every seamless one"
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

    /// Table 1 of arXiv:2511.19448, verbatim, against the authors' own maps.
    /// Six rows reproduce; period 36 does not, and 327 is what closes.
    #[test]
    fn paper_table_1_reproduces_except_the_period_36_row() {
        // (period, paper's `start y`)
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
            let closes = is_seamless(squares, start_y % MAP_B_ROWS);
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
            is_seamless(36, 327 % MAP_B_ROWS),
            "327 is the documented correction for the period-36 row"
        );
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
        assert_eq!(twelve, vec![73, 118]);
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

    #[test]
    fn a_strip_spans_one_more_corner_row_than_it_has_pieces() {
        let p = PuzzlePolePeriod::canonical(12).expect("period 12 is supported");
        assert_eq!(p.strip_corner_rows(), 13);
    }
}
