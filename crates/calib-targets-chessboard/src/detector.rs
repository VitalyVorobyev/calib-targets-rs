//! Detector entry points.
//!
//! [`Detector`] is a thin facade over the topological grid builder in
//! [`crate::topological`]: each `detect*` method runs
//! [`detect_all_topological`] and shapes the result. The labelled grid is
//! produced by `projective-grid`'s topological square-grid finder; the
//! chessboard crate owns the ChESS corner pre-filter, the recall boosters,
//! the mandatory geometry check, and the [`ChessboardDetection`] payload.
//!
//! Stage names follow the canonical pipeline enumeration in the
//! crate-level docs (`crate::`).

use crate::params::{ChessboardParamsError, DetectorParams};
use crate::pipeline;
use crate::topological::detect_all_topological;

use crate::corner::ChessCorner;

// Re-export from the pipeline: stable result types used in method signatures
// and the internal helpers reused by the topological dispatch path.
pub use pipeline::{
    build_detection_from_grow, run_geometry_check, ChessboardCorner, ChessboardDetection,
};

/// Top-level detector.
pub struct Detector {
    /// The parameters every `detect*` call on this detector runs with.
    pub params: DetectorParams,
}

impl Detector {
    /// Construct a detector with the given parameters, validating the
    /// configuration up front.
    ///
    /// Returns a typed [`ChessboardParamsError`] for any combination the
    /// detector cannot honour. No combination the current public surface can
    /// express is rejected, so this is presently infallible in practice; the
    /// `Result` is retained so the binding layer wraps a single `Result`
    /// surface uniformly across the sibling detectors (`CharucoDetector`,
    /// `PuzzleBoardDetector`) and so a future validation can be added without a
    /// breaking change. See [`DetectorParams::validate`].
    pub fn new(params: DetectorParams) -> Result<Self, ChessboardParamsError> {
        params.validate()?;
        Ok(Self { params })
    }

    /// Simple entry point: run the pipeline and return the best detection.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect(&self, corners: &[ChessCorner]) -> Option<ChessboardDetection> {
        self.detect_all(corners).into_iter().next()
    }

    /// Return every qualifying grid component from a single scene.
    ///
    /// Useful for ChArUco and similar setups where a single physical board
    /// can be split into multiple disconnected chessboard pieces by
    /// markers or occlusions. Each returned [`ChessboardDetection`] carries
    /// its own locally-rebased `(i, j)` labels; alignment to a global frame
    /// is the caller's responsibility (ChArUco's marker decoding does this).
    ///
    /// Capped by [`DetectorParams::max_components`].
    ///
    /// Does NOT support scenes with multiple separate physical boards — one
    /// target per frame is the contract.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_all(&self, corners: &[ChessCorner]) -> Vec<ChessboardDetection> {
        detect_all_topological(corners, &self.params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corner::ChessCorner;
    use calib_targets_core::AxisEstimate;
    use nalgebra::Point2;

    fn make_corner(idx: usize, x: f32, y: f32, swapped: bool) -> ChessCorner {
        let (a0, a1) = if swapped {
            (std::f32::consts::FRAC_PI_2, 0.0)
        } else {
            (0.0, std::f32::consts::FRAC_PI_2)
        };
        let _ = idx;
        ChessCorner {
            position: Point2::new(x, y),
            axes: [
                AxisEstimate {
                    angle: a0,
                    sigma: 0.01,
                },
                AxisEstimate {
                    angle: a1,
                    sigma: 0.01,
                },
            ],
            contrast: 10.0,
            fit_rms: 1.0,
            // A sharp synthetic corner: well above the default
            // `min_corner_strength` floor (33.0) so these grid-building
            // tests exercise the real default path rather than the
            // strength pre-filter.
            strength: 100.0,
        }
    }

    fn clean_grid(rows: i32, cols: i32, s: f32) -> Vec<ChessCorner> {
        let mut corners = Vec::new();
        let mut k = 0;
        for j in 0..rows {
            for i in 0..cols {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let swapped = (i + j).rem_euclid(2) == 1;
                corners.push(make_corner(k, x, y, swapped));
                k += 1;
            }
        }
        corners
    }

    #[test]
    fn end_to_end_clean_grid() {
        let corners = clean_grid(7, 7, 20.0);
        let det = Detector::new(DetectorParams::default()).expect("default params valid");
        let d = det.detect(&corners).expect("detection");
        assert_eq!(d.corners.len(), 49);
    }

    /// The stable `detect()` path populates `ChessboardDetection.cell_size`.
    /// On a clean 20 px-pitch grid the estimated cell size lands within a
    /// few pixels of the true pitch.
    #[test]
    fn detect_populates_cell_size() {
        let s = 20.0_f32;
        let corners = clean_grid(7, 7, s);
        let det = Detector::new(DetectorParams::default()).expect("default params valid");
        let d = det.detect(&corners).expect("detection");
        let cell = d.cell_size.expect("cell_size populated on detect() path");
        assert!(
            (cell - s).abs() < 2.0,
            "cell_size {cell} not within 2 px of true pitch {s}"
        );
    }

    #[test]
    fn rejects_when_too_few_corners() {
        let det = Detector::new(DetectorParams::default()).expect("default params valid");
        assert!(det.detect(&[]).is_none());
    }

    #[test]
    fn grid_origin_at_visual_top_left() {
        // Synthesize a 7×7 grid where the +x image axis corresponds to
        // (1, 0) and +y to (0, 1). Regardless of which axis-slot the
        // builder picks, `build_detection` must canonicalize so
        // (0, 0) lands at the smallest (x, y) corner.
        let corners = clean_grid(7, 7, 20.0);
        let det = Detector::new(DetectorParams::default()).expect("default params valid");
        let d = det.detect(&corners).expect("detection");
        // Locate (0, 0) and the two neighbors.
        let by_ij: std::collections::HashMap<(i32, i32), (f32, f32)> = d
            .corners
            .iter()
            .map(|c| ((c.grid.i, c.grid.j), (c.position.x, c.position.y)))
            .collect();
        let p00 = by_ij.get(&(0, 0)).copied().expect("(0,0) labelled");
        let p10 = by_ij.get(&(1, 0)).copied().expect("(1,0) labelled");
        let p01 = by_ij.get(&(0, 1)).copied().expect("(0,1) labelled");
        // (0, 0) must be the top-left in pixel coords.
        assert!(
            p00.0 <= p10.0 && p00.1 <= p01.1,
            "(0,0) at {:?} not top-left vs (1,0)={:?} (0,1)={:?}",
            p00,
            p10,
            p01
        );
        // +i step must move right (+x).
        assert!(p10.0 > p00.0, "+i axis not right-pointing");
        // +j step must move down (+y).
        assert!(p01.1 > p00.1, "+j axis not down-pointing");
    }
}
