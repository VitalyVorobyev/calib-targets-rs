//! End-to-end PuzzleBoard detection pipeline.

use calib_targets_chessboard::{ChessboardDetectionResult, ChessboardDetector};
use calib_targets_core::{
    Corner, GrayImageView, GridCoords, LabeledCorner, TargetDetection, TargetKind,
};
use nalgebra::Point2;

use crate::board::{PuzzleBoardSpec, PuzzleBoardSpecError, MASTER_COLS};
use crate::code_maps::ObservedEdge;
use crate::detector::decode::decode as run_decode;
use crate::detector::edge_sampling::{
    corner_at, local_cell_references, observed_horizontal_edge, observed_vertical_edge,
    sample_edge_bit,
};
use crate::detector::error::PuzzleBoardDetectError;
use crate::detector::result::{PuzzleBoardDecodeInfo, PuzzleBoardDetectionResult};
use crate::params::PuzzleBoardParams;

/// Owned PuzzleBoard detector.
pub struct PuzzleBoardDetector {
    params: PuzzleBoardParams,
    chessboard: ChessboardDetector,
}

impl PuzzleBoardDetector {
    pub fn new(params: PuzzleBoardParams) -> Result<Self, PuzzleBoardSpecError> {
        let _ = PuzzleBoardSpec::with_origin(
            params.board.rows,
            params.board.cols,
            params.board.cell_size,
            params.board.origin_row,
            params.board.origin_col,
        )?;
        let chessboard = ChessboardDetector::new(params.chessboard.clone());
        Ok(Self { params, chessboard })
    }

    pub fn params(&self) -> &PuzzleBoardParams {
        &self.params
    }

    pub fn detect(
        &self,
        image: &GrayImageView<'_>,
        corners: &[Corner],
    ) -> Result<PuzzleBoardDetectionResult, PuzzleBoardDetectError> {
        let chess_results = self.chessboard.detect_all_from_corners(corners);
        if chess_results.is_empty() {
            return Err(PuzzleBoardDetectError::ChessboardNotDetected);
        }

        let mut last_err: Option<PuzzleBoardDetectError> = None;
        let mut best: Option<PuzzleBoardDetectionResult> = None;

        for chess in &chess_results {
            match self.decode_component(image, chess) {
                Ok(result) => {
                    let better = match &best {
                        None => true,
                        Some(b) => {
                            result.decode.edges_matched > b.decode.edges_matched
                                || (result.decode.edges_matched == b.decode.edges_matched
                                    && result.decode.mean_confidence > b.decode.mean_confidence)
                        }
                    };
                    if better {
                        best = Some(result);
                    }
                    if !self.params.decode.search_all_components {
                        break;
                    }
                }
                Err(e) => last_err = Some(e),
            }
        }

        best.ok_or_else(|| last_err.unwrap_or(PuzzleBoardDetectError::DecodeFailed))
    }

    fn decode_component(
        &self,
        image: &GrayImageView<'_>,
        chess: &ChessboardDetectionResult,
    ) -> Result<PuzzleBoardDetectionResult, PuzzleBoardDetectError> {
        let labeled: &[LabeledCorner] = &chess.detection.corners;
        let inliers: &[usize] = &chess.inliers;

        let observed = self.sample_all_edges(image, labeled, inliers);
        let min_edges = required_edges(self.params.decode.min_window);
        ensure_min_edges(observed.len(), min_edges)?;

        let filtered: Vec<ObservedEdge> = observed
            .iter()
            .copied()
            .filter(|e| e.confidence >= self.params.decode.min_bit_confidence)
            .collect();
        ensure_min_edges(filtered.len(), min_edges)?;

        let decoded = run_decode(&filtered, self.params.decode.max_bit_error_rate)
            .ok_or(PuzzleBoardDetectError::DecodeFailed)?;

        let mut out_corners: Vec<LabeledCorner> = Vec::with_capacity(labeled.len());
        for (idx, lc) in labeled.iter().enumerate() {
            if !inliers.contains(&idx) {
                continue;
            }
            let Some(grid) = lc.grid else {
                continue;
            };
            let [master_i, master_j] = decoded.alignment.map(grid.i, grid.j);
            let id = master_ij_to_id(master_i, master_j);
            let target = master_target_position(master_i, master_j, self.params.board.cell_size);
            out_corners.push(LabeledCorner {
                position: lc.position,
                grid: Some(GridCoords {
                    i: master_i,
                    j: master_j,
                }),
                id: Some(id),
                target_position: Some(target),
                score: lc.score,
            });
        }

        let detection = TargetDetection {
            kind: TargetKind::PuzzleBoard,
            corners: out_corners,
        };
        let decode_info = PuzzleBoardDecodeInfo {
            edges_observed: decoded.edges_observed,
            edges_matched: decoded.edges_matched,
            mean_confidence: decoded.mean_confidence,
            bit_error_rate: decoded.bit_error_rate,
            master_origin_row: decoded.master_origin_row,
            master_origin_col: decoded.master_origin_col,
        };

        Ok(PuzzleBoardDetectionResult {
            detection,
            alignment: decoded.alignment,
            decode: decode_info,
            observed_edges: observed,
        })
    }

    fn sample_all_edges(
        &self,
        image: &GrayImageView<'_>,
        corners: &[LabeledCorner],
        inliers: &[usize],
    ) -> Vec<ObservedEdge> {
        let mut out = Vec::with_capacity(inliers.len() * 2);
        let radius = self.params.decode.sample_radius_rel;

        let in_set: std::collections::HashSet<usize> = inliers.iter().copied().collect();

        // Convention: `GridCoords.i` = column, `.j` = row.
        // - Horizontal edge at (row, col): between corner (col, row) and (col+1, row),
        //   separates squares (row-1, col) and (row, col). Reads from map A.
        // - Vertical   edge at (row, col): between corner (col, row) and (col, row+1),
        //   separates squares (row, col-1) and (row, col). Reads from map B.
        for (idx, lc) in corners.iter().enumerate() {
            if !in_set.contains(&idx) {
                continue;
            }
            let Some(grid) = lc.grid else {
                continue;
            };
            let r = grid.j;
            let c = grid.i;

            // Rightward horizontal edge.
            if let Some(right) = corner_at(corners, c + 1, r) {
                if let (Some(top_left), Some(top_right), Some(bot_right), Some(bot_left)) = (
                    corner_at(corners, c, r - 1),
                    corner_at(corners, c + 1, r - 1),
                    corner_at(corners, c + 1, r + 1),
                    corner_at(corners, c, r + 1),
                ) {
                    let (bright, dark) = local_cell_references(
                        image,
                        [
                            top_left.position,
                            top_right.position,
                            lc.position,
                            right.position,
                        ],
                        [
                            lc.position,
                            right.position,
                            bot_right.position,
                            bot_left.position,
                        ],
                    );
                    let (bit, conf) =
                        sample_edge_bit(image, lc.position, right.position, bright, dark, radius);
                    out.push(observed_horizontal_edge(r, c, bit, conf));
                }
            }

            // Downward vertical edge.
            if let Some(below) = corner_at(corners, c, r + 1) {
                if let (Some(tl), Some(tr), Some(br), Some(bl)) = (
                    corner_at(corners, c - 1, r),
                    corner_at(corners, c + 1, r),
                    corner_at(corners, c + 1, r + 1),
                    corner_at(corners, c - 1, r + 1),
                ) {
                    let (bright, dark) = local_cell_references(
                        image,
                        [tl.position, lc.position, below.position, bl.position],
                        [lc.position, tr.position, br.position, below.position],
                    );
                    let (bit, conf) =
                        sample_edge_bit(image, lc.position, below.position, bright, dark, radius);
                    out.push(observed_vertical_edge(r, c, bit, conf));
                }
            }
        }
        out
    }
}

fn required_edges(min_window: u32) -> usize {
    let w = min_window.max(3) as usize;
    2 * w * (w - 1)
}

fn ensure_min_edges(observed: usize, needed: usize) -> Result<(), PuzzleBoardDetectError> {
    if observed < needed {
        return Err(PuzzleBoardDetectError::NotEnoughEdges { observed, needed });
    }
    Ok(())
}

fn master_ij_to_id(master_i: i32, master_j: i32) -> u32 {
    let cols = MASTER_COLS as i32;
    let i = master_i.rem_euclid(cols);
    let j = master_j.rem_euclid(cols);
    (j as u32) * (cols as u32) + (i as u32)
}

fn master_target_position(master_i: i32, master_j: i32, cell_size: f32) -> Point2<f32> {
    Point2::new(master_i as f32 * cell_size, master_j as f32 * cell_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_edges_scales_with_window() {
        assert_eq!(required_edges(3), 12);
        assert_eq!(required_edges(4), 24);
        assert_eq!(required_edges(5), 40);
    }

    #[test]
    fn min_edges_check_reports_filtered_count() {
        let err = ensure_min_edges(7, required_edges(4)).expect_err("too few edges");
        assert!(matches!(
            err,
            PuzzleBoardDetectError::NotEnoughEdges {
                observed: 7,
                needed: 24
            }
        ));
    }
}
