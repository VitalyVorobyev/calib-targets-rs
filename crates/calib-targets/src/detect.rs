use crate::{charuco, chessboard, core, marker};
use chess_corners::find_chess_corners_image;
use nalgebra::Point2;

#[cfg(feature = "tracing")]
use tracing::instrument;

pub use core::{
    CenterOfMassConfig, ChessConfig, ChessCornerParams, CoarseToFineParams, ForstnerConfig,
    PyramidParams, RefinerConfig, SaddlePointConfig,
};

/// Errors produced by the high-level facade helpers.
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
}

/// Reasonable default settings for the `chess-corners` ChESS detector.
///
/// This is tuned for the repo examples and is expected to be overridden by callers
/// for difficult real-world images.
pub fn default_chess_config() -> ChessConfig {
    ChessConfig {
        params: ChessCornerParams {
            threshold_rel: 0.2,
            nms_radius: 2,
            ..ChessCornerParams::default()
        },
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
    chess_cfg: &ChessConfig,
    params: chessboard::ChessboardParams,
) -> Option<chessboard::ChessboardDetectionResult> {
    let corners = detect_corners(img, chess_cfg);
    let detector = chessboard::ChessboardDetector::new(params);
    detector.detect_from_corners(&corners)
}

/// Run the ChArUco detector end-to-end: ChESS corners -> grid -> markers -> alignment -> IDs.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, chess_cfg, params),
        fields(
            width = img.width(),
            height = img.height(),
            board_rows = params.charuco.rows,
            board_cols = params.charuco.cols
        )
    )
)]
pub fn detect_charuco(
    img: &::image::GrayImage,
    chess_cfg: &ChessConfig,
    params: charuco::CharucoDetectorParams,
) -> Result<charuco::CharucoDetectionResult, DetectError> {
    let corners = detect_corners(img, chess_cfg);
    let detector = charuco::CharucoDetector::new(params)?;
    Ok(detector.detect(&gray_view(img), &corners)?)
}

/// Convenience overload using `default_chess_config()`.
pub fn detect_charuco_default(
    img: &::image::GrayImage,
    params: charuco::CharucoDetectorParams,
) -> Result<charuco::CharucoDetectionResult, DetectError> {
    let chess_cfg = default_chess_config();
    detect_charuco(img, &chess_cfg, params)
}

/// Run the checkerboard+circles marker board detector end-to-end.
#[cfg_attr(
    feature = "tracing",
    instrument(
        level = "info",
        skip(img, chess_cfg, params),
        fields(width = img.width(), height = img.height())
    )
)]
pub fn detect_marker_board(
    img: &::image::GrayImage,
    chess_cfg: &ChessConfig,
    params: marker::MarkerBoardParams,
) -> Option<marker::MarkerBoardDetectionResult> {
    let corners = detect_corners(img, chess_cfg);
    let detector = marker::MarkerBoardDetector::new(params);
    detector.detect_from_image_and_corners(&gray_view(img), &corners)
}

/// Convenience overload using `default_chess_config()`.
pub fn detect_marker_board_default(
    img: &::image::GrayImage,
    params: marker::MarkerBoardParams,
) -> Option<marker::MarkerBoardDetectionResult> {
    let chess_cfg = default_chess_config();
    detect_marker_board(img, &chess_cfg, params)
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

pub fn detect_chessboard_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: &ChessConfig,
    params: chessboard::ChessboardParams,
) -> Result<Option<chessboard::ChessboardDetectionResult>, DetectError> {
    let img = gray_image_from_slice(width, height, pixels)?;
    Ok(detect_chessboard(&img, chess_cfg, params))
}

pub fn detect_charuco_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: &ChessConfig,
    params: charuco::CharucoDetectorParams,
) -> Result<charuco::CharucoDetectionResult, DetectError> {
    let img = gray_image_from_slice(width, height, pixels)?;
    detect_charuco(&img, chess_cfg, params)
}

pub fn detect_marker_board_from_gray_u8(
    width: u32,
    height: u32,
    pixels: &[u8],
    chess_cfg: &ChessConfig,
    params: marker::MarkerBoardParams,
) -> Result<Option<marker::MarkerBoardDetectionResult>, DetectError> {
    let img = gray_image_from_slice(width, height, pixels)?;
    Ok(detect_marker_board(&img, chess_cfg, params))
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
    out.params = to_chess_params(&cfg.params);
    out.multiscale = to_coarse_to_fine_params(&cfg.multiscale);
    out
}

fn to_chess_params(params: &ChessCornerParams) -> chess_corners::ChessParams {
    let mut out = chess_corners::ChessParams::default();
    out.use_radius10 = params.use_radius10;
    out.descriptor_use_radius10 = params.descriptor_use_radius10;
    out.threshold_rel = params.threshold_rel;
    out.threshold_abs = params.threshold_abs;
    out.nms_radius = params.nms_radius;
    out.min_cluster_size = params.min_cluster_size;
    out.refiner = to_refiner_kind(&params.refiner);
    out
}

fn to_coarse_to_fine_params(params: &CoarseToFineParams) -> chess_corners::CoarseToFineParams {
    let mut out = chess_corners::CoarseToFineParams::default();
    out.pyramid = to_pyramid_params(&params.pyramid);
    out.refinement_radius = params.refinement_radius;
    out.merge_radius = params.merge_radius;
    out
}

fn to_pyramid_params(params: &PyramidParams) -> chess_corners::PyramidParams {
    let mut out = chess_corners::PyramidParams::default();
    out.num_levels = params.num_levels;
    out.min_size = params.min_size;
    out
}

fn to_refiner_kind(refiner: &RefinerConfig) -> chess_corners::RefinerKind {
    match refiner {
        RefinerConfig::CenterOfMass(cfg) => {
            chess_corners::RefinerKind::CenterOfMass(chess_corners::CenterOfMassConfig {
                radius: cfg.radius,
            })
        }
        RefinerConfig::Forstner(cfg) => {
            chess_corners::RefinerKind::Forstner(chess_corners::ForstnerConfig {
                radius: cfg.radius,
                min_trace: cfg.min_trace,
                min_det: cfg.min_det,
                max_condition_number: cfg.max_condition_number,
                max_offset: cfg.max_offset,
            })
        }
        RefinerConfig::SaddlePoint(cfg) => {
            chess_corners::RefinerKind::SaddlePoint(chess_corners::SaddlePointConfig {
                radius: cfg.radius,
                det_margin: cfg.det_margin,
                max_offset: cfg.max_offset,
                min_abs_det: cfg.min_abs_det,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_refiner_eq(
        actual: &chess_corners::RefinerKind,
        expected: &chess_corners::RefinerKind,
    ) {
        match (actual, expected) {
            (
                chess_corners::RefinerKind::CenterOfMass(actual),
                chess_corners::RefinerKind::CenterOfMass(expected),
            ) => assert_eq!(actual.radius, expected.radius),
            (
                chess_corners::RefinerKind::Forstner(actual),
                chess_corners::RefinerKind::Forstner(expected),
            ) => {
                assert_eq!(actual.radius, expected.radius);
                assert_eq!(actual.min_trace, expected.min_trace);
                assert_eq!(actual.min_det, expected.min_det);
                assert_eq!(actual.max_condition_number, expected.max_condition_number);
                assert_eq!(actual.max_offset, expected.max_offset);
            }
            (
                chess_corners::RefinerKind::SaddlePoint(actual),
                chess_corners::RefinerKind::SaddlePoint(expected),
            ) => {
                assert_eq!(actual.radius, expected.radius);
                assert_eq!(actual.det_margin, expected.det_margin);
                assert_eq!(actual.max_offset, expected.max_offset);
                assert_eq!(actual.min_abs_det, expected.min_abs_det);
            }
            _ => panic!("refiner kind mismatch"),
        }
    }

    fn assert_chess_params_eq(
        actual: &chess_corners::ChessParams,
        expected: &chess_corners::ChessParams,
    ) {
        assert_eq!(actual.use_radius10, expected.use_radius10);
        assert_eq!(
            actual.descriptor_use_radius10,
            expected.descriptor_use_radius10
        );
        assert_eq!(actual.threshold_rel, expected.threshold_rel);
        assert_eq!(actual.threshold_abs, expected.threshold_abs);
        assert_eq!(actual.nms_radius, expected.nms_radius);
        assert_eq!(actual.min_cluster_size, expected.min_cluster_size);
        assert_refiner_eq(&actual.refiner, &expected.refiner);
    }

    fn assert_chess_config_eq(
        actual: &chess_corners::ChessConfig,
        expected: &chess_corners::ChessConfig,
    ) {
        assert_chess_params_eq(&actual.params, &expected.params);
        assert_eq!(
            actual.multiscale.pyramid.num_levels,
            expected.multiscale.pyramid.num_levels
        );
        assert_eq!(
            actual.multiscale.pyramid.min_size,
            expected.multiscale.pyramid.min_size
        );
        assert_eq!(
            actual.multiscale.refinement_radius,
            expected.multiscale.refinement_radius
        );
        assert_eq!(
            actual.multiscale.merge_radius,
            expected.multiscale.merge_radius
        );
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
            params: ChessCornerParams {
                use_radius10: true,
                descriptor_use_radius10: Some(false),
                threshold_rel: 0.35,
                threshold_abs: Some(12.5),
                nms_radius: 5,
                min_cluster_size: 7,
                refiner: RefinerConfig::Forstner(ForstnerConfig {
                    radius: 3,
                    min_trace: 9.0,
                    min_det: 2.0,
                    max_condition_number: 123.0,
                    max_offset: 2.5,
                }),
            },
            multiscale: CoarseToFineParams {
                pyramid: PyramidParams {
                    num_levels: 4,
                    min_size: 96,
                },
                refinement_radius: 6,
                merge_radius: 4.5,
            },
        };

        let converted = to_chess_corners_config(&cfg);
        let mut expected = chess_corners::ChessConfig::default();
        expected.params.use_radius10 = true;
        expected.params.descriptor_use_radius10 = Some(false);
        expected.params.threshold_rel = 0.35;
        expected.params.threshold_abs = Some(12.5);
        expected.params.nms_radius = 5;
        expected.params.min_cluster_size = 7;
        expected.params.refiner =
            chess_corners::RefinerKind::Forstner(chess_corners::ForstnerConfig {
                radius: 3,
                min_trace: 9.0,
                min_det: 2.0,
                max_condition_number: 123.0,
                max_offset: 2.5,
            });
        expected.multiscale.pyramid = chess_corners::PyramidParams::default();
        expected.multiscale.pyramid.num_levels = 4;
        expected.multiscale.pyramid.min_size = 96;
        expected.multiscale.refinement_radius = 6;
        expected.multiscale.merge_radius = 4.5;

        assert_chess_config_eq(&converted, &expected);
    }

    #[test]
    fn all_refiner_variants_convert() {
        let refiners = [
            RefinerConfig::CenterOfMass(CenterOfMassConfig { radius: 4 }),
            RefinerConfig::Forstner(ForstnerConfig {
                radius: 3,
                min_trace: 11.0,
                min_det: 0.75,
                max_condition_number: 512.0,
                max_offset: 1.75,
            }),
            RefinerConfig::SaddlePoint(SaddlePointConfig {
                radius: 5,
                det_margin: 0.25,
                max_offset: 1.25,
                min_abs_det: 0.125,
            }),
        ];

        for refiner in refiners {
            let params = ChessCornerParams {
                refiner,
                ..ChessCornerParams::default()
            };
            let converted = to_chess_params(&params);
            assert_refiner_eq(&converted.refiner, &to_refiner_kind(&params.refiner));
        }
    }
}
