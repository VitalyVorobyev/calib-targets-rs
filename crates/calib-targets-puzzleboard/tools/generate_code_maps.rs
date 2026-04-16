//! Generator for the two binary sub-perfect maps used by PuzzleBoard.
//!
//! # What we generate
//!
//! - **Map A**: cyclic binary array of shape `3 × 167`.
//! - **Map B**: cyclic binary array of shape `167 × 3`.
//!
//! Both maps satisfy the **paper's** `(3, 167; 3, 3)₂` sub-perfect property:
//! **every cyclic 3×3 window** — including all three row shifts — is pairwise
//! distinct across the map (501 distinct windows per map). Uniqueness across
//! the full 501×501 master PuzzleBoard follows for any observed ≥ 3×3 window.
//!
//! # Algorithm
//!
//! Treat each 3-bit column of A as a letter `l ∈ Σ = {0..7}` via the bit
//! packing `l = b0 | (b1 << 1) | (b2 << 2)`. A row-shift by 1 corresponds to
//! the letter-permutation σ: (b0, b1, b2) → (b1, b2, b0). σ has order 3 with
//! fixed points {0, 7}, so the 512 binary 3-grams split into 8 singleton
//! orbits (all-0, all-7 triples and their kin) and 168 orbits of size 3.
//!
//! For the 501 cyclic 3×3 windows of A to all be distinct we need:
//! 1. Each of the 167 column-aligned triples `t_c = (s[c], s[c+1], s[c+2])`
//!    is *not* a σ-fixed point (else the three row-shifts coincide).
//! 2. Each `t_c` lives in a different σ-orbit from every other `t_c'`.
//!
//! We find such sequences via **stochastic hill-climbing**: start from a
//! random length-167 sequence; the energy is the number of invalid triples
//! (fixed-points + orbit duplicates); propose single-letter mutations and
//! accept if energy decreases. Random restarts break out of local minima.
//! With 168 available orbits and 167 required, near-optimum is trivially
//! reached in milliseconds.
//!
//! # Output
//!
//! `src/data/map_a.bin`, `src/data/map_b.bin` (63 bytes each, row-major,
//! LSB-first) and `src/data/map_metadata.json`.

use std::fs;
use std::path::{Path, PathBuf};

const MAP_CYCLIC_LEN: usize = 167;

const SEED_A: u64 = 0xA5A5_F00D_B2B2_1357;
const SEED_B: u64 = 0xB3B3_C0FF_EEFE_2468;

struct Xorshift64(u64);
impl Xorshift64 {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn next_usize(&mut self, n: usize) -> usize {
        (self.next_u64() as usize) % n.max(1)
    }
}

/// σ: (b0, b1, b2) → (b1, b2, b0) on 3-bit letters.
#[inline]
fn sigma(l: u8) -> u8 {
    let b0 = l & 1;
    let b1 = (l >> 1) & 1;
    let b2 = (l >> 2) & 1;
    b1 | (b2 << 1) | (b0 << 2)
}

/// Pack a triple to a 9-bit code (component 0 → bits 0..3).
#[inline]
fn pack_triple(t: (u8, u8, u8)) -> u16 {
    (t.0 as u16) | ((t.1 as u16) << 3) | ((t.2 as u16) << 6)
}

#[inline]
fn sigma_triple(t: (u8, u8, u8)) -> (u8, u8, u8) {
    (sigma(t.0), sigma(t.1), sigma(t.2))
}

#[inline]
fn orbit_id(t: (u8, u8, u8)) -> u16 {
    let s1 = sigma_triple(t);
    let s2 = sigma_triple(s1);
    pack_triple(t).min(pack_triple(s1)).min(pack_triple(s2))
}

#[inline]
fn is_sigma_fixed(t: (u8, u8, u8)) -> bool {
    sigma_triple(t) == t
}

#[inline]
fn triple_at(s: &[u8], c: usize) -> (u8, u8, u8) {
    let n = s.len();
    (s[c], s[(c + 1) % n], s[(c + 2) % n])
}

/// Energy = number of column triples that are σ-fixed + number of orbit
/// duplicates (orbit count - 1 summed over duplicated orbits).
fn energy(s: &[u8]) -> (usize, std::collections::HashMap<u16, usize>) {
    let mut counts: std::collections::HashMap<u16, usize> =
        std::collections::HashMap::with_capacity(s.len());
    let mut fixed_count = 0usize;
    for c in 0..s.len() {
        let t = triple_at(s, c);
        if is_sigma_fixed(t) {
            fixed_count += 1;
        }
        *counts.entry(orbit_id(t)).or_insert(0) += 1;
    }
    let dup: usize = counts.values().map(|&v| v.saturating_sub(1)).sum();
    (fixed_count + dup, counts)
}

/// Which column indices are affected by mutating `s[k]`? The triples at
/// columns `k-2, k-1, k` all include `s[k]`.
fn affected_columns(n: usize, k: usize) -> [usize; 3] {
    [(k + n - 2) % n, (k + n - 1) % n, k % n]
}

fn find_orbit_unique_cycle(seed: u64, max_iters: u64) -> Option<Vec<u8>> {
    let mut rng = Xorshift64::new(seed);
    let n = MAP_CYCLIC_LEN;

    for restart in 0..128u64 {
        // Random initial sequence.
        let mut s: Vec<u8> = (0..n).map(|_| (rng.next_u64() as u8) & 7).collect();

        let (mut e, mut counts) = energy(&s);
        if e == 0 {
            return Some(s);
        }

        let per_restart = (max_iters / 128).max(1);
        let mut stale = 0u64;

        for _step in 0..per_restart {
            let k = rng.next_usize(n);
            let old_val = s[k];
            // Pick a different value for s[k].
            let mut new_val = old_val;
            while new_val == old_val {
                new_val = (rng.next_u64() as u8) & 7;
            }

            // Evaluate delta by incrementally updating counts.
            // Snapshot affected cols' old triples.
            let cols = affected_columns(n, k);
            let mut old_triples = [(0u8, 0u8, 0u8); 3];
            for (i, &c) in cols.iter().enumerate() {
                old_triples[i] = triple_at(&s, c);
            }
            // Apply mutation tentatively and evaluate new triples.
            s[k] = new_val;
            let mut new_triples = [(0u8, 0u8, 0u8); 3];
            for (i, &c) in cols.iter().enumerate() {
                new_triples[i] = triple_at(&s, c);
            }

            // Compute proposed energy change.
            let mut trial_counts = counts.clone();
            let mut delta: i32 = 0;

            for t in &old_triples {
                if is_sigma_fixed(*t) {
                    delta -= 1;
                }
                let oid = orbit_id(*t);
                let entry = trial_counts.entry(oid).or_insert(0);
                if *entry > 1 {
                    delta -= 1;
                }
                *entry = entry.saturating_sub(1);
                if *entry == 0 {
                    trial_counts.remove(&oid);
                }
            }
            for t in &new_triples {
                if is_sigma_fixed(*t) {
                    delta += 1;
                }
                let oid = orbit_id(*t);
                let entry = trial_counts.entry(oid).or_insert(0);
                if *entry >= 1 {
                    delta += 1;
                }
                *entry += 1;
            }

            if delta <= 0 {
                counts = trial_counts;
                e = ((e as i32) + delta) as usize;
                if e == 0 {
                    println!("  restart {restart}: solved after {_step} steps");
                    return Some(s);
                }
                stale = 0;
            } else {
                // Reject.
                s[k] = old_val;
                stale += 1;
                if stale > 200_000 {
                    break; // random restart
                }
            }
        }
        // restart
    }
    None
}

fn letters_to_bit_rows(seq: &[u8], rows: usize) -> Vec<Vec<u8>> {
    assert_eq!(rows, 3);
    let mut out = vec![Vec::with_capacity(seq.len()); rows];
    for &letter in seq {
        for (r, row) in out.iter_mut().enumerate() {
            row.push((letter >> r) & 1);
        }
    }
    out
}

fn verify_cyclic_3x3_all_unique(bit_rows: &[Vec<u8>]) -> bool {
    let rows = bit_rows.len();
    let cols = bit_rows[0].len();
    let mut seen: std::collections::HashSet<u16> = std::collections::HashSet::with_capacity(rows * cols);
    for r0 in 0..rows {
        for c0 in 0..cols {
            let mut code: u16 = 0;
            for dr in 0..3 {
                for dc in 0..3 {
                    let r = (r0 + dr) % rows;
                    let c = (c0 + dc) % cols;
                    code = (code << 1) | (bit_rows[r][c] as u16);
                }
            }
            if !seen.insert(code) {
                return false;
            }
        }
    }
    true
}

fn pack_bits_row_major(bit_rows: &[Vec<u8>]) -> Vec<u8> {
    let cols = bit_rows[0].len();
    let total = bit_rows.len() * cols;
    let bytes = total.div_ceil(8);
    let mut out = vec![0u8; bytes];
    for (r, row) in bit_rows.iter().enumerate() {
        for (c, &bit) in row.iter().enumerate() {
            let idx = r * cols + c;
            if bit != 0 {
                out[idx / 8] |= 1 << (idx % 8);
            }
        }
    }
    out
}

fn data_dir() -> PathBuf {
    let manifest = env!("CARGO_MANIFEST_DIR");
    Path::new(manifest).join("src").join("data")
}

fn write_bytes(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create data dir");
    }
    fs::write(path, bytes).expect("write data blob");
    println!("wrote {} ({} bytes)", path.display(), bytes.len());
}

fn main() {
    println!("generating map A (3 × 167) …");
    let seq_a = find_orbit_unique_cycle(SEED_A, 50_000_000).expect("generate map A");
    let bits_a = letters_to_bit_rows(&seq_a, 3);
    assert!(
        verify_cyclic_3x3_all_unique(&bits_a),
        "map A: all 501 cyclic 3×3 windows must be distinct"
    );
    let bytes_a = pack_bits_row_major(&bits_a);

    println!("generating map B (167 × 3) …");
    let seq_b = find_orbit_unique_cycle(SEED_B, 50_000_000).expect("generate map B");
    let bits_b_src = letters_to_bit_rows(&seq_b, 3);
    assert!(
        verify_cyclic_3x3_all_unique(&bits_b_src),
        "map B (pre-transpose): all 501 cyclic 3×3 windows must be distinct"
    );
    let bits_b: Vec<Vec<u8>> = (0..MAP_CYCLIC_LEN)
        .map(|r| (0..3).map(|c| bits_b_src[c][r]).collect::<Vec<u8>>())
        .collect();
    let bytes_b = pack_bits_row_major(&bits_b);

    let dir = data_dir();
    write_bytes(&dir.join("map_a.bin"), &bytes_a);
    write_bytes(&dir.join("map_b.bin"), &bytes_b);

    let meta = serde_json::json!({
        "_comment": "Generated by calib-targets-puzzleboard/tools/generate_code_maps.rs. Do not edit.",
        "map_a": {
            "rows": 3,
            "cols": MAP_CYCLIC_LEN,
            "seed": format!("0x{:016X}", SEED_A),
            "bytes": bytes_a.len(),
            "packing": "row-major, LSB-first",
            "property": "all 501 cyclic 3×3 windows pairwise distinct (paper's (3,167;3,3)_2 sub-perfect)",
        },
        "map_b": {
            "rows": MAP_CYCLIC_LEN,
            "cols": 3,
            "seed": format!("0x{:016X}", SEED_B),
            "bytes": bytes_b.len(),
            "packing": "row-major, LSB-first",
            "property": "all 501 cyclic 3×3 windows pairwise distinct (paper's (167,3;3,3)_2 sub-perfect)",
        },
        "master_pattern": {
            "rows": 3 * MAP_CYCLIC_LEN,
            "cols": 3 * MAP_CYCLIC_LEN,
            "note": "Master 501×501 PuzzleBoard (arXiv:2409.20127).",
        },
    });
    let meta_path = dir.join("map_metadata.json");
    fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta).expect("serialize metadata"),
    )
    .expect("write metadata");
    println!("wrote {}", meta_path.display());
}
