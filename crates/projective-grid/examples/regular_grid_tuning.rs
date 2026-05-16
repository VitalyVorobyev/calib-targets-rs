//! Tuning the regular-grid detector with [`RegularGridParams`].
//!
//! The zero-config [`detect_regular_grid`] is fine for clean clouds.
//! When you need to control the pipeline, build a [`RegularGridDetector`]
//! from a [`RegularGridParams`]. This example teaches three knobs:
//!
//! - `prune_disconnected` — a precision backstop that drops any
//!   labelled cell not 4-connected to the main component.
//! - `canonicalize_top_left` — rotate/reflect the labels so `(0, 0)`
//!   sits at the visual top-left (`+i` right, `+j` down).
//! - [`ExtensionStrategy`] — `Disabled` vs `Global` vs `Local`
//!   boundary extension after BFS grow.
//!
//! Every variant prints its [`RegularGridStats`] so the difference is
//! visible side by side.
//!
//! Run it with:
//!
//! ```text
//! cargo run -p projective-grid --example regular_grid_tuning
//! ```

use nalgebra::Point2;
// `ExtensionStrategy` is re-exported at the crate root; the param
// structs it wraps live under `square::extension`.
use projective_grid::square::detect::SquareGridParams;
use projective_grid::square::extension::{ExtensionParams, LocalExtensionParams};
use projective_grid::{ExtensionStrategy, RegularGridDetector, RegularGridParams};

/// A clean `cols x rows` axis-aligned lattice at `pitch` px spacing,
/// translated by `(ox, oy)`.
fn clean_grid(cols: i32, rows: i32, pitch: f32, ox: f32, oy: f32) -> Vec<Point2<f32>> {
    let mut points = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            points.push(Point2::new(i as f32 * pitch + ox, j as f32 * pitch + oy));
        }
    }
    points
}

/// Print the labelled count and the full stats block for one run.
fn report(label: &str, detector: &RegularGridDetector, points: &[Point2<f32>]) {
    match detector.detect(points) {
        Ok(g) => {
            let s = &g.stats;
            println!(
                "{label:<34} labelled={:<3} before_prune={:<3} pruned={:<2} \
                 dropped_validation={:<2} canonicalized={}",
                g.points.len(),
                s.labelled_before_prune,
                s.pruned_disconnected,
                s.dropped_by_validation,
                s.canonicalized,
            );
        }
        Err(e) => println!("{label:<34} ERROR: {e}"),
    }
}

fn main() {
    let pitch = 32.0;

    // ---- A. Connectivity pruning --------------------------------------
    // `prune_disconnected` is a precision backstop. The detector reports
    // the single largest grid; a few spurious points scattered far from
    // any lattice node are simply never labelled by BFS grow, so they
    // never reach the output regardless of this toggle. The toggle
    // matters in the rarer case where boundary extension bridges a
    // stray cell into the labelled set — pruning then removes it.
    //
    // Here we confirm the spurious indices are absent from the labelled
    // map either way: a clean 6x5 grid (indices 0..30) plus three far
    // outliers (indices 30, 31, 32).
    let mut noisy = clean_grid(6, 5, pitch, 60.0, 60.0);
    noisy.push(Point2::new(7.0, 9.0)); // index 30 — above-left
    noisy.push(Point2::new(900.0, 12.0)); // index 31 — far right
    noisy.push(Point2::new(15.0, 800.0)); // index 32 — far below
    println!("== connectivity pruning ({} input points) ==", noisy.len());

    for &prune in &[true, false] {
        let det =
            RegularGridDetector::new(RegularGridParams::default().with_prune_disconnected(prune));
        report(&format!("prune_disconnected = {prune}"), &det, &noisy);
        let g = det.detect(&noisy).expect("clean core grid detects");
        let stray: Vec<usize> = g
            .points
            .iter()
            .map(|p| p.source_index)
            .filter(|&idx| idx >= 30)
            .collect();
        println!("  spurious input indices in output: {stray:?} (expect [])");
    }

    // ---- B. Top-left canonicalisation ---------------------------------
    // `canonicalize_top_left` rotates/reflects the labels so `(0, 0)`
    // lands at the visual top-left and `+i`/`+j` point right/down. We
    // feed a grid whose rows are laid out *bottom-to-top, right-to-left*
    // so BFS-grow produces a non-top-left origin; canonicalisation then
    // visibly moves `(0, 0)` and flips the axes.
    println!("\n== top-left canonicalisation ==");
    let mut flipped = Vec::new();
    for j in 0..5 {
        for i in 0..6 {
            // +i steps left (-x), +j steps up (-y) in pixel space.
            flipped.push(Point2::new(
                300.0 - i as f32 * pitch,
                220.0 - j as f32 * pitch,
            ));
        }
    }
    for &canon in &[true, false] {
        let det = RegularGridDetector::new(
            RegularGridParams::default().with_canonicalize_top_left(canon),
        );
        let g = det.detect(&flipped).expect("flipped grid detects");
        let origin = g
            .points
            .iter()
            .find(|p| p.grid == (0, 0))
            .map(|p| p.position)
            .unwrap();
        println!(
            "canonicalize_top_left = {canon:<5}  ->  (0,0) at ({:5.1}, {:5.1})  \
             axis_i=({:+.2}, {:+.2})  axis_j=({:+.2}, {:+.2})",
            origin.x, origin.y, g.axis_i.x, g.axis_i.y, g.axis_j.x, g.axis_j.y,
        );
    }
    println!("  (canonicalized: (0,0) sits top-left and axis_i/axis_j point right/down)");

    // ---- C. Extension strategy ----------------------------------------
    // The boundary-extension stage runs after BFS grow. On a clean,
    // fully-connected grid all three strategies recover every corner —
    // the point here is the API, not a recall delta. `Disabled` is the
    // BFS result verbatim; `Global` fits one homography over the whole
    // labelled set; `Local` fits a per-candidate homography from the
    // nearest labelled corners (more robust under heavy distortion).
    println!("\n== extension strategy ==");
    let clean = clean_grid(6, 5, pitch, 60.0, 60.0);
    let strategies = [
        ("Disabled", ExtensionStrategy::Disabled),
        (
            "Global(default)",
            ExtensionStrategy::Global(ExtensionParams::default()),
        ),
        (
            "Local(default)",
            ExtensionStrategy::Local(LocalExtensionParams::default()),
        ),
    ];
    for (name, strategy) in strategies {
        // `RegularGridParams::new` takes the core pipeline knobs; the
        // extension strategy lives on `SquareGridParams::extension` and
        // is also reachable builder-style via `with_extension`.
        let params = RegularGridParams::new(SquareGridParams::default()).with_extension(strategy);
        let det = RegularGridDetector::new(params);
        report(&format!("extension = {name}"), &det, &clean);
    }
}
