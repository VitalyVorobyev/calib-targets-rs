//! Invariant-first chessboard detector.
//!
//! Takes a slice of [ChESS](https://www.cl.cam.ac.uk/research/rainbow/projects/chess/)
//! X-junction corners and returns an integer-labelled chessboard grid. The
//! crate's only output contract is: *every labelled corner has been proven
//! to sit at a real grid intersection.* Missing corners are acceptable;
//! wrong corners are not.
//!
//! On the canonical 120-snap regression dataset (`testdata/3536119669`):
//! **119 / 120 frames detected, 0 wrong `(i, j)` labels**.
//!
//! Algorithm reference: see `book/src/chessboard.md`.
//!
//! # Pipeline
//!
//! 1. Pre-filter (strength + fit-quality + axes validity).
//! 2. Global grid-direction centers `{Θ₀, Θ₁}` via axes-histogram + 2-means.
//! 3. Per-corner cluster label (canonical vs swapped axis assignment).
//! 4. Global cell-size `s` (cross-cluster nearest-neighbor mode).
//! 5. Seed: pick a 2×2 quad passing every geometric invariant; cell size
//!    comes OUT of the seed.
//! 6. Grow: BFS over `(i, j)` boundary with the full invariant stack
//!    enforced at every attachment.
//! 7. Validate: line collinearity + local-H residual; blacklist outliers;
//!    restart Stages 5–7 with the blacklist excluded.
//! 8. Recall boosters: line extrapolation, interior gap fill, component
//!    merge, weak-cluster rescue.
//!
//! Each stage is its own module; see the submodules.
//!
//! # Quickstart
//!
//! ```rust,ignore
//! use calib_targets_chessboard::{Detector, DetectorParams};
//! use calib_targets_core::Corner;
//!
//! fn detect(corners: &[Corner]) {
//!     let det = Detector::new(DetectorParams::default());
//!     if let Some(d) = det.detect(corners) {
//!         println!("labelled {} corners", d.target.corners.len());
//!     }
//! }
//! ```
//!
//! # Rectification helpers
//!
//! Two pattern-agnostic rectifiers are exposed under [`mesh_warp`] and
//! [`rectified_view`]: they take a `&[LabeledCorner]` plus an inlier index
//! list (typically `Detection::strong_indices`) and produce a rectified
//! view. They are independent of the v2 detector pipeline and can be used
//! with any consistent `(i, j)` labelling.

pub mod boosters;
pub mod cell_size;
pub mod cluster;
pub mod corner;
pub mod detector;
pub mod grow;
pub mod mesh_warp;
pub mod params;
pub mod rectified_view;
pub mod seed;
pub mod validate;

pub use boosters::{apply_boosters, BoosterResult};
pub use cell_size::estimate_cell_size;
pub use cluster::{cluster_axes, AxisCluster, ClusterCenters};
pub use corner::{ClusterLabel, CornerAug, CornerStage};
pub use detector::{
    DebugFrame, Detection, Detector, InstrumentedResult, IterationTrace, StageCounts,
    DEBUG_FRAME_SCHEMA,
};
pub use grow::{grow_from_seed, GrowResult};
pub use mesh_warp::{rectify_mesh_from_grid, MeshWarpError, RectifiedMeshView};
pub use params::DetectorParams;
pub use rectified_view::{rectify_from_chessboard_result, RectifiedBoardView, RectifyError};
pub use seed::{find_seed, Seed, SeedOutput};
pub use validate::{validate, ValidationResult};
