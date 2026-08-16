//! End-to-end detection helpers.
//!
//! Each `detect_*` helper runs the `chess-corners` ChESS corner detector over
//! an image (or raw grayscale buffer) and then runs the matching target
//! detector. Every helper returns `Result<{X}Detection, DetectError>` — never a
//! bare `Option` — so consuming several target types shares one `?`/`match`
//! control-flow shape. A board that is simply not present surfaces as the
//! detector's own typed miss ([`DetectError::CharucoDetect`],
//! [`DetectError::PuzzleBoardDetect`], [`DetectError::MarkerBoardDetect`]) or,
//! for the corner-cloud-level chessboard detector, as
//! [`DetectError::NoDetection`]. The `detect_*_best` variants additionally
//! sweep multiple parameter presets and keep the richest detection. This module
//! is gated on the `image` feature.
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

    /// Marker-board detection failed.
    #[error(transparent)]
    MarkerBoardDetect(#[from] marker::MarkerBoardDetectError),

    /// Invalid chessboard or marker-board detector parameters.
    ///
    /// Both [`chessboard::ChessboardDetector::new`] and
    /// [`marker::MarkerBoardDetector::new`] validate their
    /// [`chessboard::ChessboardParams`] and surface this on a bad value.
    #[error(transparent)]
    ChessboardParams(#[from] chessboard::ChessboardParamsError),

    /// No board of the requested target kind was found in the image.
    ///
    /// The chessboard detector reports "nothing here" at the corner-cloud
    /// level — its crate-level `detect` returns `None`. The facade lifts that
    /// into this variant so every `detect_*` entry point shares one `Result`
    /// control-flow shape. A miss is expected and recoverable: match this
    /// variant to treat it like `None`.
    ///
    /// The multi-config sweeps ([`detect_chessboard_best`],
    /// [`detect_marker_board_best`]) also report an all-configs-failed sweep
    /// this way.
    #[error("no {} was detected in the image", target_label(.target))]
    NoDetection {
        /// Which target's detector came up empty.
        target: core::TargetKind,
    },
}

/// Human-readable label for a target kind, used by
/// [`DetectError::NoDetection`]'s message.
fn target_label(target: &core::TargetKind) -> &'static str {
    match target {
        core::TargetKind::Chessboard => "chessboard",
        core::TargetKind::Charuco => "ChArUco board",
        core::TargetKind::CheckerboardMarker => "marker board",
        core::TargetKind::PuzzleBoard => "PuzzleBoard",
        _ => "calibration target",
    }
}

/// Return the ChESS corners for `chess_cfg`, running the corner pass exactly
/// once per distinct [`DetectorConfig`] within a single sweep.
///
/// The `detect_*_best` helpers honour each config's own `chess` front-end but
/// must not pay for a corner pass per config when several configs share one.
/// They iterate the configs in the caller's original order — reordering would
/// change tie-break outcomes — and consult this memoized cache, so corner
/// detection is deduplicated across configs that request the same front-end
/// while every distinct front-end still gets its own pass.
///
/// [`DetectorConfig`] is `Copy + PartialEq` but not hashable (it carries `f32`
/// fields), so a linear scan over the handful of entries a sweep produces is
/// the right structure. The lookup is index-based (`position` then index) so
/// the immutable borrow of a cached entry never overlaps the `push` of a new
/// one.
fn cached_corners<'cache>(
    img: &::image::GrayImage,
    chess_cfg: &DetectorConfig,
    cache: &'cache mut Vec<(DetectorConfig, Vec<chessboard::ChessCorner>)>,
) -> &'cache [chessboard::ChessCorner] {
    let idx = match cache.iter().position(|(cfg, _)| cfg == chess_cfg) {
        Some(idx) => idx,
        None => {
            let corners = detect_corners(img, chess_cfg);
            cache.push((*chess_cfg, corners));
            cache.len() - 1
        }
    };
    &cache[idx].1
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
///
/// This is the [`::image::GrayImage`] overload of
/// [`chessboard::detect_corners`], which is the single corner front-end every
/// detector in the workspace runs.
#[cfg_attr(
    feature = "tracing",
    instrument(level = "info", skip(img, cfg), fields(width = img.width(), height = img.height()))
)]
pub fn detect_corners(
    img: &::image::GrayImage,
    cfg: &DetectorConfig,
) -> Vec<chessboard::ChessCorner> {
    chessboard::detect_corners(&gray_view(img), cfg)
}

/// Convenience overload using [`default_chess_config`].
pub fn detect_corners_default(img: &::image::GrayImage) -> Vec<chessboard::ChessCorner> {
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
    params: &chessboard::ChessboardParams,
) -> Result<chessboard::ChessboardDetection, DetectError> {
    let corners = detect_corners(img, chess_cfg);
    let detector = chessboard::ChessboardDetector::new(params.clone())?;
    detector.detect(&corners).ok_or(DetectError::NoDetection {
        target: core::TargetKind::Chessboard,
    })
}

/// Multi-component variant of [`detect_chessboard`]: returns every same-board
/// component the detector recovers (capped by [`chessboard::ChessboardParams::max_components`]).
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
    params: &chessboard::ChessboardParams,
) -> Vec<chessboard::ChessboardDetection> {
    let corners = detect_corners(img, chess_cfg);
    let Ok(detector) = chessboard::ChessboardDetector::new(params.clone()) else {
        return Vec::new();
    };
    detector.detect_all(&corners)
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
) -> Result<charuco::CharucoDetection, DetectError> {
    Ok(charuco::CharucoDetector::new(params.clone())?.detect(&gray_view(img))?)
}

/// Run the ChArUco detector on a pre-detected corner cloud.
///
/// [`params.chess`](charuco::CharucoParams::chess) is not read: `corners` is
/// taken as given.
pub fn detect_charuco_with_corners(
    img: &::image::GrayImage,
    corners: &[chessboard::ChessCorner],
    params: &charuco::CharucoParams,
) -> Result<charuco::CharucoDetection, DetectError> {
    Ok(charuco::CharucoDetector::new(params.clone())?
        .detect_with_corners(&gray_view(img), corners)?)
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
) -> Result<puzzleboard::PuzzleBoardDetection, DetectError> {
    Ok(puzzleboard::PuzzleBoardDetector::new(params.clone())?.detect(&gray_view(img))?)
}

/// Run the PuzzleBoard detector on a pre-detected corner cloud.
///
/// [`params.chess`](puzzleboard::PuzzleBoardParams::chess) is not read:
/// `corners` is taken as given. The sweep [`detect_puzzleboard_best`] uses this
/// path with its per-front-end corner cache.
pub fn detect_puzzleboard_with_corners(
    img: &::image::GrayImage,
    corners: &[chessboard::ChessCorner],
    params: &puzzleboard::PuzzleBoardParams,
) -> Result<puzzleboard::PuzzleBoardDetection, DetectError> {
    Ok(puzzleboard::PuzzleBoardDetector::new(params.clone())?
        .detect_with_corners(&gray_view(img), corners)?)
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
        fields(width = img.width(), height = img.height())
    )
)]
pub fn detect_marker_board(
    img: &::image::GrayImage,
    params: &marker::MarkerBoardParams,
) -> Result<marker::MarkerBoardDetection, DetectError> {
    Ok(marker::MarkerBoardDetector::new(params.clone())?.detect(&gray_view(img))?)
}

/// Run the checkerboard+circles marker board detector on a pre-detected corner
/// cloud.
///
/// [`params.chess`](marker::MarkerBoardParams::chess) is not read: `corners` is
/// taken as given.
pub fn detect_marker_board_with_corners(
    img: &::image::GrayImage,
    corners: &[chessboard::ChessCorner],
    params: &marker::MarkerBoardParams,
) -> Result<marker::MarkerBoardDetection, DetectError> {
    Ok(marker::MarkerBoardDetector::new(params.clone())?
        .detect_with_corners(&gray_view(img), corners)?)
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
    param_configs: &[chessboard::ChessboardParams],
) -> Result<chessboard::ChessboardDetection, DetectError> {
    let corners = detect_corners(img, chess_cfg);
    param_configs
        .iter()
        .filter_map(|params| {
            chessboard::ChessboardDetector::new(params.clone())
                .ok()?
                .detect(&corners)
        })
        .max_by_key(|d| d.corners.len())
        .ok_or(DetectError::NoDetection {
            target: core::TargetKind::Chessboard,
        })
}

/// Try multiple ChArUco parameter configs, return the best result
/// (most markers, then most corners).
///
/// Each config's own [`CharucoParams::chess`] front-end is honoured; corner
/// detection is deduplicated across configs that share one, so configs may
/// freely mix front-ends (e.g. a default pass alongside an
/// `UpscaleConfig::Fixed(2)` pass) without paying for a redundant corner pass
/// when they agree. Configs are tried in the given order; on ties the first
/// best result wins.
///
/// [`CharucoParams::chess`]: charuco::CharucoParams::chess
pub fn detect_charuco_best(
    img: &::image::GrayImage,
    configs: &[charuco::CharucoParams],
) -> Result<charuco::CharucoDetection, DetectError> {
    let mut best: Option<charuco::CharucoDetection> = None;
    let mut last_err = None;
    let mut corner_cache: Vec<(DetectorConfig, Vec<chessboard::ChessCorner>)> = Vec::new();

    for params in configs {
        let detector = match charuco::CharucoDetector::new(params.clone()) {
            Ok(d) => d,
            Err(e) => {
                last_err = Some(DetectError::from(e));
                continue;
            }
        };
        let corners = cached_corners(img, &params.chess, &mut corner_cache);
        match detector.detect_with_corners(&gray_view(img), corners) {
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
/// Each config's own [`PuzzleBoardParams::chess`] front-end is honoured, so a
/// sweep may mix corner front-ends (e.g. a single-scale pass alongside an
/// `UpscaleConfig::Fixed(2)` pass — as [`PuzzleBoardParams::sweep_for_board`]
/// does). Corner detection is deduplicated across configs that share a front-
/// end, so the common all-same-`chess` sweep pays for exactly one corner pass.
/// Configs are tried in the given order; a later config replaces the current
/// best only when it is strictly better, so the first best result wins ties.
///
/// [`PuzzleBoardParams::chess`]: puzzleboard::PuzzleBoardParams::chess
/// [`PuzzleBoardParams::sweep_for_board`]: puzzleboard::PuzzleBoardParams::sweep_for_board
pub fn detect_puzzleboard_best(
    img: &::image::GrayImage,
    configs: &[puzzleboard::PuzzleBoardParams],
) -> Result<puzzleboard::PuzzleBoardDetection, DetectError> {
    let mut best: Option<puzzleboard::PuzzleBoardDetection> = None;
    let mut last_err: Option<DetectError> = None;
    let mut corner_cache: Vec<(DetectorConfig, Vec<chessboard::ChessCorner>)> = Vec::new();
    for params in configs {
        let corners = cached_corners(img, &params.chess, &mut corner_cache);
        match detect_puzzleboard_with_corners(img, corners, params) {
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
///
/// [`MarkerBoardParams::sweep_for_board`] builds the stock config list.
///
/// Each config's own [`MarkerBoardParams::chess`] front-end is honoured; corner
/// detection is deduplicated across configs that share one, so configs may mix
/// front-ends without triggering a redundant corner pass when they agree.
/// Configs are tried in the given order and, matching the previous
/// `max_by_key` behaviour, the **last** config attaining the maximum corner
/// count wins ties.
///
/// # Errors
///
/// If no config yields a detection, the last config's own
/// [`MarkerBoardDetectError`] is reported, matching
/// [`detect_charuco_best`] and [`detect_puzzleboard_best`]. Only when no
/// config got as far as attempting a detection — an empty `configs`, or every
/// config rejected by [`MarkerBoardDetector::new`] — is there no typed error to
/// report, and the result is [`DetectError::NoDetection`].
///
/// [`MarkerBoardParams::chess`]: marker::MarkerBoardParams::chess
/// [`MarkerBoardParams::sweep_for_board`]: marker::MarkerBoardParams::sweep_for_board
/// [`MarkerBoardDetectError`]: marker::MarkerBoardDetectError
/// [`MarkerBoardDetector::new`]: marker::MarkerBoardDetector::new
pub fn detect_marker_board_best(
    img: &::image::GrayImage,
    configs: &[marker::MarkerBoardParams],
) -> Result<marker::MarkerBoardDetection, DetectError> {
    let mut best: Option<marker::MarkerBoardDetection> = None;
    let mut last_err: Option<DetectError> = None;
    let mut corner_cache: Vec<(DetectorConfig, Vec<chessboard::ChessCorner>)> = Vec::new();
    for params in configs {
        let Ok(detector) = marker::MarkerBoardDetector::new(params.clone()) else {
            continue;
        };
        let corners = cached_corners(img, &params.chess, &mut corner_cache);
        let result = match detector.detect_with_corners(&gray_view(img), corners) {
            Ok(result) => result,
            Err(err) => {
                last_err = Some(err.into());
                continue;
            }
        };
        // Replicate `Iterator::max_by_key`, which keeps the *last* maximum on
        // ties: replace whenever the new count is >= the incumbent's.
        let replace = best
            .as_ref()
            .is_none_or(|b| result.corners.len() >= b.corners.len());
        if replace {
            best = Some(result);
        }
    }
    best.ok_or_else(|| {
        last_err.unwrap_or(DetectError::NoDetection {
            target: core::TargetKind::CheckerboardMarker,
        })
    })
}

/// Scoring key for ChArUco results: (marker count, corner count).
fn charuco_score(r: &charuco::CharucoDetection) -> (usize, usize) {
    (r.markers.len(), r.corners.len())
}

// ---------------------------------------------------------------------------
// Diagnostics helpers (feature `diagnostics`)
// ---------------------------------------------------------------------------
//
// Each `diagnose_*` helper mirrors its `detect_*` counterpart and additionally
// returns the detector's per-call diagnostics. The detector method returns
// `(Result, Diagnostics)` — diagnostics come back even on a failed detection —
// but the facade has one failure mode the method does not: constructing the
// detector from `params` can fail before the pipeline ever runs. That is
// exactly what the `Option` encodes; see each function's doc.

/// [`detect_charuco`] + per-component diagnostics.
///
/// The corner pass runs from [`params.chess`](charuco::CharucoParams::chess).
///
/// `None` diagnostics means **detector construction failed** — `params`
/// described a board the detector cannot build, so the pipeline never ran and
/// no diagnostics exist. Whenever the pipeline ran the diagnostics are
/// `Some`, including on a failed detection (best-effort evidence for
/// overlays).
#[cfg(feature = "diagnostics")]
pub fn diagnose_charuco(
    img: &::image::GrayImage,
    params: &charuco::CharucoParams,
) -> (
    Result<charuco::CharucoDetection, DetectError>,
    Option<charuco::diagnostics::CharucoDetectDiagnostics>,
) {
    let detector = match charuco::CharucoDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose(&gray_view(img));
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_charuco_with_corners`] + per-component diagnostics.
///
/// [`params.chess`](charuco::CharucoParams::chess) is not read: `corners` is
/// taken as given. `None` diagnostics means detector construction failed, so
/// the pipeline never ran — see [`diagnose_charuco`].
#[cfg(feature = "diagnostics")]
pub fn diagnose_charuco_with_corners(
    img: &::image::GrayImage,
    corners: &[chessboard::ChessCorner],
    params: &charuco::CharucoParams,
) -> (
    Result<charuco::CharucoDetection, DetectError>,
    Option<charuco::diagnostics::CharucoDetectDiagnostics>,
) {
    let detector = match charuco::CharucoDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose_with_corners(&gray_view(img), corners);
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_puzzleboard`] + per-call decode diagnostics.
///
/// The corner pass runs from
/// [`params.chess`](puzzleboard::PuzzleBoardParams::chess).
///
/// `None` diagnostics means **detector construction failed** — `params`
/// described a board spec the detector cannot build, so the pipeline never ran
/// and no diagnostics exist. Whenever the pipeline ran the diagnostics are
/// `Some`, including on a failed decode (best-effort evidence for overlays).
#[cfg(feature = "diagnostics")]
pub fn diagnose_puzzleboard(
    img: &::image::GrayImage,
    params: &puzzleboard::PuzzleBoardParams,
) -> (
    Result<puzzleboard::PuzzleBoardDetection, DetectError>,
    Option<puzzleboard::PuzzleBoardDiagnostics>,
) {
    let detector = match puzzleboard::PuzzleBoardDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose(&gray_view(img));
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_puzzleboard_with_corners`] + per-call decode diagnostics.
///
/// [`params.chess`](puzzleboard::PuzzleBoardParams::chess) is not read:
/// `corners` is taken as given. `None` diagnostics means detector construction
/// failed, so the pipeline never ran — see [`diagnose_puzzleboard`].
#[cfg(feature = "diagnostics")]
pub fn diagnose_puzzleboard_with_corners(
    img: &::image::GrayImage,
    corners: &[chessboard::ChessCorner],
    params: &puzzleboard::PuzzleBoardParams,
) -> (
    Result<puzzleboard::PuzzleBoardDetection, DetectError>,
    Option<puzzleboard::PuzzleBoardDiagnostics>,
) {
    let detector = match puzzleboard::PuzzleBoardDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose_with_corners(&gray_view(img), corners);
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_marker_board`] + per-call circle diagnostics.
///
/// The corner pass runs from
/// [`params.chess`](marker::MarkerBoardParams::chess).
///
/// `None` diagnostics means **detector construction failed** — `params`
/// carried a chessboard configuration the detector cannot honour, so the
/// pipeline never ran and no diagnostics exist. Whenever the pipeline ran the
/// diagnostics are `Some`, including on a failed detection (best-effort
/// evidence for overlays).
#[cfg(feature = "diagnostics")]
pub fn diagnose_marker_board(
    img: &::image::GrayImage,
    params: &marker::MarkerBoardParams,
) -> (
    Result<marker::MarkerBoardDetection, DetectError>,
    Option<marker::MarkerBoardDiagnostics>,
) {
    let detector = match marker::MarkerBoardDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose(&gray_view(img));
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_marker_board_with_corners`] + per-call circle diagnostics.
///
/// [`params.chess`](marker::MarkerBoardParams::chess) is not read: `corners`
/// is taken as given. `None` diagnostics means detector construction failed,
/// so the pipeline never ran — see [`diagnose_marker_board`].
#[cfg(feature = "diagnostics")]
pub fn diagnose_marker_board_with_corners(
    img: &::image::GrayImage,
    corners: &[chessboard::ChessCorner],
    params: &marker::MarkerBoardParams,
) -> (
    Result<marker::MarkerBoardDetection, DetectError>,
    Option<marker::MarkerBoardDiagnostics>,
) {
    let detector = match marker::MarkerBoardDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose_with_corners(&gray_view(img), corners);
    (result.map_err(DetectError::from), Some(diagnostics))
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
    let img = gray_image_from_slice(width, height, pixels)?;
    detect_chessboard(&img, chess_cfg, params)
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
    let img = gray_image_from_slice(width, height, pixels)?;
    detect_charuco(&img, params)
}

/// Run the PuzzleBoard detector from a raw grayscale byte buffer.
pub fn detect_puzzleboard_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    params: &puzzleboard::PuzzleBoardParams,
) -> Result<puzzleboard::PuzzleBoardDetection, DetectError> {
    let img = gray_image_from_slice(width, height, pixels)?;
    detect_puzzleboard(&img, params)
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
    let img = gray_image_from_slice(width, height, pixels)?;
    detect_marker_board(&img, params)
}
