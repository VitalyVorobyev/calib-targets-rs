//! Shared detection back-half used by both square strategies.
//!
//! A strategy's only job is to build connected components; everything after —
//! local component merge ([`merge`]), geometric validation ([`validate`]), and
//! the per-component lattice fit ([`fit`]) — is shared here so the
//! seed-and-grow and topological strategies stay symmetric.

pub mod fit;
pub mod merge;
pub mod validate;

pub(crate) use fit::{fit_component, FitComponentResult};
