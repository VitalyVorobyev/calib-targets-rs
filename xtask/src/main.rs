//! Workspace task runner.
//!
//! Currently exposes `emit-schemas`, which writes the JSON Schemas of every
//! config the `@vitavision/calib-targets` npm (wasm) package accepts to
//! `schemas/`. The schemas ship inside the npm package and drive schema-based
//! config forms.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

mod emit_schemas;
#[cfg(test)]
mod schema_tests;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let cmd = args
        .next()
        .context("usage: cargo xtask <command>\n\ncommands:\n  emit-schemas [--check]\n")?;

    match cmd.as_str() {
        "emit-schemas" => {
            let check = args.any(|a| a == "--check");
            emit_schemas::run(&workspace_root()?, check)
        }
        other => bail!("unknown xtask `{other}`; available: emit-schemas [--check]"),
    }
}

fn workspace_root() -> Result<PathBuf> {
    // CARGO_MANIFEST_DIR points at xtask/, so the workspace root is its parent.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .context("CARGO_MANIFEST_DIR is unset; xtask must be run via `cargo xtask`")?;
    let root = Path::new(&manifest_dir)
        .parent()
        .context("xtask/ has no parent directory")?
        .to_path_buf();
    Ok(root)
}
