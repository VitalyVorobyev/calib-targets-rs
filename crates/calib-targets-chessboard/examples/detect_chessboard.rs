//! Onboarding example: detect a chessboard from a corner cloud.
//!
//! `calib-targets-chessboard` is **image-free** — it takes a slice of
//! ChESS X-junction [`ChessCorner`]s and returns an integer-labelled
//! `(i, j)` grid. (The image -> corner step is the `chess-corners`
//! crate's job; the facade crate `calib-targets` wires the two
//! together in its own `detect_chessboard` example.)
//!
//! To keep this example dependency-free and reproducible, it
//! synthesizes a clean 9x7 chessboard corner cloud in code. Each
//! corner carries the two orthogonal axis estimates ChESS would
//! produce, with the **axis-slot parity** that adjacent chessboard
//! corners exhibit: neighbours have opposite `axes[0]`/`axes[1]`
//! orderings.
//!
//! Run it with:
//!
//! ```text
//! cargo run -p calib-targets-chessboard --example detect_chessboard
//! ```

use calib_targets_chessboard::{ChessCorner, Detector, DetectorParams};
use calib_targets_core::AxisEstimate;
use nalgebra::Point2;
use std::f32::consts::FRAC_PI_2;

/// Build a clean `cols x rows` chessboard corner cloud at `pitch` px
/// spacing. Adjacent corners get opposite axis-slot orderings — the
/// parity invariant the detector relies on.
fn synth_chessboard(cols: i32, rows: i32, pitch: f32) -> Vec<ChessCorner> {
    let mut corners = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            // Parity: at "swapped" corners axes[0] is the vertical
            // direction; at "canonical" corners it is the horizontal.
            let swapped = (i + j) % 2 == 1;
            let (a0, a1) = if swapped {
                (FRAC_PI_2, 0.0)
            } else {
                (0.0, FRAC_PI_2)
            };
            corners.push(ChessCorner {
                position: Point2::new(i as f32 * pitch + 80.0, j as f32 * pitch + 60.0),
                axes: [
                    AxisEstimate {
                        angle: a0,
                        sigma: 0.01,
                    },
                    AxisEstimate {
                        angle: a1,
                        sigma: 0.01,
                    },
                ],
                contrast: 10.0,
                fit_rms: 1.0,
                strength: 1.0,
            });
        }
    }
    corners
}

fn main() {
    // ---- 1. Synthesize the corner cloud -------------------------------
    // A 9-wide, 7-tall inner-corner grid = 63 corners.
    let (cols, rows) = (9, 7);
    let corners = synth_chessboard(cols, rows, 24.0);
    println!(
        "input: {} ChESS corners ({cols}x{rows} grid)\n",
        corners.len()
    );

    // ---- 2. Run the detector ------------------------------------------
    // `Detector::detect` returns `Option<ChessboardDetection>` — `None`
    // means no board passed the precision-by-construction invariant stack.
    let detector = Detector::new(DetectorParams::default()).expect("valid detector params");
    let Some(detection) = detector.detect(&corners) else {
        eprintln!("no chessboard detected");
        return;
    };

    // ---- 3. Inspect the labelled grid ---------------------------------
    let labelled = &detection.corners;
    println!("detected a chessboard");
    println!("  labelled corners : {}", labelled.len());

    // Each `ChessboardCorner` carries a non-optional `grid: Coord`,
    // the pixel `position`, an `input_index` back-reference into the
    // input `corners` slice, and a `score`. Print the first few
    // `(u, v) -> pixel` rows.
    println!("\nfirst labelled corners ((u, v) -> pixel  [input_index]):");
    for lc in labelled.iter().take(8) {
        println!(
            "  (u={:>2}, v={:>2})  ->  ({:7.2}, {:7.2})  [{}]",
            lc.grid.u, lc.grid.v, lc.position.x, lc.position.y, lc.input_index
        );
    }
    if labelled.len() > 8 {
        println!("  ... and {} more", labelled.len() - 8);
    }
}
