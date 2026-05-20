//! Printable calibration target generation.
#![deny(missing_docs)]

mod model;
mod render;
mod render_dxf;

pub use model::{
    stem_paths, CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec, MarkerCircleSpec,
    PageOrientation, PageSize, PageSpec, PrintableTargetDocument, PrintableTargetError,
    PuzzleBoardTargetSpec, RenderOptions, ResolvedTargetLayout, ResolvedTargetPoint, StemPaths,
    TargetSpec,
};
pub use render::{render_target_bundle, GeneratedTargetBundle};

use std::{
    fs,
    path::{Path, PathBuf},
};

/// Paths of the files written by [`write_target_bundle`].
///
/// Marked `#[non_exhaustive]` (mirroring [`StemPaths`] and
/// [`GeneratedTargetBundle`]) so that future formats can be added
/// without breaking cross-crate consumers.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrittenTargetBundle {
    /// Path of the written JSON description.
    pub json_path: PathBuf,
    /// Path of the written SVG rendering.
    pub svg_path: PathBuf,
    /// Path of the written PNG rendering.
    pub png_path: PathBuf,
    /// Path of the written DXF rendering (photolithography handoff).
    pub dxf_path: PathBuf,
}

impl WrittenTargetBundle {
    /// Construct a `WrittenTargetBundle` from explicit per-format paths.
    pub fn new(
        json_path: PathBuf,
        svg_path: PathBuf,
        png_path: PathBuf,
        dxf_path: PathBuf,
    ) -> Self {
        Self {
            json_path,
            svg_path,
            png_path,
            dxf_path,
        }
    }
}

/// Render a printable target and write the JSON, SVG, PNG, and DXF
/// files to disk, deriving their paths from `output_stem`.
///
/// Parent directories are created as needed.
pub fn write_target_bundle(
    document: &PrintableTargetDocument,
    output_stem: impl AsRef<Path>,
) -> Result<WrittenTargetBundle, PrintableTargetError> {
    let bundle = render_target_bundle(document)?;
    let paths = StemPaths::from_stem(output_stem);
    for path in [&paths.json, &paths.svg, &paths.png, &paths.dxf] {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&paths.json, bundle.json_text)?;
    fs::write(&paths.svg, bundle.svg_text)?;
    fs::write(&paths.png, bundle.png_bytes)?;
    fs::write(&paths.dxf, bundle.dxf_text)?;
    Ok(WrittenTargetBundle::new(
        paths.json, paths.svg, paths.png, paths.dxf,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_bundle_files() {
        let dir = tempdir().expect("tempdir");
        let doc = PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
            inner_rows: 6,
            inner_cols: 8,
            square_size_mm: 20.0,
        }));
        let paths = write_target_bundle(&doc, dir.path().join("sample")).expect("bundle");
        assert!(paths.json_path.is_file());
        assert!(paths.svg_path.is_file());
        assert!(paths.png_path.is_file());
        assert!(paths.dxf_path.is_file());
    }
}
