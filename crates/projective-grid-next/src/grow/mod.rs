//! Square seed-and-grow BFS pipeline.
//!
//! Five sub-modules:
//!
//! * [`predict`] — shared per-cell prediction (`PredictCtx<F>`,
//!   `LabelledNeighbour<F>`, `PredictedPosition<F>`). Same function is reused
//!   by Phase 4's `refine::fill` so prediction logic lives in one place
//!   (closes Gap 6).
//! * [`attach`] — KD-tree radius search + nearest/2nd-nearest ambiguity gate.
//! * [`context`] — `SquareGrowContext<F>` trait, `EdgeCtx<F>`, and
//!   `OpenContext` (zero-config impl shared with `SeedQuadContext`).
//! * [`params`] — `GrowParams<F>` and `GrowResult<F>`.
//! * [`engine`] — the BFS engine itself.

pub mod attach;
pub mod context;
pub mod engine;
pub mod params;
pub mod predict;

pub use attach::{
    choose_unambiguous, collect_candidates, AmbiguityReason, Candidate, UnambiguousChoice,
};
pub use context::{EdgeCtx, OpenContext, SquareGrowContext};
pub use engine::bfs_grow;
pub use params::{GrowParams, GrowResult};
pub use predict::{predict_from_neighbours, LabelledNeighbour, PredictCtx, PredictedPosition};
