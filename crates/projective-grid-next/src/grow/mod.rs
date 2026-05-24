//! BFS seed-and-grow for square lattices, evidence type
//! [`Evidence::Oriented2`](crate::detect::Evidence::Oriented2).
//!
//! Walks the boundary outward from the [`crate::seed::Seed`] using
//! axis-aligned predict-and-attach. For every empty cell adjacent to a
//! labelled one we:
//!
//! 1. Predict its image position from labelled neighbours (local steps
//!    when available, global axes otherwise).
//! 2. Find the closest unlabelled feature within a search radius
//!    proportional to cell size.
//! 3. Verify the runner-up is meaningfully farther (ambiguity gate).
//! 4. Verify the candidate's two axes align with the seed's axes.
//! 5. Verify every cardinal edge length is within an absolute band of the
//!    seed-derived cell size.
//!
//! Attaching the candidate enqueues its own cardinal neighbours.

mod square;

pub use square::{bfs_grow, GrowParams, GrowResult};
