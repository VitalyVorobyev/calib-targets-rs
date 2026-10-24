//! The four detector families of the accuracy suite: one printable board
//! each, its ground-truth corner set, and a detection run with the board's
//! default parameters.

use calib_targets::aruco::resolve_dictionary;
use calib_targets::charuco::{CharucoBoardSpec, CharucoDetector, CharucoParams, MarkerLayout};
use calib_targets::chessboard::ChessboardParams;
use calib_targets::core::GrayImageView;
use calib_targets::detect::{default_chess_config, detect_chessboard, DetectorConfig};
use calib_targets::marker::{MarkerBoardDetector, MarkerBoardParams, MarkerBoardSpec};
use calib_targets::printable::{
    board_primitives, CharucoTargetSpec, ChessboardTargetSpec, MarkerBoardTargetSpec,
    PuzzleBoardTargetSpec, TargetSpec,
};
use calib_targets::puzzleboard::{PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec};
use calib_targets::{Coord, DetectionQuality, PositionCovariance};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};

use super::render::{Rendered, Scene};

/// Board square size, mm. Only the ratio to the rendered cell size matters.
pub const SQUARE_MM: f64 = 10.0;
/// Board size in squares: 10 × 8, i.e. 9 × 7 inner corners for every family.
const SQUARES_X: u32 = 10;
const SQUARES_Y: u32 = 8;
const DICTIONARY: &str = "DICT_4X4_50";

/// A detector family of the suite.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    /// Plain chessboard (relative labels).
    Chessboard,
    /// ChArUco (absolute ids from the markers).
    Charuco,
    /// PuzzleBoard (absolute master labels from the edge dots).
    PuzzleBoard,
    /// Checkerboard with three circle markers.
    MarkerBoard,
}

impl Family {
    /// Every family, in report order.
    pub const ALL: [Family; 4] = [
        Family::Chessboard,
        Family::Charuco,
        Family::PuzzleBoard,
        Family::MarkerBoard,
    ];

    /// Short report label — the serde tag, spelled as
    /// [`TargetKind`](calib_targets::TargetKind) spells the family.
    pub fn label(self) -> &'static str {
        match self {
            Family::Chessboard => "chessboard",
            Family::Charuco => "charuco",
            Family::PuzzleBoard => "puzzle_board",
            Family::MarkerBoard => "marker_board",
        }
    }
}

/// One ground-truth inner corner.
#[derive(Clone, Copy, Debug)]
pub struct GtCorner {
    /// Inner-corner index `(i, j)`, `i` right, `j` down, from 0.
    pub index: (i32, i32),
    /// Position on the board, mm from its top-left.
    pub board_mm: Point2<f64>,
}

/// One corner a detector delivered, reduced to what the scoring needs.
#[derive(Clone, Copy, Debug)]
pub struct Delivered {
    /// Reported image position (pixel centres at integers).
    pub position: Point2<f32>,
    /// Reported position covariance.
    pub covariance: Option<PositionCovariance>,
    /// Reported grid label.
    pub grid: Coord,
    /// The board position the detector claims for the corner (mm), when it
    /// names one.
    pub claim_mm: Option<Point2<f32>>,
    /// Whether the corner's id agrees with its claimed board position (always
    /// true for a family without ids).
    pub id_consistent: bool,
}

/// A detection run on one rendered frame.
pub struct FamilyRun {
    /// The delivered corners.
    pub delivered: Vec<Delivered>,
    /// The result's own quality summary.
    pub quality: DetectionQuality,
}

/// A family's board: its print description, scene and truth.
pub struct Board {
    /// Which family.
    pub family: Family,
    /// Board extent, mm.
    pub size_mm: [f64; 2],
    /// The indexed primitives the renderer draws.
    pub scene: Scene,
    /// Every inner corner.
    pub gt: Vec<GtCorner>,
    target: TargetSpec,
}

fn inner_grid_truth() -> Vec<GtCorner> {
    let mut gt = Vec::new();
    for j in 0..(SQUARES_Y as i32 - 1) {
        for i in 0..(SQUARES_X as i32 - 1) {
            gt.push(GtCorner {
                index: (i, j),
                board_mm: Point2::new(f64::from(i + 1) * SQUARE_MM, f64::from(j + 1) * SQUARE_MM),
            });
        }
    }
    gt
}

fn charuco_target() -> CharucoTargetSpec {
    let dict = resolve_dictionary(DICTIONARY).expect("builtin dictionary");
    CharucoTargetSpec::new(SQUARES_Y, SQUARES_X, SQUARE_MM, 0.75, dict)
        .with_marker_layout(MarkerLayout::OpenCvCharuco)
}

fn charuco_detector_board() -> CharucoBoardSpec {
    let dict = resolve_dictionary(DICTIONARY).expect("builtin dictionary");
    CharucoBoardSpec::new(SQUARES_Y, SQUARES_X, SQUARE_MM as f32, 0.75, dict)
        .with_marker_layout(MarkerLayout::OpenCvCharuco)
}

fn marker_target() -> MarkerBoardTargetSpec {
    let (rows, cols) = (SQUARES_Y - 1, SQUARES_X - 1);
    MarkerBoardTargetSpec::new(
        rows,
        cols,
        SQUARE_MM,
        MarkerBoardTargetSpec::default_circles(rows, cols),
    )
}

impl Board {
    /// The suite's board for `family`.
    pub fn new(family: Family) -> Self {
        let (target, gt) = match family {
            Family::Chessboard => (
                TargetSpec::Chessboard(ChessboardTargetSpec::new(
                    SQUARES_Y - 1,
                    SQUARES_X - 1,
                    SQUARE_MM,
                )),
                inner_grid_truth(),
            ),
            Family::Charuco => {
                let detector =
                    CharucoDetector::new(CharucoParams::for_board(charuco_detector_board()))
                        .expect("valid ChArUco board");
                let board = detector.board();
                let count = (SQUARES_X - 1) * (SQUARES_Y - 1);
                let gt = (0..count)
                    .map(|id| {
                        let p = board.charuco_object_xy(id).expect("inner corner id");
                        GtCorner {
                            index: ((id % (SQUARES_X - 1)) as i32, (id / (SQUARES_X - 1)) as i32),
                            board_mm: Point2::new(f64::from(p.x), f64::from(p.y)),
                        }
                    })
                    .collect();
                (TargetSpec::Charuco(charuco_target()), gt)
            }
            Family::PuzzleBoard => (
                TargetSpec::PuzzleBoard(PuzzleBoardTargetSpec::new(
                    SQUARES_Y, SQUARES_X, SQUARE_MM,
                )),
                inner_grid_truth(),
            ),
            Family::MarkerBoard => (TargetSpec::MarkerBoard(marker_target()), inner_grid_truth()),
        };
        let (w, h) = target.board_size_mm().expect("valid board");
        let prims = board_primitives(&target).expect("valid board");
        Self {
            family,
            size_mm: [w, h],
            scene: Scene::new(&prims, w, h, SQUARE_MM),
            gt,
            target,
        }
    }

    /// The family's default corner front-end with `refine` applied.
    fn front_end(&self, refine: &dyn Fn(DetectorConfig) -> DetectorConfig) -> DetectorConfig {
        let base = match self.family {
            Family::Chessboard => default_chess_config(),
            Family::Charuco => CharucoParams::for_board(charuco_detector_board()).chess,
            Family::PuzzleBoard => PuzzleBoardParams::for_board(self.puzzle_spec()).chess,
            Family::MarkerBoard => MarkerBoardParams::for_board(self.marker_spec()).chess,
        };
        refine(base)
    }

    fn puzzle_spec(&self) -> PuzzleBoardSpec {
        PuzzleBoardSpec::with_origin(SQUARES_Y, SQUARES_X, SQUARE_MM as f32, 0, 0)
            .expect("valid PuzzleBoard")
    }

    fn marker_spec(&self) -> MarkerBoardSpec {
        let TargetSpec::MarkerBoard(t) = &self.target else {
            unreachable!("marker_spec on a non-marker board")
        };
        MarkerBoardSpec::new(
            t.inner_rows,
            t.inner_cols,
            t.circles.map(|c| c.to_detector_spec()),
        )
        .with_cell_size(SQUARE_MM as f32)
        .with_circle_diameter_rel(t.circle_diameter_rel as f32)
    }

    /// Detect on `image` with the family's default parameters, the corner
    /// front-end passed through `refine` (the refiner under test). `Err` is a
    /// miss, with the detector's reason.
    pub fn detect(
        &self,
        image: &Rendered,
        refine: &dyn Fn(DetectorConfig) -> DetectorConfig,
    ) -> Result<FamilyRun, String> {
        let chess = self.front_end(refine);
        let view = GrayImageView {
            width: image.width,
            height: image.height,
            data: &image.pixels,
        };
        match self.family {
            Family::Chessboard => {
                let img = image::GrayImage::from_raw(
                    image.width as u32,
                    image.height as u32,
                    image.pixels.clone(),
                )
                .expect("buffer matches its size");
                let d = detect_chessboard(&img, &chess, &ChessboardParams::default())
                    .map_err(|e| e.to_string())?;
                Ok(FamilyRun {
                    delivered: d
                        .corners
                        .iter()
                        .map(|c| Delivered {
                            position: c.position,
                            covariance: c.position_covariance,
                            grid: c.grid,
                            claim_mm: None,
                            id_consistent: true,
                        })
                        .collect(),
                    quality: d.quality,
                })
            }
            Family::Charuco => {
                let mut params = CharucoParams::for_board(charuco_detector_board());
                params.chess = chess;
                let detector = CharucoDetector::new(params).map_err(|e| e.to_string())?;
                let d = detector.detect(&view).map_err(|e| e.to_string())?;
                let board = detector.board();
                Ok(FamilyRun {
                    delivered: d
                        .corners
                        .iter()
                        .map(|c| Delivered {
                            position: c.position,
                            covariance: c.position_covariance,
                            grid: c.grid,
                            claim_mm: Some(c.target_position),
                            id_consistent: board
                                .charuco_object_xy(c.id)
                                .is_some_and(|p| (p - c.target_position).norm() < 1e-3),
                        })
                        .collect(),
                    quality: d.quality,
                })
            }
            Family::PuzzleBoard => {
                let mut params = PuzzleBoardParams::for_board(self.puzzle_spec());
                params.chess = chess;
                let detector = PuzzleBoardDetector::new(params).map_err(|e| e.to_string())?;
                let d = detector.detect(&view).map_err(|e| e.to_string())?;
                let cell = SQUARE_MM as f32;
                Ok(FamilyRun {
                    delivered: d
                        .corners
                        .iter()
                        .map(|c| {
                            let from_id =
                                Point2::new((c.id % 501) as f32 * cell, (c.id / 501) as f32 * cell);
                            Delivered {
                                position: c.position,
                                covariance: c.position_covariance,
                                grid: c.grid,
                                claim_mm: Some(c.target_position),
                                id_consistent: (from_id - c.target_position).norm() < 1e-3,
                            }
                        })
                        .collect(),
                    quality: d.quality,
                })
            }
            Family::MarkerBoard => {
                let mut params = MarkerBoardParams::for_board(self.marker_spec());
                params.chess = chess;
                let detector = MarkerBoardDetector::new(params).map_err(|e| e.to_string())?;
                let d = detector.detect(&view).map_err(|e| e.to_string())?;
                Ok(FamilyRun {
                    delivered: d
                        .corners
                        .iter()
                        .map(|c| Delivered {
                            position: c.position,
                            covariance: c.position_covariance,
                            grid: c.grid,
                            claim_mm: c.target_position,
                            id_consistent: true,
                        })
                        .collect(),
                    quality: d.quality,
                })
            }
        }
    }
}
