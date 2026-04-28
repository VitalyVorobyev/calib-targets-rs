//! Internal regression / performance harness for the chessboard grid builder.
//!
//! The library half: dataset loading, single-image runner, baseline diffing,
//! report serialization. The CLI lives in `src/bin/bench.rs`.
//!
//! Design notes are in `docs/projective_grid_overview.md` (gap-fix sketch).
//!
//! # Public-vs-private contract
//!
//! Public images (`testdata/`) are tracked in git and gated against
//! committed baselines under `crates/calib-targets-bench/baselines/`.
//! Private images live under the gitignored `privatedata/`; their
//! baselines live alongside them under `privatedata/baselines/` and are
//! also gitignored. Per the workspace `CLAUDE.md` policy, no concrete
//! private numbers leak into public surfaces.

pub mod baseline;
pub mod dataset;
pub mod diff;
pub mod overlay;
pub mod runner;

pub use baseline::{Baseline, BaselineCorner, BaselineImage};
pub use dataset::{Dataset, DatasetEntry, ImageKind, Stitched};
pub use diff::{BaselineDiff, WrongPosition};
pub use runner::{run_entry, RunOutcome};

/// Workspace root inferred from `CARGO_MANIFEST_DIR`. The bench crate sits
/// two levels below the workspace root.
pub fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Schema version stamped on every baseline + run report.
pub const SCHEMA_VERSION: u32 = 1;

/// Position-equality tolerance, in pixels.
///
/// The chessboard detector's corner positions are inherited byte-for-byte
/// from the upstream ChESS detector — they cannot drift unless the grid
/// builder picked a *different* corner index for the same `(i, j)`. We
/// allow a small `f32::EPSILON`-scale slack to absorb a single rounding
/// step in the SVD-backed homography (the only stage where bitwise
/// equality is fragile across compiler / nalgebra revisions).
pub const POSITION_DRIFT_EPS_PX: f32 = 1e-3;
