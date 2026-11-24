//! Multi-config parameter sweeps for top-level detectors.

use super::{
    charuco, chessboard, core, marker, puzzleboard, with_frame_noise, DetectError, DetectorConfig,
};

/// Return the corner pass for `chess_cfg`, running it exactly once per
/// distinct [`DetectorConfig`] within a single sweep.
///
/// The `detect_*_best` helpers honour each config's own `chess` front-end but
/// must not pay for a corner pass per config when several configs share one.
/// They iterate the configs in the caller's original order — reordering would
/// change tie-break outcomes — and consult this memoized cache, so corner
/// detection is deduplicated across configs that request the same front-end
/// while every distinct front-end still gets its own pass. The cached frame
/// also carries the pass's noise level, which the ChArUco sweep hands on to
/// the corners it re-detects.
///
/// [`DetectorConfig`] is `Copy + PartialEq` but not hashable (it carries `f32`
/// fields), so a linear scan over the handful of entries a sweep produces is
/// the right structure. The lookup is index-based (`position` then index) so
/// the immutable borrow of a cached entry never overlaps the `push` of a new
/// one.
fn cached_frame<'cache>(
    img: &core::GrayImageView<'_>,
    chess_cfg: &DetectorConfig,
    cache: &'cache mut Vec<(DetectorConfig, chessboard::CornerFrame)>,
) -> &'cache chessboard::CornerFrame {
    let idx = match cache.iter().position(|(cfg, _)| cfg == chess_cfg) {
        Some(idx) => idx,
        None => {
            let frame = chessboard::detect_corner_frame(img, chess_cfg);
            cache.push((*chess_cfg, frame));
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

// Multi-config sweep helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct TotalF32(f32);

impl PartialEq for TotalF32 {
    fn eq(&self, other: &Self) -> bool {
        self.0.total_cmp(&other.0) == std::cmp::Ordering::Equal
    }
}

impl Eq for TotalF32 {}

impl PartialOrd for TotalF32 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TotalF32 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

fn sweep_best<C, R, K>(
    op: &'static str,
    configs: &[C],
    mut run: impl FnMut(&C) -> Result<R, DetectError>,
    rank: impl Fn(&R) -> K,
) -> Result<R, DetectError>
where
    K: Ord,
{
    if configs.is_empty() {
        return Err(DetectError::EmptySweep { op });
    }
    let mut best: Option<(K, R)> = None;
    let mut last_miss: Option<DetectError> = None;
    for params in configs {
        match run(params) {
            Ok(result) => {
                let key = rank(&result);
                if best.as_ref().is_none_or(|(best_key, _)| key > *best_key) {
                    best = Some((key, result));
                }
            }
            Err(err) => {
                if !err.is_not_found() {
                    return Err(err);
                }
                last_miss = Some(err);
            }
        }
    }
    best.map(|(_, r)| r)
        .ok_or_else(|| last_miss.expect("non-empty sweep returned neither result nor miss"))
}

/// Multi-config-sweep variant of [`detect_chessboard`](super::detect_chessboard): tries every chessboard
/// parameter config and returns the best result (most corners). Configs are
/// tried in order; on ties the first best result wins.
///
/// ChESS corner detection runs once with the supplied [`DetectorConfig`] and
/// the corners are reused across every config in the sweep.
pub fn detect_chessboard_best(
    img: &core::GrayImageView<'_>,
    chess_cfg: &DetectorConfig,
    param_configs: &[chessboard::ChessboardParams],
) -> Result<chessboard::ChessboardDetection, DetectError> {
    let mut frame: Option<chessboard::CornerFrame> = None;
    sweep_best(
        "detect_chessboard_best",
        param_configs,
        |params| {
            let detector = chessboard::ChessboardDetector::new(params.clone())?;
            let f = frame.get_or_insert_with(|| chessboard::detect_corner_frame(img, chess_cfg));
            let d = detector
                .detect(&f.corners)
                .ok_or(DetectError::NoDetection {
                    target: core::TargetKind::Chessboard,
                })?;
            Ok(with_frame_noise(d, f))
        },
        |d| d.corners.len(),
    )
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
    img: &core::GrayImageView<'_>,
    configs: &[charuco::CharucoParams],
) -> Result<charuco::CharucoDetection, DetectError> {
    let mut corner_cache: Vec<(DetectorConfig, chessboard::CornerFrame)> = Vec::new();
    sweep_best(
        "detect_charuco_best",
        configs,
        |params| {
            let detector = charuco::CharucoDetector::new(params.clone())?;
            let frame = cached_frame(img, &params.chess, &mut corner_cache);
            Ok(detector.detect_with_corner_frame(img, frame)?)
        },
        charuco_score,
    )
}

/// Try multiple PuzzleBoard parameter configs. Picks the configuration that
/// labels the most corners with the highest mean decode confidence.
///
/// Each config's own [`PuzzleBoardParams::chess`] front-end is honoured, so a
/// caller's sweep may mix corner front-ends (e.g. a single-scale pass alongside
/// an `UpscaleConfig::Fixed(2)` pass). Corner detection is deduplicated across
/// configs that share a front-end, so a sweep whose configs all share one
/// `chess` — as [`PuzzleBoardParams::sweep_for_board`]'s do — pays for exactly
/// one corner pass.
/// Configs are tried in the given order; a later config replaces the current
/// best only when it is strictly better, so the first best result wins ties.
///
/// [`PuzzleBoardParams::chess`]: puzzleboard::PuzzleBoardParams::chess
/// [`PuzzleBoardParams::sweep_for_board`]: puzzleboard::PuzzleBoardParams::sweep_for_board
pub fn detect_puzzleboard_best(
    img: &core::GrayImageView<'_>,
    configs: &[puzzleboard::PuzzleBoardParams],
) -> Result<puzzleboard::PuzzleBoardDetection, DetectError> {
    let mut corner_cache: Vec<(DetectorConfig, chessboard::CornerFrame)> = Vec::new();
    sweep_best(
        "detect_puzzleboard_best",
        configs,
        |params| {
            let detector = puzzleboard::PuzzleBoardDetector::new(params.clone())?;
            let frame = cached_frame(img, &params.chess, &mut corner_cache);
            let mut r = detector.detect_with_corners(img, &frame.corners)?;
            r.quality = r.quality.with_noise_sigma(frame.noise_sigma);
            Ok(r)
        },
        |r| (r.corners.len(), TotalF32(r.decode.mean_confidence)),
    )
}

/// Try several PuzzlePole configs and keep the best result
/// (most labelled corners, then highest mean bit confidence).
///
/// The recommended entry point, as for every other target: see
/// [`PuzzlePoleParams::sweep_for_pole`]. Corner detection is deduplicated
/// across configs sharing a front-end, and a later config replaces the current
/// best only when strictly better, so the first best wins ties.
///
/// [`PuzzlePoleParams::sweep_for_pole`]: puzzleboard::PuzzlePoleParams::sweep_for_pole
pub fn detect_puzzlepole_best(
    img: &core::GrayImageView<'_>,
    configs: &[puzzleboard::PuzzlePoleParams],
) -> Result<puzzleboard::PuzzlePoleDetection, DetectError> {
    let mut corner_cache: Vec<(DetectorConfig, chessboard::CornerFrame)> = Vec::new();
    sweep_best(
        "detect_puzzlepole_best",
        configs,
        |params| {
            let detector = puzzleboard::PuzzlePoleDetector::new(params.clone())?;
            let frame = cached_frame(img, &params.chess, &mut corner_cache);
            let mut r = detector.detect_with_corners(img, &frame.corners)?;
            r.quality = r.quality.with_noise_sigma(frame.noise_sigma);
            Ok(r)
        },
        |r| (r.corners.len(), TotalF32(r.decode.mean_confidence)),
    )
}

/// Try multiple marker board parameter configs, return the best result (most corners).
///
/// [`MarkerBoardParams::sweep_for_board`] builds the stock config list.
///
/// Each config's own [`MarkerBoardParams::chess`] front-end is honoured; corner
/// detection is deduplicated across configs that share one, so configs may mix
/// front-ends without triggering a redundant corner pass when they agree.
/// Configs are tried in the given order; on ties the **first** best result
/// wins.
///
/// # Errors
///
/// - Empty `configs` or any config rejected by [`MarkerBoardDetector::new`]
///   returns the misuse error immediately.
/// - If every config runs and misses, the last config's own
///   [`MarkerBoardDetectError`] is returned.
///
/// [`MarkerBoardParams::chess`]: marker::MarkerBoardParams::chess
/// [`MarkerBoardParams::sweep_for_board`]: marker::MarkerBoardParams::sweep_for_board
/// [`MarkerBoardDetectError`]: marker::MarkerBoardDetectError
/// [`MarkerBoardDetector::new`]: marker::MarkerBoardDetector::new
pub fn detect_marker_board_best(
    img: &core::GrayImageView<'_>,
    configs: &[marker::MarkerBoardParams],
) -> Result<marker::MarkerBoardDetection, DetectError> {
    let mut corner_cache: Vec<(DetectorConfig, chessboard::CornerFrame)> = Vec::new();
    sweep_best(
        "detect_marker_board_best",
        configs,
        |params| {
            let detector = marker::MarkerBoardDetector::new(params.clone())?;
            let frame = cached_frame(img, &params.chess, &mut corner_cache);
            let mut r = detector.detect_with_corners(img, &frame.corners)?;
            r.quality = r.quality.with_noise_sigma(frame.noise_sigma);
            Ok(r)
        },
        |r| r.corners.len(),
    )
}

/// Scoring key for ChArUco results: (marker count, corner count).
fn charuco_score(r: &charuco::CharucoDetection) -> (usize, usize) {
    (r.markers.len(), r.corners.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn miss() -> DetectError {
        DetectError::NoDetection {
            target: core::TargetKind::Chessboard,
        }
    }

    #[test]
    fn sweep_best_keeps_the_first_of_tied_best_results() {
        // Configs 2 and 3 tie on the top rank; the earlier one must win.
        let best = sweep_best("t", &[1u32, 2, 3], |&c| Ok(c), |&c| u32::from(c != 1));
        assert_eq!(best.unwrap(), 2);
        // A strictly better later result still wins.
        let best = sweep_best("t", &[1u32, 2, 3], |&c| Ok(c), |&c| c);
        assert_eq!(best.unwrap(), 3);
    }

    #[test]
    fn sweep_best_skips_misses_and_stops_on_misuse() {
        let best = sweep_best(
            "t",
            &[1u32, 2],
            |&c| if c == 1 { Err(miss()) } else { Ok(c) },
            |&c| c,
        );
        assert_eq!(best.unwrap(), 2);

        let all_miss = sweep_best("t", &[1u32, 2], |_| Err::<u32, _>(miss()), |&c| c);
        assert!(all_miss.unwrap_err().is_not_found());

        let empty = sweep_best("t", &[] as &[u32], |&c| Ok(c), |&c| c);
        assert!(matches!(empty, Err(DetectError::EmptySweep { op: "t" })));

        let misuse = sweep_best(
            "t",
            &[1u32, 2],
            |&c| {
                if c == 1 {
                    Err(DetectError::EmptySweep { op: "inner" })
                } else {
                    Ok(c)
                }
            },
            |&c| c,
        );
        assert!(matches!(
            misuse,
            Err(DetectError::EmptySweep { op: "inner" })
        ));
    }
}
