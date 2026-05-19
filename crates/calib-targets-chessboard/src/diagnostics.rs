//! Opt-in diagnostics surface for the chessboard detector.
//!
//! The types re-exported here carry evidence about *how* a detection was
//! reached: per-corner stage assignments, per-iteration pipeline traces,
//! cluster histograms, geometry-check outcomes, and booster results.
//!
//! These types have a **looser stability promise** than the result API
//! ([`crate::ChessboardDetection`], [`crate::Detector`], [`crate::DetectorParams`]).
//! New fields may be added, field types may change, and the serialization
//! schema may evolve between minor versions. Code that only cares about
//! *whether* detection succeeded and *which corners were found* should
//! use the result API; code that needs to inspect internals for debugging,
//! benchmarking, or visualisation opts in here.

pub use crate::boosters::BoosterResult;
pub use crate::cluster::ClusterDebug;
pub use crate::corner::{ClusterLabel, CornerAug, CornerStage};
pub use crate::pipeline::{
    BfsExtendTrace, DebugFrame, ExtensionTrace, GeometryCheckTrace, IterationTrace, RefitTrace,
    StageCounts, DEBUG_FRAME_SCHEMA,
};
