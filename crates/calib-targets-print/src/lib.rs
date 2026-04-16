//! Printable calibration target generation.

mod model;
mod render;

pub use model::{
    stem_paths, CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec, MarkerCircleSpec,
    PageOrientation, PageSize, PageSpec, PrintableTargetDocument, PrintableTargetError,
    PuzzleBoardTargetSpec, RenderOptions, ResolvedTargetLayout, ResolvedTargetPoint, TargetSpec,
};
pub use render::{render_target_bundle, GeneratedTargetBundle};

use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WrittenTargetBundle {
    pub json_path: PathBuf,
    pub svg_path: PathBuf,
    pub png_path: PathBuf,
}

pub fn write_target_bundle(
    document: &PrintableTargetDocument,
    output_stem: impl AsRef<Path>,
) -> Result<WrittenTargetBundle, PrintableTargetError> {
    let bundle = render_target_bundle(document)?;
    let (json_path, svg_path, png_path) = stem_paths(output_stem);
    for path in [&json_path, &svg_path, &png_path] {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(&json_path, bundle.json_text)?;
    fs::write(&svg_path, bundle.svg_text)?;
    fs::write(&png_path, bundle.png_bytes)?;
    Ok(WrittenTargetBundle {
        json_path,
        svg_path,
        png_path,
    })
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
    }
}
