//! ChArUco detection pipeline.

use crate::alignment::{map_charuco_corners, solve_alignment, CharucoAlignment};
use crate::board::{CharucoBoard, MarkerLayout};
use calib_targets_aruco::{
    scan_decode_markers, scan_decode_markers_in_cells, MarkerCell, MarkerDetection, Matcher,
    ScanDecodeConfig,
};
use calib_targets_chessboard::{
    rectify_mesh_from_grid, ChessboardDetector, ChessboardParams, GridGraphParams, MeshWarpError,
    RectifiedMeshView,
};
use calib_targets_core::{Corner, GrayImageView, GridCoords, LabeledCorner, TargetDetection};
use nalgebra::Point2;
use std::collections::HashMap;

/// Configuration for the ChArUco detector.
#[derive(Clone, Debug)]
pub struct CharucoDetectorParams {
    /// Pixels per board square in the canonical sampling space.
    pub px_per_square: f32,
    /// Chessboard detection parameters.
    pub chessboard: ChessboardParams,
    /// Grid graph parameters.
    pub graph: GridGraphParams,
    /// Marker scan parameters.
    pub scan: ScanDecodeConfig,
    /// Maximum Hamming distance for marker matching.
    pub max_hamming: u8,
    /// Minimal number of marker inliers needed to accept the alignment.
    pub min_marker_inliers: usize,
    /// If true, build a full rectified mesh image for output/debugging.
    /// This is more expensive than per-cell decoding.
    pub build_rectified_image: bool,
    /// If true, fall back to full rectified decoding when per-cell alignment is weak.
    pub fallback_to_rectified: bool,
}

impl CharucoDetectorParams {
    /// Build a reasonable default configuration for the given board.
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
            build_rectified_image: false,
            fallback_to_rectified: true,
        }
    }
}

/// Errors returned by the ChArUco detector.
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

/// Output of a ChArUco detection run.
#[derive(Clone, Debug)]
pub struct CharucoDetectionResult {
    pub detection: TargetDetection,
    pub chessboard: TargetDetection,
    pub chessboard_inliers: Vec<usize>,
    pub markers: Vec<calib_targets_aruco::MarkerDetection>,
    pub alignment: CharucoAlignment,
    /// Optional rectified mesh view (built only if requested).
    pub rectified: Option<RectifiedMeshView>,
}

/// Grid-first ChArUco detector.
pub struct CharucoDetector {
    board: CharucoBoard,
    params: CharucoDetectorParams,
    matcher: Matcher,
}

impl CharucoDetector {
    /// Create a detector for a given board and parameters.
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

    /// Board definition used by the detector.
    #[inline]
    pub fn board(&self) -> &CharucoBoard {
        &self.board
    }

    /// Detector parameters.
    #[inline]
    pub fn params(&self) -> &CharucoDetectorParams {
        &self.params
    }

    /// Detect a ChArUco board from an image and a set of corners.
    ///
    /// This uses per-cell marker sampling by default. Set
    /// `build_rectified_image` if you need a rectified output image.
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

        let corner_map = build_corner_map(&chessboard.detection.corners, &chessboard.inliers);
        let cells = build_marker_cells(&corner_map);

        let mut scan_cfg = self.params.scan.clone();
        scan_cfg.dedup_by_id = false;

        let markers = scan_decode_markers_in_cells(
            image,
            &cells,
            self.params.px_per_square,
            &scan_cfg,
            &self.matcher,
        );

        if markers.is_empty() {
            return Err(CharucoDetectError::NoMarkers);
        }

        let mut rectified_for_output = None;
        let (mut markers, mut alignment) = select_alignment(&self.board, markers)
            .ok_or(CharucoDetectError::AlignmentFailed { inliers: 0usize })?;

        if alignment.marker_inliers.len() < self.params.min_marker_inliers
            && self.params.fallback_to_rectified
        {
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
            let rect_markers = scan_decode_markers(
                &rect_view,
                rectified.cells_x,
                rectified.cells_y,
                rectified.px_per_square,
                &scan_cfg,
                &self.matcher,
            );
            if let Some((m, a)) = select_alignment(&self.board, rect_markers) {
                markers = m;
                alignment = a;
                rectified_for_output = Some(rectified);
            }
        }

        if alignment.marker_inliers.len() < self.params.min_marker_inliers {
            return Err(CharucoDetectError::AlignmentFailed {
                inliers: alignment.marker_inliers.len(),
            });
        }

        let detection = map_charuco_corners(&self.board, &chessboard.detection, &alignment);

        let rectified = if self.params.build_rectified_image && rectified_for_output.is_none() {
            Some(rectify_mesh_from_grid(
                image,
                &chessboard.detection.corners,
                &chessboard.inliers,
                self.params.px_per_square,
            )?)
        } else {
            rectified_for_output
        };

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

fn build_corner_map(
    corners: &[LabeledCorner],
    inliers: &[usize],
) -> HashMap<GridCoords, Point2<f32>> {
    let mut map = HashMap::new();
    for &idx in inliers {
        if let Some(c) = corners.get(idx) {
            if let Some(g) = c.grid {
                map.insert(g, c.position);
            }
        }
    }
    map
}

fn build_marker_cells(map: &HashMap<GridCoords, Point2<f32>>) -> Vec<MarkerCell> {
    let mut min_i = i32::MAX;
    let mut min_j = i32::MAX;
    let mut max_i = i32::MIN;
    let mut max_j = i32::MIN;

    for g in map.keys() {
        min_i = min_i.min(g.i);
        min_j = min_j.min(g.j);
        max_i = max_i.max(g.i);
        max_j = max_j.max(g.j);
    }

    if min_i == i32::MAX || min_j == i32::MAX {
        return Vec::new();
    }

    let cells_x = (max_i - min_i).max(0) as usize;
    let cells_y = (max_j - min_j).max(0) as usize;
    let mut out = Vec::with_capacity(cells_x * cells_y);
    for j in min_j..max_j {
        for i in min_i..max_i {
            let g00 = GridCoords { i, j };
            let g10 = GridCoords { i: i + 1, j };
            let g11 = GridCoords { i: i + 1, j: j + 1 };
            let g01 = GridCoords { i, j: j + 1 };

            let (Some(&p00), Some(&p10), Some(&p11), Some(&p01)) =
                (map.get(&g00), map.get(&g10), map.get(&g11), map.get(&g01))
            else {
                continue;
            };

            out.push(MarkerCell {
                sx: i,
                sy: j,
                corners_img: [p00, p10, p11, p01],
            });
        }
    }

    out
}

fn select_alignment(
    board: &CharucoBoard,
    markers: Vec<MarkerDetection>,
) -> Option<(Vec<MarkerDetection>, CharucoAlignment)> {
    let mut candidates: Vec<(usize, CharucoAlignment, Vec<MarkerDetection>)> = Vec::new();

    if let Some(alignment) = solve_alignment(board, &markers) {
        candidates.push((alignment.marker_inliers.len(), alignment, markers.clone()));
    }

    if board.spec().marker_layout == MarkerLayout::OpenCvCharuco {
        let even = markers
            .iter()
            .cloned()
            .filter(|m| ((m.sx + m.sy) & 1) == 0)
            .collect::<Vec<_>>();
        if let Some(alignment) = solve_alignment(board, &even) {
            candidates.push((alignment.marker_inliers.len(), alignment, even));
        }

        let odd = markers
            .iter()
            .cloned()
            .filter(|m| ((m.sx + m.sy) & 1) != 0)
            .collect::<Vec<_>>();
        if let Some(alignment) = solve_alignment(board, &odd) {
            candidates.push((alignment.marker_inliers.len(), alignment, odd));
        }
    }

    candidates
        .into_iter()
        .max_by_key(|(inliers, _, _)| *inliers)
        .map(|(_, alignment, markers)| (markers, alignment))
}
