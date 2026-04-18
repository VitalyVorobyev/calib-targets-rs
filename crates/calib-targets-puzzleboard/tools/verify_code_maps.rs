//! Sanity checker for the committed PuzzleBoard code-map blobs.
//!
//! Reads `src/data/map_a.bin` and `src/data/map_b.bin` and asserts that
//! every 3×3 cyclic window of each map is unique. Useful in CI to guard
//! against accidental corruption of the embedded blobs.

use calib_targets_puzzleboard::code_maps;

fn main() {
    let a = code_maps::map_a();
    let b = code_maps::map_b();

    assert_eq!(a.rows(), 3);
    assert_eq!(a.cols(), code_maps::EDGE_MAP_A_COLS);
    assert_eq!(b.rows(), code_maps::EDGE_MAP_B_ROWS);
    assert_eq!(b.cols(), 3);

    code_maps::verify_cyclic_window_unique(a, 3, 3)
        .expect("map A cyclic 3×3 windows must be pairwise unique");
    code_maps::verify_cyclic_window_unique(b, 3, 3)
        .expect("map B cyclic 3×3 windows must be pairwise unique");
    println!("OK — both code maps satisfy (3, 167; 3, 3)_2 sub-perfect uniqueness.");
}
