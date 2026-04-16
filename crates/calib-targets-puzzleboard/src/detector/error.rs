use crate::board::PuzzleBoardSpecError;

/// Errors returned by the PuzzleBoard detector.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum PuzzleBoardDetectError {
    #[error(transparent)]
    BoardSpec(#[from] PuzzleBoardSpecError),
    #[error("chessboard not detected")]
    ChessboardNotDetected,
    #[error("not enough edges sampled (got {observed}, need {needed})")]
    NotEnoughEdges { observed: usize, needed: usize },
    #[error("decoding failed: no position match above confidence threshold")]
    DecodeFailed,
    #[error("decoded position is inconsistent with other components")]
    InconsistentPosition,
}
