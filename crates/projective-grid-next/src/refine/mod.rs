//! Grid refinement: interior hole fill, global boundary extension, local
//! boundary extension.
//!
//! All three operations reuse [`crate::grow::predict::predict_from_neighbours`]
//! / [`crate::grow::attach::collect_candidates`] /
//! [`crate::grow::attach::choose_unambiguous`] so prediction and ambiguity
//! logic stay in **one** place (closes Gap 6 from `docs/algorithmic_gaps.md`).
//! The same [`crate::grow::context::SquareGrowContext`] eligibility / parity /
//! edge-invariant hooks gate every attachment so the refine pass preserves
//! the BFS precision contract.

pub mod extend_global;
pub mod extend_local;
pub mod fill;

pub use extend_global::{extend_via_global_homography, ExtensionParams, ExtensionStats};
pub use extend_local::{extend_via_local_homography, LocalExtensionParams};
pub use fill::{fill_grid_holes, FillParams, FillStats};
