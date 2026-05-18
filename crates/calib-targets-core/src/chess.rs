//! ChESS detector configuration types surfaced by the workspace's own API.
//!
//! Re-exports only the two `chess-corners` 0.10 facade types the workspace's
//! public API legitimately exposes: [`DetectorConfig`], the high-level ChESS
//! config object callers construct (strategy + threshold + multiscale +
//! upscale), and [`OrientationMethod`], the documented orientation knob.
//!
//! Advanced ChESS tuning types (`ChessConfig`, `RadonConfig`, `Threshold`,
//! `RefinerKind`, …) are intentionally *not* re-exported here — re-exporting
//! the whole upstream surface would freeze `chess-corners`'s API into this
//! crate's semver contract. Callers needing those types depend on the
//! `chess-corners` crate directly, where they belong.
//!
//! Workspace-only preprocessing (the optional same-size Gaussian pre-blur)
//! is exposed as a standalone helper at the facade level
//! (`calib_targets::preprocess`); detection entry points operate on the
//! image as supplied so the library no longer conflates preprocessing
//! with detection.

pub use chess_corners::{DetectorConfig, OrientationMethod};
