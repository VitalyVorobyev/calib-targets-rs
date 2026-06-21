//! Invariant-first chessboard detector.
//!
//! Takes a slice of [ChESS](https://www.cl.cam.ac.uk/research/rainbow/projects/chess/)
//! X-junction corners and returns an integer-labelled chessboard grid. The
//! crate's only output contract is: *every labelled corner has been proven
//! to sit at a real grid intersection.* Missing corners are acceptable;
//! wrong corners are not.
//!
//! **High detection rate on our private regression set with zero wrong
//! `(i, j)` labels** (non-negligible lens distortion and motion blur).
//!
//! Algorithm reference: see `book/src/chessboard.md`.
//!
//! # Pipeline
//!
//! The detector builds its `(i, j)` labelling with the **topological** grid
//! finder in [`projective_grid`] (a Delaunay triangulation + an axis-driven
//! cell test), then runs a chessboard-specific recovery + precision pass over
//! the labelled components:
//!
//! | Stage | Name                   | Responsibility |
//! |-------|------------------------|----------------|
//! | 1     | `prefilter`            | Drop corners failing strength / fit-quality / axes-validity gates. |
//! | 2     | `cluster_axes`         | Recover the two global grid-direction centres `{Θ₀, Θ₁}` via histogram + 2-means; label each corner as canonical or swapped. |
//! | 3     | `topological_grid`     | Delaunay + axis-driven cell test → connected labelled `(i, j)` components ([`projective_grid::detect_grid_all`]). |
//! | 4     | `recover_components`   | Per-component recall boosters (interior gap fill + line extrapolation with a directional edge scale) + shared component merge. |
//! | 5     | `final_geometry_check` | Mandatory precision gate: line collinearity + local-H residual + direct wrong-label check + largest cardinal component. Can only drop corners; never adds. |
//! | 6     | `output`               | Labelled grid → canonicalised, non-negative-rebased [`ChessboardDetection`]. |
//!
//! Each stage is its own module or function; see the submodules.
//!
//! # Quickstart
//!
//! ```rust
//! use calib_targets_chessboard::{ChessCorner, Detector, DetectorParams};
//! use calib_targets_core::AxisEstimate;
//! use nalgebra::Point2;
//!
//! // A synthetic 7×7 chessboard corner cloud at 20 px pitch. Adjacent
//! // corners carry opposite axis-slot orderings (the parity invariant).
//! let mut corners: Vec<ChessCorner> = Vec::new();
//! for j in 0..7 {
//!     for i in 0..7 {
//!         let swapped = (i + j) % 2 == 1;
//!         let (a0, a1) = if swapped {
//!             (std::f32::consts::FRAC_PI_2, 0.0)
//!         } else {
//!             (0.0, std::f32::consts::FRAC_PI_2)
//!         };
//!         corners.push(ChessCorner {
//!             position: Point2::new(i as f32 * 20.0 + 50.0, j as f32 * 20.0 + 50.0),
//!             axes: [
//!                 AxisEstimate { angle: a0, sigma: 0.01 },
//!                 AxisEstimate { angle: a1, sigma: 0.01 },
//!             ],
//!             contrast: 10.0,
//!             fit_rms: 1.0,
//!             // Above the default `min_corner_strength` floor (33.0).
//!             strength: 100.0,
//!         });
//!     }
//! }
//!
//! let det = Detector::new(DetectorParams::default()).expect("default params are valid");
//! let detection = det.detect(&corners).expect("clean 7×7 grid detects");
//! assert_eq!(detection.corners.len(), 49);
//! ```
//!
//! # Rectification helpers
//!
//! Two rectifiers are exposed: [`rectify_from_chessboard_result`]
//! ([`RectifiedBoardView`]) and [`rectify_mesh_from_grid`]
//! ([`RectifiedMeshView`]). The former takes a [`ChessboardDetection`]
//! directly; the latter is pattern-agnostic and takes a
//! `&[LabeledCorner]` plus an inlier index list. Both produce a
//! rectified view and can be used with any consistent `(i, j)`
//! labelling.
#![deny(missing_docs)]

mod circular_stats;
mod corner;
mod detector;
mod mesh_warp;
mod params;
mod pipeline;
mod rectified_view;

// --- Public contract ---------------------------------------------------
pub use corner::ChessCorner;
pub use detector::{ChessboardCorner, ChessboardDetection, Detector};
pub use mesh_warp::{rectify_mesh_from_grid, MeshWarpError, RectifiedMeshView};
pub use params::{AdvancedTuning, ChessboardParamsError, DetectorParams, GraphBuildAlgorithm};
pub use pipeline::{detect_all_topological, trace_topological};
pub use rectified_view::{rectify_from_chessboard_result, RectifiedBoardView, RectifyError};
