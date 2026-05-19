//! End-to-end detection helpers.
//!
//! Each `detect_*` helper runs the `chess-corners` ChESS corner detector over
//! an image (or raw grayscale buffer) and then runs the matching target
//! detector, returning the detector's own result type. The `detect_*_best`
//! variants additionally sweep multiple parameter presets and keep the richest
//! detection. This module is gated on the `image` feature.

use crate::{charuco, chessboard, core, marker, puzzleboard};
use chess_corners::{Detector as ChessDetector, Threshold};
use nalgebra::Point2;

#[cfg(feature = "tracing")]
use tracing::instrument;

// Only the two `chess-corners` types the workspace's own public API
// legitimately exposes are re-exported. Advanced ChESS tuning types
// (`ChessConfig`, `RadonConfig`, `Threshold`, `RefinerKind`, …) come from the
// `chess-corners` crate directly — re-exporting the whole upstream surface
// would freeze it into this crate's semver contract.
pub use core::{DetectorConfig, OrientationMethod};

/// Errors produced by the high-level facade helpers.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum DetectError {
    /// A raw grayscale buffer's length does not match `width * height`.
    #[error("invalid grayscale image buffer length (expected {expected} bytes, got {got})")]
    InvalidGrayBuffer {
        /// Buffer length required by the declared dimensions, in bytes.
        expected: usize,
        /// Actual length of the supplied buffer, in bytes.
        got: usize,
    },

    /// The supplied grayscale image dimensions are invalid (e.g. zero-sized).
    #[error("invalid grayscale image dimensions (width={width}, height={height})")]
    InvalidGrayDimensions {
        /// Declared image width in pixels.
        width: u32,
        /// Declared image height in pixels.
        height: u32,
    },

    /// Construction of the ChArUco board layout failed.
    #[error(transparent)]
    CharucoBoard(#[from] charuco::CharucoBoardError),

    /// ChArUco detection failed.
    #[error(transparent)]
    CharucoDetect(#[from] charuco::CharucoDetectError),

    /// Construction of the PuzzleBoard specification failed.
    #[error(transparent)]
    PuzzleBoardSpec(#[from] puzzleboard::PuzzleBoardSpecError),

    /// PuzzleBoard detection failed.
    #[error(transparent)]
    PuzzleBoardDetect(#[from] puzzleboard::PuzzleBoardDetectError),
}

/// Reasonable default settings for the `chess-corners` ChESS detector.
///
/// Built on top of [`DetectorConfig::chess`] but overrides the acceptance
/// threshold to [`Threshold::Absolute(15.0)`][Threshold::Absolute]. Upstream's
/// paper-faithful default is `Threshold::Absolute(0.0)`, which is correct in
/// principle (any strictly positive ChESS response is a corner candidate) but
/// produces hundreds of weak responses on real-world images. Both the
/// seed-and-grow chessboard detector and the topological grid pipeline are
/// sensitive to that noise floor: on `testdata/puzzleboard_reference/example3.png`,
/// threshold `0.0` produces zero labelled corners while `15.0` recovers the
/// full 30-corner component; on `testdata/small0.png` the labelled count
/// rises from 78 to 129; and on the `02-topo-grid/` synthetic suite the
/// topological pipeline only clears every recall gate at `≥ 15.0`. The
/// cutoff was chosen by sweeping the public testdata regression set; see
/// `crates/calib-targets/examples/threshold_sweep.rs`.
///
/// Callers wanting the raw upstream behaviour can construct
/// [`DetectorConfig::chess`] directly.
pub fn default_chess_config() -> DetectorConfig {
    DetectorConfig::chess().with_threshold(Threshold::Absolute(15.0))
}

/// Convert an `image::GrayImage` into the lightweight `calib-targets-core` view type.
pub fn gray_view(img: &::image::GrayImage) -> core::GrayImageView<'_> {
    core::GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    }
}

/// Apply a same-size Gaussian blur with the given standard deviation.
///
/// Convenience helper for callers who want to denoise an image before
/// running corner detection. The library used to bury an optional blur
/// inside every `detect_*` function; that argument has been removed in
/// favour of this explicit helper so each detection entry point takes
/// only the (already-prepared) image and detector parameters.
///
/// Pass `blur_sigma_px = 0.0` (or any non-finite value) to get back a
/// copy of the input unchanged. Typical values for ChESS corner
/// detection sit between `0.5` and `2.0`.
pub fn preprocess(img: &::image::GrayImage, blur_sigma_px: f32) -> ::image::GrayImage {
    if blur_sigma_px.is_finite() && blur_sigma_px > 0.0 {
        ::image::imageops::blur(img, blur_sigma_px)
    } else {
        img.clone()
    }
}

/// Detect ChESS corners and adapt them into [`calib_targets_chessboard::ChessCorner`].
///
/// Operates on the image as supplied — callers should run [`preprocess`]
/// first if they want a Gaussian pre-blur. Corner positions are returned
/// in the input image frame.
#[cfg_attr(
    feature = "tracing",
    instrument(level = "info", skip(img, cfg), fields(width = img.width(), height = img.height()))
)]
pub fn detect_corners(
    img: &::image::GrayImage,
    cfg: &DetectorConfig,
) -> Vec<chessboard::ChessCorner> {
    let Ok(mut detector) = ChessDetector::new(*cfg) else {
        return Vec::new();
    };
    detector
        .detect(img)
        .unwrap_or_default()
        .iter()
        .map(adapt_chess_corner)
        .collect()
}

/// Convenience overload using [`default_chess_config`].
pub fn detect_corners_default(img: &::image::GrayImage) -> Vec<chessboard::ChessCorner> {
    detect_corners(img, &default_chess_config())
}

/// Run the chessboard detector end-to-end: ChESS corners -> chessboard grid.
///
/// This is the primary chessboard entry point. It runs ChESS corner
/// detection with the supplied [`DetectorConfig`] and then runs the
/// chessboard detector with the supplied [`chessboard::DetectorParams`];
/// corner positions are returned in the input image frame. Callers that
/// do not need to tune the ChESS detector pass [`default_chess_config`]
/// by reference (`&default_chess_config()`).
///
/// Named variants of this entry point:
/// - [`detect_chessboard_all`] — returns every same-board component, not
///   just the first.
/// - [`detect_chessboard_best`] — runs a multi-config sweep and keeps the
///   richest result.
/// - [`detect_chessboard_from_gray_u8`] — takes a raw grayscale byte
///   buffer instead of an [`::image::GrayImage`].
/// - [`detect_chessboard_with_diagnostics`] — returns the diagnostics
///   channel ([`chessboard::diagnostics::DebugFrame`]) instead of the
///   plain detection.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, chess_cfg, params),
        fields(width = img.width(), height = img.height())
    )
)]
pub fn detect_chessboard(
    img: &::image::GrayImage,
    chess_cfg: &DetectorConfig,
    params: &chessboard::DetectorParams,
) -> Option<chessboard::ChessboardDetection> {
    let corners = detect_corners(img, chess_cfg);
    let detector = chessboard::Detector::new(params.clone());
    detector.detect(&corners)
}

/// Multi-component variant of [`detect_chessboard`]: returns every same-board
/// component the detector recovers (capped by [`chessboard::DetectorParams::max_components`]).
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, chess_cfg, params),
        fields(width = img.width(), height = img.height())
    )
)]
pub fn detect_chessboard_all(
    img: &::image::GrayImage,
    chess_cfg: &DetectorConfig,
    params: &chessboard::DetectorParams,
) -> Vec<chessboard::ChessboardDetection> {
    let corners = detect_corners(img, chess_cfg);
    let detector = chessboard::Detector::new(params.clone());
    detector.detect_all(&corners)
}

/// Diagnostics-channel variant of [`detect_chessboard`]: returns a
/// [`chessboard::diagnostics::DebugFrame`] with every input corner's terminal
/// stage, per-iteration traces, and the labelled detection (when one was
/// produced). Always returns a frame — never panics — so the caller can see
/// *why* detection failed.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, chess_cfg, params),
        fields(width = img.width(), height = img.height())
    )
)]
pub fn detect_chessboard_with_diagnostics(
    img: &::image::GrayImage,
    chess_cfg: &DetectorConfig,
    params: &chessboard::DetectorParams,
) -> chessboard::diagnostics::DebugFrame {
    let corners = detect_corners(img, chess_cfg);
    let detector = chessboard::Detector::new(params.clone());
    detector.detect_with_diagnostics(&corners)
}

/// Run the ChArUco detector end-to-end: ChESS corners -> grid -> markers -> alignment -> IDs.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, params),
        fields(
            width = img.width(),
            height = img.height(),
            board_rows = params.board.rows,
            board_cols = params.board.cols
        )
    )
)]
pub fn detect_charuco(
    img: &::image::GrayImage,
    params: &charuco::CharucoParams,
) -> Result<charuco::CharucoDetectionResult, DetectError> {
    let corners = detect_corners_default(img);
    let detector = charuco::CharucoDetector::new(params.clone())?;
    Ok(detector.detect(&gray_view(img), &corners)?)
}

/// Run the PuzzleBoard detector end-to-end: ChESS corners → chessboard grid
/// → edge-bit sampling → cross-correlation decode → absolute master IDs.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, params),
        fields(
            width = img.width(),
            height = img.height(),
            board_rows = params.board.rows,
            board_cols = params.board.cols
        )
    )
)]
pub fn detect_puzzleboard(
    img: &::image::GrayImage,
    params: &puzzleboard::PuzzleBoardParams,
) -> Result<puzzleboard::PuzzleBoardDetectionResult, DetectError> {
    let corners = detect_corners_default(img);
    let detector = puzzleboard::PuzzleBoardDetector::new(params.clone())?;
    Ok(detector.detect(&gray_view(img), &corners)?)
}

/// Build a reasonable default PuzzleBoard parameter set for a
/// `rows × cols` board (square counts).
pub fn default_puzzleboard_params(
    rows: u32,
    cols: u32,
) -> Result<puzzleboard::PuzzleBoardParams, DetectError> {
    let spec = puzzleboard::PuzzleBoardSpec::new(rows, cols, 1.0)?;
    Ok(puzzleboard::PuzzleBoardParams::for_board(&spec))
}

/// Run the checkerboard+circles marker board detector end-to-end.
///
/// Corner detection uses `params.chessboard.chess`.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, params),
        fields(width = img.width(), height = img.height())
    )
)]
pub fn detect_marker_board(
    img: &::image::GrayImage,
    params: &marker::MarkerBoardParams,
) -> Option<marker::MarkerBoardDetectionResult> {
    let corners = detect_corners_default(img);
    let detector = marker::MarkerBoardDetector::new(params.clone());
    detector.detect_from_image_and_corners(&gray_view(img), &corners)
}

// ---------------------------------------------------------------------------
// Multi-config sweep helpers
// ---------------------------------------------------------------------------

/// Multi-config-sweep variant of [`detect_chessboard`]: tries every chessboard
/// parameter config and returns the best result (most corners).
///
/// ChESS corner detection runs once with the supplied [`DetectorConfig`] and
/// the corners are reused across every config in the sweep.
pub fn detect_chessboard_best(
    img: &::image::GrayImage,
    chess_cfg: &DetectorConfig,
    param_configs: &[chessboard::DetectorParams],
) -> Option<chessboard::ChessboardDetection> {
    let corners = detect_corners(img, chess_cfg);
    param_configs
        .iter()
        .filter_map(|params| chessboard::Detector::new(params.clone()).detect(&corners))
        .max_by_key(|d| d.corners.len())
}

/// Try multiple ChArUco parameter configs, return the best result
/// (most markers, then most corners).
pub fn detect_charuco_best(
    img: &::image::GrayImage,
    configs: &[charuco::CharucoParams],
) -> Result<charuco::CharucoDetectionResult, DetectError> {
    let mut best: Option<charuco::CharucoDetectionResult> = None;
    let mut last_err = None;

    let corners = detect_corners_default(img);
    for params in configs {
        let detector = match charuco::CharucoDetector::new(params.clone()) {
            Ok(d) => d,
            Err(e) => {
                last_err = Some(DetectError::from(e));
                continue;
            }
        };
        match detector.detect(&gray_view(img), &corners) {
            Ok(result) => {
                let dominated = best
                    .as_ref()
                    .is_some_and(|b| charuco_score(b) >= charuco_score(&result));
                if !dominated {
                    best = Some(result);
                }
            }
            Err(e) => {
                last_err = Some(DetectError::from(e));
            }
        }
    }

    best.ok_or_else(|| {
        last_err.unwrap_or(DetectError::CharucoDetect(
            charuco::CharucoDetectError::NoMarkers,
        ))
    })
}

/// Try multiple PuzzleBoard parameter configs. Picks the configuration that
/// labels the most corners with the highest mean decode confidence.
pub fn detect_puzzleboard_best(
    img: &::image::GrayImage,
    configs: &[puzzleboard::PuzzleBoardParams],
) -> Result<puzzleboard::PuzzleBoardDetectionResult, DetectError> {
    let mut best: Option<puzzleboard::PuzzleBoardDetectionResult> = None;
    let mut last_err: Option<DetectError> = None;
    for params in configs {
        match detect_puzzleboard(img, params) {
            Ok(r) => {
                let better = match &best {
                    None => true,
                    Some(b) => {
                        let key_new = (r.corners.len(), r.decode.mean_confidence);
                        let key_old = (b.corners.len(), b.decode.mean_confidence);
                        key_new.0 > key_old.0 || (key_new.0 == key_old.0 && key_new.1 > key_old.1)
                    }
                };
                if better {
                    best = Some(r);
                }
            }
            Err(e) => last_err = Some(e),
        }
    }
    best.ok_or_else(|| {
        last_err.unwrap_or(DetectError::PuzzleBoardDetect(
            puzzleboard::PuzzleBoardDetectError::DecodeFailed,
        ))
    })
}

/// Try multiple marker board parameter configs, return the best result (most corners).
pub fn detect_marker_board_best(
    img: &::image::GrayImage,
    configs: &[marker::MarkerBoardParams],
) -> Option<marker::MarkerBoardDetectionResult> {
    let corners = detect_corners_default(img);
    configs
        .iter()
        .filter_map(|params| {
            let detector = marker::MarkerBoardDetector::new(params.clone());
            detector.detect_from_image_and_corners(&gray_view(img), &corners)
        })
        .max_by_key(|r| r.corners.len())
}

/// Scoring key for ChArUco results: (marker count, corner count).
fn charuco_score(r: &charuco::CharucoDetectionResult) -> (usize, usize) {
    (r.markers.len(), r.corners.len())
}

/// Build an `image::GrayImage` from a raw grayscale buffer.
pub fn gray_image_from_slice(
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<::image::GrayImage, DetectError> {
    let w = usize::try_from(width).ok();
    let h = usize::try_from(height).ok();
    let Some((w, h)) = w.zip(h) else {
        return Err(DetectError::InvalidGrayDimensions { width, height });
    };
    let Some(expected) = w.checked_mul(h) else {
        return Err(DetectError::InvalidGrayDimensions { width, height });
    };
    if pixels.len() != expected {
        return Err(DetectError::InvalidGrayBuffer {
            expected,
            got: pixels.len(),
        });
    }
    ::image::GrayImage::from_raw(width, height, pixels.to_vec())
        .ok_or(DetectError::InvalidGrayDimensions { width, height })
}

/// Raw-buffer variant of [`detect_chessboard`]: runs the chessboard detector
/// from a raw grayscale byte buffer.
///
/// `pixels` must have length `width * height`. Returns `Ok(None)` when no board is found,
/// or `Err` when the buffer dimensions are invalid.
pub fn detect_chessboard_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: &DetectorConfig,
    params: &chessboard::DetectorParams,
) -> Result<Option<chessboard::ChessboardDetection>, DetectError> {
    let img = gray_image_from_slice(width, height, pixels)?;
    Ok(detect_chessboard(&img, chess_cfg, params))
}

/// Run the ChArUco detector from a raw grayscale byte buffer.
///
/// `pixels` must have length `width * height`. Returns `Err` when the buffer dimensions
/// are invalid or detection fails (e.g. no markers found, alignment failed).
pub fn detect_charuco_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    params: &charuco::CharucoParams,
) -> Result<charuco::CharucoDetectionResult, DetectError> {
    let img = gray_image_from_slice(width, height, pixels)?;
    detect_charuco(&img, params)
}

/// Run the PuzzleBoard detector from a raw grayscale byte buffer.
pub fn detect_puzzleboard_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    params: &puzzleboard::PuzzleBoardParams,
) -> Result<puzzleboard::PuzzleBoardDetectionResult, DetectError> {
    let img = gray_image_from_slice(width, height, pixels)?;
    detect_puzzleboard(&img, params)
}

/// Run the checkerboard+circles marker board detector from a raw grayscale byte buffer.
///
/// `pixels` must have length `width * height`. Returns `Ok(None)` when no board is found,
/// or `Err` when the buffer dimensions are invalid.
pub fn detect_marker_board_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    params: &marker::MarkerBoardParams,
) -> Result<Option<marker::MarkerBoardDetectionResult>, DetectError> {
    let img = gray_image_from_slice(width, height, pixels)?;
    Ok(detect_marker_board(&img, params))
}

fn adapt_chess_corner(c: &chess_corners::CornerDescriptor) -> chessboard::ChessCorner {
    chessboard::ChessCorner {
        position: Point2::new(c.x, c.y),
        axes: [
            core::AxisEstimate {
                angle: c.axes[0].angle,
                sigma: c.axes[0].sigma,
            },
            core::AxisEstimate {
                angle: c.axes[1].angle,
                sigma: c.axes[1].sigma,
            },
        ],
        contrast: c.contrast,
        fit_rms: c.fit_rms,
        strength: c.response,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess_corners::DetectionStrategy;

    #[test]
    fn default_chess_config_overrides_threshold() {
        // Workspace default deliberately overrides the upstream
        // paper-contract (`Threshold::Absolute(0.0)`) with a small
        // noise-floor cutoff tuned on the public testdata regression sweep
        // — see the rustdoc on `default_chess_config`.
        let cfg = default_chess_config();
        assert_eq!(cfg.threshold, Threshold::Absolute(15.0));

        // Strategy must still be the ChESS kernel pipeline (not Radon),
        // and the multiscale / upscale top-level fields must match the
        // single-scale ChESS preset.
        assert!(matches!(cfg.strategy, DetectionStrategy::Chess(_)));
        let baseline = DetectorConfig::chess();
        assert_eq!(cfg.multiscale, baseline.multiscale);
        assert_eq!(cfg.upscale, baseline.upscale);
        assert_eq!(cfg.merge_radius, baseline.merge_radius);
        assert_eq!(cfg.orientation_method, baseline.orientation_method);

        // The nested ChESS strategy fields stay at upstream defaults so
        // the override is purely the acceptance threshold.
        let DetectionStrategy::Chess(chess) = cfg.strategy else {
            unreachable!("matched above");
        };
        let DetectionStrategy::Chess(chess_baseline) = baseline.strategy else {
            unreachable!("baseline preset is ChESS");
        };
        assert_eq!(chess.ring, chess_baseline.ring);
        assert_eq!(chess.descriptor_ring, chess_baseline.descriptor_ring);
        assert_eq!(chess.nms_radius, chess_baseline.nms_radius);
        assert_eq!(chess.min_cluster_size, chess_baseline.min_cluster_size);
        assert_eq!(chess.refiner, chess_baseline.refiner);
    }
}
