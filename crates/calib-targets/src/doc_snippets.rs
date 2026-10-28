//! Compile-checks the Rust snippets of the user-facing Markdown docs.
//!
//! The workspace `README.md`, every crate `README.md` and the mdBook pages
//! under `book/src/` carry Rust code blocks that nothing else compiles. Under
//! `cfg(doctest)` each page becomes the doc comment of a unit struct, so
//! `cargo test --doc` (and therefore `cargo test --workspace`) runs every
//! ```` ```rust ```` block of it as a doctest: plain blocks are compiled and
//! run, `no_run` blocks (snippets that need an image or write files) are
//! compiled only, and `ignore` blocks — reserved for illustrative struct-shape
//! listings — are skipped.
//!
//! The harness lives in the facade crate because its dependencies cover every
//! snippet: the facade itself, each detector crate (crate READMEs name them
//! directly), `image`, `nalgebra`, and `projective-grid` as a dev-dependency.
//!
//! The unit test at the bottom fails when a Markdown page with a Rust block is
//! missing from [`pages!`]'s list, so a new page cannot silently go
//! unchecked.

/// Declares one doctest-carrying struct per Markdown page, plus the list of
/// covered paths (relative to the workspace root) for the coverage test.
///
/// A page entry may carry a `cfg` predicate when its snippets need an
/// off-by-default facade feature; the page is still listed for coverage.
macro_rules! pages {
    ($($(#[cfg($cfg:meta)])? $name:ident => $path:literal,)*) => {
        $(
            #[cfg(doctest)]
            $(#[cfg($cfg)])?
            #[doc = include_str!(concat!("../../../", $path))]
            pub struct $name;
        )*

        /// Every Markdown page whose Rust blocks are compiled above.
        #[cfg(test)]
        const PAGES: &[&str] = &[$($path),*];
    };
}

pages! {
    WorkspaceReadme => "README.md",
    FacadeReadme => "crates/calib-targets/README.md",
    ArucoReadme => "crates/calib-targets-aruco/README.md",
    CharucoReadme => "crates/calib-targets-charuco/README.md",
    ChessboardReadme => "crates/calib-targets-chessboard/README.md",
    CoreReadme => "crates/calib-targets-core/README.md",
    MarkerReadme => "crates/calib-targets-marker/README.md",
    PrintReadme => "crates/calib-targets-print/README.md",
    PuzzleboardReadme => "crates/calib-targets-puzzleboard/README.md",
    ProjectiveGridReadme => "crates/projective-grid/README.md",
    BookAlgoPuzzleboardCodeMaps => "book/src/algo_puzzleboard_code_maps.md",
    BookAlgoPuzzleboardDecode => "book/src/algo_puzzleboard_decode.md",
    BookChessboard => "book/src/chessboard.md",
    BookExampleChessboard => "book/src/example_chessboard.md",
    BookExampleRegularGrid => "book/src/example_regular_grid.md",
    BookFacade => "book/src/facade.md",
    BookGettingStarted => "book/src/getting-started.md",
    BookHowtoPrintPuzzlepole => "book/src/howto_print_puzzlepole.md",
    BookIntro => "book/src/intro.md",
    BookOutput => "book/src/output.md",
    BookPrintable => "book/src/printable.md",
    BookProjectiveGrid => "book/src/projective_grid.md",
    BookPuzzleboard => "book/src/puzzleboard.md",
    BookRectification => "book/src/rectification.md",
    // Calls `trace_topological` and installs a `tracing_subscriber`.
    #[cfg(all(feature = "diagnostics", feature = "tracing"))]
    BookTroubleshooting => "book/src/troubleshooting.md",
    BookTuning => "book/src/tuning.md",
    BookTutorialPuzzlepole => "book/src/tutorial_puzzlepole.md",
}

#[cfg(test)]
mod tests {
    use super::PAGES;
    use std::path::{Path, PathBuf};

    /// Markdown files under `dir` (non-recursive), as workspace-relative paths.
    fn markdown_in(root: &Path, dir: &str) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(root.join(dir))
            .unwrap_or_else(|err| panic!("read {dir}: {err}"))
            .map(|entry| entry.expect("dir entry").path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "md"))
            .map(|path| {
                path.strip_prefix(root)
                    .expect("under root")
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn every_markdown_page_with_rust_blocks_is_compiled() {
        let root: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        // Outside the workspace checkout (e.g. a packaged crate) there are no
        // docs to cover.
        if !root.join("book/src/SUMMARY.md").is_file() {
            return;
        }

        let mut candidates = vec!["README.md".to_owned()];
        candidates.extend(markdown_in(&root, "book/src"));
        let mut crate_dirs: Vec<PathBuf> = std::fs::read_dir(root.join("crates"))
            .expect("read crates/")
            .map(|entry| entry.expect("dir entry").path())
            .collect();
        crate_dirs.sort();
        for dir in crate_dirs {
            if dir.join("README.md").is_file() {
                let name = dir.file_name().expect("crate dir name").to_string_lossy();
                candidates.push(format!("crates/{name}/README.md"));
            }
        }

        let missing: Vec<String> = candidates
            .into_iter()
            .filter(|page| {
                let text = std::fs::read_to_string(root.join(page))
                    .unwrap_or_else(|err| panic!("read {page}: {err}"));
                text.lines().any(|l| l.trim_start().starts_with("```rust"))
            })
            .filter(|page| !PAGES.contains(&page.as_str()))
            .collect();
        assert!(
            missing.is_empty(),
            "Markdown pages with Rust blocks not registered in \
             crates/calib-targets/src/doc_snippets.rs: {missing:?}"
        );
    }
}
