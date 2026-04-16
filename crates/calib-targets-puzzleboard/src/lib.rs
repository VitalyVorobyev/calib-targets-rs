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
//! use calib_targets_core::{Corner, GrayImageView};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let spec = PuzzleBoardSpec::new(12, 12, 1.0)?;
//! let params = PuzzleBoardParams::for_board(&spec);
//! let detector = PuzzleBoardDetector::new(params)?;
//!
//! let pixels = vec![0u8; 32 * 32];
//! let view = GrayImageView { width: 32, height: 32, data: &pixels };
//! let corners: Vec<Corner> = Vec::new();
//! let _ = detector.detect(&view, &corners)?;
//! # Ok(()) }
//! ```

pub mod code_maps;
pub mod render_bits;

mod board;
mod detector;
mod io;
mod params;

pub use board::{PuzzleBoardSpec, PuzzleBoardSpecError, MASTER_COLS, MASTER_ROWS};
pub use code_maps::{
    ObservedEdge, EDGE_MAP_A_COLS, EDGE_MAP_A_ROWS, EDGE_MAP_B_COLS, EDGE_MAP_B_ROWS,
};
pub use detector::{
    DecodeConfig, PuzzleBoardDecodeInfo, PuzzleBoardDetectError, PuzzleBoardDetectionResult,
    PuzzleBoardDetector,
};
pub use io::{PuzzleBoardDetectConfig, PuzzleBoardDetectReport, PuzzleBoardIoError};
pub use params::PuzzleBoardParams;

pub use calib_targets_core::{GridAlignment, GridTransform, GRID_TRANSFORMS_D4};
