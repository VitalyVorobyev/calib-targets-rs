//! The generated ChArUco board reproduces OpenCV's layout exactly.
//!
//! Everything else that checks the generator is self-referential: the golden
//! DXF was bootstrapped from our own output, and the round-trip tests decode
//! with a detector that shares the generator's bit convention. Both would pass
//! happily if the whole workspace agreed on a *wrong* convention.
//!
//! These tests compare the rendered artwork against layout facts captured from
//! OpenCV and frozen below, and they do it through all three output formats,
//! since SVG, PNG and DXF are three separate emitters over one scene.
//!
//! Regenerate the frozen tables with `python tools/gen_opencv_charuco_truth.py`.

use calib_targets_aruco::resolve_dictionary;
use calib_targets_print::{
    render_target_bundle, CharucoTargetSpec, PageSize, PageSpec, PrintableTargetDocument,
    RenderOptions, TargetSpec,
};

mod sample;

use sample::{Ink, Sampler};

/// Inner marker payloads as OpenCV renders them, `'#'` black and `'.'` white,
/// rows top-to-bottom. Captured from `cv2.aruco.generateImageMarker` 4.13.0.
const DICT_4X4_50_MARKERS: &[&[&str]] = &[
    &[".#..", "#.#.", "##..", "##.#"],
    &["####", "....", ".##.", ".#.#"],
    &["##..", "##..", "##.#", "..#."],
    &[".##.", ".##.", "#.##", "#..#"],
    &["#.#.", "#.##", ".##.", "...#"],
    &["#...", ".##.", "..##", "..#."],
    &[".##.", "...#", "##.#", "...#"],
    &["..##", "#.##", "....", "##.#"],
    &["....", "...#", "..#.", ".#.#"],
    &["..##", "....", "#.#.", "#..#"],
];

/// The board OpenCV builds for `CharucoBoard((5, 4), 20, 12, DICT_4X4_50)`:
/// which square each marker id occupies, as `(column, row)`.
///
/// Squares are black where `(col + row)` is even — so the **top-left square is
/// black** and markers sit on the white squares. This is OpenCV's *modern*
/// (non-legacy, >= 4.6) pattern; `setLegacyPattern(true)` would put a white
/// square and marker 0 at `(0, 0)` instead.
const MARKER_CELLS_5X4: &[(u32, u32)] = &[
    (1, 0),
    (3, 0),
    (0, 1),
    (2, 1),
    (4, 1),
    (1, 2),
    (3, 2),
    (0, 3),
    (2, 3),
    (4, 3),
];

const COLS: u32 = 5;
const ROWS: u32 = 4;
const SQUARE_MM: f64 = 20.0;
const MARKER_REL: f64 = 0.6;
const BORDER_BITS: usize = 1;

fn document(cols: u32, rows: u32, dictionary: &str, border_bits: usize) -> PrintableTargetDocument {
    let dict =
        resolve_dictionary(dictionary).unwrap_or_else(|| panic!("missing dictionary {dictionary}"));
    let spec = CharucoTargetSpec::new(rows, cols, SQUARE_MM, MARKER_REL, dict)
        .with_border_bits(border_bits);
    PrintableTargetDocument::new(TargetSpec::Charuco(spec))
        .with_page(
            PageSpec::default()
                .with_size(PageSize::Custom {
                    width_mm: cols as f64 * SQUARE_MM,
                    height_mm: rows as f64 * SQUARE_MM,
                })
                .with_margin_mm(0.0),
        )
        .with_render(RenderOptions::default().with_png_dpi(300))
}

/// Build one sampler per output format, so every assertion runs three times.
fn samplers(doc: &PrintableTargetDocument) -> Vec<(&'static str, Sampler)> {
    let bundle = render_target_bundle(doc).expect("render bundle");
    let layout = doc.resolve_layout().expect("layout");
    vec![
        ("svg", Sampler::from_svg(&bundle.svg_text)),
        (
            "png",
            Sampler::from_png(&bundle.png_bytes, doc.render.png_dpi),
        ),
        (
            "dxf",
            Sampler::from_dxf(&bundle.dxf_text, layout.page_height_mm),
        ),
    ]
}

/// Centre of the `(col, row)` square, in page millimetres.
fn square_centre(doc: &PrintableTargetDocument, col: u32, row: u32) -> (f64, f64) {
    let layout = doc.resolve_layout().expect("layout");
    (
        layout.board_origin_mm[0] + (col as f64 + 0.5) * SQUARE_MM,
        layout.board_origin_mm[1] + (row as f64 + 0.5) * SQUARE_MM,
    )
}

/// A point inside the `(col, row)` square but outside the marker footprint, so
/// the square's own colour can be read regardless of what it carries.
fn square_corner_probe(doc: &PrintableTargetDocument, col: u32, row: u32) -> (f64, f64) {
    let layout = doc.resolve_layout().expect("layout");
    // The marker occupies the centred `MARKER_REL` fraction, leaving a margin
    // of `(1 - MARKER_REL) / 2` on each side; probe the middle of that margin.
    let inset = 0.25 * (1.0 - MARKER_REL) * SQUARE_MM;
    (
        layout.board_origin_mm[0] + col as f64 * SQUARE_MM + inset,
        layout.board_origin_mm[1] + row as f64 * SQUARE_MM + inset,
    )
}

/// Read the `(bits + 2 * border) ^ 2` bin grid of the marker in this square.
fn read_marker(
    sampler: &Sampler,
    doc: &PrintableTargetDocument,
    col: u32,
    row: u32,
    bits: usize,
    border: usize,
) -> Vec<String> {
    let (cx, cy) = square_centre(doc, col, row);
    let side = SQUARE_MM * MARKER_REL;
    let cells = bits + 2 * border;
    let step = side / cells as f64;
    let x0 = cx - 0.5 * side;
    let y0 = cy - 0.5 * side;
    (0..cells)
        .map(|by| {
            (0..cells)
                .map(|bx| {
                    let x = x0 + (bx as f64 + 0.5) * step;
                    let y = y0 + (by as f64 + 0.5) * step;
                    match sampler.ink_at(x, y) {
                        Ink::Black => '#',
                        Ink::White => '.',
                    }
                })
                .collect()
        })
        .collect()
}

/// Wrap an inner payload in its black border ring, matching what a rendered
/// marker looks like when sampled bin by bin.
fn with_border(inner: &[&str], border: usize) -> Vec<String> {
    let cells = inner.len() + 2 * border;
    let mut out = vec!["#".repeat(cells); border];
    for line in inner {
        out.push(format!(
            "{}{}{}",
            "#".repeat(border),
            line,
            "#".repeat(border)
        ));
    }
    out.extend(vec!["#".repeat(cells); border]);
    out
}

fn rotate_180(grid: &[String]) -> Vec<String> {
    grid.iter()
        .rev()
        .map(|line| line.chars().rev().collect())
        .collect()
}

fn rotate_90_cw(grid: &[String]) -> Vec<String> {
    let n = grid.len();
    (0..n)
        .map(|y| {
            (0..n)
                .map(|x| grid[n - 1 - x].as_bytes()[y] as char)
                .collect()
        })
        .collect()
}

/// The top-left square is black — OpenCV's modern ChArUco convention — and
/// carries no marker.
#[test]
fn top_left_square_is_black() {
    let doc = document(COLS, ROWS, "DICT_4X4_50", BORDER_BITS);
    for (fmt, sampler) in samplers(&doc) {
        let (x, y) = square_corner_probe(&doc, 0, 0);
        assert_eq!(
            sampler.ink_at(x, y),
            Ink::Black,
            "{fmt}: top-left square must be black"
        );
        let (cx, cy) = square_centre(&doc, 0, 0);
        assert_eq!(
            sampler.ink_at(cx, cy),
            Ink::Black,
            "{fmt}: top-left square must be solid black (no marker)"
        );
    }
}

/// Squares alternate with `(col + row)` even = black, for every square on the
/// board — the parity that decides where markers may go.
#[test]
fn square_parity_matches_opencv() {
    let doc = document(COLS, ROWS, "DICT_4X4_50", BORDER_BITS);
    for (fmt, sampler) in samplers(&doc) {
        for row in 0..ROWS {
            for col in 0..COLS {
                let (x, y) = square_corner_probe(&doc, col, row);
                let expected = if (col + row) % 2 == 0 {
                    Ink::Black
                } else {
                    Ink::White
                };
                assert_eq!(
                    sampler.ink_at(x, y),
                    expected,
                    "{fmt}: square ({col},{row}) parity"
                );
            }
        }
    }
}

/// Every marker id sits on the square OpenCV puts it on, identified by reading
/// the printed bits rather than by asking the board model where it put them.
#[test]
fn marker_ids_land_on_opencv_cells() {
    let doc = document(COLS, ROWS, "DICT_4X4_50", BORDER_BITS);
    for (fmt, sampler) in samplers(&doc) {
        for (id, &(col, row)) in MARKER_CELLS_5X4.iter().enumerate() {
            let actual = read_marker(&sampler, &doc, col, row, 4, BORDER_BITS);
            let expected = with_border(DICT_4X4_50_MARKERS[id], BORDER_BITS);
            assert_eq!(
                actual,
                expected,
                "{fmt}: square ({col},{row}) should carry marker {id}\n\
                 expected:\n  {}\nactual:\n  {}",
                expected.join("\n  "),
                actual.join("\n  "),
            );
        }
    }
}

/// Markers are drawn unrotated. This is the direct guard for "the sequence is
/// right but every marker is upside down".
#[test]
fn marker_bits_are_unrotated() {
    let doc = document(COLS, ROWS, "DICT_4X4_50", BORDER_BITS);
    for (fmt, sampler) in samplers(&doc) {
        for (id, &(col, row)) in MARKER_CELLS_5X4.iter().enumerate() {
            let actual = read_marker(&sampler, &doc, col, row, 4, BORDER_BITS);
            let upright = with_border(DICT_4X4_50_MARKERS[id], BORDER_BITS);

            let turned = [
                ("90° clockwise", rotate_90_cw(&upright)),
                ("180°", rotate_180(&upright)),
                ("270° clockwise", rotate_90_cw(&rotate_180(&upright))),
            ];
            for (name, grid) in &turned {
                assert_ne!(
                    &actual, grid,
                    "{fmt}: marker {id} at ({col},{row}) is rendered {name}"
                );
            }
            assert_eq!(actual, upright, "{fmt}: marker {id} at ({col},{row})");
        }
    }
}

/// The `border_bits` ring around every marker payload is solid black.
#[test]
fn marker_border_ring_is_black() {
    let doc = document(COLS, ROWS, "DICT_4X4_50", BORDER_BITS);
    for (fmt, sampler) in samplers(&doc) {
        for &(col, row) in MARKER_CELLS_5X4 {
            let grid = read_marker(&sampler, &doc, col, row, 4, BORDER_BITS);
            let cells = grid.len();
            for (by, line) in grid.iter().enumerate() {
                for (bx, ch) in line.chars().enumerate() {
                    let on_ring = bx < BORDER_BITS
                        || by < BORDER_BITS
                        || bx + BORDER_BITS >= cells
                        || by + BORDER_BITS >= cells;
                    if on_ring {
                        assert_eq!(
                            ch, '#',
                            "{fmt}: marker at ({col},{row}) border bin ({bx},{by}) must be black"
                        );
                    }
                }
            }
        }
    }
}

/// A second board with different parity, dimensions and marker size, so the
/// checks above cannot be passing by a coincidence of the 5 x 4 / 4x4 case.
#[test]
fn six_by_five_dict_5x5_matches_opencv() {
    const INNER: &[&[&str]] = &[
        &[".#.##", "#.#..", "#..##", ".#.#.", "...##"],
        &["####.", "..###", "####.", ".#...", "##..#"],
        &["..#.#", "....#", "###..", ".#..#", "...#."],
    ];
    // Marker ids 0..2 occupy the first three white squares in row-major order
    // on a 6-wide board with a black top-left square.
    const CELLS: &[(u32, u32)] = &[(1, 0), (3, 0), (5, 0)];

    let doc = document(6, 5, "DICT_5X5_100", BORDER_BITS);
    for (fmt, sampler) in samplers(&doc) {
        let (x, y) = square_corner_probe(&doc, 0, 0);
        assert_eq!(sampler.ink_at(x, y), Ink::Black, "{fmt}: top-left black");

        for (id, &(col, row)) in CELLS.iter().enumerate() {
            let actual = read_marker(&sampler, &doc, col, row, 5, BORDER_BITS);
            let expected = with_border(INNER[id], BORDER_BITS);
            assert_eq!(
                actual, expected,
                "{fmt}: marker {id} at ({col},{row}) on the 6x5 DICT_5X5_100 board"
            );
        }
    }
}
