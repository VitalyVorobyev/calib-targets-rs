//! Embedded code maps and lookup helpers.
//!
//! Two cyclic binary maps are shipped with the crate:
//!
//! - [`map_a`]: shape `(3, 167)` — bit for each **vertical** edge
//! - [`map_b`]: shape `(167, 3)` — bit for each **horizontal** edge
//!
//! Both maps tile cyclically to cover the full 501×501 master PuzzleBoard.
//! This matches the authors' (PStelldinger/PuzzleBoard) convention:
//!
//! - Vertical edge bit at `(row, col)`:   `map_a[row % 3][col % 167]`
//!   (derived from `vfullCode = tile(code1)`)
//! - Horizontal edge bit at `(row, col)`: `map_b[row % 167][col % 3]`
//!   (derived from `hfullCode = tile(rot90(code2[::-1,::-1]))`)
//!
//! ## Uniqueness property
//!
//! The shipped maps satisfy the **paper's** `(3, 167; 3, 3)₂` sub-perfect
//! property: every cyclic 3×3 window (all three row shifts, all 167 column
//! shifts) is pairwise distinct across the map — 501 unique windows per
//! map. See [`verify_cyclic_window_unique`] and the bundled tests.
//!
//! Construction is done once via stochastic hill-climbing in
//! `tools/generate_code_maps.rs` (the companion paper arXiv:2405.03309 has
//! a closed-form construction but no reference implementation; local search
//! converges in milliseconds because 167 needed windows out of 168
//! available orbits is a tiny assignment problem).
//!
//! The maps are generated **once** by the `generate-puzzleboard-code-maps`
//! binary under `tools/` and committed as `src/data/map_a.bin` /
//! `src/data/map_b.bin`. They are loaded at compile time via `include_bytes!`.

use crate::board::{MASTER_COLS, MASTER_ROWS};

/// Cyclic period of map A along its *col* axis.
pub const EDGE_MAP_A_COLS: usize = 167;
/// Row count of map A (always 3).
pub const EDGE_MAP_A_ROWS: usize = 3;
/// Cyclic period of map B along its *row* axis.
pub const EDGE_MAP_B_ROWS: usize = 167;
/// Col count of map B (always 3).
pub const EDGE_MAP_B_COLS: usize = 3;

const DATA_A: &[u8] = include_bytes!("data/map_a.bin");
const DATA_B: &[u8] = include_bytes!("data/map_b.bin");

/// Read-only view over a packed binary map.
#[derive(Clone, Copy)]
pub struct BitMap {
    bytes: &'static [u8],
    rows: usize,
    cols: usize,
}

impl BitMap {
    #[inline]
    pub fn rows(&self) -> usize {
        self.rows
    }
    #[inline]
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Fetch the bit at logical `(row, col)` with cyclic wrap-around.
    #[inline]
    pub fn get_cyclic(&self, row: i64, col: i64) -> u8 {
        let r = row.rem_euclid(self.rows as i64) as usize;
        let c = col.rem_euclid(self.cols as i64) as usize;
        let idx = r * self.cols + c;
        (self.bytes[idx / 8] >> (idx % 8)) & 1
    }

    /// Fetch the bit at logical `(row, col)` with bounds checking (no wrap).
    #[inline]
    pub fn get(&self, row: usize, col: usize) -> Option<u8> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        let idx = row * self.cols + col;
        Some((self.bytes[idx / 8] >> (idx % 8)) & 1)
    }
}

/// The committed A map — governs *horizontal*-edge bits.
#[inline]
pub fn map_a() -> BitMap {
    BitMap {
        bytes: DATA_A,
        rows: EDGE_MAP_A_ROWS,
        cols: EDGE_MAP_A_COLS,
    }
}

/// The committed B map — governs *vertical*-edge bits.
#[inline]
pub fn map_b() -> BitMap {
    BitMap {
        bytes: DATA_B,
        rows: EDGE_MAP_B_ROWS,
        cols: EDGE_MAP_B_COLS,
    }
}

/// Orientation of an interior edge between two adjacent checkerboard squares.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeOrientation {
    /// Edge runs horizontally → separates cells `(r, c)` and `(r+1, c)`.
    Horizontal,
    /// Edge runs vertically → separates cells `(r, c)` and `(r, c+1)`.
    Vertical,
}

/// A single observed edge bit in the local board frame.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct PuzzleBoardObservedEdge {
    /// Board row coordinate of the edge (see edge-indexing conventions below).
    pub row: i32,
    /// Board column coordinate of the edge.
    pub col: i32,
    /// Edge orientation.
    pub orientation: EdgeOrientation,
    /// Observed bit (0 / 1).
    pub bit: u8,
    /// Per-bit confidence in `[0, 1]` — 1.0 = crisp, 0.0 = ambiguous.
    pub confidence: f32,
}

/// Expected master-pattern bit for a horizontal edge at `(row, col)`.
///
/// The horizontal edge at `(row, col)` is the edge *below* row `row` at
/// column `col`, i.e. between square `(row, col)` and square `(row+1, col)`.
///
/// Uses `map_b` (167×3), matching the authors' `hfullCode = tile(rot90(code2[::-1,::-1]))`.
/// At `(mr, mc)` this evaluates to `map_b[mr % 167][mc % 3]`.
#[inline]
pub fn horizontal_edge_bit(master_row: i32, master_col: i32) -> u8 {
    map_b().get_cyclic(master_row as i64, master_col as i64)
}

/// Expected master-pattern bit for a vertical edge at `(row, col)`.
///
/// The vertical edge at `(row, col)` is the edge *right of* column `col` at
/// row `row`, i.e. between square `(row, col)` and square `(row, col+1)`.
///
/// Uses `map_a` (3×167), matching the authors' `vfullCode = tile(code1)`.
/// At `(mr, mc)` this evaluates to `map_a[mr % 3][mc % 167]`.
#[inline]
pub fn vertical_edge_bit(master_row: i32, master_col: i32) -> u8 {
    map_a().get_cyclic(master_row as i64, master_col as i64)
}

/// Verify every cyclic `(wr × wc)` window of `map` is pairwise unique.
///
/// This is the full PuzzleBoard uniqueness property: `(M, N; wr, wc)_2`
/// sub-perfect. For the shipped maps, `verify_cyclic_window_unique(map_a(),
/// 3, 3)` enumerates 3·167 = 501 windows; `verify_cyclic_window_unique(
/// map_b(), 3, 3)` does likewise for B. Both succeed.
///
/// Returns [`WindowError::InvalidWindow`] if `wr` or `wc` is zero, or
/// exceeds the map dimensions.
pub fn verify_cyclic_window_unique(map: BitMap, wr: usize, wc: usize) -> Result<(), WindowError> {
    if wr == 0 || wc == 0 || wr > map.rows() || wc > map.cols() {
        return Err(WindowError::InvalidWindow {
            wr,
            wc,
            max_rows: map.rows(),
            max_cols: map.cols(),
        });
    }
    let mut seen: std::collections::HashMap<u64, (usize, usize)> =
        std::collections::HashMap::with_capacity(map.rows() * map.cols());
    for r in 0..map.rows() {
        for c in 0..map.cols() {
            let mut code: u64 = 0;
            for dr in 0..wr {
                for dc in 0..wc {
                    let bit = map.get_cyclic((r + dr) as i64, (c + dc) as i64);
                    code = (code << 1) | (bit as u64);
                }
            }
            if let Some(prev) = seen.insert(code, (r, c)) {
                return Err(WindowError::Duplicate {
                    first: prev,
                    second: (r, c),
                    code,
                });
            }
        }
    }
    Ok(())
}

/// Error from [`verify_cyclic_window_unique`].
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum WindowError {
    #[error("duplicate window at {first:?} and {second:?} (code = {code:#x})")]
    Duplicate {
        first: (usize, usize),
        second: (usize, usize),
        code: u64,
    },
    #[error(
        "invalid window size ({wr}×{wc}): must be non-zero and fit within map ({max_rows}×{max_cols})"
    )]
    InvalidWindow {
        wr: usize,
        wc: usize,
        max_rows: usize,
        max_cols: usize,
    },
}

/// Number of distinct master-edge positions in the horizontal direction.
#[inline]
pub const fn master_horizontal_edges_rows() -> u32 {
    MASTER_ROWS
}
/// Number of distinct master-edge positions in the vertical direction.
#[inline]
pub const fn master_vertical_edges_cols() -> u32 {
    MASTER_COLS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_a_all_3x3_cyclic_windows_unique() {
        verify_cyclic_window_unique(map_a(), 3, 3).expect("map A 3×3 cyclic");
    }

    #[test]
    fn map_b_all_3x3_cyclic_windows_unique() {
        verify_cyclic_window_unique(map_b(), 3, 3).expect("map B 3×3 cyclic");
    }

    #[test]
    fn cyclic_indexing_wraps() {
        let a = map_a();
        assert_eq!(a.get_cyclic(0, 0), a.get_cyclic(0, EDGE_MAP_A_COLS as i64));
        assert_eq!(a.get_cyclic(0, 0), a.get_cyclic(3, 0));
        assert_eq!(a.get_cyclic(0, 1), a.get_cyclic(-3, 1));
    }

    /// Empirical master-board 4×4 window uniqueness: for every pair of
    /// positions `(R, C)` on the 501×501 master board, the concatenation of
    /// horizontal and vertical edge-bits of a 4×4 window differs from the
    /// concatenation at any other position. This is the end-to-end guarantee
    /// the decoder relies on to localise absolute position.
    #[test]
    fn master_4x4_windows_unique() {
        let m_rows = MASTER_ROWS as i32;
        let m_cols = MASTER_COLS as i32;
        let mut seen: std::collections::HashMap<Vec<u8>, (i32, i32)> =
            std::collections::HashMap::with_capacity((m_rows * m_cols) as usize);
        let w = 4i32;
        for r0 in 0..m_rows {
            for c0 in 0..m_cols {
                // 3 horizontal-edge rows × 4 cols = 12 bits, stored first.
                // 4 vertical-edge rows × 3 cols = 12 bits next.
                let mut key = Vec::with_capacity(3);
                let mut acc: u32 = 0;
                let mut nb: u32 = 0;
                let emit = |acc: &mut u32, nb: &mut u32, key: &mut Vec<u8>, bit: u8| {
                    *acc = (*acc << 1) | (bit as u32);
                    *nb += 1;
                    if *nb == 8 {
                        key.push(*acc as u8);
                        *acc = 0;
                        *nb = 0;
                    }
                };
                for dr in 0..(w - 1) {
                    for dc in 0..w {
                        let bit = horizontal_edge_bit(r0 + dr, c0 + dc);
                        emit(&mut acc, &mut nb, &mut key, bit);
                    }
                }
                for dr in 0..w {
                    for dc in 0..(w - 1) {
                        let bit = vertical_edge_bit(r0 + dr, c0 + dc);
                        emit(&mut acc, &mut nb, &mut key, bit);
                    }
                }
                if nb > 0 {
                    key.push(acc as u8);
                }
                if let Some(prev) = seen.insert(key, (r0, c0)) {
                    panic!("duplicate 4×4 window at {:?} and {:?}", prev, (r0, c0));
                }
            }
        }
    }
}
