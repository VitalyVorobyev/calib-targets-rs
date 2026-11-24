//! Diagnostics-enabled detector entry points returning raw evidence alongside
//! the typed detection result.

use super::{charuco, chessboard, core, marker, puzzleboard, DetectError};

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

/// [`detect_charuco`](super::detect_charuco) + per-component diagnostics.
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
    img: &core::GrayImageView<'_>,
    params: &charuco::CharucoParams,
) -> (
    Result<charuco::CharucoDetection, DetectError>,
    Option<charuco::diagnostics::CharucoDetectDiagnostics>,
) {
    let detector = match charuco::CharucoDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose(img);
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_charuco_with_corners`](super::detect_charuco_with_corners) + per-component diagnostics.
///
/// `corners` is taken as given; of [`params.chess`](charuco::CharucoParams::chess)
/// only the uncertainty settings are read (see [`detect_charuco_with_corners`](super::detect_charuco_with_corners)). `None` diagnostics means detector construction failed, so
/// the pipeline never ran — see [`diagnose_charuco`].
#[cfg(feature = "diagnostics")]
pub fn diagnose_charuco_with_corners(
    img: &core::GrayImageView<'_>,
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
    let (result, diagnostics) = detector.diagnose_with_corners(img, corners);
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_puzzleboard`](super::detect_puzzleboard) + per-call decode diagnostics.
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
    img: &core::GrayImageView<'_>,
    params: &puzzleboard::PuzzleBoardParams,
) -> (
    Result<puzzleboard::PuzzleBoardDetection, DetectError>,
    Option<puzzleboard::PuzzleBoardDiagnostics>,
) {
    let detector = match puzzleboard::PuzzleBoardDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose(img);
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_puzzleboard_with_corners`](super::detect_puzzleboard_with_corners) + per-call decode diagnostics.
///
/// [`params.chess`](puzzleboard::PuzzleBoardParams::chess) is not read:
/// `corners` is taken as given. `None` diagnostics means detector construction
/// failed, so the pipeline never ran — see [`diagnose_puzzleboard`].
#[cfg(feature = "diagnostics")]
pub fn diagnose_puzzleboard_with_corners(
    img: &core::GrayImageView<'_>,
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
    let (result, diagnostics) = detector.diagnose_with_corners(img, corners);
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_marker_board`](super::detect_marker_board) + per-call circle diagnostics.
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
    img: &core::GrayImageView<'_>,
    params: &marker::MarkerBoardParams,
) -> (
    Result<marker::MarkerBoardDetection, DetectError>,
    Option<marker::MarkerBoardDiagnostics>,
) {
    let detector = match marker::MarkerBoardDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose(img);
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_marker_board_with_corners`](super::detect_marker_board_with_corners) + per-call circle diagnostics.
///
/// [`params.chess`](marker::MarkerBoardParams::chess) is not read: `corners`
/// is taken as given. `None` diagnostics means detector construction failed,
/// so the pipeline never ran — see [`diagnose_marker_board`].
#[cfg(feature = "diagnostics")]
pub fn diagnose_marker_board_with_corners(
    img: &core::GrayImageView<'_>,
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
    let (result, diagnostics) = detector.diagnose_with_corners(img, corners);
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_puzzlepole`](super::detect_puzzlepole) + per-call decode diagnostics.
///
/// The corner pass runs from
/// [`params.chess`](puzzleboard::PuzzlePoleParams::chess).
///
/// `None` diagnostics means **detector construction failed** — `params`
/// described a pole spec the detector cannot build, so the pipeline never ran
/// and no diagnostics exist. Whenever the pipeline ran the diagnostics are
/// `Some`, including on a failed decode (best-effort evidence for overlays).
#[cfg(feature = "diagnostics")]
pub fn diagnose_puzzlepole(
    img: &core::GrayImageView<'_>,
    params: &puzzleboard::PuzzlePoleParams,
) -> (
    Result<puzzleboard::PuzzlePoleDetection, DetectError>,
    Option<puzzleboard::PuzzleBoardDiagnostics>,
) {
    let detector = match puzzleboard::PuzzlePoleDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose(img);
    (result.map_err(DetectError::from), Some(diagnostics))
}

/// [`detect_puzzlepole_with_corners`](super::detect_puzzlepole_with_corners) + per-call decode diagnostics.
///
/// [`params.chess`](puzzleboard::PuzzlePoleParams::chess) is not read:
/// `corners` is taken as given. `None` diagnostics means detector construction
/// failed, so the pipeline never ran — see [`diagnose_puzzlepole`].
#[cfg(feature = "diagnostics")]
pub fn diagnose_puzzlepole_with_corners(
    img: &core::GrayImageView<'_>,
    corners: &[chessboard::ChessCorner],
    params: &puzzleboard::PuzzlePoleParams,
) -> (
    Result<puzzleboard::PuzzlePoleDetection, DetectError>,
    Option<puzzleboard::PuzzleBoardDiagnostics>,
) {
    let detector = match puzzleboard::PuzzlePoleDetector::new(params.clone()) {
        Ok(detector) => detector,
        Err(e) => return (Err(e.into()), None),
    };
    let (result, diagnostics) = detector.diagnose_with_corners(img, corners);
    (result.map_err(DetectError::from), Some(diagnostics))
}
