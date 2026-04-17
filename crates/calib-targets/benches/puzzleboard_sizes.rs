//! Criterion bench: PuzzleBoard detection across board sizes.
//!
//! Each size is rendered once via `calib-targets-print` and decoded many
//! times inside the timed loop. Two groups are run:
//!
//! - `puzzleboard/full/<rows>x<cols>` — default `PuzzleBoardSearchMode::Full`
//!   scan of all 501² × 8 origins.
//! - `puzzleboard/known_origin/<rows>x<cols>` — the fast path, seeded with
//!   the `master_origin_{row,col}` reported by a prior `Full` decode on the
//!   same image.
//!
//! Sizes that fail detection (e.g. the chessboard detector can't assemble a
//! grid) are skipped with a printed note so the overall run still completes.
//!
//! Run with:
//! ```text
//! cargo bench -p calib-targets --bench puzzleboard_sizes
//! ```

use calib_targets::detect;
use calib_targets::printable::{
    render_target_bundle, PageSize, PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec,
};
use calib_targets::puzzleboard::{PuzzleBoardParams, PuzzleBoardSpec};
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use image::GrayImage;

const SIZES: &[u32] = &[6, 8, 10, 12, 13, 16, 20, 30];

fn render_puzzleboard_gray(rows: u32, cols: u32, px_per_cell: u32) -> GrayImage {
    // Pick a page size that comfortably fits the board at `px_per_cell`. The
    // board itself occupies `cols * square_size_mm` × `rows * square_size_mm`;
    // we add a small margin.
    let square_size_mm: f64 = 12.0;
    let margin_mm: f64 = 5.0;
    let width_mm = cols as f64 * square_size_mm + 2.0 * margin_mm;
    let height_mm = rows as f64 * square_size_mm + 2.0 * margin_mm;
    // px_per_cell / square_size_mm = dpi / 25.4 → dpi = px_per_cell * 25.4 / square_size_mm.
    let png_dpi = ((px_per_cell as f64) * 25.4 / square_size_mm).round() as u32;

    let spec = PuzzleBoardTargetSpec {
        rows,
        cols,
        square_size_mm,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    };
    let mut doc = PrintableTargetDocument::new(TargetSpec::PuzzleBoard(spec));
    doc.page.size = PageSize::Custom {
        width_mm,
        height_mm,
    };
    doc.page.margin_mm = margin_mm;
    doc.render.png_dpi = png_dpi;

    let bundle = render_target_bundle(&doc).expect("render");
    image::load_from_memory(&bundle.png_bytes)
        .expect("decode PNG")
        .to_luma8()
}

fn params_for(rows: u32, cols: u32) -> PuzzleBoardParams {
    let spec = PuzzleBoardSpec::with_origin(rows, cols, 12.0, 0, 0).expect("spec");
    PuzzleBoardParams::for_board(&spec)
}

fn bench_full(c: &mut Criterion) {
    let mut group = c.benchmark_group("puzzleboard/full");
    for &n in SIZES {
        // ~40 px/cell keeps the 30×30 image under 1500 px/side while leaving
        // ChESS corners detectable at the ~12 px/cell lower bound.
        let img = render_puzzleboard_gray(n, n, 40);
        let params = params_for(n, n);
        match detect::detect_puzzleboard(&img, &params) {
            Ok(_) => {
                group.throughput(Throughput::Elements(1));
                group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
                    b.iter(|| {
                        let _ = detect::detect_puzzleboard(&img, &params);
                    });
                });
            }
            Err(e) => eprintln!("[skip] full {n}x{n}: {e}"),
        }
    }
    group.finish();
}

fn bench_known_origin(c: &mut Criterion) {
    let mut group = c.benchmark_group("puzzleboard/known_origin");
    for &n in SIZES {
        let img = render_puzzleboard_gray(n, n, 40);
        let base = params_for(n, n);
        // Seed KnownOrigin from a prior Full decode on the same image so the
        // fast path and the baseline measure the same physical detection.
        let seed = match detect::detect_puzzleboard(&img, &base) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[skip] known_origin {n}x{n}: seed Full failed: {e}");
                continue;
            }
        };
        let mut params = base;
        params.decode.search_mode = seed.as_known_origin(2);
        match detect::detect_puzzleboard(&img, &params) {
            Ok(_) => {
                group.throughput(Throughput::Elements(1));
                group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
                    b.iter(|| {
                        let _ = detect::detect_puzzleboard(&img, &params);
                    });
                });
            }
            Err(e) => eprintln!("[skip] known_origin {n}x{n}: {e}"),
        }
    }
    group.finish();
}

criterion_group!(puzzleboard_detect_size, bench_full, bench_known_origin);
criterion_main!(puzzleboard_detect_size);
