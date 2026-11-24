//! Failures a PuzzlePole detection can report.

use crate::PuzzlePoleSpecError;
use calib_targets_chessboard::ChessboardParamsError;

/// Why a PuzzlePole detection did not produce a result.
///
/// Most variants represent a **miss** (no decodable fragment in the image),
/// while configuration errors ([`Self::Spec`], [`Self::Chessboard`],
/// [`Self::WindowBelowFloor`]) report
/// invalid caller parameters (`is_not_found() == false`).
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum PuzzlePoleDetectError {
    /// The pole specification is not one that can be built.
    #[error(transparent)]
    Spec(#[from] PuzzlePoleSpecError),
    /// The chessboard detector parameters were invalid.
    #[error(transparent)]
    Chessboard(#[from] ChessboardParamsError),
    /// The decode window is smaller than the measured uniqueness floor
    /// ([`crate::MIN_CIRCUMFERENCE_SPAN`] × [`crate::MIN_AXIAL_SPAN`]); a
    /// smaller window admits placements that provably alias.
    #[error(
        "decode window {circumference} × {axial} is below the uniqueness floor \
         {min_circumference} × {min_axial}"
    )]
    WindowBelowFloor {
        /// Configured `min_circumference_span`.
        circumference: u32,
        /// Configured `min_axial_span`.
        axial: u32,
        /// The floor for `min_circumference_span`.
        min_circumference: u32,
        /// The floor for `min_axial_span`.
        min_axial: u32,
    },
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
    /// Two corners at different image positions decoded to the same place on
    /// the pole.
    ///
    /// A wrapped pole cannot present the same corner twice: the strip's spare
    /// piece is glued *under* the first, so only one of the pair is ever
    /// visible. Seeing both means either the target was never wrapped — a flat
    /// print of the wrap strip, which is a valid thing to have but is not a
    /// pole — or the grid was mislabelled. Neither can be told apart from here,
    /// and the contract says a miss beats a wrong ID, so the component is
    /// refused.
    #[error(
        "two corners decoded to pole position (axial {axial}, cyclic {cyclic}); a \
         wrapped pole cannot show the same corner twice"
    )]
    InconsistentPosition {
        /// Axial index claimed twice.
        axial: u32,
        /// Cyclic index claimed twice.
        cyclic: u32,
    },
}

impl PuzzlePoleDetectError {
    /// Whether this is a miss — no pole fragment in the image decodes to a
    /// unique position — rather than a misuse (an invalid pole spec or params).
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::Spec(_) | Self::Chessboard(_) | Self::WindowBelowFloor { .. } => false,
            Self::ChessboardNotDetected
            | Self::NotEnoughEdges { .. }
            | Self::FragmentTooSmall { .. }
            | Self::NotEnoughLogicalBits { .. }
            | Self::DecodeFailed
            | Self::InconsistentPosition { .. } => true,
        }
    }
}
