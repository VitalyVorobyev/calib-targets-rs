//! Detector entry points.
//!
//! [`Detector`] is a thin facade over the staged pipeline in
//! [`crate::pipeline`]: each `detect*` method dispatches on
//! [`DetectorParams::graph_build_algorithm`] and, for the
//! seed-and-grow path, defers to [`pipeline::run_pipeline_lean`]. The
//! `find_seed → grow → validate` loop, the post-grow stage sequence,
//! and the [`ChessboardDetection`] payload all live under
//! [`crate::pipeline`]. The opt-in `DebugFrame` introspection payload
//! (behind the `diagnostics` feature) is assembled by
//! [`pipeline::run_pipeline`].
//!
//! Stage names follow the canonical pipeline enumeration in the
//! crate-level docs (`crate::`).

use crate::params::{ChessboardParamsError, DetectorParams, GraphBuildAlgorithm};
use crate::pipeline::{self, run_pipeline_lean};
use crate::topological::detect_all_topological;

use crate::corner::ChessCorner;
use std::collections::HashSet;

// Re-export from the pipeline: stable result types used in method signatures
// and the internal helpers reused by the topological dispatch path.
pub use pipeline::{
    build_detection_from_grow, run_geometry_check, ChessboardCorner, ChessboardDetection,
};

// Diagnostic payload: only on the public surface behind the feature.
#[cfg(feature = "diagnostics")]
pub use pipeline::{run_pipeline, DebugFrame};

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
    /// detector cannot honour (currently
    /// [`OrientationSource::NeighbourEdges`](crate::params::OrientationSource::NeighbourEdges)
    /// with [`GraphBuildAlgorithm::SeedAndGrow`] — see
    /// [`DetectorParams::validate`]). This mirrors the fallible constructors
    /// on the sibling detectors (`CharucoDetector`, `PuzzleBoardDetector`),
    /// so the binding layer wraps a single `Result` surface uniformly.
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
        match self.params.graph_build_algorithm {
            GraphBuildAlgorithm::Topological => self.detect_all(corners).into_iter().next(),
            GraphBuildAlgorithm::SeedAndGrow => {
                run_pipeline_lean(&self.params, corners, &HashSet::new()).detection
            }
        }
    }

    /// Diagnostics entry point for a single best detection.
    ///
    /// Runs the pipeline and returns the full [`DebugFrame`] — the
    /// detection plus every per-stage trace. Use [`Self::detect`] when only
    /// the detection is needed; this is the channel for inspecting *how* it
    /// was reached.
    ///
    /// Available only with the `diagnostics` feature enabled.
    #[cfg(feature = "diagnostics")]
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_with_diagnostics(&self, corners: &[ChessCorner]) -> DebugFrame {
        run_pipeline(&self.params, corners, &HashSet::new())
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
        match self.params.graph_build_algorithm {
            GraphBuildAlgorithm::Topological => detect_all_topological(corners, &self.params),
            GraphBuildAlgorithm::SeedAndGrow => {
                // Lean multi-component sweep: run the pipeline once per
                // component, marking each recovered component's corners
                // consumed so the next pass sees a fresh scene. Mirrors
                // the diagnostics sweep but never builds a `DebugFrame`.
                let cap = self.params.max_components.max(1) as usize;
                let mut consumed: HashSet<usize> = HashSet::new();
                let mut detections: Vec<ChessboardDetection> = Vec::with_capacity(cap);
                for _ in 0..cap {
                    let outcome = run_pipeline_lean(&self.params, corners, &consumed);
                    let Some(detection) = outcome.detection else {
                        break;
                    };
                    for corner in &detection.corners {
                        consumed.insert(corner.input_index);
                    }
                    detections.push(detection);
                }
                detections
            }
        }
    }

    /// Diagnostics multi-component entry point. See [`Self::detect_all`].
    ///
    /// Returns one [`DebugFrame`] per recovered grid component — the
    /// per-component detection plus every per-stage trace. Use
    /// [`Self::detect_all`] when only the detections are needed; this is the
    /// channel for inspecting *how* each component was reached.
    ///
    /// Available only with the `diagnostics` feature enabled.
    #[cfg(feature = "diagnostics")]
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_all_with_diagnostics(&self, corners: &[ChessCorner]) -> Vec<DebugFrame> {
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
            for corner in &detection.corners {
                consumed.insert(corner.input_index);
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
    use crate::params::OrientationSource;
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
        let det = Detector::new(DetectorParams::default()).expect("default params valid");
        let d = det.detect(&corners).expect("detection");
        assert_eq!(d.corners.len(), 49);
    }

    /// The stable `detect()` path populates `ChessboardDetection.cell_size`
    /// without any diagnostics. On a clean 20 px-pitch grid the seed-
    /// derived cell size lands within a few pixels of the true pitch.
    #[test]
    fn detect_populates_cell_size_on_lean_path() {
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

    /// Neighbour-edge orientation (axes synthesized from neighbour geometry,
    /// ChESS axes ignored) recovers a clean grid via the topological builder
    /// with zero duplicate `(i, j)` labels.
    #[test]
    fn neighbour_edges_topological_recovers_clean_grid() {
        let s = 20.0_f32;
        let mut corners = Vec::new();
        let mut k = 0;
        for j in 0..7_i32 {
            for i in 0..7_i32 {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                // Axes are deliberately set; neighbour-edge mode must ignore
                // them and synthesize directions from neighbour geometry.
                corners.push(make_corner(k, x, y, (i + j).rem_euclid(2) == 1));
                k += 1;
            }
        }
        let params = DetectorParams {
            graph_build_algorithm: GraphBuildAlgorithm::Topological,
            orientation_source: OrientationSource::NeighbourEdges,
            ..DetectorParams::default()
        };
        let det = Detector::new(params).expect("topological + neighbour-edges valid");
        let d = det.detect(&corners).expect("neighbour-edge detection");
        assert!(
            d.corners.len() >= 36,
            "neighbour-edge recall too low on clean grid: {}",
            d.corners.len()
        );
        let mut seen = std::collections::HashSet::new();
        for c in &d.corners {
            assert!(
                seen.insert((c.grid.i, c.grid.j)),
                "duplicate label {:?}",
                (c.grid.i, c.grid.j)
            );
        }
    }

    /// Neighbour-edge orientation on the native SeedAndGrow pipeline is
    /// unsupported. It is now a *typed* error surfaced by `validate()` and the
    /// fallible [`Detector::new`] rather than a runtime panic, so a head-to-head
    /// measurement can never silently fall back to ChESS axes.
    #[test]
    fn neighbour_edges_seed_and_grow_is_typed_error() {
        let params = DetectorParams {
            graph_build_algorithm: GraphBuildAlgorithm::SeedAndGrow,
            orientation_source: OrientationSource::NeighbourEdges,
            ..DetectorParams::default()
        };
        assert_eq!(
            params.validate(),
            Err(ChessboardParamsError::NeighbourEdgesRequiresTopological)
        );
        assert!(matches!(
            Detector::new(params.clone()),
            Err(ChessboardParamsError::NeighbourEdgesRequiresTopological)
        ));
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

    #[cfg(feature = "diagnostics")]
    #[test]
    fn instrumented_counts_match_clean_grid() {
        use crate::pipeline::StageCounts;
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
        let det = Detector::new(DetectorParams::default()).expect("default params valid");
        let frame = det.detect_with_diagnostics(&corners);
        let counts = StageCounts::from_frame(&frame);
        assert!(frame.detection.is_some(), "expected detection on 7x7 grid");
        assert_eq!(counts.input_corners, 49);
        assert_eq!(counts.after_strength_filter, 49);
        assert_eq!(counts.after_clustering, 49);
        assert!(counts.seed_found);
        assert_eq!(counts.labelled_final, 49);
        assert_eq!(counts.blacklisted_total, 0);
        assert!(counts.validation_iterations >= 1);
    }
}
