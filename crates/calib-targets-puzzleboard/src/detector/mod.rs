//! PuzzleBoard detection pipeline.

pub(crate) mod consensus;
pub(crate) mod decode;
pub(crate) mod edge_sampling;
mod error;
pub(crate) mod params;
mod pipeline;
mod result;

pub use error::PuzzleBoardDetectError;
pub use params::{
    PuzzleBoardAdvancedTuning, PuzzleBoardDecodeConfig, PuzzleBoardScoringMode,
    PuzzleBoardSearchMode, PuzzleBoardSymmetryMode,
};
pub use pipeline::PuzzleBoardDetector;
pub use result::{PuzzleBoardCorner, PuzzleBoardDecodeInfo, PuzzleBoardDetection};
