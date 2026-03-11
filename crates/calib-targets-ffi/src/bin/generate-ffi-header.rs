use std::env;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process;

fn main() -> Result<(), Box<dyn Error>> {
    let check = matches!(env::args().nth(1).as_deref(), Some("--check"));
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let header_path = header_path(&crate_dir);
    let config = cbindgen::Config::from_file(crate_dir.join("cbindgen.toml"))?;
    let bindings = cbindgen::generate_with_config(&crate_dir, config)?;
    let mut generated = Vec::new();
    bindings.write(&mut generated);
    let generated = String::from_utf8(generated)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    if check {
        let existing = fs::read_to_string(&header_path).map_err(|err| {
            format!(
                "failed to read existing header {}: {err}",
                header_path.display()
            )
        })?;
        if existing != generated {
            eprintln!(
                "header is out of date: run `cargo run -p calib-targets-ffi --bin generate-ffi-header`"
            );
            process::exit(1);
        }
        println!("header is up to date: {}", header_path.display());
        return Ok(());
    }

    if let Some(parent) = header_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&header_path, generated)?;
    println!("wrote {}", header_path.display());
    Ok(())
}

fn header_path(crate_dir: &Path) -> PathBuf {
    crate_dir.join("include").join("calib_targets_ffi.h")
}
