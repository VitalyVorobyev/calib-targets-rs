//! Square-lattice detector dispatch.
//!
//! Phase D adds a second `(LatticeKind::Square, Evidence::Oriented2)`
//! algorithm — the axis-driven topological grid finder — alongside the
//! Phase C seed-and-grow port. The caller picks via
//! [`SquareAlgorithm`](super::SquareAlgorithm); both paths share the
//! same input/output shape so downstream consumers stay agnostic.

mod seed_grow;
mod topological;

pub(super) use seed_grow::detect_square_oriented2_seed_grow;
pub(super) use topological::detect_square_oriented2_topological_all;
pub use topological::TopologicalParams;
