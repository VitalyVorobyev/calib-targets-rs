//! End-to-end detection helpers.
//!
//! Each `detect_*` helper runs the `chess-corners` ChESS corner detector over
//! a [`GrayImageView`](core::GrayImageView) (or raw grayscale buffer) and then
//! runs the matching target detector. Every helper returns `Result<{X}Detection, DetectError>` — never a
//! bare `Option` — so consuming several target types shares one `?`/`match`
//! control-flow shape. A board that is simply not present surfaces as the
//! detector's own typed miss ([`DetectError::CharucoDetect`],
//! [`DetectError::PuzzleBoardDetect`], [`DetectError::MarkerBoardDetect`]) or,
//! for the corner-cloud-level chessboard detector, as
//! [`DetectError::NoDetection`]. The `detect_*_best` variants additionally
//! sweep multiple parameter presets and keep the richest detection.
//!
//! When the default `image` feature is enabled, use [`gray_view`] to borrow an
//! `image::GrayImage` zero-copy and [`preprocess`] for optional Gaussian pre-blur.
//!
//! # Where the corner front-end config lives
//!
//! The corner (ChESS) front-end is configured differently for the two shapes of
//! target, and it is worth knowing which before you reach for a helper:
//!
//! - **ChArUco, PuzzleBoard, and marker boards** are whole-image pipelines that
//!   own their corner pass. Set the front-end on the params bundle you pass:
//!   `params.chess` (see [`CharucoParams::chess`](charuco::CharucoParams::chess),
//!   [`PuzzleBoardParams::chess`](puzzleboard::PuzzleBoardParams::chess),
//!   [`MarkerBoardParams::chess`](marker::MarkerBoardParams::chess)). There is
//!   no separate front-end argument for these entry points — pass a corner
//!   cloud to the `*_with_corners` variants instead when you have one.
//! - **Plain chessboards** are corner-cloud consumers, so
//!   [`detect_chessboard`] takes the front-end explicitly:
//!   `detect_chessboard(img, &chess_cfg, &params)`. Pass
//!   [`default_chess_config`] unless you have a reason to override it.
//!
//! The asymmetry is deliberate: the chessboard detector is reusable on any
//! corner cloud (custom upstreams, pre-detected corners), so the facade keeps
//! its corner config separable; a compound target always owns its full image
//! pipeline, so bundling the front-end into its params is the ergonomic choice.
//!
//! # Entry points per compound target
//!
//! Each of ChArUco / PuzzleBoard / marker board has the same five helpers:
//!
//! | Helper | Corner cloud | Diagnostics |
//! |---|---|---|
//! | `detect_t` | run from `params.chess` | no |
//! | `detect_t_with_corners` | supplied by the caller | no |
//! | `detect_t_best` | run per distinct `chess`, deduplicated | no |
//! | `diagnose_t` | run from `params.chess` | yes |
//! | `diagnose_t_with_corners` | supplied by the caller | yes |
//!
//! Argument order is data-first, config-last: `(img, corners, params)`. The
//! `*_with_corners` pair exists for callers that already have a corner cloud —
//! a custom upstream, or one shared across several detectors — and want to skip
//! the redundant pass.

use crate::{charuco, chessboard, core, marker, puzzleboard};

#[cfg(feature = "tracing")]
use tracing::instrument;

// Only the `chess-corners` types the workspace's own public API legitimately
// exposes are re-exported: `DetectorConfig` and the types of the top-level
// fields callers routinely set on it. Advanced ChESS tuning types
// (`ChessConfig`, `RadonConfig`, `SaddlePointConfig`, …) come from the
// `chess-corners` crate directly — re-exporting the whole upstream surface
// would freeze it into this crate's semver contract.
pub use core::{
    DetectorConfig, ImageRefiner, NoiseLevel, OnRefinerReject, OrientationMethod, Uncertainty,
};

#[cfg(feature = "diagnostics")]
mod diagnose;
mod sweep;

#[cfg(feature = "diagnostics")]
pub use diagnose::*;
pub use sweep::*;

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

    /// Construction of the PuzzlePole specification failed.
    #[error(transparent)]
    PuzzlePoleSpec(#[from] puzzleboard::PuzzlePoleSpecError),

    /// PuzzlePole detection failed.
    #[error(transparent)]
    PuzzlePoleDetect(#[from] puzzleboard::PuzzlePoleDetectError),

    /// Marker-board detection failed.
    #[error(transparent)]
    MarkerBoardDetect(#[from] marker::MarkerBoardDetectError),

    /// Invalid marker-board detector parameters.
    #[error(transparent)]
    MarkerBoardParams(#[from] marker::MarkerBoardParamsError),

    /// Invalid chessboard detector parameters.
    #[error(transparent)]
    ChessboardParams(#[from] chessboard::ChessboardParamsError),

    /// An empty configuration slice was passed to a `detect_*_best` sweep.
    #[error("empty parameter sweep passed to `{op}`")]
    EmptySweep {
        /// Name of the sweep entry point.
        op: &'static str,
    },

    /// No board of the requested target kind was found in the image.
    ///
    /// The chessboard detector reports "nothing here" at the corner-cloud
    /// level — its crate-level `detect` returns `None`. The facade lifts that
    /// into this variant so every `detect_*` entry point shares one `Result`
    /// control-flow shape. A miss is expected and recoverable: match this
    /// variant to treat it like `None`.
    ///
    /// [`detect_chessboard_best`] also reports an all-configs-missed sweep
    /// this way.
    #[error("no {} was detected in the image", target_label(.target))]
    NoDetection {
        /// Which target's detector came up empty.
        target: core::TargetKind,
    },
}

impl DetectError {
    /// Whether this is a miss — the image holds no board of the requested
    /// kind that the detector can identify — rather than a misuse (a malformed
    /// image buffer, an invalid board spec or invalid parameters).
    ///
    /// A miss is expected and recoverable; the Python and WASM bindings return
    /// `None` / `null` for it and raise or throw only on a misuse. The C ABI
    /// reports it as `CT_STATUS_NOT_FOUND`.
    pub fn is_not_found(&self) -> bool {
        match self {
            Self::NoDetection { .. } => true,
            Self::CharucoDetect(err) => err.is_not_found(),
            Self::PuzzleBoardDetect(err) => err.is_not_found(),
            Self::PuzzlePoleDetect(err) => err.is_not_found(),
            Self::MarkerBoardDetect(err) => err.is_not_found(),
            Self::InvalidGrayBuffer { .. }
            | Self::InvalidGrayDimensions { .. }
            | Self::EmptySweep { .. }
            | Self::CharucoBoard(_)
            | Self::PuzzleBoardSpec(_)
            | Self::PuzzlePoleSpec(_)
            | Self::MarkerBoardParams(_)
            | Self::ChessboardParams(_) => false,
        }
    }
}

/// Human-readable label for a target kind, used by
/// [`DetectError::NoDetection`]'s message.
fn target_label(target: &core::TargetKind) -> &'static str {
    match target {
        core::TargetKind::Chessboard => "chessboard",
        core::TargetKind::Charuco => "ChArUco board",
        core::TargetKind::CheckerboardMarker => "marker board",
        core::TargetKind::PuzzleBoard => "PuzzleBoard",
        core::TargetKind::PuzzlePole => "PuzzlePole",
        _ => "calibration target",
    }
}

/// Convert an `image::GrayImage` into the lightweight `calib-targets-core` view type.
#[cfg(feature = "image")]
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
#[cfg(feature = "image")]
pub fn preprocess(img: &::image::GrayImage, blur_sigma_px: f32) -> ::image::GrayImage {
    if blur_sigma_px.is_finite() && blur_sigma_px > 0.0 {
        ::image::imageops::blur(img, blur_sigma_px)
    } else {
        img.clone()
    }
}

/// Detect ChESS corners and adapt them into [`calib_targets_chessboard::ChessCorner`].
///
/// Operates on the image as supplied — callers with an `image::GrayImage`
/// should run [`preprocess`] first if they want a Gaussian pre-blur. Corner
/// positions are returned in the input image frame.
#[cfg_attr(
    feature = "tracing",
    instrument(level = "info", skip(img, cfg), fields(width = img.width, height = img.height))
)]
pub fn detect_corners(
    img: &core::GrayImageView<'_>,
    cfg: &DetectorConfig,
) -> Vec<chessboard::ChessCorner> {
    chessboard::detect_corners(img, cfg)
}

/// Convenience overload using [`default_chess_config`].
pub fn detect_corners_default(img: &core::GrayImageView<'_>) -> Vec<chessboard::ChessCorner> {
    detect_corners(img, &default_chess_config())
}

/// Run the chessboard detector end-to-end: ChESS corners -> chessboard grid.
///
/// This is the primary chessboard entry point. It runs ChESS corner
/// detection with the supplied [`DetectorConfig`] and then runs the
/// chessboard detector with the supplied [`chessboard::ChessboardParams`];
/// corner positions are returned in the input image frame. Callers that
/// do not need to tune the ChESS detector pass [`default_chess_config`]
/// by reference (`&default_chess_config()`).
///
/// Named variants of this entry point:
/// - [`detect_chessboard_all`] — returns every same-board component, not
///   just the first.
/// - [`detect_chessboard_best`] — runs a multi-config sweep and keeps the
///   richest result.
/// - [`detect_chessboard_from_gray_u8`] — takes a raw `(width, height, pixels)`
///   triple instead of a [`GrayImageView`](core::GrayImageView).
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, chess_cfg, params),
        fields(width = img.width, height = img.height)
    )
)]
pub fn detect_chessboard(
    img: &core::GrayImageView<'_>,
    chess_cfg: &DetectorConfig,
    params: &chessboard::ChessboardParams,
) -> Result<chessboard::ChessboardDetection, DetectError> {
    let frame = chessboard::detect_corner_frame(img, chess_cfg);
    let detector = chessboard::ChessboardDetector::new(params.clone())?;
    let detection = detector
        .detect(&frame.corners)
        .ok_or(DetectError::NoDetection {
            target: core::TargetKind::Chessboard,
        })?;
    Ok(with_frame_noise(detection, &frame))
}

/// Record the frame's noise level on a chessboard detection's quality: the
/// chessboard detector runs on a corner cloud and cannot know it.
pub(super) fn with_frame_noise(
    mut detection: chessboard::ChessboardDetection,
    frame: &chessboard::CornerFrame,
) -> chessboard::ChessboardDetection {
    detection.quality = detection.quality.with_noise_sigma(frame.noise_sigma);
    detection
}

/// Multi-component variant of [`detect_chessboard`]: returns every same-board
/// component the detector recovers (capped by [`chessboard::ChessboardParams::max_components`]).
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, chess_cfg, params),
        fields(width = img.width, height = img.height)
    )
)]
pub fn detect_chessboard_all(
    img: &core::GrayImageView<'_>,
    chess_cfg: &DetectorConfig,
    params: &chessboard::ChessboardParams,
) -> Vec<chessboard::ChessboardDetection> {
    let frame = chessboard::detect_corner_frame(img, chess_cfg);
    let Ok(detector) = chessboard::ChessboardDetector::new(params.clone()) else {
        return Vec::new();
    };
    detector
        .detect_all(&frame.corners)
        .into_iter()
        .map(|d| with_frame_noise(d, &frame))
        .collect()
}

/// Run the ChArUco detector end-to-end: ChESS corners -> grid -> markers -> alignment -> IDs.
///
/// The corner pass runs from
/// [`params.chess`](charuco::CharucoParams::chess). Use
/// [`detect_charuco_with_corners`] when you already have a corner cloud.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, params),
        fields(
            width = img.width,
            height = img.height,
            board_rows = params.board.rows,
            board_cols = params.board.cols
        )
    )
)]
pub fn detect_charuco(
    img: &core::GrayImageView<'_>,
    params: &charuco::CharucoParams,
) -> Result<charuco::CharucoDetection, DetectError> {
    Ok(charuco::CharucoDetector::new(params.clone())?.detect(img)?)
}

/// Run the ChArUco detector on a pre-detected corner cloud.
///
/// `corners` is taken as given; of [`params.chess`](charuco::CharucoParams::chess)
/// only the uncertainty settings are read, for the few corners the detector
/// re-detects itself (see [`charuco::CharucoDetector::detect_with_corners`]).
pub fn detect_charuco_with_corners(
    img: &core::GrayImageView<'_>,
    corners: &[chessboard::ChessCorner],
    params: &charuco::CharucoParams,
) -> Result<charuco::CharucoDetection, DetectError> {
    Ok(charuco::CharucoDetector::new(params.clone())?.detect_with_corners(img, corners)?)
}

/// Run the PuzzleBoard detector end-to-end: ChESS corners → chessboard grid
/// → edge-bit sampling → cross-correlation decode → absolute master IDs.
///
/// The corner pass runs from
/// [`params.chess`](puzzleboard::PuzzleBoardParams::chess). Use
/// [`detect_puzzleboard_with_corners`] when you already have a corner cloud.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, params),
        fields(
            width = img.width,
            height = img.height,
            board_rows = params.board.rows,
            board_cols = params.board.cols
        )
    )
)]
pub fn detect_puzzleboard(
    img: &core::GrayImageView<'_>,
    params: &puzzleboard::PuzzleBoardParams,
) -> Result<puzzleboard::PuzzleBoardDetection, DetectError> {
    Ok(puzzleboard::PuzzleBoardDetector::new(params.clone())?.detect(img)?)
}

/// Run the PuzzleBoard detector on a pre-detected corner cloud.
///
/// [`params.chess`](puzzleboard::PuzzleBoardParams::chess) is not read:
/// `corners` is taken as given. The sweep [`detect_puzzleboard_best`] uses this
/// path with its per-front-end corner cache.
pub fn detect_puzzleboard_with_corners(
    img: &core::GrayImageView<'_>,
    corners: &[chessboard::ChessCorner],
    params: &puzzleboard::PuzzleBoardParams,
) -> Result<puzzleboard::PuzzleBoardDetection, DetectError> {
    Ok(puzzleboard::PuzzleBoardDetector::new(params.clone())?.detect_with_corners(img, corners)?)
}

/// Run the checkerboard+circles marker board detector end-to-end.
///
/// Corner detection uses [`params.chess`](marker::MarkerBoardParams::chess) —
/// a marker board is a whole-image pipeline that owns its corner pass. Use
/// [`detect_marker_board_with_corners`] when you already have a corner cloud.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, params),
        fields(width = img.width, height = img.height)
    )
)]
pub fn detect_marker_board(
    img: &core::GrayImageView<'_>,
    params: &marker::MarkerBoardParams,
) -> Result<marker::MarkerBoardDetection, DetectError> {
    Ok(marker::MarkerBoardDetector::new(params.clone())?.detect(img)?)
}

/// Run the checkerboard+circles marker board detector on a pre-detected corner
/// cloud.
///
/// [`params.chess`](marker::MarkerBoardParams::chess) is not read: `corners` is
/// taken as given.
pub fn detect_marker_board_with_corners(
    img: &core::GrayImageView<'_>,
    corners: &[chessboard::ChessCorner],
    params: &marker::MarkerBoardParams,
) -> Result<marker::MarkerBoardDetection, DetectError> {
    Ok(marker::MarkerBoardDetector::new(params.clone())?.detect_with_corners(img, corners)?)
}

/// Detect a PuzzlePole — the PuzzleBoard pattern wrapped round a cylinder.
///
/// Unlike every other target here, the result carries a **3-D** object point
/// per corner as well as an image point, because the target is not planar. Feed
/// [`PuzzlePoleDetection::correspondences`] to a PnP solver; this library
/// stops at the correspondences, which is the hard half.
///
/// [`PuzzlePoleDetection::correspondences`]: puzzleboard::PuzzlePoleDetection::correspondences
pub fn detect_puzzlepole(
    img: &core::GrayImageView<'_>,
    params: &puzzleboard::PuzzlePoleParams,
) -> Result<puzzleboard::PuzzlePoleDetection, DetectError> {
    Ok(puzzleboard::PuzzlePoleDetector::new(params.clone())?.detect(img)?)
}

/// [`detect_puzzlepole`] over an already-detected corner cloud.
pub fn detect_puzzlepole_with_corners(
    img: &core::GrayImageView<'_>,
    corners: &[chessboard::ChessCorner],
    params: &puzzleboard::PuzzlePoleParams,
) -> Result<puzzleboard::PuzzlePoleDetection, DetectError> {
    Ok(puzzleboard::PuzzlePoleDetector::new(params.clone())?.detect_with_corners(img, corners)?)
}

/// Zero-copy [`GrayImageView`](core::GrayImageView) from a raw grayscale buffer.
pub fn gray_view_from_slice(
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Result<core::GrayImageView<'_>, DetectError> {
    if width == 0 || height == 0 {
        return Err(DetectError::InvalidGrayDimensions { width, height });
    }
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
    Ok(core::GrayImageView {
        width: w,
        height: h,
        data: pixels,
    })
}

/// Raw-buffer variant of [`detect_chessboard`]: runs the chessboard detector
/// from a raw grayscale byte buffer.
///
/// `pixels` must have length `width * height`. Returns
/// `Err(DetectError::NoDetection { .. })` when no board is found, or `Err` with
/// an `InvalidGray*` variant when the buffer dimensions are invalid.
pub fn detect_chessboard_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: &DetectorConfig,
    params: &chessboard::ChessboardParams,
) -> Result<chessboard::ChessboardDetection, DetectError> {
    let view = gray_view_from_slice(width, height, pixels)?;
    detect_chessboard(&view, chess_cfg, params)
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
) -> Result<charuco::CharucoDetection, DetectError> {
    let view = gray_view_from_slice(width, height, pixels)?;
    detect_charuco(&view, params)
}

/// Run the PuzzleBoard detector from a raw grayscale byte buffer.
pub fn detect_puzzleboard_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    params: &puzzleboard::PuzzleBoardParams,
) -> Result<puzzleboard::PuzzleBoardDetection, DetectError> {
    let view = gray_view_from_slice(width, height, pixels)?;
    detect_puzzleboard(&view, params)
}

/// Run the PuzzlePole detector from a raw grayscale byte buffer.
pub fn detect_puzzlepole_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    params: &puzzleboard::PuzzlePoleParams,
) -> Result<puzzleboard::PuzzlePoleDetection, DetectError> {
    let view = gray_view_from_slice(width, height, pixels)?;
    detect_puzzlepole(&view, params)
}

/// Run the checkerboard+circles marker board detector from a raw grayscale byte buffer.
///
/// `pixels` must have length `width * height`. Returns
/// `Err(DetectError::MarkerBoardDetect { .. })` when no board is found, or
/// `Err` with an `InvalidGray*` variant when the buffer dimensions are invalid.
pub fn detect_marker_board_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    params: &marker::MarkerBoardParams,
) -> Result<marker::MarkerBoardDetection, DetectError> {
    let view = gray_view_from_slice(width, height, pixels)?;
    detect_marker_board(&view, params)
}
