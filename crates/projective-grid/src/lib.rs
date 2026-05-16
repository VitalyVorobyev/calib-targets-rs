//! Generic 2D projective grid construction and homography tools.
//!
//! This crate turns a cloud of 2D points into a labelled grid.
//!
//! # Start here: [`detect_regular_grid`]
//!
//! [`detect_regular_grid`] is the zero-config onboarding entry point.
//! Hand it a `&[Point2<f32>]`; it returns a [`RegularGridDetection`]
//! with every recovered corner carrying its `(i, j)` label and the
//! index back into your input slice — **no caller-written validator
//! scaffolding required**:
//!
//! ```rust
//! use nalgebra::Point2;
//! use projective_grid::detect_regular_grid;
//!
//! let mut points = Vec::new();
//! for j in 0..4 {
//!     for i in 0..5 {
//!         points.push(Point2::new(i as f32 * 30.0, j as f32 * 30.0));
//!     }
//! }
//! let grid = detect_regular_grid(&points).expect("clean grid detects");
//! assert_eq!(grid.points.len(), 20);
//! ```
//!
//! For tuning, use [`RegularGridDetector`] + [`RegularGridParams`].
//! The detector internally estimates the cell size and grid axes from
//! the point cloud and drives the pipeline with a built-in open
//! regular-grid policy.
//!
//! # Advanced / specialized entry points
//!
//! When you need pattern-specific rules (parity, marker slots, colour
//! splits), reach for the validator-driven path:
//!
//! - [`detect_square_grid`] — square-lattice recovery driven by a
//!   caller-supplied [`SeedQuadValidator`] + [`GrowValidator`] pair.
//!   This is what the chessboard / ChArUco / puzzleboard detectors
//!   build on. [`detect_regular_grid`] is a thin wrapper over it with
//!   a built-in permissive policy.
//! - [`detect_topological_grid`] — Shu/Brunton/Fiala 2009 topological
//!   recovery, image-free. Requires per-corner grid axes inline on
//!   the input via [`TopologicalInputCorner`].
//!
//! [`SeedQuadValidator`]: square::seed_finder::SeedQuadValidator
//! [`GrowValidator`]: square::grow::GrowValidator
//!
//! All entry points share a common output shape: a `(i, j) →
//! corner_idx` map plus per-stage diagnostics. Use
//! [`merge_components_local`] to attempt to merge multiple
//! disjoint components into a single grid.
//!
//! The crate is pattern-agnostic — it knows nothing about chessboards,
//! ArUco markers, or images. Lower-level primitives (KD-tree,
//! circular stats, mean-shift, DLT homography, BFS grow, Delaunay
//! triangulation) are exposed under their natural modules for callers
//! who want to compose their own pipeline.
//!
//! # Module layout
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`square::regular`] | Zero-config point-cloud regular-grid detection (onboarding entry point) |
//! | [`square::cleanup`] | Generic output cleanup: rebase, connectivity prune, top-left canonicalise, sort |
//! | [`square::grow`] | Seed-and-grow BFS over a square lattice |
//! | [`square::extension`] | Boundary extension via globally-fit or local homography |
//! | [`square::seed`] | 2×2 seed primitives (cell size, midpoint violation) |
//! | [`square::validate`](mod@square::validate) | Post-grow line / local-H residual checks |
//! | [`square::mesh`] / [`square::rectify`] | Per-cell mesh / global homography rectification |
//! | [`square::smoothness`] | Midpoint prediction + step-aware outlier detection |
//! | [`square::alignment`] | D4 transforms on integer grid coordinates |
//! | [`hex`] | Hex grid: index, mesh, rectify, smoothness, alignment (no grow path yet) |
//! | [`circular_stats`] | Undirected-angle helpers (smoothing, plateau peak picking, double-angle 2-means) |
//! | [`global_step`] / [`local_step`] | Cell-size estimation primitives |
//! | [`homography`] | 4-point + DLT homography with reprojection-quality diagnostics |

mod float_helpers;

pub mod affine;
pub mod circular_stats;
pub mod component_merge;
pub mod global_step;
pub mod hex;
pub mod homography;
pub mod local_step;
pub mod square;
pub mod topological;

/// Trait alias for floating-point types supported by this crate.
///
/// Both `f32` and `f64` satisfy this bound. All public generic types default
/// to `f32` for backward compatibility.
pub trait Float: nalgebra::RealField + Copy {}
impl<T: nalgebra::RealField + Copy> Float for T {}

// --- Generic building blocks (no square / hex assumption) --------------------
pub use affine::AffineTransform2D;
pub use global_step::{estimate_global_cell_size, GlobalStepEstimate, GlobalStepParams};
pub use homography::{
    estimate_homography, estimate_homography_with_quality, homography_from_4pt,
    homography_from_4pt_with_quality, Homography, HomographyQuality,
};
pub use local_step::{estimate_local_steps, LocalStep, LocalStepParams, LocalStepPointData};

// --- Square-grid surface re-exported at the crate root --------
pub use square::alignment::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
pub use square::cleanup::{
    apply_transform, canonicalize_top_left, prune_to_main_component, rebase_to_origin,
    sorted_grid_points, top_left_transform,
};
pub use square::index::GridCoords;
pub use square::mesh::SquareGridHomographyMesh;
pub use square::rectify::SquareGridHomography;
pub use square::smoothness::{
    square_find_inconsistent_corners, square_find_inconsistent_corners_step_aware,
    square_predict_grid_position,
};

// --- Square-grid onboarding entry point ----------------------
pub use square::regular::{
    detect_regular_grid, DetectedGridPoint, ExtensionMode, RegularGridDetection,
    RegularGridDetector, RegularGridParams, RegularGridStats,
};

// --- Square-grid validator-driven (advanced) entry points ----
pub use square::detect::{
    detect_square_grid, detect_square_grid_all, ExtensionStrategy, MultiComponentParams,
    SquareGridDetection, SquareGridParams, SquareGridStats,
};

// --- Topological-grid surface --------------------------------
pub use topological::{
    build_grid_topological, build_grid_topological_trace, detect_topological_grid,
    recover_topological_grid, AxisClusterCenters, AxisEstimate, TopologicalComponent,
    TopologicalComponentTrace, TopologicalCornerTrace, TopologicalEdgeMetricTrace,
    TopologicalError, TopologicalGrid, TopologicalInputCorner, TopologicalLabelTrace,
    TopologicalParams, TopologicalQuadTrace, TopologicalStats, TopologicalTrace,
    TopologicalTriangleTrace, TriangleClass,
};

// --- Component merge (shared by both pipelines) --------------
pub use component_merge::{
    merge_components_local, ComponentInput, ComponentMergeResult, ComponentMergeStats,
    LocalMergeParams,
};
