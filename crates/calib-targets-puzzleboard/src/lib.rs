//! PuzzleBoard calibration target detector.
//!
//! The *PuzzleBoard* is a self-identifying chessboard target introduced by
//! Stelldinger (2024, [arXiv:2409.20127](https://arxiv.org/abs/2409.20127)):
//! a standard checkerboard with a small binary dot placed at the midpoint of
//! every interior edge. The dot pattern is designed so that a locally observed
//! fragment of the board identifies its absolute position on a master
//! 501 × 501 pattern — giving every detected chessboard corner an absolute
//! `(I, J)` label, with graceful degradation under occlusion, perspective
//! distortion, and low pixel-per-edge ratios.
//!
//! A 4 × 4 fragment pins the position *at a known orientation*, but a fragment
//! carries no cue for how the board was printed, so the decoder also searches
//! the board's admissible transforms — and over `transform × position` a 4 × 4
//! window is not unique. Clean uniqueness begins at 6 × 6 *squares*, which is a
//! span of 7 *corners*: that is why the pipeline asks for
//! [`min_window`](PuzzleBoardDecodeConfig::min_window) = 7 corners by default
//! and reports a *miss* rather than a guess below it. See [`code_maps`] for the
//! measurement.
//!
//! Encoding is the superposition of two cyclic binary sub-perfect maps:
//! - **A**: shape `(3, 167)` with window `(3, 3)₂` — one bit per horizontal edge
//! - **B**: shape `(167, 3)` with window `(3, 3)₂` — one bit per vertical edge
//!
//! The shipped maps are imported from the reference implementation
//! (PStelldinger/PuzzleBoard, CC0) so that boards interoperate with it, and are
//! embedded as bytes (`src/data/map_a.bin` / `map_b.bin`). All runtime lookups
//! go through [`code_maps`], which also documents the provenance and the
//! from-scratch generator.
//!
//! ## Quickstart
//!
//! ```no_run
//! use calib_targets_puzzleboard::{
//!     PuzzleBoardDetector, PuzzleBoardParams, PuzzleBoardSpec,
//! };
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let spec = PuzzleBoardSpec::new(12, 12, 1.0)?;
//! let params = PuzzleBoardParams::for_board(spec);
//! let detector = PuzzleBoardDetector::new(params)?;
//! // Feed a real greyscale image and raw ChESS corners to `detector.detect(…)`.
//! // See `examples/detect_puzzleboard.rs` for a working end-to-end example.
//! # Ok(()) }
//! ```
#![deny(missing_docs)]

pub mod code_maps;

// Opt-in introspection surface, gated behind the `diagnostics` feature (default
// off), mirroring `calib-targets-chessboard`. The diagnostics types are always
// compiled (the detector captures them internally) but only reach the public
// surface — the `diagnostics` module, the type re-exports, and
// `PuzzleBoardDetector::diagnose` / `diagnose_with_corners` — when the feature
// is enabled.
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
#[cfg(not(feature = "diagnostics"))]
pub(crate) mod diagnostics;

mod board;
mod detector;
mod params;

pub use board::{PuzzleBoardSpec, PuzzleBoardSpecError, MASTER_COLS, MASTER_ROWS};
pub use code_maps::{EDGE_MAP_A_COLS, EDGE_MAP_A_ROWS, EDGE_MAP_B_COLS, EDGE_MAP_B_ROWS};
pub use detector::{
    PuzzleBoardAdvancedTuning, PuzzleBoardCorner, PuzzleBoardDecodeConfig, PuzzleBoardDecodeInfo,
    PuzzleBoardDetectError, PuzzleBoardDetection, PuzzleBoardDetector, PuzzleBoardScoringMode,
    PuzzleBoardSearchMode, PuzzleBoardSymmetryMode,
};
#[cfg(feature = "diagnostics")]
pub use diagnostics::{
    PuzzleBoardDecodeDiagnostics, PuzzleBoardDiagnostics, PuzzleBoardObservedEdge,
};
pub use params::PuzzleBoardParams;

// Re-export the foreign types this crate's public API requires — the corner
// input and the image view — so depending on calib-targets-puzzleboard alone is
// sufficient (no direct calib-targets-chessboard / -core dependency needed).
pub use calib_targets_chessboard::ChessCorner;
pub use calib_targets_core::{
    GrayImageView, GridAlignment, GridTransform, GRID_TRANSFORMS_C4, GRID_TRANSFORMS_D4,
};
