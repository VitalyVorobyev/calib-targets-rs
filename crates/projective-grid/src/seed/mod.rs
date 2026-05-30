//! Seed-quad finders for the detection pipeline.
//!
//! Phase C ships the square seed-quad finder only. Hex remains unimplemented.

mod square;

pub use square::{find_quad, SeedParams, SeedSearchOutput};

#[cfg(test)]
pub use square::Seed;
