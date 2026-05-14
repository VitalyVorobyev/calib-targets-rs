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
//! | 12    | `final_geometry_check`       | Mandatory precision gate: per-edge length + axis-slot-swap parity + largest cardinal component. Can only drop corners; never adds. |
//!
//! Each stage is its own module or function; see the submodules.
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
//! view. They are independent of the detector pipeline and can be used
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
pub mod topological;
pub mod validate;

pub use boosters::{apply_boosters, BoosterResult};
pub use cell_size::estimate_cell_size;
pub use cluster::{cluster_axes, cluster_axes_debug, AxisCluster, ClusterCenters, ClusterDebug};
pub use corner::{ClusterLabel, CornerAug, CornerStage};
pub use detector::{
    build_detection_from_grow, DebugFrame, Detection, Detector, InstrumentedResult, IterationTrace,
    StageCounts, DEBUG_FRAME_SCHEMA,
};
pub use grow::{grow_from_seed, GrowResult};
pub use mesh_warp::{rectify_mesh_from_grid, MeshWarpError, RectifiedMeshView};
pub use params::{DetectorParams, GraphBuildAlgorithm};
pub use rectified_view::{rectify_from_chessboard_result, RectifiedBoardView, RectifyError};
pub use seed::{find_seed, Seed, SeedOutput};
pub use topological::{detect_all_topological, trace_topological};
pub use validate::{validate, ValidationResult};
