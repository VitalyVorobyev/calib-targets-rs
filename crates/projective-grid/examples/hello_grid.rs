//! Minimal, image-free quickstart for `projective-grid`.
//!
//! This example recovers a labelled square grid from a handful of 2D feature
//! points and their two local axis directions — no image, no pixels, no other
//! workspace crate. It is the runnable twin of the README "Quickstart".
//!
//! Run it with:
//!
//! ```text
//! cargo run -p projective-grid --example hello_grid
//! ```
//!
//! The pipeline is: build oriented features -> wrap them as `Evidence` ->
//! call `detect_grid` -> read the recovered `(i, j)` labels off the solution.

use nalgebra::Point2;
use projective_grid::{
    detect_grid, DetectionParams, DetectionRequest, Evidence, LatticeKind, LocalAxis,
    OrientedFeature, PointFeature,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // --- Step 1: build the input features ------------------------------------
    //
    // We synthesize a small 3x3 grid of corner-like features. In a real
    // program these positions and axes would come from your own corner /
    // feature detector — this crate never touches an image, it only consumes
    // the points you bring.
    //
    // To show that detection handles perspective (not just a perfectly
    // axis-aligned lattice), we add a gentle shear that grows with the row.
    // The shear is small (a few degrees), so each feature's two local axes
    // stay close to horizontal (0 rad) and vertical (pi/2 rad).
    let mut features: Vec<OrientedFeature<2>> = Vec::new();
    let spacing = 40.0_f32;
    for j in 0..3 {
        for i in 0..3 {
            // Image-frame position (origin top-left, x right, y down).
            // The `+ j * 6.0` term tilts each successive row, giving the grid
            // a mild projective skew.
            let x = 60.0 + i as f32 * spacing + j as f32 * 6.0;
            let y = 50.0 + j as f32 * spacing;

            // `source_index` is a stable, caller-owned handle. The solution
            // reports it back so you can map a recovered `(i, j)` label to the
            // exact input feature it came from.
            let source_index = features.len();
            let point = PointFeature::new(source_index, Point2::new(x, y));

            // Two local lattice directions per feature. `Oriented2` evidence
            // expects two roughly-orthogonal axes; we pass horizontal and
            // vertical with a small angular uncertainty (sigma, in radians).
            let axes = [
                LocalAxis::new(0.0, Some(0.02)),
                LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(0.02)),
            ];
            features.push(OrientedFeature::new(point, axes));
        }
    }

    // --- Step 2: wrap the features as Evidence and build a request -----------
    //
    // `Evidence::Oriented2` is the shape the square detector consumes. We ask
    // for a `Square` lattice, leave the grid dimensions unknown (`None`), and
    // use the default topological assembler.
    let params = DetectionParams::default();
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None, // grid dimensions unknown; the detector infers extent
        params,
    );

    // --- Step 3: detect the grid ---------------------------------------------
    //
    // `detect_grid` returns the largest recovered component. (Use
    // `detect_grid_all` when secondary components must be kept too.)
    let solution = detect_grid(request)?;

    // --- Step 4: read the recovered labels -----------------------------------
    //
    // Each `GridEntry` carries its lattice coordinate (`coord.u`, `coord.v` —
    // i.e. `i`, `j`), the source index back into our input slice, the image
    // position, and (because a fit was computed) the reprojection residual.
    //
    // Labels are rebased so the labelled bounding box starts at (0, 0).
    println!(
        "recovered {} labelled features:",
        solution.grid.entries.len()
    );
    for entry in &solution.grid.entries {
        let residual = entry
            .residual_px
            .map(|r| format!("{r:.3} px"))
            .unwrap_or_else(|| "n/a".to_string());
        println!(
            "  (i={:>2}, j={:>2})  src={:>2}  pos=({:6.1}, {:6.1})  residual={}",
            entry.coord.u,
            entry.coord.v,
            entry.source_index,
            entry.image_position.x,
            entry.image_position.y,
            residual,
        );
    }

    // The fitted projective transform and its residual summary live on
    // `solution.fit`; features the detector could not place land in
    // `solution.rejected`.
    if let Some(fit) = &solution.fit {
        println!(
            "fit: max residual {:.3} px over {} features",
            fit.residuals.max_px, fit.residuals.count,
        );
    }

    Ok(())
}
