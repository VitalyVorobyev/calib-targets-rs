//! ChESS detector configuration types surfaced by the workspace's own API.
//!
//! Re-exports only the two `chess-corners` facade types the workspace's
//! public API legitimately exposes: [`DetectorConfig`], the high-level ChESS
//! config object callers construct (strategy + threshold + multiscale +
//! upscale), and [`OrientationMethod`], the documented orientation knob.
//!
//! Advanced ChESS tuning types (`ChessConfig`, `RadonConfig`, `RefinerKind`,
//! …) are intentionally *not* re-exported here — re-exporting the whole
//! upstream surface would freeze `chess-corners`'s API into this crate's
//! semver contract. Callers needing those types depend on the
//! `chess-corners` crate directly, where they belong.
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

pub use chess_corners::{DetectorConfig, OrientationMethod};

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
/// This is the default value of the `chess` field on every detector's params
/// struct ([`calib_targets_chessboard`]-driven detectors take it as an
/// explicit argument instead), so overriding the corner front-end — a
/// coarse-to-fine pyramid via `MultiscaleConfig::Pyramid`, or a pre-pipeline
/// `UpscaleConfig::Fixed` for low-resolution boards — is a matter of
/// replacing this value on the params struct.
///
/// Callers wanting the raw upstream behaviour can construct
/// [`DetectorConfig::chess`] directly.
///
/// [`calib_targets_chessboard`]: https://docs.rs/calib-targets-chessboard
pub fn default_chess_config() -> DetectorConfig {
    DetectorConfig::chess().with_threshold(WORKSPACE_CHESS_THRESHOLD)
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

        // The nested ChESS strategy fields stay at upstream defaults so
        // the override is purely the acceptance threshold.
        let DetectionStrategy::Chess(chess) = cfg.strategy else {
            unreachable!("matched above");
        };
        let DetectionStrategy::Chess(chess_baseline) = baseline.strategy else {
            unreachable!("baseline preset is ChESS");
        };
        assert_eq!(chess.ring, chess_baseline.ring);
        assert_eq!(chess.refiner, chess_baseline.refiner);
    }
}
