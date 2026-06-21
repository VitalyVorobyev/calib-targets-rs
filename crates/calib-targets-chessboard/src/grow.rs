//! `grow` stage types shared by the topological dispatch path.
//!
//! The pattern-agnostic [`GrowResult`] container — the `(i, j) → index`
//! labelling plus the cardinal step vectors and rebase parity — lives in
//! [`projective_grid::seed_and_grow::grow`]. The chessboard crate re-exports
//! it here so the topological recovery, boosters, geometry check, and output
//! stages share one labelled-grid type.

use projective_grid::seed_and_grow::grow as pg_grow;

pub use pg_grow::GrowResult;
