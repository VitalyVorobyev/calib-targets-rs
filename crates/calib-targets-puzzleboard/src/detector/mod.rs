//! PuzzleBoard detection pipeline.

mod decode;
mod edge_sampling;
mod error;
mod params;
mod pipeline;
mod result;

pub use error::PuzzleBoardDetectError;
pub use params::DecodeConfig;
pub use pipeline::PuzzleBoardDetector;
pub use result::{PuzzleBoardDecodeInfo, PuzzleBoardDetectionResult};
