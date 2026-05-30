//! Advanced square-lattice assembly primitives.
//!
//! This module owns target-agnostic mechanics: seed search, BFS growth,
//! fill/extension, validation, and component merging. Target crates supply
//! policy through traits such as [`grow::SquareAttachPolicy`] and
//! [`seed::finder::SquareSeedPolicy`].

mod angle;

pub mod component_merge;
pub mod extension;
pub mod fill;
pub mod grow;
pub mod grow_extend;
pub mod seed;
pub mod topological_trace;
pub mod validate;
