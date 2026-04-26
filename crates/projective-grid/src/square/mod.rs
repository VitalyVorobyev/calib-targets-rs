//! Square (4-connected) grid support.
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`alignment`] | D4 rotations / reflections on `(i, j)` |
//! | [`grow`] | Seed-and-grow BFS with adaptive local-step prediction |
//! | [`grow_extension`] | Boundary extension via globally-fit homography |
//! | [`index`] | `(i, j)` cell identifier |
//! | [`mesh`] | Per-cell homography mesh over a regular grid |
//! | [`rectify`] | Global homography from grid corner positions |
//! | [`seed`] | 2×2 seed primitives (cell size, midpoint violation) |
//! | [`smoothness`] | Midpoint prediction and outlier detection |
//! | [`validate`](mod@validate) | Post-grow line / local-H residual checks |
//!
//! Top-level types are re-exported at the crate root.

pub mod alignment;
pub mod grow;
pub mod grow_extension;
pub mod index;
pub mod mesh;
pub mod rectify;
pub mod seed;
pub mod seed_finder;
pub mod smoothness;
pub mod validate;

pub use alignment::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
pub use grow::{
    bfs_grow, predict_from_neighbours, Admit, GrowParams, GrowResult, GrowValidator,
    LabelledNeighbour, Seed,
};
pub use grow_extension::{extend_via_global_homography, ExtensionParams, ExtensionStats};
pub use index::GridIndex;
pub use mesh::GridHomographyMesh;
pub use rectify::GridHomography;
pub use seed::{
    seed_cell_size, seed_has_midpoint_violation, seed_homography, SeedOutput, SEED_QUAD_GRID,
};
pub use seed_finder::{find_quad, SeedQuadParams, SeedQuadValidator};
pub use smoothness::{
    find_inconsistent_corners, find_inconsistent_corners_step_aware, predict_grid_position,
};
pub use validate::{validate, LabelledEntry, ValidationParams, ValidationResult};
