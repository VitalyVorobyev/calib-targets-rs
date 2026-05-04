//! Generic 2D projective grid construction and homography tools.
//!
//! This crate provides reusable algorithms for turning a cloud of 2D
//! points into a labelled grid: seed-and-grow BFS, boundary extension
//! via fitted homography, and per-cell / global rectification.
//!
//! Pattern-agnostic at the bottom (KD-tree, circular stats, mean-shift,
//! DLT homography); pattern-specific at the top via the
//! [`square::grow::GrowValidator`] trait — chessboard parity, ChArUco
//! marker rules, etc. plug in there.
//!
//! # Module layout
//!
//! | Module | Responsibility |
//! |---|---|
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
pub use square::index::GridCoords;
pub use square::mesh::SquareGridHomographyMesh;
pub use square::rectify::SquareGridHomography;
pub use square::smoothness::{
    square_find_inconsistent_corners, square_find_inconsistent_corners_step_aware,
    square_predict_grid_position,
};

// --- Topological-grid surface --------------------------------
pub use topological::{
    build_grid_topological, build_grid_topological_trace, AxisHint, TopologicalComponent,
    TopologicalComponentTrace, TopologicalCornerTrace, TopologicalEdgeMetricTrace,
    TopologicalError, TopologicalGrid, TopologicalLabelTrace, TopologicalParams,
    TopologicalQuadTrace, TopologicalStats, TopologicalTrace, TopologicalTriangleTrace,
    TriangleClass,
};

// --- Component merge (shared by both pipelines) --------------
pub use component_merge::{
    merge_components_local, ComponentInput, ComponentMergeResult, ComponentMergeStats,
    LocalMergeParams,
};
