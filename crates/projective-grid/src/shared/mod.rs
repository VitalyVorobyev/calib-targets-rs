//! Shared detection back-half used by both square strategies.
//!
//! A strategy's only job is to build connected components; everything after —
//! local component merge ([`merge`]), geometric validation ([`validate`]), and
//! the per-component lattice fit (`fit`) — is shared here so the
//! seed-and-grow and topological strategies stay symmetric.

// `fit` is engine-internal: only the in-crate strategy facades consume
// `fit_component` / `FitComponentResult` (re-exported `pub(crate)` below). No
// external consumer reaches it, so the module is crate-private — keeping the
// advanced tier no wider than what callers actually use.
pub(crate) mod fit;
pub mod merge;
pub mod validate;

pub(crate) use fit::{fit_component, FitComponentResult};
