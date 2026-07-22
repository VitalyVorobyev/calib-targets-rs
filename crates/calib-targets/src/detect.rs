//! End-to-end detection helpers.
//!
//! Each `detect_*` helper runs the `chess-corners` ChESS corner detector over
//! an image (or raw grayscale buffer) and then runs the matching target
//! detector, returning the detector's own result type. The `detect_*_best`
//! variants additionally sweep multiple parameter presets and keep the richest
//! detection. This module is gated on the `image` feature.

use crate::{charuco, chessboard, core, marker, puzzleboard};
use chess_corners::Detector as ChessDetector;
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

    /// A multi-config sweep was handed configs that disagree on `chess`.
    ///
    /// The `detect_*_best` helpers run ChESS corner detection **once** and
    /// reuse the corner cloud across every config in the sweep, so the sweep
    /// has no way to honour more than one corner front-end. Sweep the
    /// target-detector parameters, and if you need to compare corner
    /// front-ends, call the single-config entry point once per
    /// [`DetectorConfig`].
    #[error(
        "sweep configs disagree on the `chess` corner front-end; corner \
         detection runs once per sweep, so every config must request the same \
         DetectorConfig"
    )]
    InconsistentChessConfig,
}

/// Resolve the one ChESS config a `detect_*_best` sweep may use.
///
/// Returns [`DetectError::InconsistentChessConfig`] if the configs disagree.
/// An empty sweep resolves to the workspace default; the callers below then
/// return their own "no config produced a result" outcome.
fn sweep_chess_config(
    configs: impl IntoIterator<Item = DetectorConfig>,
) -> Result<DetectorConfig, DetectError> {
    let mut it = configs.into_iter();
    let Some(first) = it.next() else {
        return Ok(default_chess_config());
    };
    if it.any(|cfg| cfg != first) {
        return Err(DetectError::InconsistentChessConfig);
    }
    Ok(first)
}

// `default_chess_config` is defined in `calib-targets-core` so every
// detector's params struct can default its `chess` field without depending
// on this facade; it is re-exported here because
// `calib_targets::detect::default_chess_config` is the documented path.
pub use calib_targets_core::default_chess_config;

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
    let detector = chessboard::Detector::new(params.clone()).ok()?;
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
    let Ok(detector) = chessboard::Detector::new(params.clone()) else {
        return Vec::new();
    };
    detector.detect_all(&corners)
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
    let corners = detect_corners(img, &params.chess);
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
    let corners = detect_corners(img, &params.chess);
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
    let corners = detect_corners(img, &params.chess);
    let detector = marker::MarkerBoardDetector::new(params.clone()).ok()?;
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
        .filter_map(|params| {
            chessboard::Detector::new(params.clone())
                .ok()?
                .detect(&corners)
        })
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

    let chess_cfg = sweep_chess_config(configs.iter().map(|p| p.chess))?;
    let corners = detect_corners(img, &chess_cfg);
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
///
/// Unlike [`detect_charuco_best`] and [`detect_marker_board_best`], this
/// sweep runs a **full detection per config**, corner pass included, so each
/// config's [`PuzzleBoardParams::chess`] is honoured independently and the
/// configs need not agree on it. That costs one ChESS pass per config; it is
/// what makes sweeping corner front-ends (e.g. single-scale vs
/// `UpscaleConfig::Fixed(2)`) possible here.
///
/// [`PuzzleBoardParams::chess`]: puzzleboard::PuzzleBoardParams::chess
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
    let chess_cfg = sweep_chess_config(configs.iter().map(|p| p.chess)).ok()?;
    let corners = detect_corners(img, &chess_cfg);
    configs
        .iter()
        .filter_map(|params| {
            let detector = marker::MarkerBoardDetector::new(params.clone()).ok()?;
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
    // `CornerDescriptor::axes` is `None` when the upstream orientation fit was
    // skipped (`DetectorConfig::without_orientation`). Every detection entry
    // point in this crate leaves orientation enabled, so this is the
    // defensive branch rather than the common one — but it must not fabricate
    // a confident axis. `AxisEstimate::default()` is the workspace's existing
    // no-information sentinel (`sigma = π`), which every axis-aware stage
    // already treats as "skip this corner", so an orientation-free descriptor
    // degrades to a bare position instead of poisoning the grid with a
    // zero-sigma axis at angle 0.
    let axes = c.axes.map_or_else(
        || [core::AxisEstimate::default(); 2],
        |axes| {
            [
                core::AxisEstimate {
                    angle: axes[0].angle,
                    sigma: axes[0].sigma,
                },
                core::AxisEstimate {
                    angle: axes[1].angle,
                    sigma: axes[1].sigma,
                },
            ]
        },
    );
    chessboard::ChessCorner {
        position: Point2::new(c.x, c.y),
        axes,
        strength: c.response,
    }
}
