use crate::{charuco, chessboard, core, marker};
use chess_corners::{find_chess_corners_image, ChessConfig, CornerDescriptor};
use nalgebra::Point2;

#[cfg(feature = "tracing")]
use tracing::instrument;

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
    let mut cfg = ChessConfig::single_scale();
    cfg.params.threshold_rel = 0.2;
    cfg.params.nms_radius = 2;
    cfg
}

/// Convert an `image::GrayImage` into the lightweight `calib-targets-core` view type.
pub fn gray_view(img: &::image::GrayImage) -> core::GrayImageView<'_> {
    core::GrayImageView {
        width: img.width() as usize,
        height: img.height() as usize,
        data: img.as_raw(),
    }
}

/// Detect raw ChESS corners using `chess-corners`.
#[cfg_attr(
    feature = "tracing",
    instrument(level = "info", skip(img, cfg), fields(width = img.width(), height = img.height()))
)]
pub fn detect_chess_corners_raw(
    img: &::image::GrayImage,
    cfg: &ChessConfig,
) -> Vec<CornerDescriptor> {
    find_chess_corners_image(img, cfg)
}

/// Detect ChESS corners and adapt them into `calib-targets-core::Corner`.
pub fn detect_corners(img: &::image::GrayImage, cfg: &ChessConfig) -> Vec<core::Corner> {
    detect_chess_corners_raw(img, cfg)
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
        skip(img, chess_cfg, board, params),
        fields(
            width = img.width(),
            height = img.height(),
            board_rows = board.rows,
            board_cols = board.cols
        )
    )
)]
pub fn detect_charuco(
    img: &::image::GrayImage,
    chess_cfg: &ChessConfig,
    board: charuco::CharucoBoardSpec,
    params: charuco::CharucoDetectorParams,
) -> Result<charuco::CharucoDetectionResult, DetectError> {
    let corners = detect_corners(img, chess_cfg);
    let detector = charuco::CharucoDetector::new(board, params)?;
    Ok(detector.detect(&gray_view(img), &corners)?)
}

/// Convenience overload using `default_chess_config()` and `CharucoDetectorParams::for_board`.
pub fn detect_charuco_default(
    img: &::image::GrayImage,
    board: charuco::CharucoBoardSpec,
) -> Result<charuco::CharucoDetectionResult, DetectError> {
    let chess_cfg = default_chess_config();
    let params = charuco::CharucoDetectorParams::for_board(&board);
    detect_charuco(img, &chess_cfg, board, params)
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
    board: charuco::CharucoBoardSpec,
    params: charuco::CharucoDetectorParams,
) -> Result<charuco::CharucoDetectionResult, DetectError> {
    let img = gray_image_from_slice(width, height, pixels)?;
    detect_charuco(&img, chess_cfg, board, params)
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

fn adapt_chess_corner(c: &CornerDescriptor) -> core::Corner {
    core::Corner {
        position: Point2::new(c.x, c.y),
        orientation: c.orientation,
        orientation_cluster: None,
        strength: c.response,
    }
}
