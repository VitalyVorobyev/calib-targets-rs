//! Float-generic statistical helpers: undirected-circular angle handling,
//! global cell-size estimation, per-corner local step.
//!
//! Closes Gap 2 from `docs/algorithmic_gaps.md` — the legacy
//! `projective_grid::circular_stats` was hardcoded to `f32`; everything in
//! this module is generic over the crate's [`Float`](crate::Float) bound.

pub mod circular;
pub mod global_step;
pub mod local_step;

pub use circular::{
    angle_to_bin, angular_dist_pi, bin_to_angle, pick_two_peaks, refine_2means_double_angle,
    smooth_circular_5, wrap_pi, AngleVote, PeakPickOptions,
};
pub use global_step::{estimate_global_cell_size, GlobalStepEstimate, GlobalStepParams};
pub use local_step::{estimate_local_steps, LocalStep, LocalStepParams, LocalStepPointData};
