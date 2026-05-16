//! The zero-config story for `projective-grid`.
//!
//! This example teaches the single most important entry point:
//! [`detect_regular_grid`]. You hand it a bare cloud of 2D points and
//! it returns a labelled `(i, j)` grid — no validators, no image, no
//! tuning.
//!
//! It synthesizes a perspective-warped 6x5 lattice in code (so the
//! example needs no image files), runs the detector, and pretty-prints
//! everything the result carries: the labelled `(i, j) -> pixel` map,
//! the inferred `cell_size`, the two grid-axis directions, and the
//! per-stage [`RegularGridStats`].
//!
//! It also shows real `Result` handling: a `match` on every
//! [`RegularGridError`] variant, so a reader sees exactly how the
//! detector reports failure.
//!
//! Run it with:
//!
//! ```text
//! cargo run -p projective-grid --example regular_grid
//! ```

use nalgebra::Point2;
use projective_grid::{detect_regular_grid, RegularGridError};

/// Build a `cols x rows` lattice and push it through a fixed
/// perspective homography, so the synthetic cloud looks like a board
/// photographed at an angle rather than a clean axis-aligned grid.
fn warped_grid(cols: i32, rows: i32) -> Vec<Point2<f32>> {
    // A mild 3x3 perspective homography. The last row's small non-zero
    // terms are what bend straight rows into a foreshortened trapezoid.
    #[rustfmt::skip]
    let h = [
        [42.0_f32, 4.0,     90.0],
        [2.5,      40.0,    70.0],
        [3.0e-4,   1.5e-4,  1.0 ],
    ];

    let mut points = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            let (x, y) = (i as f32, j as f32);
            let w = h[2][0] * x + h[2][1] * y + h[2][2];
            let px = (h[0][0] * x + h[0][1] * y + h[0][2]) / w;
            let py = (h[1][0] * x + h[1][1] * y + h[1][2]) / w;
            points.push(Point2::new(px, py));
        }
    }
    points
}

fn main() {
    // ---- 1. Synthesize the input cloud --------------------------------
    // A 6-wide, 5-tall lattice = 30 corners, perspective-warped.
    let points = warped_grid(6, 5);
    println!("input: {} perspective-warped points\n", points.len());

    // ---- 2. The whole API in one call ---------------------------------
    // `detect_regular_grid` is equivalent to
    // `RegularGridDetector::default().detect(points)`.
    let detection = match detect_regular_grid(&points) {
        Ok(grid) => grid,
        // Every failure mode is an explicit enum variant. Matching them
        // out shows a reader exactly what can go wrong.
        Err(RegularGridError::TooFewPoints { found }) => {
            eprintln!("need at least 4 points for a 2x2 seed, got {found}");
            return;
        }
        Err(RegularGridError::DegeneratePointCloud) => {
            eprintln!("cloud is degenerate (coincident points / no spread)");
            return;
        }
        Err(RegularGridError::NoGridFound) => {
            eprintln!("no square lattice could be seeded from the cloud");
            return;
        }
        // `RegularGridError` is `#[non_exhaustive]`: future releases may
        // add failure modes, so a wildcard arm is required.
        Err(other) => {
            eprintln!("detection failed: {other}");
            return;
        }
    };

    // ---- 3. Inspect the geometry the detector inferred ----------------
    println!("inferred grid geometry");
    println!("  cell_size : {:.2} px", detection.cell_size);
    println!(
        "  axis_i    : ({:+.3}, {:+.3})   (+i direction in pixel space)",
        detection.axis_i.x, detection.axis_i.y
    );
    println!(
        "  axis_j    : ({:+.3}, {:+.3})   (+j direction in pixel space)\n",
        detection.axis_j.x, detection.axis_j.y
    );

    // ---- 4. The labelled grid -----------------------------------------
    // `points` is sorted row-major: top-to-bottom then left-to-right.
    // Each `DetectedGridPoint` carries its rebased `(i, j)` label, the
    // pixel position, and the index back into the input slice.
    println!("labelled corners: {}", detection.points.len());
    for p in &detection.points {
        println!(
            "  (i={:>2}, j={:>2})  ->  ({:7.2}, {:7.2})   [input #{}]",
            p.grid.0, p.grid.1, p.position.x, p.position.y, p.source_index
        );
    }

    // `labelled_map()` rebuilds the `(i, j) -> source_index` lookup if
    // you want random access instead of the sorted vector.
    let map = detection.labelled_map();
    if let Some(&idx) = map.get(&(0, 0)) {
        println!("\norigin (0, 0) is input point #{idx}");
    }

    // ---- 5. Per-stage diagnostics -------------------------------------
    let s = &detection.stats;
    println!("\nstats");
    println!("  input_points         : {}", s.input_points);
    println!("  components_found     : {}", s.components_found);
    println!("  labelled_before_prune: {}", s.labelled_before_prune);
    println!("  pruned_disconnected  : {}", s.pruned_disconnected);
    println!("  dropped_by_validation: {}", s.dropped_by_validation);
    println!("  canonicalized        : {}", s.canonicalized);
}
