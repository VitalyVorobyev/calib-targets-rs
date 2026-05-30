//! Post-grow validation: line collinearity, per-corner local homography
//! residual, and per-edge length consistency.
//!
//! The stage runs three pattern-agnostic checks over the labelled grid:
//!
//! 1. **Line collinearity.** For every row (`v = const`) and column
//!    (`u = const`) with at least [`ValidateParams::line_min_members`]
//!    labelled members, fit a least-squares line in pixel space and flag
//!    any member whose perpendicular residual exceeds
//!    `line_tol_rel * cell_size`.
//! 2. **Local-H residual.** For every labelled corner with at least four
//!    non-collinear labelled neighbours, fit a 4-point local homography
//!    from the four grid-closest neighbours, predict the corner's pixel
//!    position, and flag corners whose residual exceeds
//!    `local_h_tol_rel * cell_size`.
//! 3. **Per-edge length band.** Collect every cardinal labelled-pair edge
//!    length; compute the median; flag any endpoint participating in an
//!    edge whose `len / median` falls outside
//!    `[1 / (1 + band), 1 + band]`.
//!
//! Flags from (1) and (2) are combined via local attribution rules
//! (≥ 2 line flags ⇒ outlier; high local-H AND ≥ 1 line flag ⇒ outlier;
//! local-H-only with a flagged base ⇒ blame the base). Flags from (3)
//! drop unconditionally because a severe local edge-scale mismatch is not a
//! safe foundation for later recovery.

mod square;

pub use square::{validate, LabelledEntry, ValidateParams};
