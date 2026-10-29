//! ChESS detector configuration types surfaced by the workspace's own API.
//!
//! Re-exports only the `chess-corners` facade types the workspace's public
//! API legitimately exposes: [`DetectorConfig`], the high-level ChESS config
//! object callers construct, and the types of the top-level fields callers
//! routinely set on it — [`OrientationMethod`], [`ImageRefiner`] (the
//! saddle-point refiner) with [`OnRefinerReject`], and [`Uncertainty`] /
//! [`NoiseLevel`] (the opt-in position covariance). They are part of `DetectorConfig`'s shape, so naming
//! them adds no contract.
//!
//! Advanced ChESS tuning types (`ChessConfig`, `RadonConfig`,
//! `SaddlePointConfig`, …) are intentionally *not* re-exported here —
//! re-exporting the whole upstream surface would freeze `chess-corners`'s API
//! into this crate's semver contract. Callers needing those types depend on
//! the `chess-corners` crate directly, where they belong.
//!
//! This module also owns [`default_chess_config`], the single definition of
//! the workspace's corner front-end settings. It lives here rather than on
//! the facade because every detector's params struct carries a
//! `DetectorConfig` and must be able to default it without depending on the
//! facade (which would be a dependency cycle).
//!
//! Workspace-only preprocessing (the optional same-size Gaussian pre-blur)
//! is exposed as a standalone helper at the facade level
//! (`calib_targets::preprocess`); detection entry points operate on the
//! image as supplied so the library no longer conflates preprocessing
//! with detection.

pub use chess_corners::{
    DetectorConfig, ImageRefiner, NoiseLevel, OnRefinerReject, OrientationMethod, Uncertainty,
};

/// The workspace's ChESS acceptance threshold.
///
/// Kept as a named constant so the value has exactly one definition and the
/// test that guards it against upstream drift can name it.
const WORKSPACE_CHESS_THRESHOLD: f32 = 15.0;

/// Reasonable default settings for the `chess-corners` ChESS detector.
///
/// Built on top of [`DetectorConfig::chess`] but overrides the acceptance
/// threshold to `15.0`. Since `chess-corners` 1.0 the threshold is a single
/// `f32` that the ChESS strategy reads as an absolute floor on the raw
/// response; the paper-faithful contract is `0.0`, which is correct in
/// principle (any strictly positive ChESS response is a corner candidate) but
/// produces hundreds of weak responses on real-world images. The
/// topological grid pipeline is sensitive to that noise floor: on
/// `testdata/puzzleboard_reference/example3.png`, threshold `0.0` produces
/// zero labelled corners while `15.0` recovers the full 30-corner component;
/// on `testdata/small0.png` the labelled count rises from 78 to 129; and on
/// the `02-topo-grid/` synthetic suite the topological pipeline only clears
/// every recall gate at `≥ 15.0`. The cutoff was chosen by sweeping the
/// public testdata regression set; see
/// `crates/calib-targets/examples/threshold_sweep.rs`.
///
/// Note this is *lower* than upstream's own 1.0 default of `30.0`. That is
/// deliberate and unchanged in substance: the workspace has always overridden
/// the upstream default, and the ChESS response scale did not change in 1.0,
/// so `15.0` keeps producing exactly the corner set it did under 0.11.
///
/// **Localizer: the saddle-point image refiner, with the center of mass as
/// its fallback.** Each corner is placed by a saddle-point fit of the image
/// patch ([`ImageRefiner::saddle_point`]); a corner the fit rejects keeps the
/// ChESS center-of-mass position ([`OnRefinerReject::KeepCenterOfMass`])
/// instead of being dropped. On the workspace's ground-truth accuracy suite
/// the saddle-point fit is four to five times more accurate than the center
/// of mass on every target family, and with the fallback it finds at least as
/// many corners — without it, a cluttered or blurred board loses real corners
/// the fit rejects. Select [`DetectorConfig::without_refiner`] for the center
/// of mass alone.
///
/// Every other field keeps the upstream default — including the
/// position-uncertainty tier, which is `Uncertainty::Off`: corners carry no
/// position covariance. Select `Uncertainty::Propagated` to get one (it costs
/// a per-image noise estimate plus a few percent of the corner pass); corner
/// positions do not depend on the tier. The covariance is a report: weighting
/// a calibration by it has not been shown to help, and its absolute scale is
/// fitted on one camera, so compare it across frames of one setup rather than
/// across setups.
///
/// This is the default value of the `chess` field carried by the
/// compound-target params structs — `CharucoParams`, `PuzzleBoardParams`, and
/// `MarkerBoardParams` — whose facade image entry points own their corner pass;
/// override the front-end by replacing `params.chess`. The plain chessboard
/// path is the deliberate exception: `detect_chessboard(img, &chess_cfg,
/// &params)` takes the corner config as an explicit argument alongside
/// `ChessboardParams`, because the chessboard detector is a reusable
/// corner-cloud consumer rather than a whole-image pipeline. Either way, a
/// coarse-to-fine pyramid via `MultiscaleConfig::Pyramid` or a pre-pipeline
/// `UpscaleConfig::Fixed` for low-resolution boards is a matter of replacing
/// this config where the entry point reads it.
///
/// Callers wanting the raw upstream behaviour can construct
/// [`DetectorConfig::chess`] directly.
pub fn default_chess_config() -> DetectorConfig {
    DetectorConfig::chess()
        .with_threshold(WORKSPACE_CHESS_THRESHOLD)
        .with_refiner(ImageRefiner::saddle_point())
        .with_chess(|c| c.on_refiner_reject = OnRefinerReject::KeepCenterOfMass)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess_corners::DetectionStrategy;

    #[test]
    fn default_chess_config_overrides_threshold() {
        // Workspace default deliberately overrides the upstream contract
        // (`threshold = 0.0`, "every strictly-positive response is a corner")
        // with a small noise-floor cutoff tuned on the public testdata
        // regression sweep — see the rustdoc on `default_chess_config`.
        let cfg = default_chess_config();
        assert_eq!(cfg.threshold, WORKSPACE_CHESS_THRESHOLD);

        // Guard the override against an upstream default drifting into it:
        // chess-corners 1.0 raised its own ChESS default from 0.0 to 30.0, so
        // an assertion of the form `cfg.threshold != DetectorConfig::chess()`
        // is the part that actually has teeth.
        assert_ne!(
            cfg.threshold,
            DetectorConfig::chess().threshold,
            "workspace default must stay an explicit override, not silently \
             inherit whatever upstream picks"
        );

        // Strategy must still be the ChESS kernel pipeline (not Radon),
        // and the multiscale / upscale top-level fields must match the
        // single-scale ChESS preset.
        assert!(matches!(cfg.strategy, DetectionStrategy::Chess(_)));
        let baseline = DetectorConfig::chess();
        assert_eq!(cfg.multiscale, baseline.multiscale);
        assert_eq!(cfg.upscale, baseline.upscale);
        assert_eq!(cfg.merge_radius, baseline.merge_radius);
        assert_eq!(cfg.orientation_method, baseline.orientation_method);

        // The shared detection knobs (`nms_radius` / `min_cluster_size` moved
        // here from the per-strategy config in 1.0) stay at upstream defaults.
        assert_eq!(cfg.detection, baseline.detection);

        // The position-uncertainty tier stays at the upstream default: no
        // covariance.
        assert_eq!(cfg.uncertainty, baseline.uncertainty);
        assert_eq!(cfg.uncertainty, chess_corners::Uncertainty::Off);

        // The localizer is the workspace's own choice: the saddle-point
        // refiner, keeping the center of mass for the corners it rejects.
        assert_eq!(cfg.refiner, Some(ImageRefiner::saddle_point()));
        assert_ne!(cfg.refiner, baseline.refiner);

        // The nested ChESS strategy fields stay at upstream defaults, except
        // that a corner the refiner rejects keeps its center-of-mass position.
        let DetectionStrategy::Chess(chess) = cfg.strategy else {
            unreachable!("matched above");
        };
        let DetectionStrategy::Chess(chess_baseline) = baseline.strategy else {
            unreachable!("baseline preset is ChESS");
        };
        assert_eq!(chess.ring, chess_baseline.ring);
        assert_eq!(chess.center_of_mass, chess_baseline.center_of_mass);
        assert_eq!(chess.on_refiner_reject, OnRefinerReject::KeepCenterOfMass);
    }
}
