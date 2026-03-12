use calib_targets_aruco::Dictionary;
use calib_targets_charuco::{CharucoBoard, CharucoBoardError, CharucoBoardSpec, MarkerLayout};
use calib_targets_core::GridCoords;
use calib_targets_marker::CirclePolarity;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const SCHEMA_VERSION_V1: u32 = 1;
const MM_PER_INCH: f64 = 25.4;

fn default_schema_version() -> u32 {
    SCHEMA_VERSION_V1
}

fn default_page_spec() -> PageSpec {
    PageSpec::default()
}

fn default_render_options() -> RenderOptions {
    RenderOptions::default()
}

fn default_border_bits() -> usize {
    1
}

fn default_circle_diameter_rel() -> f64 {
    0.5
}

fn default_margin_mm() -> f64 {
    10.0
}

fn default_png_dpi() -> u32 {
    300
}

#[derive(thiserror::Error, Debug)]
pub enum PrintableTargetError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("unsupported schema_version {0}, expected {SCHEMA_VERSION_V1}")]
    UnsupportedSchemaVersion(u32),
    #[error("page width/height must be finite and > 0")]
    InvalidPageSize,
    #[error("page margin must be finite and >= 0")]
    InvalidMargin,
    #[error("printable area is empty after margins")]
    EmptyPrintableArea,
    #[error("png_dpi must be > 0")]
    InvalidPngDpi,
    #[error("inner_rows and inner_cols must be >= 2")]
    InvalidChessboardSize,
    #[error("rows and cols must be >= 2")]
    InvalidCharucoSize,
    #[error("square_size_mm must be finite and > 0")]
    InvalidSquareSize,
    #[error("marker_size_rel must be finite and in (0, 1]")]
    InvalidMarkerSizeRel,
    #[error("border_bits must be > 0")]
    InvalidBorderBits,
    #[error("circle_diameter_rel must be finite and in (0, 1]")]
    InvalidCircleDiameter,
    #[error("marker circle coordinates must fall inside the board squares")]
    InvalidCircleCell,
    #[error("marker circle cells must be unique")]
    DuplicateCircleCells,
    #[error("board needs {needed} markers, dictionary has {available}")]
    NotEnoughDictionaryCodes { needed: usize, available: usize },
    #[error("board does not fit page: board {board_width_mm:.3}x{board_height_mm:.3} mm, printable area {printable_width_mm:.3}x{printable_height_mm:.3} mm")]
    BoardDoesNotFit {
        board_width_mm: f64,
        board_height_mm: f64,
        printable_width_mm: f64,
        printable_height_mm: f64,
    },
    #[error(transparent)]
    CharucoBoard(#[from] CharucoBoardError),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PageOrientation {
    #[default]
    Portrait,
    Landscape,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageSize {
    #[default]
    A4,
    Letter,
    Custom {
        width_mm: f64,
        height_mm: f64,
    },
}

impl PageSize {
    pub fn base_dimensions_mm(&self) -> Result<(f64, f64), PrintableTargetError> {
        match *self {
            Self::A4 => Ok((210.0, 297.0)),
            Self::Letter => Ok((8.5 * MM_PER_INCH, 11.0 * MM_PER_INCH)),
            Self::Custom {
                width_mm,
                height_mm,
            } => {
                if !width_mm.is_finite()
                    || !height_mm.is_finite()
                    || width_mm <= 0.0
                    || height_mm <= 0.0
                {
                    return Err(PrintableTargetError::InvalidPageSize);
                }
                Ok((width_mm, height_mm))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageSpec {
    #[serde(default)]
    pub size: PageSize,
    #[serde(default)]
    pub orientation: PageOrientation,
    #[serde(default = "default_margin_mm")]
    pub margin_mm: f64,
}

impl Default for PageSpec {
    fn default() -> Self {
        Self {
            size: PageSize::default(),
            orientation: PageOrientation::default(),
            margin_mm: default_margin_mm(),
        }
    }
}

impl PageSpec {
    pub fn dimensions_mm(&self) -> Result<(f64, f64), PrintableTargetError> {
        if !self.margin_mm.is_finite() || self.margin_mm < 0.0 {
            return Err(PrintableTargetError::InvalidMargin);
        }
        let (mut width_mm, mut height_mm) = self.size.base_dimensions_mm()?;
        if matches!(self.orientation, PageOrientation::Landscape) {
            std::mem::swap(&mut width_mm, &mut height_mm);
        }
        Ok((width_mm, height_mm))
    }

    pub fn printable_dimensions_mm(&self) -> Result<(f64, f64), PrintableTargetError> {
        let (width_mm, height_mm) = self.dimensions_mm()?;
        let printable_width_mm = width_mm - 2.0 * self.margin_mm;
        let printable_height_mm = height_mm - 2.0 * self.margin_mm;
        if printable_width_mm <= 0.0 || printable_height_mm <= 0.0 {
            return Err(PrintableTargetError::EmptyPrintableArea);
        }
        Ok((printable_width_mm, printable_height_mm))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RenderOptions {
    #[serde(default)]
    pub debug_annotations: bool,
    #[serde(default = "default_png_dpi")]
    pub png_dpi: u32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            debug_annotations: false,
            png_dpi: default_png_dpi(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MarkerCircleSpec {
    pub i: u32,
    pub j: u32,
    pub polarity: CirclePolarity,
}

impl MarkerCircleSpec {
    pub fn to_detector_spec(self) -> calib_targets_marker::MarkerCircleSpec {
        calib_targets_marker::MarkerCircleSpec {
            cell: calib_targets_marker::CellCoords {
                i: self.i as i32,
                j: self.j as i32,
            },
            polarity: self.polarity,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChessboardTargetSpec {
    pub inner_rows: u32,
    pub inner_cols: u32,
    pub square_size_mm: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CharucoTargetSpec {
    pub rows: u32,
    pub cols: u32,
    pub square_size_mm: f64,
    pub marker_size_rel: f64,
    pub dictionary: Dictionary,
    #[serde(default)]
    pub marker_layout: MarkerLayout,
    #[serde(default = "default_border_bits")]
    pub border_bits: usize,
}

impl PartialEq for CharucoTargetSpec {
    fn eq(&self, other: &Self) -> bool {
        self.rows == other.rows
            && self.cols == other.cols
            && self.square_size_mm == other.square_size_mm
            && self.marker_size_rel == other.marker_size_rel
            && self.dictionary.name == other.dictionary.name
            && self.marker_layout == other.marker_layout
            && self.border_bits == other.border_bits
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MarkerBoardTargetSpec {
    pub inner_rows: u32,
    pub inner_cols: u32,
    pub square_size_mm: f64,
    pub circles: [MarkerCircleSpec; 3],
    #[serde(default = "default_circle_diameter_rel")]
    pub circle_diameter_rel: f64,
}

impl MarkerBoardTargetSpec {
    pub fn default_circles(inner_rows: u32, inner_cols: u32) -> [MarkerCircleSpec; 3] {
        let squares_x = inner_cols + 1;
        let squares_y = inner_rows + 1;
        let cx = squares_x / 2;
        let cy = squares_y / 2;
        [
            MarkerCircleSpec {
                i: cx.saturating_sub(1),
                j: cy.saturating_sub(1),
                polarity: CirclePolarity::White,
            },
            MarkerCircleSpec {
                i: cx,
                j: cy.saturating_sub(1),
                polarity: CirclePolarity::Black,
            },
            MarkerCircleSpec {
                i: cx,
                j: cy,
                polarity: CirclePolarity::White,
            },
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetSpec {
    Chessboard(ChessboardTargetSpec),
    Charuco(CharucoTargetSpec),
    MarkerBoard(MarkerBoardTargetSpec),
}

impl TargetSpec {
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Chessboard(_) => "chessboard",
            Self::Charuco(_) => "charuco",
            Self::MarkerBoard(_) => "marker_board",
        }
    }

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
        }
    }

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
        }
    }
}

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
    pub fn new(target: TargetSpec) -> Self {
        Self {
            schema_version: default_schema_version(),
            target,
            page: PageSpec::default(),
            render: RenderOptions::default(),
        }
    }

    pub fn load_json(path: impl AsRef<Path>) -> Result<Self, PrintableTargetError> {
        let raw = fs::read_to_string(path)?;
        let doc: Self = serde_json::from_str(&raw)?;
        doc.validate()?;
        Ok(doc)
    }

    pub fn write_json(&self, path: impl AsRef<Path>) -> Result<(), PrintableTargetError> {
        self.validate()?;
        fs::write(path, self.to_json_pretty()?)?;
        Ok(())
    }

    pub fn to_json_pretty(&self) -> Result<String, PrintableTargetError> {
        self.validate()?;
        Ok(serde_json::to_string_pretty(self)?)
    }

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedTargetPoint {
    pub position_mm: [f64; 2],
    #[serde(default)]
    pub grid: Option<GridCoords>,
    #[serde(default)]
    pub id: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResolvedTargetLayout {
    pub page_width_mm: f64,
    pub page_height_mm: f64,
    pub board_origin_mm: [f64; 2],
    pub board_width_mm: f64,
    pub board_height_mm: f64,
    pub points: Vec<ResolvedTargetPoint>,
}

pub fn stem_paths(output_stem: impl AsRef<Path>) -> (PathBuf, PathBuf, PathBuf) {
    let stem = output_stem.as_ref();
    (
        stem.with_extension("json"),
        stem.with_extension("svg"),
        stem.with_extension("png"),
    )
}

pub(crate) fn validate_inner_corner_grid(
    inner_rows: u32,
    inner_cols: u32,
    square_size_mm: f64,
) -> Result<(), PrintableTargetError> {
    if inner_rows < 2 || inner_cols < 2 {
        return Err(PrintableTargetError::InvalidChessboardSize);
    }
    validate_square_size(square_size_mm)
}

fn validate_square_size(square_size_mm: f64) -> Result<(), PrintableTargetError> {
    if !square_size_mm.is_finite() || square_size_mm <= 0.0 {
        return Err(PrintableTargetError::InvalidSquareSize);
    }
    Ok(())
}

pub(crate) fn validate_charuco_spec(spec: &CharucoTargetSpec) -> Result<(), PrintableTargetError> {
    if spec.rows < 2 || spec.cols < 2 {
        return Err(PrintableTargetError::InvalidCharucoSize);
    }
    validate_square_size(spec.square_size_mm)?;
    if !spec.marker_size_rel.is_finite()
        || spec.marker_size_rel <= 0.0
        || spec.marker_size_rel > 1.0
    {
        return Err(PrintableTargetError::InvalidMarkerSizeRel);
    }
    if spec.border_bits == 0 {
        return Err(PrintableTargetError::InvalidBorderBits);
    }
    let board = spec.to_board_spec();
    let board = CharucoBoard::new(board)?;
    let needed = board.marker_count();
    let available = spec.dictionary.codes.len();
    if available < needed {
        return Err(PrintableTargetError::NotEnoughDictionaryCodes { needed, available });
    }
    Ok(())
}

pub(crate) fn validate_marker_board_spec(
    spec: &MarkerBoardTargetSpec,
) -> Result<(), PrintableTargetError> {
    validate_inner_corner_grid(spec.inner_rows, spec.inner_cols, spec.square_size_mm)?;
    if !spec.circle_diameter_rel.is_finite()
        || spec.circle_diameter_rel <= 0.0
        || spec.circle_diameter_rel > 1.0
    {
        return Err(PrintableTargetError::InvalidCircleDiameter);
    }
    let squares_x = spec.inner_cols + 1;
    let squares_y = spec.inner_rows + 1;
    let mut seen = std::collections::BTreeSet::new();
    for circle in spec.circles {
        if circle.i >= squares_x || circle.j >= squares_y {
            return Err(PrintableTargetError::InvalidCircleCell);
        }
        if !seen.insert((circle.i, circle.j)) {
            return Err(PrintableTargetError::DuplicateCircleCells);
        }
    }
    Ok(())
}

impl CharucoTargetSpec {
    pub fn to_board_spec(&self) -> CharucoBoardSpec {
        CharucoBoardSpec {
            rows: self.rows,
            cols: self.cols,
            cell_size: self.square_size_mm as f32,
            marker_size_rel: self.marker_size_rel as f32,
            dictionary: self.dictionary,
            marker_layout: self.marker_layout,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_aruco::builtins;

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
}
