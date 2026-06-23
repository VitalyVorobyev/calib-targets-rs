//! Shared detection back-half plus the geometry-only grid-growth engine.
//!
//! The topological strategy's only job is to build connected components;
//! everything after — local component merge ([`merge`]), geometric validation
//! ([`validate`]), and the per-component lattice fit (`fit`) — is shared here.
//!
//! This module also hosts the pattern-agnostic grid-growth primitives and the
//! geometry-only recovery schedule that powers the topological synthesized-axis
//! (`Evidence::Positions` / `Evidence::Oriented1`) path:
//!
//! - [`grow`] — the [`SquareAttachPolicy`](grow::SquareAttachPolicy) contract,
//!   candidate search, ambiguity resolution, and the per-edge cardinal gate.
//! - [`grow_extend`] / [`extension`] / [`fill`] — boundary-extension and
//!   interior-fill engines built on those primitives.
//! - [`recovery_schedule`] — the
//!   [`RecoverySchedule`](recovery_schedule::RecoverySchedule) fixed-point that
//!   composes extension + fill + revalidation + drop filters.
//!
//! A crate-private geometry-first attach policy for synthesized-axis evidence
//! backs the recovery schedule. The undirected-angle helpers it needs live in
//! [`crate::cluster`] (the single source of truth for `wrap_pi` /
//! `angular_dist_pi`).
//!
//! The chessboard crate composes [`grow`] / [`fill`] / [`extension`] /
//! [`grow_extend`] directly for its own topological recovery path.

// `fit` is engine-internal: only the in-crate strategy facades consume
// `fit_component` / `FitComponentResult` (re-exported `pub(crate)` below). No
// external consumer reaches it, so the module is crate-private — keeping the
// advanced tier no wider than what callers actually use.
pub(crate) mod fit;
pub mod merge;
pub mod validate;

// Geometry-only grid-growth engine + recovery schedule (relocated from the
// retired `seed_and_grow` module; consumed by the topological recovery path
// and, externally, by the chessboard crate).
pub mod extension;
pub mod fill;
pub mod grow;
pub mod grow_extend;
mod positions_policy;
pub mod recovery_schedule;

pub(crate) use fit::{fit_component, FitComponentResult};
