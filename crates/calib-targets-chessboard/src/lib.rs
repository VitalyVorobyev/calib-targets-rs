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
//! The pipeline runs as a sequence of named stages. Internal source
//! comments and diagnostic field names refer to these names; the
//! numbered shorthand below is the canonical enumeration. Earlier
//! versions of this crate used fractional sub-stages ("6.25", "6.5",
//! "6.5b", "6.75") — those have been folded into descriptive names
//! that capture *what* the pass does rather than where it sits in a
//! numerical lattice.
//!
//! | Stage | Name                         | Responsibility |
//! |-------|------------------------------|----------------|
//! | 1     | `prefilter`                  | Drop corners failing strength / fit-quality / axes-validity gates. |
//! | 2     | `cluster_axes`               | Recover the two global grid-direction centres `{Θ₀, Θ₁}` via histogram + 2-means; label each corner as canonical or swapped. |
//! | 3     | `estimate_cell_size`         | Cross-cluster nearest-neighbour mode → global cell size `s`. |
//! | 4     | `find_seed`                  | Pick a 2×2 quad passing every geometric invariant; refine `s` from the seed. |
//! | 5     | `grow`                       | BFS over `(i, j)` boundary with the full invariant stack enforced at every attachment. |
//! | 6     | `extend_boundary`            | Homography-based extension (global or per-candidate local-H) of the labelled boundary outward and into interior holes. |
//! | 7     | `fix_partial_slot_flip`      | Defensive sweep that re-checks the axis-slot-swap parity after boundary extension; flips entries whose induced edges disagree. |
//! | 8     | `rescue_no_cluster`          | Re-admit `Strong` / `NoCluster` corners within `rescue_axis_tol_deg` via local-H prediction. |
//! | 9     | `refit_cluster_centers`      | Re-estimate `{Θ₀, Θ₁}` from labelled corners; if the shift exceeds `refit_min_shift_deg`, run a second extension + rescue. |
//! | 10    | `validate`                   | Line collinearity + local-H residual checks; blacklist outliers; restart `find_seed` through `validate` with the blacklist excluded. |
//! | 11    | `apply_boosters`             | Recall boosters: interior gap fill + line extrapolation. |
//! | 12    | `final_geometry_check`       | Mandatory precision gate: shared square-grid validation + local edge-shape checks + largest cardinal component. Can only drop corners; never adds. |
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

mod boosters;
mod circular_stats;
mod cluster;
mod corner;
mod detector;
mod grow;
mod mesh_warp;
mod params;
mod pipeline;
mod rectified_view;
mod seed;
mod topological;
mod validate;

/// Opt-in detector introspection surface (`DebugFrame`, per-stage traces,
/// `StageCounts`). Compiled only with the `diagnostics` feature; the hot
/// [`Detector::detect`] path never assembles these.
#[cfg(feature = "diagnostics")]
pub mod diagnostics;

// --- Public contract ---------------------------------------------------
pub use corner::ChessCorner;
pub use detector::{ChessboardCorner, ChessboardDetection, Detector};
pub use mesh_warp::{rectify_mesh_from_grid, MeshWarpError, RectifiedMeshView};
pub use params::{
    AdvancedTuning, ChessboardParamsError, DetectorParams, GraphBuildAlgorithm, OrientationSource,
};
pub use rectified_view::{rectify_from_chessboard_result, RectifiedBoardView, RectifyError};
pub use topological::{detect_all_topological, trace_topological};
