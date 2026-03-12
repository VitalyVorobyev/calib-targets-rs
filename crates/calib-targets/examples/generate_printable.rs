use calib_targets::printable::{write_target_bundle, PrintableTargetDocument};
use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let spec_path = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("testdata/printable/charuco_a4.json"));
    let out_stem = env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("tmpdata/printable/charuco_a4"));

    let doc = PrintableTargetDocument::load_json(&spec_path)?;
    let written = write_target_bundle(&doc, &out_stem)?;
    println!("{}", written.json_path.display());
    println!("{}", written.svg_path.display());
    println!("{}", written.png_path.display());
    Ok(())
}
