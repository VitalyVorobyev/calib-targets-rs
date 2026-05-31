//! Square-lattice detector dispatch.
//!
//! Two `(LatticeKind::Square, Evidence::Oriented2)` algorithms share the
//! same input/output shape so downstream consumers stay agnostic:
//!
//! - [`SquareAlgorithm::SeedAndGrow`](super::SquareAlgorithm::SeedAndGrow):
//!   advanced seed-quad finder + BFS grow + local component merge +
//!   validate + fit (multi-component capable).
//! - [`SquareAlgorithm::Topological`](super::SquareAlgorithm::Topological):
//!   axis-driven topological grid finder + validate + fit.
//!
//! The two paths share the [`shared::fit_component`] back-half.

mod oriented2_policy;
mod seed_grow;
mod shared;
mod topological;

pub(super) use seed_grow::detect_square_oriented2_seed_grow;
pub(super) use topological::detect_square_oriented2_topological_all;
pub use topological::TopologicalParams;
