//! Detect a PuzzlePole and print the 2D-3D correspondences it yields.
//!
//! A PuzzlePole is the PuzzleBoard pattern wrapped round a cylinder, so it is
//! identifiable from any direction. Unlike every planar target here, each
//! detected corner carries a **3-D** object point — which is what a PnP solver
//! needs, and what this library exists to hand you.
//!
//! ```bash
//! cargo run -p calib-targets --example detect_puzzlepole -- pole.png
//! ```

use calib_targets::detect;
use calib_targets::puzzleboard::{PuzzlePoleParams, PuzzlePoleSpec};
use image::ImageReader;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "tracing")]
    init_tracing_subscriber();

    let Some(path) = std::env::args().nth(1) else {
        eprintln!("Usage: detect_puzzlepole <image_path>");
        return Ok(());
    };
    let img = ImageReader::open(path)?.decode()?.to_luma8();

    // 24 pieces around, 12 along, 20 mm pieces -> a 153 mm cylinder. The
    // circumference must be a supported period; the diameter follows from it.
    let spec = PuzzlePoleSpec::new(24, 12, 20.0)?;
    println!(
        "pole: {:.1} mm across, {:.0} mm tall",
        spec.diameter_mm(),
        spec.axial_extent_mm()
    );

    let configs = PuzzlePoleParams::sweep_for_pole(&spec);
    match detect::detect_puzzlepole_best(&img, &configs) {
        Ok(found) => {
            println!(
                "decoded {} corners over {} configs (mean confidence {:.3})",
                found.corners.len(),
                configs.len(),
                found.decode.mean_confidence
            );
            for (image, object) in found.correspondences().take(5) {
                println!(
                    "  ({:7.2}, {:7.2}) px  ->  ({:7.2}, {:7.2}, {:7.2}) mm",
                    image.x, image.y, object.x, object.y, object.z
                );
            }
            if found.corners.len() > 5 {
                println!("  ... and {} more", found.corners.len() - 5);
            }
            println!("Feed these to a PnP solver for the pole's pose.");
        }
        Err(err) => println!("no pole detected: {err}"),
    }
    Ok(())
}

#[cfg(feature = "tracing")]
fn init_tracing_subscriber() {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_target(false))
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .try_init();
}
