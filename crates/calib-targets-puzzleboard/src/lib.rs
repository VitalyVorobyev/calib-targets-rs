//! PuzzleBoard calibration target detector.
//!
//! The *PuzzleBoard* is a self-identifying chessboard target introduced by
//! Stelldinger (2024, [arXiv:2409.20127](https://arxiv.org/abs/2409.20127)):
//! a standard checkerboard with a small binary dot placed at the midpoint of
//! every interior edge. The dot pattern is designed so that any locally
//! observed fragment of the board (≥ 4 × 4 squares) uniquely identifies its
//! absolute position on a master 501 × 501 pattern — giving every detected
//! chessboard corner an absolute `(I, J)` label, with graceful degradation
//! under occlusion, perspective distortion, and low pixel-per-edge ratios.
//!
//! Encoding is the superposition of two cyclic binary sub-perfect maps:
//! - **A**: shape `(3, 167)` with window `(3, 3)₂` — one bit per horizontal edge
//! - **B**: shape `(167, 3)` with window `(3, 3)₂` — one bit per vertical edge
//!
//! The maps are generated once (see `tools/generate_code_maps.rs`) and shipped
//! as embedded bytes (`src/data/map_a.bin` / `map_b.bin`). All runtime lookups
//! go through [`code_maps`].
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
//! let params = PuzzleBoardParams::for_board(&spec);
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
// `PuzzleBoardDetector::detect_with_diagnostics` — when the feature is enabled.
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
#[cfg(not(feature = "diagnostics"))]
pub(crate) mod diagnostics;

mod board;
mod detector;
mod io;
mod params;

pub use board::{PuzzleBoardSpec, PuzzleBoardSpecError, MASTER_COLS, MASTER_ROWS};
pub use code_maps::{EDGE_MAP_A_COLS, EDGE_MAP_A_ROWS, EDGE_MAP_B_COLS, EDGE_MAP_B_ROWS};
pub use detector::{
    PuzzleBoardCorner, PuzzleBoardDecodeConfig, PuzzleBoardDecodeInfo, PuzzleBoardDetectError,
    PuzzleBoardDetectionResult, PuzzleBoardDetector, PuzzleBoardScoringMode, PuzzleBoardSearchMode,
};
#[cfg(feature = "diagnostics")]
pub use diagnostics::{
    PuzzleBoardDecodeDiagnostics, PuzzleBoardDiagnostics, PuzzleBoardObservedEdge,
};
pub use io::{PuzzleBoardDetectConfig, PuzzleBoardDetectReport, PuzzleBoardIoError};
pub use params::PuzzleBoardParams;

pub use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
