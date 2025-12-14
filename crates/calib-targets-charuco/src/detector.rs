use std::collections::HashMap;

use calib_targets_aruco::{
    scan_decode_markers, Dictionary, MarkerDetection, Matcher, ScanDecodeConfig,
};
use calib_targets_chessboard::{
    rectify_mesh_from_grid, ChessboardDetector, ChessboardParams, GridGraphParams, MeshWarpError,
    RectifiedMeshView,
};
use calib_targets_core::{
    Corner, GrayImageView, GridCoords, LabeledCorner, TargetDetection, TargetKind,
};
use nalgebra::Point2;

/// Marker placement scheme for the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerLayout {
    /// OpenCV-style ChArUco layout:
    /// - markers are placed on white squares only (assuming top-left square is black),
    /// - marker IDs are assigned sequentially in row-major order over those squares.
    OpenCvCharuco,
}

/// Static ChArUco board specification.
///
/// `rows`/`cols` are **square counts** (not inner corner counts).
#[derive(Clone, Copy, Debug)]
pub struct CharucoBoardSpec {
    pub rows: u32,
    pub cols: u32,
    pub cell_size: f32,
    pub marker_size_rel: f32,
    pub dictionary: Dictionary,
    pub marker_layout: MarkerLayout,
}

#[derive(thiserror::Error, Debug)]
pub enum CharucoBoardError {
    #[error("rows and cols must be >= 2")]
    InvalidSize,
    #[error("cell_size must be > 0")]
    InvalidCellSize,
    #[error("marker_size_rel must be in (0, 1]")]
    InvalidMarkerSizeRel,
    #[error("dictionary has no codes")]
    EmptyDictionary,
    #[error("board needs {needed} markers, dictionary has {available}")]
    NotEnoughDictionaryCodes { needed: usize, available: usize },
}

/// Precomputed board mapping helpers.
#[derive(Clone, Debug)]
pub struct CharucoBoard {
    spec: CharucoBoardSpec,
    marker_positions: Vec<[i32; 2]>,
}

impl CharucoBoard {
    pub fn new(spec: CharucoBoardSpec) -> Result<Self, CharucoBoardError> {
        if spec.rows < 2 || spec.cols < 2 {
            return Err(CharucoBoardError::InvalidSize);
        }
        if !spec.cell_size.is_finite() || spec.cell_size <= 0.0 {
            return Err(CharucoBoardError::InvalidCellSize);
        }
        if !spec.marker_size_rel.is_finite()
            || spec.marker_size_rel <= 0.0
            || spec.marker_size_rel > 1.0
        {
            return Err(CharucoBoardError::InvalidMarkerSizeRel);
        }
        if spec.dictionary.codes.is_empty() {
            return Err(CharucoBoardError::EmptyDictionary);
        }

        let marker_positions = match spec.marker_layout {
            MarkerLayout::OpenCvCharuco => open_cv_charuco_marker_positions(spec.rows, spec.cols),
        };

        let needed = marker_positions.len();
        let available = spec.dictionary.codes.len();
        if available < needed {
            return Err(CharucoBoardError::NotEnoughDictionaryCodes { needed, available });
        }

        Ok(Self {
            spec,
            marker_positions,
        })
    }

    #[inline]
    pub fn spec(&self) -> CharucoBoardSpec {
        self.spec
    }

    /// Expected number of *inner* chessboard corners in vertical direction.
    #[inline]
    pub fn expected_inner_rows(&self) -> u32 {
        self.spec.rows - 1
    }

    /// Expected number of *inner* chessboard corners in horizontal direction.
    #[inline]
    pub fn expected_inner_cols(&self) -> u32 {
        self.spec.cols - 1
    }

    /// Mapping from marker id -> board cell (square) coordinates.
    #[inline]
    pub fn marker_position(&self, id: u32) -> Option<[i32; 2]> {
        self.marker_positions.get(id as usize).copied()
    }

    /// Convert a board **corner coordinate** `(i, j)` into a ChArUco corner id.
    ///
    /// Returns `None` if the corner is outside the inner corner range.
    pub fn charuco_corner_id_from_board_corner(&self, i: i32, j: i32) -> Option<u32> {
        let cols = i32::try_from(self.spec.cols).ok()?;
        let rows = i32::try_from(self.spec.rows).ok()?;

        if i <= 0 || j <= 0 || i >= cols || j >= rows {
            return None;
        }

        let inner_cols = cols - 1;
        let ii = i - 1;
        let jj = j - 1;
        Some((jj as u32) * (inner_cols as u32) + (ii as u32))
    }

    /// Physical 2D point (board plane) for a ChArUco corner id.
    ///
    /// Coordinates are in the board reference frame with origin at the top-left board corner.
    pub fn charuco_object_xy(&self, id: u32) -> Option<Point2<f32>> {
        let cols = self.spec.cols.checked_sub(1)?; // inner corner cols
        let rows = self.spec.rows.checked_sub(1)?; // inner corner rows
        let count = cols.checked_mul(rows)?;
        if id >= count {
            return None;
        }
        let i = (id % cols) as f32 + 1.0;
        let j = (id / cols) as f32 + 1.0;
        Some(Point2::new(
            i * self.spec.cell_size,
            j * self.spec.cell_size,
        ))
    }
}

fn open_cv_charuco_marker_positions(rows: u32, cols: u32) -> Vec<[i32; 2]> {
    let mut out = Vec::new();
    for j in 0..(rows as i32) {
        for i in 0..(cols as i32) {
            // OpenCV: top-left square is black => white squares have (i+j) odd.
            if ((i + j) & 1) == 1 {
                out.push([i, j]);
            }
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct CharucoDetectorParams {
    pub px_per_square: f32,
    pub chessboard: ChessboardParams,
    pub graph: GridGraphParams,
    pub scan: ScanDecodeConfig,
    pub max_hamming: u8,
    /// Minimal number of marker inliers needed to accept the alignment.
    pub min_marker_inliers: usize,
}

impl CharucoDetectorParams {
    pub fn for_board(board: &CharucoBoard) -> Self {
        let chessboard = ChessboardParams {
            min_corner_strength: 0.5,
            min_corners: 32,
            expected_rows: Some(board.expected_inner_rows()),
            expected_cols: Some(board.expected_inner_cols()),
            completeness_threshold: 0.05,
            ..ChessboardParams::default()
        };

        let graph = GridGraphParams::default();

        let scan = ScanDecodeConfig {
            marker_size_rel: board.spec().marker_size_rel,
            ..ScanDecodeConfig::default()
        };

        let max_hamming = board.spec().dictionary.max_correction_bits.min(2);

        Self {
            px_per_square: 60.0,
            chessboard,
            graph,
            scan,
            max_hamming,
            min_marker_inliers: 8,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum CharucoDetectError {
    #[error("chessboard not detected")]
    ChessboardNotDetected,
    #[error(transparent)]
    MeshWarp(#[from] MeshWarpError),
    #[error("no markers decoded")]
    NoMarkers,
    #[error("marker-to-board alignment failed (inliers={inliers})")]
    AlignmentFailed { inliers: usize },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridTransform {
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
}

impl GridTransform {
    #[inline]
    pub fn apply(&self, i: i32, j: i32) -> [i32; 2] {
        [self.a * i + self.b * j, self.c * i + self.d * j]
    }
}

#[derive(Clone, Debug)]
pub struct CharucoAlignment {
    pub transform: GridTransform,
    pub translation: [i32; 2],
    pub marker_inliers: Vec<usize>,
}

impl CharucoAlignment {
    #[inline]
    pub fn map(&self, i: i32, j: i32) -> [i32; 2] {
        let [x, y] = self.transform.apply(i, j);
        [x + self.translation[0], y + self.translation[1]]
    }
}

#[derive(Clone, Debug)]
pub struct CharucoDetectionResult {
    pub detection: TargetDetection,
    pub chessboard: TargetDetection,
    pub chessboard_inliers: Vec<usize>,
    pub markers: Vec<MarkerDetection>,
    pub alignment: CharucoAlignment,
    pub rectified: RectifiedMeshView,
}

pub struct CharucoDetector {
    board: CharucoBoard,
    params: CharucoDetectorParams,
    matcher: Matcher,
}

impl CharucoDetector {
    pub fn new(board: CharucoBoard, mut params: CharucoDetectorParams) -> Self {
        if params.chessboard.expected_rows.is_none() {
            params.chessboard.expected_rows = Some(board.expected_inner_rows());
        }
        if params.chessboard.expected_cols.is_none() {
            params.chessboard.expected_cols = Some(board.expected_inner_cols());
        }
        params.scan.marker_size_rel = board.spec().marker_size_rel;

        let max_hamming = params
            .max_hamming
            .min(board.spec().dictionary.max_correction_bits);
        params.max_hamming = max_hamming;

        let matcher = Matcher::new(board.spec().dictionary, max_hamming);

        Self {
            board,
            params,
            matcher,
        }
    }

    #[inline]
    pub fn board(&self) -> &CharucoBoard {
        &self.board
    }

    #[inline]
    pub fn params(&self) -> &CharucoDetectorParams {
        &self.params
    }

    pub fn detect(
        &self,
        image: &GrayImageView<'_>,
        corners: &[Corner],
    ) -> Result<CharucoDetectionResult, CharucoDetectError> {
        let detector = ChessboardDetector::new(self.params.chessboard.clone())
            .with_grid_search(self.params.graph.clone());
        let chessboard = detector
            .detect_from_corners(corners)
            .ok_or(CharucoDetectError::ChessboardNotDetected)?;

        let rectified = rectify_mesh_from_grid(
            image,
            &chessboard.detection.corners,
            &chessboard.inliers,
            self.params.px_per_square,
        )?;

        let rect_view = GrayImageView {
            width: rectified.rect.width,
            height: rectified.rect.height,
            data: &rectified.rect.data,
        };

        let markers = scan_decode_markers(
            &rect_view,
            rectified.cells_x,
            rectified.cells_y,
            rectified.px_per_square,
            &self.params.scan,
            &self.matcher,
        );

        if markers.is_empty() {
            return Err(CharucoDetectError::NoMarkers);
        }

        let alignment = solve_alignment(&self.board, &markers)
            .ok_or(CharucoDetectError::AlignmentFailed { inliers: 0usize })?;

        if alignment.marker_inliers.len() < self.params.min_marker_inliers {
            return Err(CharucoDetectError::AlignmentFailed {
                inliers: alignment.marker_inliers.len(),
            });
        }

        let detection = map_charuco_corners(&self.board, &chessboard.detection, &alignment);

        Ok(CharucoDetectionResult {
            detection,
            chessboard: chessboard.detection,
            chessboard_inliers: chessboard.inliers,
            markers,
            alignment,
            rectified,
        })
    }
}

fn solve_alignment(board: &CharucoBoard, markers: &[MarkerDetection]) -> Option<CharucoAlignment> {
    #[derive(Clone, Copy)]
    struct Pair {
        idx: usize,
        sx: i32,
        sy: i32,
        ex: i32,
        ey: i32,
    }

    let pairs: Vec<Pair> = markers
        .iter()
        .enumerate()
        .filter_map(|(idx, m)| {
            board.marker_position(m.id).map(|[ex, ey]| Pair {
                idx,
                sx: m.sx,
                sy: m.sy,
                ex,
                ey,
            })
        })
        .collect();

    if pairs.is_empty() {
        return None;
    }

    let transforms = [
        GridTransform {
            a: 1,
            b: 0,
            c: 0,
            d: 1,
        },
        GridTransform {
            a: 0,
            b: 1,
            c: -1,
            d: 0,
        },
        GridTransform {
            a: -1,
            b: 0,
            c: 0,
            d: -1,
        },
        GridTransform {
            a: 0,
            b: -1,
            c: 1,
            d: 0,
        },
        GridTransform {
            a: -1,
            b: 0,
            c: 0,
            d: 1,
        },
        GridTransform {
            a: 1,
            b: 0,
            c: 0,
            d: -1,
        },
        GridTransform {
            a: 0,
            b: 1,
            c: 1,
            d: 0,
        },
        GridTransform {
            a: 0,
            b: -1,
            c: -1,
            d: 0,
        },
    ];

    let mut best: Option<(usize, GridTransform, [i32; 2], Vec<usize>)> = None;

    for transform in transforms {
        let mut counts: HashMap<[i32; 2], usize> = HashMap::new();
        for p in &pairs {
            let [rx, ry] = transform.apply(p.sx, p.sy);
            let t = [p.ex - rx, p.ey - ry];
            *counts.entry(t).or_insert(0) += 1;
        }

        let (translation, _) = counts.into_iter().max_by_key(|(_, c)| *c)?;

        let mut inliers = Vec::new();
        for p in &pairs {
            let [x, y] = transform.apply(p.sx, p.sy);
            if x + translation[0] == p.ex && y + translation[1] == p.ey {
                inliers.push(p.idx);
            }
        }

        let candidate = (inliers.len(), transform, translation, inliers);
        match best {
            None => best = Some(candidate),
            Some((best_n, _, _, _)) => {
                if candidate.0 > best_n {
                    best = Some(candidate);
                }
            }
        }
    }

    let (_, transform, translation, marker_inliers) = best?;
    Some(CharucoAlignment {
        transform,
        translation,
        marker_inliers,
    })
}

fn map_charuco_corners(
    board: &CharucoBoard,
    chessboard: &TargetDetection,
    alignment: &CharucoAlignment,
) -> TargetDetection {
    let mut corners = Vec::new();

    for c in &chessboard.corners {
        let Some(g) = c.grid else {
            continue;
        };

        let [bi, bj] = alignment.map(g.i, g.j);
        let Some(id) = board.charuco_corner_id_from_board_corner(bi, bj) else {
            continue;
        };

        corners.push(LabeledCorner {
            position: c.position,
            grid: Some(GridCoords {
                i: bi - 1,
                j: bj - 1,
            }),
            id: Some(id),
            confidence: c.confidence,
        });
    }

    corners.sort_by_key(|c| c.id.unwrap_or(u32::MAX));

    TargetDetection {
        kind: TargetKind::Charuco,
        corners,
    }
}
