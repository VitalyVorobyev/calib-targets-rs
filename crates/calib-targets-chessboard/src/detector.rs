//! Detector entry points.
//!
//! [`Detector`] is a thin facade over the staged pipeline in
//! [`crate::pipeline`]: each `detect*` method dispatches on
//! [`DetectorParams::graph_build_algorithm`] and, for the
//! seed-and-grow path, defers to [`pipeline::run_pipeline`]. The
//! `find_seed → grow → validate` loop, the post-grow stage sequence,
//! and the [`Detection`] / [`DebugFrame`] payloads all live under
//! [`crate::pipeline`].
//!
//! Stage names follow the canonical pipeline enumeration in the
//! crate-level docs (`crate::`).

use crate::params::{DetectorParams, GraphBuildAlgorithm};
use crate::pipeline::{self, run_pipeline};
use crate::topological::detect_all_topological;

use crate::corner::ChessCorner;
use std::collections::HashSet;

// Re-export the pipeline's public output / diagnostic surface so
// `crate::detector::*` paths (and the crate prelude) keep working
// after the orchestrator moved into `crate::pipeline`.
pub use pipeline::{
    build_detection_from_grow, run_geometry_check, BfsExtendTrace, DebugFrame, Detection,
    ExtensionTrace, GeometryCheckTrace, InstrumentedResult, IterationTrace, RefitTrace,
    StageCounts, DEBUG_FRAME_SCHEMA,
};

/// Top-level detector.
pub struct Detector {
    pub params: DetectorParams,
}

impl Detector {
    pub fn new(params: DetectorParams) -> Self {
        Self { params }
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
    pub fn detect(&self, corners: &[ChessCorner]) -> Option<Detection> {
        match self.params.graph_build_algorithm {
            GraphBuildAlgorithm::Topological => self.detect_all(corners).into_iter().next(),
            GraphBuildAlgorithm::ChessboardV2 => self.detect_debug(corners).detection,
        }
    }

    /// Full-debug entry point for a single best detection.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_debug(&self, corners: &[ChessCorner]) -> DebugFrame {
        run_pipeline(&self.params, corners, &HashSet::new())
    }

    /// Return every qualifying grid component from a single scene.
    ///
    /// Useful for ChArUco and similar setups where a single physical board
    /// can be split into multiple disconnected chessboard pieces by
    /// markers or occlusions. Each returned [`Detection`] carries its own
    /// locally-rebased `(i, j)` labels; alignment to a global frame is the
    /// caller's responsibility (ChArUco's marker decoding does this).
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
    pub fn detect_all(&self, corners: &[ChessCorner]) -> Vec<Detection> {
        match self.params.graph_build_algorithm {
            GraphBuildAlgorithm::Topological => detect_all_topological(corners, &self.params),
            GraphBuildAlgorithm::ChessboardV2 => self
                .detect_all_debug(corners)
                .into_iter()
                .filter_map(|f| f.detection)
                .collect(),
        }
    }

    /// Single-detection entry with derived per-stage counts.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_instrumented(&self, corners: &[ChessCorner]) -> InstrumentedResult {
        let frame = self.detect_debug(corners);
        let counts = StageCounts::from_frame(&frame);
        InstrumentedResult {
            detection: frame.detection,
            counts,
        }
    }

    /// Multi-component entry with per-component derived counts.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_all_instrumented(&self, corners: &[ChessCorner]) -> Vec<InstrumentedResult> {
        self.detect_all_debug(corners)
            .into_iter()
            .map(|frame| {
                let counts = StageCounts::from_frame(&frame);
                InstrumentedResult {
                    detection: frame.detection,
                    counts,
                }
            })
            .collect()
    }

    /// Full-debug multi-component entry point. See [`Self::detect_all`].
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_all_debug(&self, corners: &[ChessCorner]) -> Vec<DebugFrame> {
        let cap = self.params.max_components.max(1) as usize;
        let mut consumed: HashSet<usize> = HashSet::new();
        let mut frames: Vec<DebugFrame> = Vec::with_capacity(cap);

        for _ in 0..cap {
            let frame = run_pipeline(&self.params, corners, &consumed);
            let Some(detection) = frame.detection.as_ref() else {
                // No further detectable component — include the empty frame
                // so caller can introspect the failure stage if desired.
                if frames.is_empty() {
                    frames.push(frame);
                }
                break;
            };
            for &idx in &detection.strong_indices {
                consumed.insert(idx);
            }
            frames.push(frame);
        }

        frames
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
            strength: 1.0,
        }
    }

    #[test]
    fn end_to_end_clean_grid() {
        let s = 20.0_f32;
        let mut corners = Vec::new();
        let mut k = 0;
        for j in 0..7_i32 {
            for i in 0..7_i32 {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let swapped = (i + j).rem_euclid(2) == 1;
                corners.push(make_corner(k, x, y, swapped));
                k += 1;
            }
        }
        let det = Detector::new(DetectorParams::default());
        let d = det.detect(&corners).expect("detection");
        assert_eq!(d.target.corners.len(), 49);
    }

    #[test]
    fn rejects_when_too_few_corners() {
        let det = Detector::new(DetectorParams::default());
        assert!(det.detect(&[]).is_none());
    }

    #[test]
    fn grid_origin_at_visual_top_left() {
        // Synthesize a 7×7 grid where the +x image axis corresponds to
        // (1, 0) and +y to (0, 1). Regardless of which axis-slot the
        // clusterer picks, `build_detection` must canonicalize so
        // (0, 0) lands at the smallest (x, y) corner.
        let s = 20.0_f32;
        let mut corners = Vec::new();
        let mut k = 0;
        for j in 0..7_i32 {
            for i in 0..7_i32 {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let swapped = (i + j).rem_euclid(2) == 1;
                corners.push(make_corner(k, x, y, swapped));
                k += 1;
            }
        }
        let det = Detector::new(DetectorParams::default());
        let d = det.detect(&corners).expect("detection");
        // Locate (0, 0) and the two neighbors.
        let by_ij: std::collections::HashMap<(i32, i32), (f32, f32)> = d
            .target
            .corners
            .iter()
            .filter_map(|c| {
                let g = c.grid?;
                Some(((g.i, g.j), (c.position.x, c.position.y)))
            })
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

    #[test]
    fn instrumented_counts_match_clean_grid() {
        let s = 20.0_f32;
        let mut corners = Vec::new();
        let mut k = 0;
        for j in 0..7_i32 {
            for i in 0..7_i32 {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let swapped = (i + j).rem_euclid(2) == 1;
                corners.push(make_corner(k, x, y, swapped));
                k += 1;
            }
        }
        let det = Detector::new(DetectorParams::default());
        let res = det.detect_instrumented(&corners);
        assert!(res.detection.is_some(), "expected detection on 7x7 grid");
        assert_eq!(res.counts.input_corners, 49);
        assert_eq!(res.counts.after_strength_filter, 49);
        assert_eq!(res.counts.after_clustering, 49);
        assert!(res.counts.seed_found);
        assert_eq!(res.counts.labelled_final, 49);
        assert_eq!(res.counts.blacklisted_total, 0);
        assert!(res.counts.validation_iterations >= 1);
    }
}
