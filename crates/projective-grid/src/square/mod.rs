//! Square (4-connected) grid support.
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`alignment`] | D4 rotations / reflections on `(i, j)` |
//! | [`extension`] | Boundary extension via homography (global + local) |
//! | [`grow`] | Seed-and-grow BFS with adaptive local-step prediction |
//! | [`grow_extend`] | BFS extension from an existing labelled grid |
//! | [`index`] | `(i, j)` cell identifier |
//! | [`mesh`] | Per-cell homography mesh over a regular grid |
//! | [`rectify`] | Global homography from grid corner positions |
//! | [`seed`] | 2×2 seed struct, geometry helpers, and pattern-agnostic finder |
//! | [`smoothness`] | Midpoint prediction and outlier detection |
//! | [`validate`](mod@validate) | Post-grow line / local-H residual checks |
//!
//! Top-level types are re-exported at the crate root.
//!
//! `grow_extension` is a compatibility alias for [`extension`] retained
//! for consumers that reference the old module path directly.
//!
//! `seed_finder` is a compatibility alias for [`seed::finder`] retained
//! for consumers that import from the old path directly.

pub mod alignment;
pub mod extension;
pub mod grow;
pub mod grow_extend;
/// Compatibility alias for [`extension`].
///
/// New code should import from [`extension`] directly. This alias will
/// be removed in a future version.
pub mod grow_extension {
    pub use crate::square::extension::{
        extend_via_global_homography, extend_via_local_homography, ExtensionCommonParams,
        ExtensionParams, ExtensionStats, LocalExtensionParams,
    };
}
pub mod index;
pub mod mesh;
pub mod rectify;
pub mod seed;
/// Compatibility alias for [`seed::finder`].
///
/// New code should import from [`seed::finder`] directly.
pub mod seed_finder {
    pub use crate::square::seed::finder::{find_quad, SeedQuadParams, SeedQuadValidator};
}
pub mod smoothness;
pub mod validate;

pub use alignment::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
pub use extension::{
    extend_via_global_homography, ExtensionCommonParams, ExtensionParams, ExtensionStats,
    LocalExtensionParams,
};
pub use grow::{
    bfs_grow, predict_from_neighbours, Admit, GrowParams, GrowResult, GrowValidator,
    LabelledNeighbour, Seed,
};
pub use grow_extend::{extend_from_labelled, BfsExtensionStats};
pub use index::GridCoords;
pub use mesh::SquareGridHomographyMesh;
pub use rectify::SquareGridHomography;
pub use seed::finder::{find_quad, SeedQuadParams, SeedQuadValidator};
pub use seed::{
    seed_cell_size, seed_has_midpoint_violation, seed_homography, SeedOutput, SEED_QUAD_GRID,
};
pub use smoothness::{
    square_find_inconsistent_corners, square_find_inconsistent_corners_step_aware,
    square_predict_grid_position,
};
pub use validate::{validate, LabelledEntry, ValidationParams, ValidationResult};
