//! Detector pipeline: stage modules + the thin orchestrator.
//!
//! [`crate::detector::Detector`] is a thin set of entry points; the
//! actual work is decomposed here, one module per stage group:
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`types`] | [`ChessboardDetection`], the lean `PipelineOutcome`, and (behind the `diagnostics` feature) the `DebugFrame` + per-stage trace structs. |
//! | [`prefilter`] | Stage 1 — strength / fit-quality gates. |
//! | [`extension`] | Stages 6 / 8 — boundary extension + NoCluster rescue. |
//! | [`refit`] | Stage 9 — post-grow centre refit + second extension pass. |
//! | [`geometry_check`] | Stage 12 — the mandatory final precision gate. |
//! | [`output`] | Stage 13 — labelled grid → [`ChessboardDetection`]. |
//! | [`run`] | The orchestrator: the seed → grow → validate loop. |
//!
//! Clustering (Stage 2/3), seed (Stage 4), grow (Stage 5), and
//! boosters (Stage 11) keep their own top-level modules — this
//! subtree only houses what previously lived inline in `detector.rs`.

mod extension;
mod geometry_check;
mod output;
mod prefilter;
mod refit;
mod run;
mod types;

pub use geometry_check::run_geometry_check;
pub use output::build_detection_from_grow;
pub(crate) use run::run_pipeline_lean;
pub use types::{ChessboardCorner, ChessboardDetection};

// Diagnostic-only surface: assembled solely by `detect*_with_diagnostics`
// (behind the `diagnostics` feature). The hot `detect()` path returns a
// lean `PipelineOutcome` and never builds a [`DebugFrame`].
#[cfg(feature = "diagnostics")]
pub use run::run_pipeline;
#[cfg(feature = "diagnostics")]
pub use types::{
    BfsExtendTrace, DebugFrame, ExtensionTrace, GeometryCheckTrace, IterationTrace, RefitTrace,
    StageCounts, DEBUG_FRAME_SCHEMA,
};
