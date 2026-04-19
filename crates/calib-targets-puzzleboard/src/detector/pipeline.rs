//! End-to-end PuzzleBoard detection pipeline.

use calib_targets_chessboard::{Detection as ChessDetection, Detector as ChessDetector};
use calib_targets_core::{
    Corner, GrayImageView, GridCoords, LabeledCorner, TargetDetection, TargetKind,
};
use nalgebra::Point2;

use crate::board::{PuzzleBoardSpec, PuzzleBoardSpecError, MASTER_COLS, MASTER_ROWS};
use crate::code_maps::PuzzleBoardObservedEdge;
use crate::detector::decode::{decode as run_decode, decode_fixed_board};
use crate::detector::edge_sampling::{
    corner_at_map, local_cell_references, observed_horizontal_edge, observed_vertical_edge,
    sample_edge_bit,
};
use crate::detector::error::PuzzleBoardDetectError;
use crate::detector::params::{ensure_min_edges, required_edges, PuzzleBoardSearchMode};
use crate::detector::result::{PuzzleBoardDecodeInfo, PuzzleBoardDetectionResult};
use crate::params::PuzzleBoardParams;

/// Owned PuzzleBoard detector.
pub struct PuzzleBoardDetector {
    params: PuzzleBoardParams,
    chessboard: ChessDetector,
}

impl PuzzleBoardDetector {
    /// Create a new detector from the given parameters.
    ///
    /// Validates the board geometry and returns
    /// [`PuzzleBoardSpecError`] if `params.board` is invalid (e.g. rows or
    /// cols are zero, or the master origin is out of range).
    pub fn new(params: PuzzleBoardParams) -> Result<Self, PuzzleBoardSpecError> {
        let _ = PuzzleBoardSpec::with_origin(
            params.board.rows,
            params.board.cols,
            params.board.cell_size,
            params.board.origin_row,
            params.board.origin_col,
        )?;
        let chessboard = ChessDetector::new(params.chessboard.clone());
        Ok(Self { params, chessboard })
    }

    /// Return a reference to the detector parameters.
    pub fn params(&self) -> &PuzzleBoardParams {
        &self.params
    }

    /// Detect a PuzzleBoard in `image` using raw ChESS corner features.
    ///
    /// # Arguments
    ///
    /// - `image` — greyscale image view; **not** processed to extract corners.
    /// - `corners` — raw ChESS corner detections (subpixel position + strength);
    ///   typically obtained from `chess_corners::detect_corners` or the facade
    ///   helper `detect::detect_corners`. The detector will internally refine
    ///   them into a chessboard grid.
    ///
    /// # Errors
    ///
    /// - [`PuzzleBoardDetectError::ChessboardNotDetected`] — the chessboard
    ///   stage found no usable grid components in `corners`.
    /// - [`PuzzleBoardDetectError::NotEnoughEdges`] — fewer than
    ///   `params.decode.min_window²` interior edges were observable.
    /// - [`PuzzleBoardDetectError::DecodeFailed`] — no master-pattern origin
    ///   produced a bit-error-rate below `params.decode.max_bit_error_rate`.
    /// - [`PuzzleBoardDetectError::InconsistentPosition`] — two independent
    ///   grid components decoded to different master origins (only possible
    ///   when `params.decode.search_all_components` is `true`).
    ///
    /// # Tie-breaking
    ///
    /// When `params.decode.search_all_components` is `true`, all chessboard
    /// components are decoded and the one with the highest `edges_matched`
    /// count (or `mean_confidence` on a tie) is returned, unless two
    /// successful decodes disagree on the master origin, in which case
    /// `InconsistentPosition` is returned instead.
    pub fn detect(
        &self,
        image: &GrayImageView<'_>,
        corners: &[Corner],
    ) -> Result<PuzzleBoardDetectionResult, PuzzleBoardDetectError> {
        let chess_results = self.chessboard.detect_all(corners);
        if chess_results.is_empty() {
            return Err(PuzzleBoardDetectError::ChessboardNotDetected);
        }

        let mut last_err: Option<PuzzleBoardDetectError> = None;
        let mut best: Option<PuzzleBoardDetectionResult> = None;
        let min_edges = required_edges(self.params.decode.min_window);

        for chess in &chess_results {
            match self.decode_component(image, chess) {
                Ok(result) => {
                    // When searching all components, check for a master-origin
                    // conflict: two well-supported decodes that disagree on
                    // the absolute position (cyclic modulo 501×501).
                    if self.params.decode.search_all_components {
                        if let Some(ref prev) = best {
                            let both_well_supported = prev.decode.edges_matched >= min_edges
                                && result.decode.edges_matched >= min_edges;
                            if both_well_supported
                                && origins_conflict(
                                    prev.decode.master_origin_row,
                                    prev.decode.master_origin_col,
                                    result.decode.master_origin_row,
                                    result.decode.master_origin_col,
                                )
                            {
                                return Err(PuzzleBoardDetectError::InconsistentPosition);
                            }
                        }
                    }

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
        chess: &ChessDetection,
    ) -> Result<PuzzleBoardDetectionResult, PuzzleBoardDetectError> {
        let labeled: &[LabeledCorner] = &chess.target.corners;
        // detector emits only validated corners in `target.corners` — every entry
        // is an inlier by construction. The original inliers index list
        // (subset of v1's pre-quality-filtered corners) is no longer
        // meaningful; we treat every labelled corner as an inlier.
        let inliers: Vec<usize> = (0..labeled.len()).collect();
        let inliers: &[usize] = &inliers;

        let observed = self.sample_all_edges(image, labeled, inliers);
        let min_edges = required_edges(self.params.decode.min_window);
        ensure_min_edges(observed.len(), min_edges)?;

        let filtered: Vec<PuzzleBoardObservedEdge> = observed
            .iter()
            .copied()
            .filter(|e| e.confidence >= self.params.decode.min_bit_confidence)
            .collect();
        ensure_min_edges(filtered.len(), min_edges)?;

        let max_err = self.params.decode.max_bit_error_rate;
        let decoded = match self.params.decode.search_mode {
            PuzzleBoardSearchMode::Full => run_decode(&filtered, max_err),
            PuzzleBoardSearchMode::FixedBoard => decode_fixed_board(
                &filtered,
                self.params.board.origin_row,
                self.params.board.origin_col,
                self.params.board.rows,
                self.params.board.cols,
                max_err,
            ),
        }
        .ok_or(PuzzleBoardDetectError::DecodeFailed)?;

        let mut out_corners: Vec<LabeledCorner> = Vec::with_capacity(labeled.len());
        for (idx, lc) in labeled.iter().enumerate() {
            if !inliers.contains(&idx) {
                continue;
            }
            let Some(grid) = lc.grid else {
                continue;
            };
            let [raw_i, raw_j] = decoded.alignment.map(grid.i, grid.j);
            // Invariant: master coords must be wrapped into [0, 501) so that
            // `target_position == Point2::new((id % 501) * cell, (id / 501) * cell)`
            // holds for every LabeledCorner regardless of which D4 transform was
            // selected. Without wrapping, the 4 D4 transforms with negative a/d
            // entries can produce negative coords that give consistent `id` (via
            // rem_euclid inside master_ij_to_id) but wrong `target_position`.
            let (master_i, master_j) = wrap_master(raw_i, raw_j);
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
    ) -> Vec<PuzzleBoardObservedEdge> {
        let mut out = Vec::with_capacity(inliers.len() * 2);
        let radius = self.params.decode.sample_radius_rel;

        // Build a (i, j) → &LabeledCorner map once for O(1) neighbour lookups.
        let grid_map: std::collections::HashMap<(i32, i32), &LabeledCorner> = corners
            .iter()
            .filter_map(|c| c.grid.map(|g| ((g.i, g.j), c)))
            .collect();

        // Convention: `GridCoords.i` = column, `.j` = row.
        //
        // Edge-coordinate convention (matches PStelldinger/PuzzleBoard authors' convention):
        //
        // The rightward horizontal edge between corners `(c, r)` and `(c+1, r)` is
        // `hbits[r][c]` in the authors' Python code, which maps to master `(pos_row + r, pos_col + c)`.
        // We record it as local `(r, c)` so the decode formula
        //   `horizontal_edge_bit(master_origin_row + r, master_origin_col + c)`
        // correctly recovers the master position when `master_origin = (pos_row, pos_col)`.
        //
        // The downward vertical edge between corners `(c, r)` and `(c, r+1)` is
        // `vbits[r][c]` in the authors' Python code, which maps to master `(pos_row + r, pos_col + c)`.
        // We record it as local `(r, c)` so the decode formula
        //   `vertical_edge_bit(master_origin_row + r, master_origin_col + c)`
        // correctly recovers the master position when `master_origin = (pos_row, pos_col)`.
        for (idx, lc) in corners.iter().enumerate() {
            if !inliers.contains(&idx) {
                continue;
            }
            let Some(grid) = lc.grid else {
                continue;
            };
            let r = grid.j;
            let c = grid.i;

            // Rightward horizontal edge. Records at local (r, c).
            if let Some(right) = corner_at_map(&grid_map, c + 1, r) {
                if let (Some(top_left), Some(top_right), Some(bot_right), Some(bot_left)) = (
                    corner_at_map(&grid_map, c, r - 1),
                    corner_at_map(&grid_map, c + 1, r - 1),
                    corner_at_map(&grid_map, c + 1, r + 1),
                    corner_at_map(&grid_map, c, r + 1),
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

            // Downward vertical edge. Records at local (r, c).
            if let Some(below) = corner_at_map(&grid_map, c, r + 1) {
                if let (Some(tl), Some(tr), Some(br), Some(bl)) = (
                    corner_at_map(&grid_map, c - 1, r),
                    corner_at_map(&grid_map, c + 1, r),
                    corner_at_map(&grid_map, c + 1, r + 1),
                    corner_at_map(&grid_map, c - 1, r + 1),
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

/// Wrap raw master coordinates (which may be negative after a D4 transform with
/// negative a/d entries) into `[0, MASTER_COLS)` so that both `master_ij_to_id`
/// and `master_target_position` receive canonical non-negative inputs.
///
/// Call this **before** both functions to guarantee the invariant:
/// `target_position == Point2::new((id % 501) * cell, (id / 501) * cell)`.
pub(crate) fn wrap_master(master_i: i32, master_j: i32) -> (i32, i32) {
    let cols = MASTER_COLS as i32;
    (master_i.rem_euclid(cols), master_j.rem_euclid(cols))
}

/// Compute the flat corner id from already-wrapped master coordinates in `[0, 501)`.
pub(crate) fn master_ij_to_id(master_i: i32, master_j: i32) -> u32 {
    // Inputs are pre-wrapped by wrap_master; the rem_euclid is a defensive no-op.
    debug_assert!(master_i >= 0 && master_i < MASTER_COLS as i32);
    debug_assert!(master_j >= 0 && master_j < MASTER_COLS as i32);
    (master_j as u32) * MASTER_COLS + (master_i as u32)
}

/// Compute the physical board-frame position from already-wrapped master coordinates.
///
/// Invariant: `target_position.x == (id % 501) as f32 * cell_size`
///            `target_position.y == (id / 501) as f32 * cell_size`.
pub(crate) fn master_target_position(master_i: i32, master_j: i32, cell_size: f32) -> Point2<f32> {
    debug_assert!(master_i >= 0 && master_j >= 0);
    Point2::new(master_i as f32 * cell_size, master_j as f32 * cell_size)
}

/// Return `true` if two master origins are inconsistent (i.e. they map to
/// different positions on the 501×501 cyclic master grid).
fn origins_conflict(row_a: i32, col_a: i32, row_b: i32, col_b: i32) -> bool {
    let ra = row_a.rem_euclid(MASTER_ROWS as i32);
    let ca = col_a.rem_euclid(MASTER_COLS as i32);
    let rb = row_b.rem_euclid(MASTER_ROWS as i32);
    let cb = col_b.rem_euclid(MASTER_COLS as i32);
    (ra, ca) != (rb, cb)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- C1 regression: wrap_master / id / target_position consistency -----------

    /// Verify that wrap_master produces values in [0, 501) for negative inputs
    /// (which arise from D4 transforms with negative a/d entries, e.g. 180°
    /// rotation or reflections).
    #[test]
    fn wrap_master_produces_non_negative_coords() {
        // Simulate what happens with 180° rotation (transform a=-1, d=-1) +
        // a small positive translation: for corner at local (i=2, j=3) and
        // translation (mr=5, mc=5), we get master = (-2+5, -3+5) = (3, 2) — fine.
        // But for corner at (i=10, j=8) with translation (mr=5, mc=5):
        // master = (-10+5, -8+5) = (-5, -3) — negative!
        let (wi, wj) = wrap_master(-5, -3);
        assert!(wi >= 0 && wi < MASTER_COLS as i32, "wi={wi}");
        assert!(wj >= 0 && wj < MASTER_COLS as i32, "wj={wj}");

        // Also test wrap of zero and positive — must be identity.
        assert_eq!(wrap_master(0, 0), (0, 0));
        assert_eq!(wrap_master(100, 250), (100, 250));

        // Test boundary: -1 wraps to 500.
        assert_eq!(wrap_master(-1, -1), (500, 500));

        // Large negatives must still end up in [0, 501).
        let (wi2, wj2) = wrap_master(-1000, -2000);
        assert!(wi2 >= 0 && wi2 < MASTER_COLS as i32, "wi2={wi2}");
        assert!(wj2 >= 0 && wj2 < MASTER_COLS as i32, "wj2={wj2}");
    }

    /// Core invariant: for every LabeledCorner produced via wrap_master +
    /// master_ij_to_id + master_target_position, the following must hold:
    ///   target_position.x == (id % 501) as f32 * cell_size
    ///   target_position.y == (id / 501) as f32 * cell_size
    ///
    /// This is tested with negative raw master coords (as produced by D4
    /// transforms with negative entries) so that the pre-wrap is required.
    #[test]
    fn id_and_target_position_are_consistent_after_wrap() {
        let cell_size = 12.0_f32;
        // Test a range of raw coords including negatives.
        for raw_i in [-503, -250, -1, 0, 1, 100, 499, 500, 501, 1002] {
            for raw_j in [-503, -1, 0, 1, 100, 499, 500] {
                let (mi, mj) = wrap_master(raw_i, raw_j);
                assert!(
                    mi >= 0 && mi < MASTER_COLS as i32,
                    "mi={mi} for raw_i={raw_i}"
                );
                assert!(
                    mj >= 0 && mj < MASTER_COLS as i32,
                    "mj={mj} for raw_j={raw_j}"
                );

                let id = master_ij_to_id(mi, mj);
                let target = master_target_position(mi, mj, cell_size);

                // Invariant: x == (id % MASTER_COLS) * cell_size, y == (id / MASTER_COLS) * cell_size
                let expected_x = (id % MASTER_COLS) as f32 * cell_size;
                let expected_y = (id / MASTER_COLS) as f32 * cell_size;
                assert!(
                    (target.x - expected_x).abs() < 1e-4,
                    "x mismatch for raw=({raw_i},{raw_j}): got {}, expected {expected_x}",
                    target.x
                );
                assert!(
                    (target.y - expected_y).abs() < 1e-4,
                    "y mismatch for raw=({raw_i},{raw_j}): got {}, expected {expected_y}",
                    target.y
                );
                // Non-negative positions.
                assert!(target.x >= 0.0, "x negative for raw=({raw_i},{raw_j})");
                assert!(target.y >= 0.0, "y negative for raw=({raw_i},{raw_j})");
            }
        }
    }

    // --- existing tests ----------------------------------------------------------

    #[test]
    fn origins_conflict_catches_distinct_positions() {
        // Two clearly different origins should conflict.
        assert!(origins_conflict(0, 0, 1, 0));
        assert!(origins_conflict(0, 0, 0, 1));
        assert!(origins_conflict(10, 20, 11, 20));
    }

    #[test]
    fn origins_conflict_same_position_no_conflict() {
        // Identical origins do not conflict.
        assert!(!origins_conflict(5, 7, 5, 7));
        // Cyclic equivalents within 501×501 also don't conflict.
        let m = MASTER_ROWS as i32;
        assert!(!origins_conflict(3, 4, 3 + m, 4));
        assert!(!origins_conflict(3, 4, 3, 4 + m));
    }
}
