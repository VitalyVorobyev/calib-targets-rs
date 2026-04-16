use crate::{charuco, chessboard, core, marker, puzzleboard};
use chess_corners::find_chess_corners_image;
use nalgebra::Point2;

#[cfg(feature = "tracing")]
use tracing::instrument;

pub use core::{
    CenterOfMassConfig, ChessConfig, DescriptorMode, DetectorMode, ForstnerConfig,
    RefinementMethod, RefinerConfig, SaddlePointConfig, ThresholdMode,
};

/// Errors produced by the high-level facade helpers.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum DetectError {
    #[error("invalid grayscale image buffer length (expected {expected} bytes, got {got})")]
    InvalidGrayBuffer { expected: usize, got: usize },

    #[error("invalid grayscale image dimensions (width={width}, height={height})")]
    InvalidGrayDimensions { width: u32, height: u32 },

    #[error(transparent)]
    CharucoBoard(#[from] charuco::CharucoBoardError),

    #[error(transparent)]
    CharucoDetect(#[from] charuco::CharucoDetectError),

    #[error(transparent)]
    PuzzleBoardSpec(#[from] puzzleboard::PuzzleBoardSpecError),

    #[error(transparent)]
    PuzzleBoardDetect(#[from] puzzleboard::PuzzleBoardDetectError),
}

/// Reasonable default settings for the `chess-corners` ChESS detector.
///
/// This is tuned for the repo examples and is expected to be overridden by callers
/// for difficult real-world images.
pub fn default_chess_config() -> ChessConfig {
    ChessConfig {
        threshold_mode: ThresholdMode::Relative,
        threshold_value: 0.2,
        nms_radius: 2,
        ..ChessConfig::single_scale()
    }
}

/// Convert an `image::GrayImage` into the lightweight `calib-targets-core` view type.
pub fn gray_view(img: &::image::GrayImage) -> core::GrayImageView<'_> {
    core::GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    }
}

/// Detect ChESS corners and adapt them into `calib-targets-core::Corner`.
#[cfg_attr(
    feature = "tracing",
    instrument(level = "info", skip(img, cfg), fields(width = img.width(), height = img.height()))
)]
pub fn detect_corners(img: &::image::GrayImage, cfg: &ChessConfig) -> Vec<core::Corner> {
    let cfg = to_chess_corners_config(cfg);
    find_chess_corners_image(img, &cfg)
        .iter()
        .map(adapt_chess_corner)
        .collect()
}

/// Convenience overload using `default_chess_config()`.
pub fn detect_corners_default(img: &::image::GrayImage) -> Vec<core::Corner> {
    let cfg = default_chess_config();
    detect_corners(img, &cfg)
}

/// Run the chessboard detector end-to-end: ChESS corners -> chessboard grid.
///
/// Corner detection uses `params.chess`.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, params),
        fields(width = img.width(), height = img.height())
    )
)]
pub fn detect_chessboard(
    img: &::image::GrayImage,
    params: &chessboard::ChessboardParams,
) -> Option<chessboard::ChessboardDetectionResult> {
    let corners = detect_corners(img, &params.chess);
    let detector = chessboard::ChessboardDetector::new(params.clone());
    detector.detect_from_corners(&corners)
}

/// Run the ChArUco detector end-to-end: ChESS corners -> grid -> markers -> alignment -> IDs.
///
/// Corner detection uses `params.chessboard.chess`.
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
    let corners = detect_corners(img, &params.chessboard.chess);
    let detector = charuco::CharucoDetector::new(params.clone())?;
    Ok(detector.detect(&gray_view(img), &corners)?)
}

/// Run the PuzzleBoard detector end-to-end: ChESS corners → chessboard grid
/// → edge-bit sampling → cross-correlation decode → absolute master IDs.
///
/// Corner detection uses `params.chessboard.chess`.
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
    let corners = detect_corners(img, &params.chessboard.chess);
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
    let corners = detect_corners(img, &params.chessboard.chess);
    let detector = marker::MarkerBoardDetector::new(params.clone());
    detector.detect_from_image_and_corners(&gray_view(img), &corners)
}

// ---------------------------------------------------------------------------
// Multi-config sweep helpers
// ---------------------------------------------------------------------------

/// Try multiple chessboard parameter configs, return the best result (most corners).
pub fn detect_chessboard_best(
    img: &::image::GrayImage,
    configs: &[chessboard::ChessboardParams],
) -> Option<chessboard::ChessboardDetectionResult> {
    configs
        .iter()
        .filter_map(|params| {
            let corners = detect_corners(img, &params.chess);
            let detector = chessboard::ChessboardDetector::new(params.clone());
            detector.detect_from_corners(&corners)
        })
        .max_by_key(|r| r.detection.corners.len())
}

/// Try multiple ChArUco parameter configs, return the best result
/// (most markers, then most corners).
pub fn detect_charuco_best(
    img: &::image::GrayImage,
    configs: &[charuco::CharucoParams],
) -> Result<charuco::CharucoDetectionResult, DetectError> {
    let mut best: Option<charuco::CharucoDetectionResult> = None;
    let mut last_err = None;

    for params in configs {
        let corners = detect_corners(img, &params.chessboard.chess);
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
                        let key_new = (r.detection.corners.len(), r.decode.mean_confidence);
                        let key_old = (b.detection.corners.len(), b.decode.mean_confidence);
                        key_new.0 > key_old.0
                            || (key_new.0 == key_old.0 && key_new.1 > key_old.1)
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
    configs
        .iter()
        .filter_map(|params| {
            let corners = detect_corners(img, &params.chessboard.chess);
            let detector = marker::MarkerBoardDetector::new(params.clone());
            detector.detect_from_image_and_corners(&gray_view(img), &corners)
        })
        .max_by_key(|r| r.detection.corners.len())
}

/// Scoring key for ChArUco results: (marker count, corner count).
fn charuco_score(r: &charuco::CharucoDetectionResult) -> (usize, usize) {
    (r.markers.len(), r.detection.corners.len())
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

/// Run the chessboard detector from a raw grayscale byte buffer.
///
/// `pixels` must have length `width * height`. Returns `Ok(None)` when no board is found,
/// or `Err` when the buffer dimensions are invalid.
pub fn detect_chessboard_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    params: &chessboard::ChessboardParams,
) -> Result<Option<chessboard::ChessboardDetectionResult>, DetectError> {
    let img = gray_image_from_slice(width, height, pixels)?;
    Ok(detect_chessboard(&img, params))
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

fn adapt_chess_corner(c: &chess_corners::CornerDescriptor) -> core::Corner {
    core::Corner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
        strength: c.response,
    }
}

fn to_chess_corners_config(cfg: &ChessConfig) -> chess_corners::ChessConfig {
    let mut out = chess_corners::ChessConfig::default();
    out.detector_mode = to_detector_mode(cfg.detector_mode);
    out.descriptor_mode = to_descriptor_mode(cfg.descriptor_mode);
    out.threshold_mode = to_threshold_mode(cfg.threshold_mode);
    out.threshold_value = cfg.threshold_value;
    out.nms_radius = cfg.nms_radius;
    out.min_cluster_size = cfg.min_cluster_size;
    out.refiner = to_refiner_config(&cfg.refiner);
    out.pyramid_levels = cfg.pyramid_levels;
    out.pyramid_min_size = cfg.pyramid_min_size;
    out.refinement_radius = cfg.refinement_radius;
    out.merge_radius = cfg.merge_radius;
    out
}

fn to_detector_mode(mode: DetectorMode) -> chess_corners::DetectorMode {
    match mode {
        DetectorMode::Canonical => chess_corners::DetectorMode::Canonical,
        DetectorMode::Broad => chess_corners::DetectorMode::Broad,
        _ => unimplemented!("unknown DetectorMode variant"),
    }
}

fn to_descriptor_mode(mode: DescriptorMode) -> chess_corners::DescriptorMode {
    match mode {
        DescriptorMode::FollowDetector => chess_corners::DescriptorMode::FollowDetector,
        DescriptorMode::Canonical => chess_corners::DescriptorMode::Canonical,
        DescriptorMode::Broad => chess_corners::DescriptorMode::Broad,
        _ => unimplemented!("unknown DescriptorMode variant"),
    }
}

fn to_threshold_mode(mode: ThresholdMode) -> chess_corners::ThresholdMode {
    match mode {
        ThresholdMode::Relative => chess_corners::ThresholdMode::Relative,
        ThresholdMode::Absolute => chess_corners::ThresholdMode::Absolute,
        _ => unimplemented!("unknown ThresholdMode variant"),
    }
}

fn to_refinement_method(method: RefinementMethod) -> chess_corners::RefinementMethod {
    match method {
        RefinementMethod::CenterOfMass => chess_corners::RefinementMethod::CenterOfMass,
        RefinementMethod::Forstner => chess_corners::RefinementMethod::Forstner,
        RefinementMethod::SaddlePoint => chess_corners::RefinementMethod::SaddlePoint,
        _ => unimplemented!("unknown RefinementMethod variant"),
    }
}

fn to_refiner_config(refiner: &RefinerConfig) -> chess_corners::RefinerConfig {
    chess_corners::RefinerConfig {
        kind: to_refinement_method(refiner.kind),
        center_of_mass: chess_corners::CenterOfMassConfig {
            radius: refiner.center_of_mass.radius,
        },
        forstner: chess_corners::ForstnerConfig {
            radius: refiner.forstner.radius,
            min_trace: refiner.forstner.min_trace,
            min_det: refiner.forstner.min_det,
            max_condition_number: refiner.forstner.max_condition_number,
            max_offset: refiner.forstner.max_offset,
        },
        saddle_point: chess_corners::SaddlePointConfig {
            radius: refiner.saddle_point.radius,
            det_margin: refiner.saddle_point.det_margin,
            max_offset: refiner.saddle_point.max_offset,
            min_abs_det: refiner.saddle_point.min_abs_det,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_refiner_eq(
        actual: &chess_corners::RefinerConfig,
        expected: &chess_corners::RefinerConfig,
    ) {
        assert_eq!(actual.kind, expected.kind);
        assert_eq!(actual.center_of_mass, expected.center_of_mass);
        assert_eq!(actual.forstner, expected.forstner);
        assert_eq!(actual.saddle_point, expected.saddle_point);
    }

    fn assert_chess_config_eq(
        actual: &chess_corners::ChessConfig,
        expected: &chess_corners::ChessConfig,
    ) {
        assert_eq!(actual.detector_mode, expected.detector_mode);
        assert_eq!(actual.descriptor_mode, expected.descriptor_mode);
        assert_eq!(actual.threshold_mode, expected.threshold_mode);
        assert_eq!(actual.threshold_value, expected.threshold_value);
        assert_eq!(actual.nms_radius, expected.nms_radius);
        assert_eq!(actual.min_cluster_size, expected.min_cluster_size);
        assert_refiner_eq(&actual.refiner, &expected.refiner);
        assert_eq!(actual.pyramid_levels, expected.pyramid_levels);
        assert_eq!(actual.pyramid_min_size, expected.pyramid_min_size);
        assert_eq!(actual.refinement_radius, expected.refinement_radius);
        assert_eq!(actual.merge_radius, expected.merge_radius);
    }

    #[test]
    fn owned_default_matches_upstream_default() {
        let actual = to_chess_corners_config(&ChessConfig::default());
        let expected = chess_corners::ChessConfig::default();
        assert_chess_config_eq(&actual, &expected);
    }

    #[test]
    fn owned_multiscale_matches_upstream_multiscale() {
        let actual = to_chess_corners_config(&ChessConfig::multiscale());
        let expected = chess_corners::ChessConfig::multiscale();
        assert_chess_config_eq(&actual, &expected);
    }

    #[test]
    fn non_default_conversion_preserves_all_fields() {
        let cfg = ChessConfig {
            detector_mode: DetectorMode::Broad,
            descriptor_mode: DescriptorMode::Canonical,
            threshold_mode: ThresholdMode::Absolute,
            threshold_value: 12.5,
            nms_radius: 5,
            min_cluster_size: 7,
            refiner: RefinerConfig {
                kind: RefinementMethod::Forstner,
                forstner: ForstnerConfig {
                    radius: 3,
                    min_trace: 9.0,
                    min_det: 2.0,
                    max_condition_number: 123.0,
                    max_offset: 2.5,
                },
                ..RefinerConfig::default()
            },
            pyramid_levels: 4,
            pyramid_min_size: 96,
            refinement_radius: 6,
            merge_radius: 4.5,
        };

        let converted = to_chess_corners_config(&cfg);
        let mut expected = chess_corners::ChessConfig::default();
        expected.detector_mode = chess_corners::DetectorMode::Broad;
        expected.descriptor_mode = chess_corners::DescriptorMode::Canonical;
        expected.threshold_mode = chess_corners::ThresholdMode::Absolute;
        expected.threshold_value = 12.5;
        expected.nms_radius = 5;
        expected.min_cluster_size = 7;
        expected.refiner = chess_corners::RefinerConfig {
            kind: chess_corners::RefinementMethod::Forstner,
            center_of_mass: chess_corners::CenterOfMassConfig::default(),
            forstner: chess_corners::ForstnerConfig {
                radius: 3,
                min_trace: 9.0,
                min_det: 2.0,
                max_condition_number: 123.0,
                max_offset: 2.5,
            },
            saddle_point: chess_corners::SaddlePointConfig::default(),
        };
        expected.pyramid_levels = 4;
        expected.pyramid_min_size = 96;
        expected.refinement_radius = 6;
        expected.merge_radius = 4.5;

        assert_chess_config_eq(&converted, &expected);
    }

    #[test]
    fn all_refiner_variants_convert() {
        let refiners = [
            RefinerConfig {
                kind: RefinementMethod::CenterOfMass,
                center_of_mass: CenterOfMassConfig { radius: 4 },
                ..RefinerConfig::default()
            },
            RefinerConfig {
                kind: RefinementMethod::Forstner,
                forstner: ForstnerConfig {
                    radius: 3,
                    min_trace: 11.0,
                    min_det: 0.75,
                    max_condition_number: 512.0,
                    max_offset: 1.75,
                },
                ..RefinerConfig::default()
            },
            RefinerConfig {
                kind: RefinementMethod::SaddlePoint,
                saddle_point: SaddlePointConfig {
                    radius: 5,
                    det_margin: 0.25,
                    max_offset: 1.25,
                    min_abs_det: 0.125,
                },
                ..RefinerConfig::default()
            },
        ];

        for refiner in refiners {
            let cfg = ChessConfig {
                refiner,
                ..ChessConfig::default()
            };
            let converted = to_chess_corners_config(&cfg);
            assert_refiner_eq(&converted.refiner, &to_refiner_config(&cfg.refiner));
        }
    }
}
