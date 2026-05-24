//! Zero-config detection on a synthetic 7×7 axis-aligned grid.
//!
//! Demonstrates the headline path through the new task facade:
//! build observations, hand them to `detect_square_grid` with the open
//! context, and inspect the labelled `(i, j)` map plus the per-image cell
//! size estimate.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example detect_regular_square -p projective-grid-next
//! ```

use nalgebra::Point2;

use projective_grid_next::grow::OpenContext;
use projective_grid_next::{detect_square_grid, DetectParams, LabelPolicy, NoOpSink, Observation};

fn main() {
    let cell: f32 = 25.0;
    let origin: f32 = 60.0;
    let rows: i32 = 7;
    let cols: i32 = 7;

    let mut obs: Vec<Observation<f32>> = Vec::with_capacity((rows * cols) as usize);
    for j in 0..rows {
        for i in 0..cols {
            let x = i as f32 * cell + origin;
            let y = j as f32 * cell + origin;
            obs.push(Observation::new(Point2::new(x, y)));
        }
    }

    let ctx = OpenContext::<f32>::new(obs.len());
    let policy = LabelPolicy::<f32>::builder(obs.len()).build();
    let params = DetectParams::default();
    let mut sink = NoOpSink;

    let detection = detect_square_grid(&obs, &policy, &ctx, &params, &mut sink)
        .expect("clean 7x7 grid must detect");

    println!(
        "detected {n} labelled corners on a {rows}x{cols} grid; bbox {bbox:?}; cell size {cs:.2} px",
        n = detection.labelled.len(),
        bbox = detection.bbox,
        cs = detection.cell_size,
    );

    // Print a handful of labelled entries — the top-left 3x3 — so the user
    // can sanity-check the labelling.
    let mut shown = 0;
    for j in 0..3_i32 {
        for i in 0..3_i32 {
            if let Some(&idx) = detection.labelled.get(&(i, j)) {
                let p = obs[idx].position;
                println!("  ({i}, {j}) -> obs {idx} @ ({:.1}, {:.1})", p.x, p.y);
                shown += 1;
            }
        }
    }
    println!(
        "({shown} sample entries shown of {} total)",
        detection.labelled.len()
    );
}
