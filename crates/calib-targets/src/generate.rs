//! Ergonomic constructors for printable-target documents.
//!
//! Thin wrappers over [`PrintableTargetDocument::new`](crate::printable::PrintableTargetDocument::new)
//! that hide the `TargetSpec` enum wrapping. The returned document uses default
//! [`PageSpec`](crate::printable::PageSpec) (A4 portrait, 10 mm margins) and default
//! [`RenderOptions`](crate::printable::RenderOptions) (300 DPI, no debug annotations); callers
//! mutate the returned document's `page` / `render` fields for customisation.
//!
//! ```no_run
//! use calib_targets::generate::chessboard_document;
//! use calib_targets::printable::{write_target_bundle, PageSize, PageOrientation};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut doc = chessboard_document(6, 8, 20.0);
//! doc.page.size = PageSize::Letter;
//! doc.page.orientation = PageOrientation::Landscape;
//! let bundle = write_target_bundle(&doc, "my_board")?;
//! println!("{}", bundle.json_path.display());
//! # Ok(())
//! # }
//! ```

use calib_targets_aruco::Dictionary;
use calib_targets_charuco::MarkerLayout;
use calib_targets_print::{
    CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec, MarkerCircleSpec,
    PrintableTargetDocument, PuzzleBoardTargetSpec, TargetSpec,
};

/// Build a chessboard printable document with default page and render settings.
///
/// `inner_rows` and `inner_cols` count **inner corner intersections** (must be ≥ 2 each).
/// The printed board has `(inner_cols + 1) × (inner_rows + 1)` squares each
/// `square_size_mm` millimeters on a side.
pub fn chessboard_document(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
) -> PrintableTargetDocument {
    PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
        inner_rows,
        inner_cols,
        square_size_mm,
    }))
}

/// Build a ChArUco printable document with default page and render settings.
///
/// `rows` and `cols` are **square counts** (not inner corner counts). `marker_size_rel`
/// is the marker side length relative to `square_size_mm` (0 < rel ≤ 1). `border_bits`
/// defaults to 1; override on the returned spec for custom layouts.
pub fn charuco_document(
    rows: u32,
    cols: u32,
    square_size_mm: f64,
    marker_size_rel: f64,
    dictionary: Dictionary,
) -> PrintableTargetDocument {
    PrintableTargetDocument::new(TargetSpec::Charuco(CharucoTargetSpec {
        rows,
        cols,
        square_size_mm,
        marker_size_rel,
        dictionary,
        marker_layout: MarkerLayout::default(),
        border_bits: 1,
    }))
}

/// Build a PuzzleBoard printable document with default page and render settings.
///
/// Anchors the sub-board at master-pattern origin `(0, 0)` with the paper-recommended
/// dot diameter (1/3 of the square size). Mutate the returned target's fields to move
/// the origin or tune the dot size.
pub fn puzzleboard_document(rows: u32, cols: u32, square_size_mm: f64) -> PrintableTargetDocument {
    PrintableTargetDocument::new(TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec {
        rows,
        cols,
        square_size_mm,
        origin_row: 0,
        origin_col: 0,
        dot_diameter_rel: 1.0 / 3.0,
    }))
}

/// Build a marker-board printable document with default page and render settings.
///
/// Uses [`MarkerBoardTargetSpec::default_circles`] for the three calibration circles
/// and a 0.5 circle-diameter ratio. Override `doc.target` fields for custom circle
/// placements or sizes.
pub fn marker_board_document(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
) -> PrintableTargetDocument {
    marker_board_document_with_circles(
        inner_rows,
        inner_cols,
        square_size_mm,
        MarkerBoardTargetSpec::default_circles(inner_rows, inner_cols),
    )
}

/// Variant of [`marker_board_document`] with caller-specified circles.
pub fn marker_board_document_with_circles(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
    circles: [MarkerCircleSpec; 3],
) -> PrintableTargetDocument {
    PrintableTargetDocument::new(TargetSpec::MarkerBoard(MarkerBoardTargetSpec {
        inner_rows,
        inner_cols,
        square_size_mm,
        circles,
        circle_diameter_rel: 0.5,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_aruco::builtins::builtin_dictionary;

    fn round_trip(doc: &PrintableTargetDocument) {
        doc.validate().expect("validate");
        let json = doc.to_json_pretty().expect("to_json");
        let parsed: PrintableTargetDocument = serde_json::from_str(&json).expect("from_json");
        parsed.validate().expect("re-validate");
        assert_eq!(doc, &parsed);
    }

    #[test]
    fn chessboard_helper_produces_valid_document() {
        let doc = chessboard_document(6, 8, 20.0);
        round_trip(&doc);
        assert_eq!(doc.target.kind_name(), "chessboard");
    }

    #[test]
    fn charuco_helper_produces_valid_document() {
        let dict = builtin_dictionary("DICT_4X4_50").expect("dict");
        let doc = charuco_document(5, 7, 20.0, 0.75, dict);
        round_trip(&doc);
        assert_eq!(doc.target.kind_name(), "charuco");
    }

    #[test]
    fn puzzleboard_helper_produces_valid_document() {
        let doc = puzzleboard_document(10, 12, 15.0);
        round_trip(&doc);
        assert_eq!(doc.target.kind_name(), "puzzleboard");
    }

    #[test]
    fn marker_board_helper_produces_valid_document() {
        let doc = marker_board_document(6, 8, 20.0);
        round_trip(&doc);
        assert_eq!(doc.target.kind_name(), "marker_board");
    }
}
