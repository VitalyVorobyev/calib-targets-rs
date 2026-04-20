//! Printable target document model.
//!
//! Sub-modules split by concern:
//! - `error`: [`PrintableTargetError`]
//! - `page`: [`PageSize`], [`PageOrientation`], [`PageSpec`], [`RenderOptions`]
//! - `chessboard`: [`ChessboardTargetSpec`]
//! - `charuco`: [`CharucoTargetSpec`]
//! - `marker`: [`MarkerCircleSpec`], [`MarkerBoardTargetSpec`]
//! - `puzzleboard`: [`PuzzleBoardTargetSpec`]

mod charuco;
mod chessboard;
mod error;
mod marker;
mod page;
mod puzzleboard;

pub use charuco::CharucoTargetSpec;
pub use chessboard::ChessboardTargetSpec;
pub use error::PrintableTargetError;
pub use marker::{MarkerBoardTargetSpec, MarkerCircleSpec};
pub use page::{PageOrientation, PageSize, PageSpec, RenderOptions};
pub use puzzleboard::PuzzleBoardTargetSpec;

pub(crate) use charuco::validate_charuco_spec;
pub(crate) use chessboard::validate_inner_corner_grid;
pub(crate) use marker::validate_marker_board_spec;
pub(crate) use puzzleboard::validate_puzzleboard_spec;

use error::SCHEMA_VERSION_V1;

use calib_targets_core::GridCoords;
use calib_targets_marker::MarkerBoardSpec;
use calib_targets_puzzleboard::MASTER_COLS;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn default_schema_version() -> u32 {
    SCHEMA_VERSION_V1
}

fn default_page_spec() -> PageSpec {
    PageSpec::default()
}

fn default_render_options() -> RenderOptions {
    RenderOptions::default()
}

/// Union of all printable target geometries.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetSpec {
    Chessboard(ChessboardTargetSpec),
    Charuco(CharucoTargetSpec),
    MarkerBoard(MarkerBoardTargetSpec),
    PuzzleBoard(PuzzleBoardTargetSpec),
}

impl TargetSpec {
    /// Short name for the target kind, suitable for filenames and JSON.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Chessboard(_) => "chessboard",
            Self::Charuco(_) => "charuco",
            Self::MarkerBoard(_) => "marker_board",
            Self::PuzzleBoard(_) => "puzzleboard",
        }
    }

    /// Returns `(width_mm, height_mm)` of the rendered board area.
    pub fn board_size_mm(&self) -> Result<(f64, f64), PrintableTargetError> {
        match self {
            Self::Chessboard(spec) => {
                validate_inner_corner_grid(spec.inner_rows, spec.inner_cols, spec.square_size_mm)?;
                Ok((
                    (spec.inner_cols as f64 + 1.0) * spec.square_size_mm,
                    (spec.inner_rows as f64 + 1.0) * spec.square_size_mm,
                ))
            }
            Self::Charuco(spec) => {
                validate_charuco_spec(spec)?;
                Ok((
                    spec.cols as f64 * spec.square_size_mm,
                    spec.rows as f64 * spec.square_size_mm,
                ))
            }
            Self::MarkerBoard(spec) => {
                validate_marker_board_spec(spec)?;
                Ok((
                    (spec.inner_cols as f64 + 1.0) * spec.square_size_mm,
                    (spec.inner_rows as f64 + 1.0) * spec.square_size_mm,
                ))
            }
            Self::PuzzleBoard(spec) => {
                validate_puzzleboard_spec(spec)?;
                Ok((
                    spec.cols as f64 * spec.square_size_mm,
                    spec.rows as f64 * spec.square_size_mm,
                ))
            }
        }
    }

    /// Compute board-space coordinates for every output point.
    pub fn resolved_points(&self) -> Result<Vec<ResolvedTargetPoint>, PrintableTargetError> {
        match self {
            Self::Chessboard(spec) => {
                validate_inner_corner_grid(spec.inner_rows, spec.inner_cols, spec.square_size_mm)?;
                let mut points =
                    Vec::with_capacity(spec.inner_rows as usize * spec.inner_cols as usize);
                for j in 0..spec.inner_rows {
                    for i in 0..spec.inner_cols {
                        points.push(ResolvedTargetPoint {
                            position_mm: [
                                (i as f64 + 1.0) * spec.square_size_mm,
                                (j as f64 + 1.0) * spec.square_size_mm,
                            ],
                            grid: Some(GridCoords {
                                i: i as i32,
                                j: j as i32,
                            }),
                            id: None,
                        });
                    }
                }
                Ok(points)
            }
            Self::Charuco(spec) => {
                validate_charuco_spec(spec)?;
                let mut points = Vec::with_capacity(
                    (spec.rows.saturating_sub(1) * spec.cols.saturating_sub(1)) as usize,
                );
                let inner_rows = spec.rows - 1;
                let inner_cols = spec.cols - 1;
                for j in 0..inner_rows {
                    for i in 0..inner_cols {
                        let id = j * inner_cols + i;
                        points.push(ResolvedTargetPoint {
                            position_mm: [
                                (i as f64 + 1.0) * spec.square_size_mm,
                                (j as f64 + 1.0) * spec.square_size_mm,
                            ],
                            grid: Some(GridCoords {
                                i: i as i32,
                                j: j as i32,
                            }),
                            id: Some(id),
                        });
                    }
                }
                Ok(points)
            }
            Self::MarkerBoard(spec) => {
                validate_marker_board_spec(spec)?;
                let mut points =
                    Vec::with_capacity(spec.inner_rows as usize * spec.inner_cols as usize);
                for j in 0..spec.inner_rows {
                    for i in 0..spec.inner_cols {
                        points.push(ResolvedTargetPoint {
                            position_mm: [
                                (i as f64 + 1.0) * spec.square_size_mm,
                                (j as f64 + 1.0) * spec.square_size_mm,
                            ],
                            grid: Some(GridCoords {
                                i: i as i32,
                                j: j as i32,
                            }),
                            id: None,
                        });
                    }
                }
                Ok(points)
            }
            Self::PuzzleBoard(spec) => {
                validate_puzzleboard_spec(spec)?;
                let inner_rows = spec.rows.saturating_sub(1);
                let inner_cols = spec.cols.saturating_sub(1);
                let mut points = Vec::with_capacity((inner_rows * inner_cols) as usize);
                // Each corner's id is `I * MASTER_COLS + J` where (I, J) are
                // master-absolute coords. (I, J) = local (i, j) + origin.
                for j in 0..inner_rows {
                    for i in 0..inner_cols {
                        let master_i = spec.origin_col + i + 1; // inner corner → (origin_col + 1 + i)
                        let master_j = spec.origin_row + j + 1;
                        let id = master_j * MASTER_COLS + master_i;
                        points.push(ResolvedTargetPoint {
                            position_mm: [
                                (i as f64 + 1.0) * spec.square_size_mm,
                                (j as f64 + 1.0) * spec.square_size_mm,
                            ],
                            grid: Some(GridCoords {
                                i: master_i as i32,
                                j: master_j as i32,
                            }),
                            id: Some(id),
                        });
                    }
                }
                Ok(points)
            }
        }
    }
}

/// Top-level printable target document (the JSON file on disk).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintableTargetDocument {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub target: TargetSpec,
    #[serde(default = "default_page_spec")]
    pub page: PageSpec,
    #[serde(default = "default_render_options")]
    pub render: RenderOptions,
}

impl PrintableTargetDocument {
    /// Create a new document with default page and render settings.
    pub fn new(target: TargetSpec) -> Self {
        Self {
            schema_version: default_schema_version(),
            target,
            page: PageSpec::default(),
            render: RenderOptions::default(),
        }
    }

    /// Build a printable document from a ChArUco board spec whose `cell_size`
    /// is already expressed in millimeters.
    pub fn from_charuco_board_spec_mm(board: &calib_targets_charuco::CharucoBoardSpec) -> Self {
        Self::new(TargetSpec::Charuco(CharucoTargetSpec::from_board_spec_mm(
            board,
        )))
    }

    /// Build a printable document from a marker-board layout whose `cell_size`
    /// is already expressed in millimeters.
    pub fn try_from_marker_board_layout_mm(
        layout: &MarkerBoardSpec,
    ) -> Result<Self, PrintableTargetError> {
        Ok(Self::new(TargetSpec::MarkerBoard(
            MarkerBoardTargetSpec::try_from_layout_mm(layout)?,
        )))
    }

    /// Load and validate a document from a JSON file.
    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, PrintableTargetError> {
        let raw = fs::read_to_string(path)?;
        let doc: Self = serde_json::from_str(&raw)?;
        doc.validate()?;
        Ok(doc)
    }

    /// Validate and write to a JSON file.
    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), PrintableTargetError> {
        self.validate()?;
        fs::write(path, self.to_json_pretty()?)?;
        Ok(())
    }

    /// Serialize to pretty-printed JSON.
    pub fn to_json_pretty(&self) -> Result<String, PrintableTargetError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Validate schema version, page dimensions, and target geometry.
    pub fn validate(&self) -> Result<(), PrintableTargetError> {
        if self.schema_version != SCHEMA_VERSION_V1 {
            return Err(PrintableTargetError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        let _ = self.page.printable_dimensions_mm()?;
        if self.render.png_dpi == 0 {
            return Err(PrintableTargetError::InvalidPngDpi);
        }
        let (board_width_mm, board_height_mm) = self.target.board_size_mm()?;
        let (printable_width_mm, printable_height_mm) = self.page.printable_dimensions_mm()?;
        if board_width_mm > printable_width_mm || board_height_mm > printable_height_mm {
            return Err(PrintableTargetError::BoardDoesNotFit {
                board_width_mm,
                board_height_mm,
                printable_width_mm,
                printable_height_mm,
            });
        }
        let _ = self.target.resolved_points()?;
        Ok(())
    }

    /// Compute the full page + board layout for rendering.
    pub fn resolve_layout(&self) -> Result<ResolvedTargetLayout, PrintableTargetError> {
        self.validate()?;
        let (page_width_mm, page_height_mm) = self.page.dimensions_mm()?;
        let (board_width_mm, board_height_mm) = self.target.board_size_mm()?;
        let printable_width_mm = page_width_mm - 2.0 * self.page.margin_mm;
        let printable_height_mm = page_height_mm - 2.0 * self.page.margin_mm;
        let board_origin_mm = [
            self.page.margin_mm + 0.5 * (printable_width_mm - board_width_mm),
            self.page.margin_mm + 0.5 * (printable_height_mm - board_height_mm),
        ];
        Ok(ResolvedTargetLayout {
            page_width_mm,
            page_height_mm,
            board_origin_mm,
            board_width_mm,
            board_height_mm,
            points: self.target.resolved_points()?,
        })
    }
}

/// A single output point in board-space coordinates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedTargetPoint {
    pub position_mm: [f64; 2],
    #[serde(default)]
    pub grid: Option<GridCoords>,
    #[serde(default)]
    pub id: Option<u32>,
}

/// Full resolved layout for a single printable target document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedTargetLayout {
    pub page_width_mm: f64,
    pub page_height_mm: f64,
    pub board_origin_mm: [f64; 2],
    pub board_width_mm: f64,
    pub board_height_mm: f64,
    pub points: Vec<ResolvedTargetPoint>,
}

/// Compute the JSON, SVG, and PNG output paths from a stem.
pub fn stem_paths(output_stem: impl AsRef<Path>) -> (PathBuf, PathBuf, PathBuf) {
    let stem = output_stem.as_ref();
    (
        stem.with_extension("json"),
        stem.with_extension("svg"),
        stem.with_extension("png"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_aruco::builtins;
    use calib_targets_charuco::MarkerLayout;
    use calib_targets_marker::CirclePolarity;
    use calib_targets_marker::{CellCoords, MarkerCircleSpec as DetectorMarkerCircleSpec};

    fn sample_chessboard() -> PrintableTargetDocument {
        PrintableTargetDocument::new(TargetSpec::Chessboard(ChessboardTargetSpec {
            inner_rows: 6,
            inner_cols: 8,
            square_size_mm: 20.0,
        }))
    }

    fn sample_charuco() -> PrintableTargetDocument {
        PrintableTargetDocument::new(TargetSpec::Charuco(CharucoTargetSpec {
            rows: 5,
            cols: 7,
            square_size_mm: 15.0,
            marker_size_rel: 0.75,
            dictionary: builtins::builtin_dictionary("DICT_4X4_50").expect("dict"),
            marker_layout: MarkerLayout::OpenCvCharuco,
            border_bits: 1,
        }))
    }

    fn sample_marker_board() -> PrintableTargetDocument {
        PrintableTargetDocument::new(TargetSpec::MarkerBoard(MarkerBoardTargetSpec {
            inner_rows: 6,
            inner_cols: 8,
            square_size_mm: 20.0,
            circles: MarkerBoardTargetSpec::default_circles(6, 8),
            circle_diameter_rel: 0.5,
        }))
    }

    #[test]
    fn resolves_chessboard_points() {
        let doc = sample_chessboard();
        let layout = doc.resolve_layout().expect("layout");
        assert_eq!(layout.points.len(), 48);
        assert_eq!(layout.points[0].position_mm, [20.0, 20.0]);
        assert_eq!(layout.board_width_mm, 180.0);
        assert_eq!(layout.board_height_mm, 140.0);
    }

    #[test]
    fn resolves_charuco_points() {
        let doc = sample_charuco();
        let layout = doc.resolve_layout().expect("layout");
        assert_eq!(layout.points.len(), 24);
        assert_eq!(layout.points[0].id, Some(0));
        assert_eq!(layout.points[0].grid, Some(GridCoords { i: 0, j: 0 }));
    }

    #[test]
    fn rejects_board_that_does_not_fit_page() {
        let mut doc = sample_chessboard();
        doc.page.size = PageSize::Custom {
            width_mm: 50.0,
            height_mm: 50.0,
        };
        let err = doc.validate().expect_err("fit check");
        assert!(matches!(err, PrintableTargetError::BoardDoesNotFit { .. }));
    }

    #[test]
    fn rejects_duplicate_marker_circles() {
        let mut doc = sample_marker_board();
        if let TargetSpec::MarkerBoard(spec) = &mut doc.target {
            spec.circles = [
                MarkerCircleSpec {
                    i: 1,
                    j: 1,
                    polarity: CirclePolarity::White,
                },
                MarkerCircleSpec {
                    i: 1,
                    j: 1,
                    polarity: CirclePolarity::Black,
                },
                MarkerCircleSpec {
                    i: 2,
                    j: 2,
                    polarity: CirclePolarity::White,
                },
            ];
        }
        let err = doc.validate().expect_err("duplicate circles");
        assert!(matches!(err, PrintableTargetError::DuplicateCircleCells));
    }

    #[test]
    fn json_roundtrip_is_stable() {
        let doc = sample_charuco();
        let json = doc.to_json_pretty().expect("json");
        let parsed: PrintableTargetDocument = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed, doc);
    }

    #[test]
    fn builds_charuco_spec_from_board_spec_mm() {
        use calib_targets_charuco::CharucoBoardSpec;
        let board = CharucoBoardSpec {
            rows: 5,
            cols: 7,
            cell_size: 20.0,
            marker_size_rel: 0.75,
            dictionary: builtins::builtin_dictionary("DICT_4X4_50").expect("dict"),
            marker_layout: MarkerLayout::OpenCvCharuco,
        };
        let spec = CharucoTargetSpec::from_board_spec_mm(&board);
        assert_eq!(spec.rows, board.rows);
        assert_eq!(spec.cols, board.cols);
        assert_eq!(spec.square_size_mm, 20.0);
        assert_eq!(spec.marker_size_rel, 0.75);
        assert_eq!(spec.dictionary.name, board.dictionary.name);
        assert_eq!(spec.marker_layout, board.marker_layout);
        assert_eq!(spec.border_bits, 1);
    }

    #[test]
    fn builds_charuco_document_from_board_spec_mm() {
        use calib_targets_charuco::CharucoBoardSpec;
        let board = CharucoBoardSpec {
            rows: 5,
            cols: 7,
            cell_size: 20.0,
            marker_size_rel: 0.75,
            dictionary: builtins::builtin_dictionary("DICT_4X4_50").expect("dict"),
            marker_layout: MarkerLayout::OpenCvCharuco,
        };
        let doc = PrintableTargetDocument::from_charuco_board_spec_mm(&board);
        assert!(matches!(
            &doc.target,
            TargetSpec::Charuco(spec)
                if spec.rows == 5
                    && spec.cols == 7
                    && spec.square_size_mm == 20.0
                    && spec.marker_size_rel == 0.75
                    && spec.border_bits == 1
        ));
        doc.validate().expect("valid printable charuco");
    }

    #[test]
    fn builds_marker_board_spec_from_layout_mm() {
        let layout = MarkerBoardSpec {
            rows: 6,
            cols: 8,
            cell_size: Some(20.0),
            circles: [
                DetectorMarkerCircleSpec {
                    cell: CellCoords { i: 3, j: 2 },
                    polarity: CirclePolarity::White,
                },
                DetectorMarkerCircleSpec {
                    cell: CellCoords { i: 4, j: 2 },
                    polarity: CirclePolarity::Black,
                },
                DetectorMarkerCircleSpec {
                    cell: CellCoords { i: 4, j: 3 },
                    polarity: CirclePolarity::White,
                },
            ],
        };
        let spec = MarkerBoardTargetSpec::try_from_layout_mm(&layout).expect("marker board spec");
        assert_eq!(spec.inner_rows, 6);
        assert_eq!(spec.inner_cols, 8);
        assert_eq!(spec.square_size_mm, 20.0);
        assert_eq!(
            spec.circles,
            [
                MarkerCircleSpec {
                    i: 3,
                    j: 2,
                    polarity: CirclePolarity::White,
                },
                MarkerCircleSpec {
                    i: 4,
                    j: 2,
                    polarity: CirclePolarity::Black,
                },
                MarkerCircleSpec {
                    i: 4,
                    j: 3,
                    polarity: CirclePolarity::White,
                },
            ]
        );
        assert_eq!(spec.circle_diameter_rel, 0.5);
    }

    #[test]
    fn builds_marker_board_document_from_layout_mm() {
        let layout = MarkerBoardSpec {
            rows: 6,
            cols: 8,
            cell_size: Some(20.0),
            circles: [
                DetectorMarkerCircleSpec {
                    cell: CellCoords { i: 3, j: 2 },
                    polarity: CirclePolarity::White,
                },
                DetectorMarkerCircleSpec {
                    cell: CellCoords { i: 4, j: 2 },
                    polarity: CirclePolarity::Black,
                },
                DetectorMarkerCircleSpec {
                    cell: CellCoords { i: 4, j: 3 },
                    polarity: CirclePolarity::White,
                },
            ],
        };
        let doc = PrintableTargetDocument::try_from_marker_board_layout_mm(&layout)
            .expect("marker board doc");
        assert!(matches!(
            &doc.target,
            TargetSpec::MarkerBoard(spec)
                if spec.inner_rows == 6
                    && spec.inner_cols == 8
                    && spec.square_size_mm == 20.0
                    && spec.circle_diameter_rel == 0.5
        ));
        doc.validate().expect("valid printable marker board");
    }

    #[test]
    fn rejects_marker_board_layout_without_cell_size() {
        let layout = MarkerBoardSpec {
            rows: 6,
            cols: 8,
            cell_size: None,
            circles: MarkerBoardSpec::default().circles,
        };
        let err =
            MarkerBoardTargetSpec::try_from_layout_mm(&layout).expect_err("missing cell size");
        assert!(matches!(
            err,
            PrintableTargetError::MissingMarkerBoardCellSize
        ));
    }

    #[test]
    fn rejects_negative_detector_circle_coords() {
        let layout = MarkerBoardSpec {
            rows: 6,
            cols: 8,
            cell_size: Some(20.0),
            circles: [
                DetectorMarkerCircleSpec {
                    cell: CellCoords { i: -1, j: 2 },
                    polarity: CirclePolarity::White,
                },
                DetectorMarkerCircleSpec {
                    cell: CellCoords { i: 4, j: 2 },
                    polarity: CirclePolarity::Black,
                },
                DetectorMarkerCircleSpec {
                    cell: CellCoords { i: 4, j: 3 },
                    polarity: CirclePolarity::White,
                },
            ],
        };
        let err =
            MarkerBoardTargetSpec::try_from_layout_mm(&layout).expect_err("negative detector cell");
        assert!(matches!(err, PrintableTargetError::InvalidCircleCell));
    }
}
