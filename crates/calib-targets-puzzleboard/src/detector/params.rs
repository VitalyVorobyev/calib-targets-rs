//! Knobs for the decoding stage and associated validation helpers.

use serde::{Deserialize, Serialize};

use crate::detector::error::PuzzleBoardDetectError;

/// Strategy for recovering the master-map origin during decode.
///
/// The full 501² × 8-D4 scan is a backstop that works regardless of what the
/// caller knows about the physical board. Once you have decoded the board
/// once (via `Full`), the recovered `master_origin_{row,col}` can be fed
/// back into `KnownOrigin` for dramatically faster subsequent decodes of the
/// same physical board.
///
/// **Note:** the decoder's `master_origin_{row,col}` is *not* generally equal
/// to the render-time [`PuzzleBoardSpec::origin_row`] / `origin_col` — they
/// differ by a Chinese-remainder offset induced by the chessboard detector's
/// local-frame convention (local `(0, 0)` sits at the first interior corner,
/// not the print's corner `(0, 0)`). Use
/// [`crate::detector::PuzzleBoardDetectionResult::as_known_origin`] to
/// derive a correctly-populated `KnownOrigin` from a prior `Full` result.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PuzzleBoardSearchMode {
    /// Scan every `(D4, master_row, master_col)` in the 501 × 501 master.
    ///
    /// The default — works for any printed board regardless of its master-map
    /// origin.
    #[default]
    Full,
    /// Scan only master origins within `window_radius` cells of the declared
    /// `(origin_row, origin_col)`, across all 8 D4 transforms.
    ///
    /// For a small `window_radius` (≲ 4) this is ~10³ × faster than
    /// [`PuzzleBoardSearchMode::Full`]. The trade-off: if the caller lies
    /// about the origin or the board is re-oriented (D4 change) beyond the
    /// window, decode may fail silently.
    KnownOrigin {
        /// Decoder's expected master-map row for chessboard-local `(0, 0)`.
        /// See the enum docs — this is the decoder's origin, not the render
        /// spec's `origin_row`.
        origin_row: i32,
        /// Decoder's expected master-map column for chessboard-local `(0, 0)`.
        origin_col: i32,
        /// Half-width of the square scan window, in master-map cells.
        ///
        /// `window_radius = 0` scans only the declared origin. Typical values
        /// are 1–3 to allow for off-by-one slop and minor D4 drift.
        window_radius: u32,
    },
}

/// Tuning parameters for the decoding stage.
#[non_exhaustive]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PuzzleBoardDecodeConfig {
    /// Minimum window size (in squares) required to attempt a decode.
    ///
    /// The paper guarantees 3×3 = 99.33 % unique and 4×4 = 100 % unique.
    /// Default is 4 for calibration use.
    #[serde(default = "default_min_window")]
    pub min_window: u32,
    /// Per-bit confidence floor — bits below this are treated as unknown.
    #[serde(default = "default_min_bit_confidence")]
    pub min_bit_confidence: f32,
    /// Maximum fraction of bits allowed to be wrong after majority voting.
    ///
    /// The paper allows up to 40 % (401 / 1002). Default is 0.3.
    #[serde(default = "default_max_bit_error_rate")]
    pub max_bit_error_rate: f32,
    /// If true, attempt to decode each connected component independently.
    #[serde(default = "default_search_all_components")]
    pub search_all_components: bool,
    /// Sample radius for edge-midpoint disk (fraction of the edge length).
    #[serde(default = "default_sample_radius_rel")]
    pub sample_radius_rel: f32,
    /// Master-origin search strategy. Defaults to
    /// [`PuzzleBoardSearchMode::Full`]; set to
    /// [`PuzzleBoardSearchMode::KnownOrigin`] when the physical board's
    /// master-map origin is known ahead of time.
    #[serde(default)]
    pub search_mode: PuzzleBoardSearchMode,
}

fn default_min_window() -> u32 {
    4
}
fn default_min_bit_confidence() -> f32 {
    0.15
}
fn default_max_bit_error_rate() -> f32 {
    0.30
}
fn default_search_all_components() -> bool {
    true
}
fn default_sample_radius_rel() -> f32 {
    1.0 / 6.0
}

impl Default for PuzzleBoardDecodeConfig {
    fn default() -> Self {
        Self {
            min_window: default_min_window(),
            min_bit_confidence: default_min_bit_confidence(),
            max_bit_error_rate: default_max_bit_error_rate(),
            search_all_components: default_search_all_components(),
            sample_radius_rel: default_sample_radius_rel(),
            search_mode: PuzzleBoardSearchMode::default(),
        }
    }
}

impl PuzzleBoardDecodeConfig {
    /// Construct with explicit values for every field except `search_mode`,
    /// which defaults to [`PuzzleBoardSearchMode::Full`].
    ///
    /// To use a different search mode, assign the field after construction:
    /// ```ignore
    /// let mut cfg = PuzzleBoardDecodeConfig::new(...);
    /// cfg.search_mode = PuzzleBoardSearchMode::KnownOrigin { window_radius: 2 };
    /// ```
    pub fn new(
        min_window: u32,
        min_bit_confidence: f32,
        max_bit_error_rate: f32,
        search_all_components: bool,
        sample_radius_rel: f32,
    ) -> Self {
        Self {
            min_window,
            min_bit_confidence,
            max_bit_error_rate,
            search_all_components,
            sample_radius_rel,
            search_mode: PuzzleBoardSearchMode::default(),
        }
    }
}

/// Minimum number of observed interior edges required to attempt decoding.
///
/// A decode window of `w × w` squares produces `2w(w-1)` interior edges.
pub(crate) fn required_edges(min_window: u32) -> usize {
    let w = min_window.max(3) as usize;
    2 * w * (w - 1)
}

/// Return an error if fewer than `needed` edges were observed.
pub(crate) fn ensure_min_edges(
    observed: usize,
    needed: usize,
) -> Result<(), PuzzleBoardDetectError> {
    if observed < needed {
        return Err(PuzzleBoardDetectError::NotEnoughEdges { observed, needed });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_edges_scales_with_window() {
        assert_eq!(required_edges(3), 12);
        assert_eq!(required_edges(4), 24);
        assert_eq!(required_edges(5), 40);
    }

    #[test]
    fn min_edges_check_reports_filtered_count() {
        let err = ensure_min_edges(7, required_edges(4)).expect_err("too few edges");
        assert!(matches!(
            err,
            PuzzleBoardDetectError::NotEnoughEdges {
                observed: 7,
                needed: 24
            }
        ));
    }
}
