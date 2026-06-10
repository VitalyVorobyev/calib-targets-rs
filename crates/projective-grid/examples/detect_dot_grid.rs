//! Orientation-free detection on a synthetic dot grid.
//!
//! Run with:
//!
//! ```text
//! cargo run -p projective-grid --example detect_dot_grid
//! ```
//!
//! This is the `Evidence::Positions` path: we have only point positions (a dot
//! / circle grid has no per-corner orientation), so `projective-grid`
//! synthesizes each point's two local grid directions from neighbour geometry
//! and then runs the chosen square strategy. We generate the points by
//! projecting a perfect lattice through a homography with a real perspective
//! term, so the recovered grid directions are genuinely non-orthogonal.

use nalgebra::{Matrix3, Point2, Vector3};
use projective_grid::{
    detect_grid, DetectionParams, DetectionRequest, Evidence, LatticeKind, PointFeature,
    SquareAlgorithm,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // A homography with a perspective term: the lattice converges toward two
    // vanishing points, so the projected cell pitch and inter-axis angle vary
    // across the image — exactly the regime orientation-free detection targets.
    let h = Matrix3::new(
        1.0, 0.18, 0.0, //
        0.0, 1.0, 0.0, //
        0.0011, 0.0007, 1.0,
    );
    let project = |gx: f32, gy: f32| -> Point2<f32> {
        let v = h * Vector3::new(gx, gy, 1.0);
        Point2::new(v.x / v.z, v.y / v.z)
    };

    // Project an 8x8 dot grid (pitch 28 px, origin (50, 50)). We keep the true
    // (i, j) label alongside only to print a comparison at the end; the
    // detector never sees it.
    let (rows, cols, pitch, origin) = (8, 8, 28.0_f32, 50.0_f32);
    let mut features: Vec<PointFeature> = Vec::new();
    let mut truth: Vec<(i32, i32)> = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            let p = project(i as f32 * pitch + origin, j as f32 * pitch + origin);
            features.push(PointFeature::new(features.len(), p));
            truth.push((i, j));
        }
    }

    // Orientation-free request. The topological strategy is the recommended
    // orientation-free path: it recovers a dense quad mesh from positions alone.
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Positions(&features),
        None, // grid dimensions unknown
        DetectionParams::default().with_algorithm(SquareAlgorithm::Topological),
    );

    let solution = detect_grid(request)?;

    println!(
        "input dots: {}   labelled: {}   fit max residual: {:.4} px",
        features.len(),
        solution.grid.entries.len(),
        solution
            .fit
            .as_ref()
            .map(|f| f.residuals.max_px)
            .unwrap_or(f32::NAN),
    );
    println!();
    println!("detected (i, j)  <-  feature  (true (i, j))");
    let mut entries = solution.grid.entries.clone();
    entries.sort_by_key(|e| (e.coord.v, e.coord.u));
    for e in &entries {
        let (ti, tj) = truth[e.source_index];
        println!(
            "  ({:>2}, {:>2})      <-  #{:<3}   (true ({:>2}, {:>2}))",
            e.coord.u, e.coord.v, e.source_index, ti, tj
        );
    }

    Ok(())
}
