//! Import the canonical PuzzleBoard code maps from the authors' reference
//! implementation (PStelldinger/PuzzleBoard, CC0 1.0).
//!
//! # What this tool does
//!
//! Reads `code1` and `code2` (both 3×167 {0,1} arrays) from the authors'
//! `puzzle_board_decoder.py`, applies the coordinate transforms mandated by the
//! contract report, and writes:
//!
//! - `src/data/map_a.bin` — 3×167 row-major LSB-first packed bits, derived from
//!   `code1` verbatim: `map_a[r][c] = code1[r][c]`.
//! - `src/data/map_b.bin` — 167×3 row-major LSB-first packed bits, derived from
//!   `code2` via `rot90(code2[::-1,::-1])`:
//!   `map_b[r][c] = code2[2 - c][r]` for `r ∈ [0, 167)`, `c ∈ [0, 3)`.
//! - `src/data/map_metadata.json` — provenance record (source + date, no seed).
//!
//! After writing, the tool runs the same uniqueness check as
//! `verify-puzzleboard-code-maps` to confirm both maps satisfy the
//! `(3, 167; 3, 3)₂` sub-perfect property.
//!
//! # Usage
//!
//! ```text
//! cargo run -p calib-targets-puzzleboard --bin import-author-puzzleboard-maps \
//!     -- /path/to/PStelldinger/PuzzleBoard/puzzle_board/puzzle_board_decoder.py
//! ```
//!
//! The path defaults to the project-local clone at
//! `/tmp/puzzleboard-ref/puzzle_board/puzzle_board_decoder.py`.

use std::{fs, path::Path};

const MAP_A_ROWS: usize = 3;
const MAP_A_COLS: usize = 167;
const MAP_B_ROWS: usize = 167;
const MAP_B_COLS: usize = 3;

/// Parse one `np.array([[…], […], […]]) * 2 - 1` block from the Python source.
///
/// The arrays in `puzzle_board_decoder.py` appear as three rows of space-separated
/// integers (0 / 1) wrapped in `[[…],[…],[…]]`. We extract the 0/1 digits ignoring
/// the `* 2 - 1` suffix (the raw {0,1} values are what we want).
fn parse_code_array(py_src: &str, var_name: &str) -> Vec<Vec<u8>> {
    // Find the variable assignment line.
    let prefix = format!("{var_name} = np.array([");
    let start = py_src
        .find(&prefix)
        .unwrap_or_else(|| panic!("could not find `{var_name}` in Python source"));
    let body_start = start + prefix.len();

    // Collect until the closing `])` that ends the outer array.
    let mut depth = 1i32; // we're already inside the outer `[`
    let mut end = body_start;
    for (i, ch) in py_src[body_start..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = body_start + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let inner = &py_src[body_start..end];

    // Split on `[` / `]` to extract each row.
    let mut rows: Vec<Vec<u8>> = Vec::new();
    let mut in_row = false;
    let mut current: Vec<u8> = Vec::new();

    for ch in inner.chars() {
        match ch {
            '[' => {
                in_row = true;
                current = Vec::new();
            }
            ']' if in_row => {
                rows.push(current.clone());
                in_row = false;
            }
            '0' if in_row => current.push(0),
            '1' if in_row => current.push(1),
            _ => {}
        }
    }
    rows
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

fn verify_cyclic_3x3_all_unique(rows: usize, cols: usize, bits: &[Vec<u8>]) {
    use std::collections::HashSet;
    let mut seen: HashSet<u16> = HashSet::with_capacity(rows * cols);
    for r0 in 0..rows {
        for c0 in 0..cols {
            let mut code: u16 = 0;
            for dr in 0..3usize {
                for dc in 0..3usize {
                    let r = (r0 + dr) % rows;
                    let c = (c0 + dc) % cols;
                    code = (code << 1) | (bits[r][c] as u16);
                }
            }
            assert!(
                seen.insert(code),
                "duplicate 3×3 window at ({r0},{c0}), code = {code:#05x}"
            );
        }
    }
    println!(
        "  uniqueness OK — all {}×{} = {} cyclic 3×3 windows distinct",
        rows,
        cols,
        rows * cols
    );
}

fn main() {
    let py_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/puzzleboard-ref/puzzle_board/puzzle_board_decoder.py".to_owned());

    println!("reading {py_path} …");
    let src = fs::read_to_string(&py_path).unwrap_or_else(|e| panic!("cannot read {py_path}: {e}"));

    // --- Parse code1 and code2 -------------------------------------------
    let code1 = parse_code_array(&src, "code1");
    let code2 = parse_code_array(&src, "code2");

    assert_eq!(
        (code1.len(), code1[0].len()),
        (MAP_A_ROWS, MAP_A_COLS),
        "code1 must be {MAP_A_ROWS}×{MAP_A_COLS}"
    );
    assert_eq!(
        (code2.len(), code2[0].len()),
        (MAP_A_ROWS, MAP_A_COLS),
        "code2 must be 3×{MAP_A_COLS}"
    );
    println!("parsed code1 ({MAP_A_ROWS}×{MAP_A_COLS}) and code2 ({MAP_A_ROWS}×{MAP_A_COLS})");

    // --- Build map_a = code1 verbatim (3×167) ----------------------------
    let map_a_bits: Vec<Vec<u8>> = code1.clone();
    println!("map_a: code1 verbatim ({MAP_A_ROWS}×{MAP_A_COLS})");

    // --- Build map_b = rot90(code2[::-1,::-1])  (167×3) -----------------
    // Derivation: rot90 is counterclockwise; for input shape (3, 167):
    //   rot90(A)[i, j] = A[j, cols-1-i]
    // Combined with flip [::-1,::-1]:
    //   let A[r, c] = code2[2-r, 166-c]
    //   rot90(A)[i, j] = A[j, 166-i] = code2[2-j, 166-(166-i)] = code2[2-j, i]
    // So: map_b[r][c] = code2[2-c][r]  for r ∈ [0,167), c ∈ [0,3)
    let map_b_bits: Vec<Vec<u8>> = (0..MAP_B_ROWS)
        .map(|r| {
            (0..MAP_B_COLS)
                .map(|c| code2[MAP_B_COLS - 1 - c][r])
                .collect()
        })
        .collect();
    println!("map_b: rot90(code2[::-1,::-1])  ({MAP_B_ROWS}×{MAP_B_COLS})");

    // --- Verify uniqueness -----------------------------------------------
    println!("verifying map_a …");
    verify_cyclic_3x3_all_unique(MAP_A_ROWS, MAP_A_COLS, &map_a_bits);
    println!("verifying map_b …");
    verify_cyclic_3x3_all_unique(MAP_B_ROWS, MAP_B_COLS, &map_b_bits);

    // --- Pack and write --------------------------------------------------
    let bytes_a = pack_bits_row_major(&map_a_bits);
    let bytes_b = pack_bits_row_major(&map_b_bits);

    let data_dir = {
        let manifest = env!("CARGO_MANIFEST_DIR");
        Path::new(manifest).join("src").join("data")
    };
    fs::create_dir_all(&data_dir).expect("create data dir");

    let path_a = data_dir.join("map_a.bin");
    let path_b = data_dir.join("map_b.bin");
    let path_meta = data_dir.join("map_metadata.json");

    fs::write(&path_a, &bytes_a).expect("write map_a.bin");
    println!("wrote {} ({} bytes)", path_a.display(), bytes_a.len());

    fs::write(&path_b, &bytes_b).expect("write map_b.bin");
    println!("wrote {} ({} bytes)", path_b.display(), bytes_b.len());

    let meta = serde_json::json!({
        "_comment": "Imported by calib-targets-puzzleboard/tools/import_author_maps.rs. Do not edit manually.",
        "source": "PStelldinger/PuzzleBoard (CC0 1.0) — code1 verbatim; code2 applied np.rot90(code2[::-1,::-1]) to produce 167x3 fundamental period",
        "imported_at": "2026-04-17",
        "map_a": {
            "rows": MAP_A_ROWS,
            "cols": MAP_A_COLS,
            "bytes": bytes_a.len(),
            "packing": "row-major, LSB-first",
            "derivation": "code1 verbatim",
            "property": "all 501 cyclic 3×3 windows pairwise distinct (paper's (3,167;3,3)_2 sub-perfect)",
        },
        "map_b": {
            "rows": MAP_B_ROWS,
            "cols": MAP_B_COLS,
            "bytes": bytes_b.len(),
            "packing": "row-major, LSB-first",
            "derivation": "rot90(code2[::-1,::-1]): map_b[r][c] = code2[2-c][r]",
            "property": "all 501 cyclic 3×3 windows pairwise distinct (paper's (167,3;3,3)_2 sub-perfect)",
        },
        "master_pattern": {
            "rows": MAP_A_ROWS * MAP_A_COLS,
            "cols": MAP_A_ROWS * MAP_A_COLS,
            "note": "Master 501×501 PuzzleBoard (arXiv:2409.20127).",
        },
    });
    fs::write(
        &path_meta,
        serde_json::to_string_pretty(&meta).expect("serialize metadata"),
    )
    .expect("write map_metadata.json");
    println!("wrote {}", path_meta.display());
    println!("done.");
}
