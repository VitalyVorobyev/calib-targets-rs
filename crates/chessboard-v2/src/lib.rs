//! Chessboard detector **v2** — standalone prototype.
//!
//! A fresh-mind redesign of the chessboard detection pipeline whose
//! only output contract is: *every labelled corner has been proven
//! to sit at a real grid intersection.* Missing corners are
//! acceptable; wrong corners are not.
//!
//! Design spec: `docs/chessboard_v2_spec.md` in the workspace root.
//!
//! This crate reuses `projective-grid` primitives (KD-tree,
//! homography fitting, local step estimation) and the
//! `calib-targets-core` types (`Corner`, `LabeledCorner`,
//! `GridCoords`). It does **not** depend on
//! `calib-targets-chessboard`.
//!
//! # Pipeline
//!
//! 1. Pre-filter (strength + fit-quality + axes validity).
//! 2. Global grid-direction centers `{Θ₀, Θ₁}` via axes-histogram + 2-means.
//! 3. Per-corner cluster label (canonical vs swapped axis assignment).
//! 4. Global cell-size `s`.
//! 5. Seed: pick a 2×2 quad passing every geometric invariant.
//! 6. Grow: BFS over `(i, j)` boundary with the full invariant stack
//!    enforced at every attachment.
//! 7. Validate: line collinearity + local-H residual; blacklist
//!    outliers; restart Stage 5–7 with blacklist excluded.
//! 8. Recall boosters: line extrapolation, interior gap fill,
//!    component merge, weak-cluster rescue.
//!
//! Each stage is its own module; see the submodules.

pub mod boosters;
pub mod cell_size;
pub mod cluster;
pub mod corner;
pub mod detector;
pub mod grow;
pub mod params;
pub mod seed;
pub mod validate;

pub use boosters::{apply_boosters, BoosterResult};
pub use cell_size::estimate_cell_size;
pub use cluster::{cluster_axes, AxisCluster, ClusterCenters};
pub use corner::{ClusterLabel, CornerAug, CornerStage};
pub use detector::{DebugFrame, Detection, Detector, IterationTrace, DEBUG_FRAME_SCHEMA};
pub use grow::{grow_from_seed, GrowResult};
pub use params::DetectorParams;
pub use seed::{find_seed, Seed, SeedOutput};
pub use validate::{validate, ValidationResult};
