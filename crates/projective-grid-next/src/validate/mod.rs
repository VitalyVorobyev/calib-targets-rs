//! Post-grow validation: line collinearity, per-corner local homography
//! residual, per-edge length consistency, axis-slot-swap parity.
//!
//! Phase C runs four pattern-agnostic checks over the labelled grid:
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
//! 4. **Axis-slot-swap parity.** For every labelled cardinal pair where
//!    both endpoints carry informative axes, compute the edge direction
//!    (mod π), pick the closer axis at each endpoint, and verify the two
//!    endpoints pick *opposite* slots (this is the chessboard-like
//!    parity check, but with slot identity derived from each corner's
//!    own axes — no consumer-supplied parity tag is required).
//!
//! Flags from (1) and (2) are combined via the legacy attribution rules
//! (≥ 2 line flags ⇒ outlier; high local-H AND ≥ 1 line flag ⇒ outlier;
//! local-H-only with a flagged base ⇒ blame the base). Flags from (3) and
//! (4) drop unconditionally — they are tighter than the legacy line check
//! and self-evidently bad geometry has no safe recovery downstream.

mod square;

pub use square::{validate, LabelledEntry, ValidateParams, ValidationResult};
