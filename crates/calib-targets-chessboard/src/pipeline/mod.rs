//! Detector pipeline: stage modules + the thin orchestrator.
//!
//! [`crate::detector::Detector`] is a thin set of entry points; the
//! actual work is decomposed here, one module per stage group:
//!
//! | Module | Responsibility |
//! |---|---|
//! | [`types`] | [`Detection`], [`DebugFrame`], and per-stage trace structs. |
//! | [`prefilter`] | Stage 1 — strength / fit-quality gates. |
//! | [`extension`] | Stages 6 / 8 — boundary extension + NoCluster rescue. |
//! | [`refit`] | Stage 9 — post-grow centre refit + second extension pass. |
//! | [`geometry_check`] | Stage 12 — the mandatory final precision gate. |
//! | [`output`] | Stage 13 — labelled grid → [`Detection`]. |
//! | [`run`] | The orchestrator: the seed → grow → validate loop. |
//!
//! Clustering (Stage 2/3), seed (Stage 4), grow (Stage 5), and
//! boosters (Stage 11) keep their own top-level modules — this
//! subtree only houses what previously lived inline in `detector.rs`.

pub mod extension;
pub mod geometry_check;
pub mod output;
pub mod prefilter;
pub mod refit;
pub mod run;
pub mod types;

pub use geometry_check::run_geometry_check;
pub use output::build_detection_from_grow;
pub use run::run_pipeline;
pub use types::{
    BfsExtendTrace, DebugFrame, Detection, ExtensionTrace, GeometryCheckTrace, InstrumentedResult,
    IterationTrace, RefitTrace, StageCounts, DEBUG_FRAME_SCHEMA,
};
