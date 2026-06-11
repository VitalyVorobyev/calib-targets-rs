//! Circular-statistics helpers — re-exported from [`projective_grid::cluster`].
//!
//! The chessboard detector's global grid-direction stage was migrated
//! down into [`projective_grid::cluster`] (which owns the generic
//! histogram + plateau-aware peak picking + double-angle 2-means math).
//! This module keeps the chessboard-local names that sibling modules
//! (`seed`, `grow`, `boosters`, `cluster`) import, pointing at the
//! generic implementation so there is a single source of truth.

pub(crate) use projective_grid::cluster::{angular_dist_pi, wrap_pi};
