//! Orientation-free detection on a synthetic hexagonal dot grid.
//!
//! Run with:
//!
//! ```text
//! cargo run -p projective-grid --example detect_hex_positions
//! ```
//!
//! This is the hex `Evidence::Positions` path: we have only point positions (a
//! hex dot grid has no per-corner orientation), so `projective-grid` synthesizes
//! each node's three local grid directions from neighbour geometry
//! ([`projective_grid::synthesize_oriented3`]) and then runs the hex topological
//! grid finder. Hex is **topological-only** (no seed-and-grow, no recovery
//! schedule), so the algorithm selector must be
//! [`SquareAlgorithm::Topological`].
//!
//! We generate the nodes by projecting a perfect axial lattice through a
//! homography with a real perspective term, so the three recovered grid
//! directions are genuinely not 60° apart.

use nalgebra::{Matrix3, Point2, Vector3};
use projective_grid::{
    detect_grid, DetectionParams, DetectionRequest, Evidence, LatticeKind, PointFeature,
    SquareAlgorithm,
};

/// Axial hex node `(q, r)` model position with unit nearest-neighbour spacing.
fn hex_model(q: i32, r: i32) -> Point2<f32> {
    let sqrt3_2 = 3.0_f32.sqrt() * 0.5;
    Point2::new(q as f32 + 0.5 * r as f32, sqrt3_2 * r as f32)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A homography with a perspective term: the lattice converges toward
    // vanishing points, so the projected node pitch and inter-axis angles vary
    // across the image — exactly the regime orientation-free detection targets.
    let h = Matrix3::new(
        1.0, 0.10, 0.0, //
        0.03, 1.0, 0.0, //
        0.0006, 0.0004, 1.0,
    );
    let project = |m: Point2<f32>| -> Point2<f32> {
        let v = h * Vector3::new(m.x, m.y, 1.0);
        Point2::new(v.x / v.z, v.y / v.z)
    };

    // Project a radius-4 hex patch (node pitch 28 px, origin (200, 200)). We
    // keep the true (q, r) label alongside only to print a comparison at the
    // end; the detector never sees it.
    let (radius, pitch, origin) = (4_i32, 28.0_f32, 200.0_f32);
    let mut features: Vec<PointFeature> = Vec::new();
    let mut truth: Vec<(i32, i32)> = Vec::new();
    for q in -radius..=radius {
        for r in (-radius).max(-q - radius)..=radius.min(-q + radius) {
            let m = hex_model(q, r);
            let p = project(Point2::new(m.x * pitch + origin, m.y * pitch + origin));
            features.push(PointFeature::new(features.len(), p));
            truth.push((q, r));
        }
    }

    // Orientation-free hex request. Hex detection is topological-only.
    let request = DetectionRequest::new(
        LatticeKind::Hex,
        Evidence::Positions(&features),
        None, // grid dimensions unknown
        DetectionParams::default().with_algorithm(SquareAlgorithm::Topological),
    );

    let solution = detect_grid(request)?;

    println!(
        "input nodes: {}   labelled: {}   fit max residual: {:.4} px",
        features.len(),
        solution.grid.entries.len(),
        solution
            .fit
            .as_ref()
            .map(|f| f.residuals.max_px)
            .unwrap_or(f32::NAN),
    );
    println!();
    println!("detected (q, r)  <-  feature  (true (q, r))");
    let mut entries = solution.grid.entries.clone();
    entries.sort_by_key(|e| (e.coord.v, e.coord.u));
    for e in &entries {
        let (tq, tr) = truth[e.source_index];
        println!(
            "  ({:>2}, {:>2})      <-  #{:<3}   (true ({:>2}, {:>2}))",
            e.coord.u, e.coord.v, e.source_index, tq, tr
        );
    }

    Ok(())
}
