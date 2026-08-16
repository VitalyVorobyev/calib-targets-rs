//! PuzzleBoard detection pipeline.

mod decode;
mod edge_sampling;
mod error;
mod params;
mod pipeline;
mod result;

pub use error::PuzzleBoardDetectError;
pub use params::{
    PuzzleBoardAdvancedTuning, PuzzleBoardDecodeConfig, PuzzleBoardScoringMode,
    PuzzleBoardSearchMode, PuzzleBoardSymmetryMode,
};
pub use pipeline::PuzzleBoardDetector;
pub use result::{PuzzleBoardCorner, PuzzleBoardDecodeInfo, PuzzleBoardDetection};
