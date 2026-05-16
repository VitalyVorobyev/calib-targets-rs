//! Recovering several disjoint grids from one point cloud.
//!
//! [`detect_regular_grid`] assumes a single lattice and returns the
//! largest one it finds. When a cloud contains two or more separate
//! grids — two boards in one photo, a board split by an occlusion —
//! use [`RegularGridDetector::detect_all`] instead. It peels off one
//! component at a time and returns a [`RegularGridDetection`] per
//! board, each independently cleaned, canonicalised, and sorted.
//!
//! This example places two well-separated lattices of different sizes
//! into one cloud and shows that `detect_all` reports each board with
//! its own `(0, 0)` origin and its own geometry.
//!
//! Run it with:
//!
//! ```text
//! cargo run -p projective-grid --example multi_component
//! ```

use nalgebra::{Point2, Vector2};
use projective_grid::RegularGridDetector;

/// Append a `cols x rows` lattice at `pitch` spacing, offset by
/// `origin`, to `out`.
fn push_grid(out: &mut Vec<Point2<f32>>, origin: Point2<f32>, cols: i32, rows: i32, pitch: f32) {
    for j in 0..rows {
        for i in 0..cols {
            out.push(origin + Vector2::new(i as f32 * pitch, j as f32 * pitch));
        }
    }
}

fn main() {
    // ---- 1. Build a cloud with two disjoint boards --------------------
    // Board A: a 5x4 grid in the top-left of the scene.
    // Board B: a 6x6 grid far to the lower-right, no shared corners and
    // a wide gap so the two never bridge into one component.
    let mut cloud = Vec::new();
    push_grid(&mut cloud, Point2::new(40.0, 40.0), 5, 4, 30.0);
    push_grid(&mut cloud, Point2::new(600.0, 400.0), 6, 6, 28.0);
    println!(
        "input cloud: {} points = 5x4 board + 6x6 board\n",
        cloud.len()
    );

    // ---- 2. detect_all peels one component per board ------------------
    // Unlike `detect`, `detect_all` has no single failure mode — an
    // empty `Vec` means nothing was found.
    let detector = RegularGridDetector::default();
    let boards = detector.detect_all(&cloud);

    if boards.is_empty() {
        eprintln!("no grids detected");
        return;
    }
    println!("detected {} disjoint grid(s)\n", boards.len());

    // ---- 3. Each board is its own RegularGridDetection ----------------
    for (k, board) in boards.iter().enumerate() {
        // Grid extent: labels are rebased per board, so each starts at
        // (0, 0). The max label gives the recovered (cols-1, rows-1).
        let max_i = board.points.iter().map(|p| p.grid.0).max().unwrap_or(0);
        let max_j = board.points.iter().map(|p| p.grid.1).max().unwrap_or(0);

        println!(
            "board #{k}: {} corners, extent {}x{} (i:0..={max_i}, j:0..={max_j})",
            board.points.len(),
            max_i + 1,
            max_j + 1,
        );
        println!("  cell_size : {:.2} px", board.cell_size);
        println!(
            "  axis_i    : ({:+.3}, {:+.3})",
            board.axis_i.x, board.axis_i.y
        );

        // The (0, 0) corner of each board, in original pixel space.
        let origin = board
            .points
            .iter()
            .find(|p| p.grid == (0, 0))
            .map(|p| p.position)
            .unwrap();
        println!("  (0,0) at  : ({:.1}, {:.1})\n", origin.x, origin.y);
    }
}
