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
//!
//! # Stage contract
//!
//! Both entry points ([`run::run_pipeline_lean`] and, behind the
//! `diagnostics` feature, [`run::run_pipeline`]) drive the **same**
//! `find_seed → grow → validate` loop and the **same** converged
//! post-grow stage sequence through shared helpers, so they cannot
//! diverge on which corners are admitted or dropped — only on how much
//! introspection they record. The ordered stage sequence is:
//!
//! 1. **Stage 1 — prefilter.** `Raw → Strong` for corners passing the
//!    strength + fit-quality gates ([`prefilter`]).
//! 2. **Stages 2–3 — cluster.** Two grid-direction centres + per-corner
//!    `Strong → Clustered`/`NoCluster` labels (`crate::cluster`).
//! 3. **Stage 4 — seed.** Self-consistent 4-corner seed quad
//!    (`crate::seed`); derives the cell size.
//! 4. **Stage 5 — grow.** BFS attach from the seed under the
//!    alternating-parity edge invariant (`crate::grow`).
//! 5. **Stage 6 — boundary extension.** Local/global-H prediction at the
//!    labelled-set boundary ([`extension::run_boundary_extension`]).
//! 6. **Stage 6.25 — partial slot-flip fix** (`enable_partial_slot_flip_fix`).
//! 7. **No-cluster rescue** (`enable_no_cluster_rescue`).
//! 8. **Stage 6.75 — post-grow refit** (`enable_post_grow_refit`),
//!    which internally runs a destructive BFS regrow
//!    (`enable_post_grow_bfs_regrow`), a non-destructive BFS extend
//!    (`enable_post_grow_bfs_extend`), then re-runs Stages 6.25 / no-cluster
//!    rescue on the refined centres ([`refit::run_refit`]).
//! 9. **Stage 11 — recall boosters.** Interior gap-fill + line
//!    extrapolation (`crate::boosters`, `enable_weak_cluster_rescue`).
//! 10. **Stage 12 — geometry check** ([`geometry_check`], **mandatory**;
//!     `enable_final_edge_shape_check` toggles only the local edge-shape
//!     sub-gate, not the whole check). Drops any labelled corner that
//!     fails per-edge length + axis-slot parity + global/local-H
//!     residual, and refuses the detection if the survivor count falls
//!     below `min_labeled_corners`.
//! 11. **Stage 6.5b — post-geometry rescue** (`enable_post_geometry_rescue`):
//!     re-run the rescue on cells the geometry check freed, then re-run
//!     the geometry check once.
//! 12. **Stage 13 — output.** Labelled grid → [`ChessboardDetection`]
//!     ([`output`]).
//!
//! ## `enable_*` flag semantics
//!
//! The recall stages (6.25 / 6.5 / 6.75 and its sub-passes, 11, 6.5b)
//! are each gated by an `enable_*` knob on `AdvancedTuning`, all
//! defaulting to `true`. They compose as **monotone, precision-safe
//! recall layers**: each only *adds* labelled corners under the same
//! invariants growth enforces (position match against local-H, parity
//! match, axis-slot-swap edge invariant, ambiguity gate), and the
//! mandatory Stage-12 geometry check runs downstream of all of them, so
//! disabling any flag can only lose recall — never introduce a wrong
//! `(i, j)` label. The flags exist as **debugging seams**: each was
//! added to isolate one chess-corners-0.9 DiskFit / heavy-distortion
//! failure mode, and toggling one off is the documented way to A/B its
//! contribution on a specific image (see each `default_enable_*` rationale
//! in `crate::params::advanced`). They are *not* a stable configuration
//! surface — `AdvancedTuning` is explicitly outside semver.
//!
//! ### Design note: flag-driven vs. trait/strategy composition
//!
//! These post-grow stages are deliberately kept as a **fixed,
//! flag-gated linear sequence** rather than a `Vec<Box<dyn Stage>>`
//! strategy pipeline. The recommendation is to **keep the flag-driven
//! form**, because:
//!
//! - The stages are **not interchangeable or reorderable**: 6.25 must
//!   run before 6.5 (it corrects slot ordering the rescue then consumes);
//!   the refit's regrow must precede its extend; the geometry check must
//!   be the last precision gate, with 6.5b sandwiched so its additions
//!   are re-validated. A trait that allowed arbitrary ordering would
//!   make illegal orderings representable, weakening the precision
//!   argument the fixed order encodes.
//! - Stages share rich mutable state (`augs`, `grow_res`, `active_centers`,
//!   `blacklist`, `cell_size`) by `&mut`; a uniform `dyn Stage::run`
//!   signature would either over-broaden every stage's interface to the
//!   union of all needs or force awkward context structs, trading real
//!   coupling for ceremony.
//! - The flags are debugging instrumentation with per-flag empirical
//!   rationale, not user configuration; a strategy abstraction would
//!   imply an extensibility contract this surface explicitly disclaims.
//!
//! A trait-composed pipeline would be the right move only if the stage
//! set became user-extensible or order-configurable — neither is a goal.

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
