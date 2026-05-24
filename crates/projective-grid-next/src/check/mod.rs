//! Consistency-check task: verify an existing labelled grid is geometrically
//! self-consistent.
//!
//! [`check_square_labels`] reuses the [`mod@crate::validate`] precision gate and
//! optionally enforces 4-connectivity. [`check_hex_labels`] is a placeholder
//! that returns the typed
//! [`UnsupportedCombination::HexConsistency`](crate::UnsupportedCombination::HexConsistency)
//! error in v1.

pub mod hex;
pub mod square;

pub use hex::check_hex_labels;
pub use square::{check_square_labels, CheckParams, CheckReport};
