//! Failures a PuzzlePole detection can report.

use crate::PuzzlePoleSpecError;

/// Why a PuzzlePole detection did not produce a result.
///
/// Every variant is a **miss**, never a wrong answer. The contract this
/// workspace gives is asymmetric — a miss is acceptable, a wrong corner ID is
/// not — so each of these is a place the detector declined to guess.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum PuzzlePoleDetectError {
    /// The pole specification is not one that can be built.
    #[error(transparent)]
    Spec(#[from] PuzzlePoleSpecError),
    /// No chessboard grid was assembled from the corner cloud.
    #[error("no chessboard grid was found")]
    ChessboardNotDetected,
    /// Too few edge dots were sampled to decode anything.
    #[error("only {observed} edge dots were sampled, {needed} are needed")]
    NotEnoughEdges {
        /// Dots actually sampled.
        observed: usize,
        /// Dots the configured window floor requires.
        needed: usize,
    },
    /// The fragment does not span enough of the pole.
    ///
    /// Reported in the pole's own axes, which are only known once the decode
    /// has fixed the orientation — a fragment 5 corners one way and 8 the other
    /// is decodable if the 8 runs along the axis and not if it runs around the
    /// circumference, and nothing before the decode can tell those apart.
    #[error(
        "the fragment spans {circumference} corner rows around the circumference and \
         {axial} along the axis; {needed_circumference} and {needed_axial} are needed"
    )]
    FragmentTooSmall {
        /// Corner rows spanned around the circumference.
        circumference: u32,
        /// Corner columns spanned along the axis.
        axial: u32,
        /// Circumference rows required.
        needed_circumference: u32,
        /// Axial columns required.
        needed_axial: u32,
    },
    /// Enough dots were sampled, but too few distinct code bits survived the
    /// period-3 vote to place the fragment.
    #[error("only {determined} distinct code bits were resolved, {needed} are needed")]
    NotEnoughLogicalBits {
        /// Bits resolved.
        determined: usize,
        /// Bits required.
        needed: usize,
    },
    /// No hypothesis was uniquely best, so none was accepted.
    #[error("no position on the pole uniquely explains the observed dots")]
    DecodeFailed,
}
