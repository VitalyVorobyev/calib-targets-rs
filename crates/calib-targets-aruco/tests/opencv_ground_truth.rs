//! OpenCV conformance for the built-in dictionaries.
//!
//! Every other bit-level test in this workspace is a *round trip*: the encoder
//! and the decoder share `Dictionary::codes()` and the `idx = by * bits + bx`
//! convention, so an error in the shared convention passes silently. These
//! tests break that circularity by asserting against bit patterns captured
//! independently from OpenCV and frozen here as ASCII art.
//!
//! Regenerate the table with `python tools/gen_opencv_charuco_truth.py`.

use calib_targets_aruco::{resolve_dictionary, rotate_code_u64};

/// OpenCV 4.13.0 ground truth, read off `cv2.aruco.generateImageMarker(id, n +
/// 2, borderBits=1)` and cropped to the inner `n × n` payload.
///
/// `'#'` is a black cell, `'.'` a white one. Rows run top to bottom, columns
/// left to right. The `u64` beside each grid is the same pattern in this
/// crate's packing — row-major, `idx = row * n + col`, bit 0 = top-left,
/// **black = 1** — which is the polarity inverse of OpenCV's own `bytesList`.
const TRUTH: &[(&str, usize, u64, &[&str])] = &[
    ("DICT_4X4_50", 0, 0xb352, &[".#..", "#.#.", "##..", "##.#"]),
    ("DICT_4X4_50", 1, 0xa60f, &["####", "....", ".##.", ".#.#"]),
    ("DICT_4X4_50", 2, 0x4b33, &["##..", "##..", "##.#", "..#."]),
    ("DICT_4X4_50", 3, 0x9d66, &[".##.", ".##.", "#.##", "#..#"]),
    (
        "DICT_5X5_100",
        0,
        0x018564ba,
        &[".#.##", "#.#..", "#..##", ".#.#.", "...##"],
    ),
    (
        "DICT_5X5_100",
        7,
        0x0053af71,
        &["#...#", "##.##", "##.#.", "###..", "#.#.."],
    ),
    (
        "DICT_6X6_250",
        0,
        0x9abe44387,
        &["###...", ".###..", "..#...", "#..###", "##.#.#", ".##..#"],
    ),
    (
        "DICT_APRILTAG_36h11",
        0,
        0x2a29d7a7b,
        &["##.###", "#..#.#", "###.#.", "###..#", ".#...#", ".#.#.."],
    ),
];

/// Pack an ASCII grid into this crate's `u64` convention: row-major,
/// `idx = row * n + col`, bit 0 = top-left, black (`'#'`) = 1.
fn pack(grid: &[&str]) -> u64 {
    let n = grid.len();
    let mut code = 0u64;
    for (row, line) in grid.iter().enumerate() {
        assert_eq!(line.len(), n, "ground-truth grid must be square");
        for (col, ch) in line.chars().enumerate() {
            match ch {
                '#' => code |= 1u64 << (row * n + col),
                '.' => {}
                other => panic!("unexpected ground-truth character {other:?}"),
            }
        }
    }
    code
}

/// Render a packed code back to the ASCII form, so assertion failures show the
/// two markers side by side instead of two opaque hex numbers.
fn unpack(code: u64, n: usize) -> Vec<String> {
    (0..n)
        .map(|row| {
            (0..n)
                .map(|col| {
                    if (code >> (row * n + col)) & 1 == 1 {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect()
}

fn assert_same_marker(what: &str, actual: u64, expected: u64, n: usize) {
    assert_eq!(
        actual,
        expected,
        "{what}\n  expected (OpenCV):\n    {}\n  actual:\n    {}",
        unpack(expected, n).join("\n    "),
        unpack(actual, n).join("\n    "),
    );
}

/// The shipped dictionary data reproduces OpenCV's markers exactly.
///
/// This is the guard on `crates/calib-targets-aruco/data/*_CODES.json`, whose
/// fidelity otherwise rests on `tools/opencvdicts.py` having been run once
/// against a correct OpenCV build.
#[test]
fn dictionary_codes_match_opencv() {
    for &(name, id, expected_hex, grid) in TRUTH {
        let dict = resolve_dictionary(name).unwrap_or_else(|| panic!("missing dictionary {name}"));
        let n = dict.marker_size();
        assert_eq!(
            n,
            grid.len(),
            "{name}: marker_size disagrees with the table"
        );

        // The ASCII art and the frozen hex must agree, so a typo in either one
        // is caught before it can mask a real regression.
        assert_same_marker(
            &format!("{name} id {id}: frozen hex vs frozen grid"),
            expected_hex,
            pack(grid),
            n,
        );

        assert_same_marker(
            &format!("{name} id {id}: shipped dictionary vs OpenCV"),
            dict.codes()[id],
            expected_hex,
            n,
        );
    }
}

/// Rotation 0 is the identity, and rotation 2 is a true 180° turn.
///
/// A generator that emitted `rotate(code, 2)` instead of `code` would produce
/// a board whose marker *sequence* is right but whose every marker is upside
/// down. This pins both ends of that failure mode.
#[test]
fn rotation_zero_is_identity_and_two_is_a_half_turn() {
    for &(name, id, expected_hex, grid) in TRUTH {
        let dict = resolve_dictionary(name).unwrap_or_else(|| panic!("missing dictionary {name}"));
        let n = dict.marker_size();

        assert_same_marker(
            &format!("{name} id {id}: rotation 0 must be the identity"),
            rotate_code_u64(expected_hex, n, 0),
            expected_hex,
            n,
        );

        // A 180° turn of a row-major raster is the reversal of its cell
        // sequence, which we build here straight from the ASCII table rather
        // than from `rotate_code_u64`, so the two derivations are independent.
        let flipped: Vec<String> = grid
            .iter()
            .rev()
            .map(|line| line.chars().rev().collect())
            .collect();
        let flipped_refs: Vec<&str> = flipped.iter().map(String::as_str).collect();
        assert_same_marker(
            &format!("{name} id {id}: rotation 2 must be a half turn"),
            rotate_code_u64(expected_hex, n, 2),
            pack(&flipped_refs),
            n,
        );

        // Non-trivial markers must actually move, otherwise the assertions
        // above would hold vacuously for a symmetric pattern.
        if expected_hex != rotate_code_u64(expected_hex, n, 2) {
            assert_ne!(
                rotate_code_u64(expected_hex, n, 1),
                expected_hex,
                "{name} id {id}: rotation 1 unexpectedly a no-op"
            );
        }
    }
}

/// `rotate_code_u64(_, _, 1)` is a 90° **clockwise** turn in image coordinates
/// (x right, y down): `dest(x, y) = src(y, n - 1 - x)`.
///
/// This is pinned because `MarkerDetection::rotation` is public API. Note the
/// index runs opposite to the order OpenCV stores rotations in its `bytesList`
/// (`ours(r) == opencv((4 - r) % 4)`): rotations 0 and 2 agree, 1 and 3 swap.
/// Nothing in this workspace reads OpenCV's index, but interop code must not
/// assume the two are interchangeable.
#[test]
fn rotation_one_is_ninety_degrees_clockwise() {
    for &(name, _id, code, grid) in TRUTH {
        let n = grid.len();
        let cell = |x: usize, y: usize| grid[y].as_bytes()[x] == b'#';

        let mut expected = 0u64;
        for y in 0..n {
            for x in 0..n {
                if cell(y, n - 1 - x) {
                    expected |= 1u64 << (y * n + x);
                }
            }
        }

        assert_same_marker(
            &format!("{name}: rotation 1 must be 90° clockwise"),
            rotate_code_u64(code, n, 1),
            expected,
            n,
        );
    }
}

/// Four quarter turns return the original marker, for real dictionary codes
/// rather than the synthetic constant used in the unit tests.
#[test]
fn four_quarter_turns_are_the_identity() {
    for &(name, id, code, _grid) in TRUTH {
        let dict = resolve_dictionary(name).unwrap_or_else(|| panic!("missing dictionary {name}"));
        let n = dict.marker_size();
        let round = (0..4).fold(code, |acc, _| rotate_code_u64(acc, n, 1));
        assert_same_marker(
            &format!("{name} id {id}: four quarter turns"),
            round,
            code,
            n,
        );
    }
}
