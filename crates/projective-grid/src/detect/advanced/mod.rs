//! Advanced detector-building blocks.
//!
//! These APIs are intended for target-specific adapters that need to compose
//! grid mechanics with their own target policy. The stable facade remains
//! [`crate::detect_grid`] / [`crate::detect_grid_all`].

pub mod square;
