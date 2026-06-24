//! Target-agnostic projective grid recovery primitives.
//!
//! This crate is intentionally small at the public boundary. It models two
//! lattice families, two tasks, and explicit evidence shapes. Target-specific
//! identifiers and detector classes belong in caller crates and should be
//! converted into generic point features or coordinate hypotheses before
//! entering this crate.
//!
//! # Design references
//!
//! The crate is organised around three orthogonal axes — lattice family,
//! recovery strategy, and input-feature kind. The architecture and the
//! orientation-as-an-optional-cue model are documented under `docs/` in the
//! crate source tree:
//!
//! - `docs/DESIGN.md` — the three design axes, the shared pipeline back-half,
//!   and how the lattice family extends to hex.
//! - `docs/ORIENTATION.md` — where each strategy consumes per-corner
//!   orientation and how each can run orientation-free (the dot-grid path).
//!
//! The per-strategy stage maps live in `docs/algorithms/topological-grid-detection.md`
//! (repo root) and `calib-targets-chessboard/docs/PIPELINE.md`.
//!
//! # Two public tiers
//!
//! The crate ships an intentional two-tier public API. Both tiers are
//! supported; they differ in audience and in how fast the surface is allowed
//! to move.
//!
//! ## Stable tier — the facade (ordinary users)
//!
//! The items re-exported at the crate root are the supported surface for an
//! ordinary caller who wants to *detect a grid* and read the labels back:
//!
//! * the entry points [`detect_grid`] / [`detect_grid_all`] and the
//!   consistency check [`check_consistency`];
//! * the request types [`Evidence`] / [`DetectionParams`] /
//!   [`DetectionRequest`] (and the consistency-task [`ConsistencyParams`] /
//!   [`ConsistencyRequest`]);
//! * the result types [`GridSolution`] / [`LabelledGrid`] /
//!   [`DetectionReport`];
//! * the lattice model [`Lattice`] / [`LatticeKind`] / [`Coord`] and the
//!   feature-evidence types ([`PointFeature`], [`OrientedFeature`],
//!   [`LocalAxis`]);
//! * the orientation-synthesis helpers ([`synthesize_oriented2`] and friends)
//!   and the [`cluster_axes`] global-direction prior with its
//!   [`AxisClusterCenters`] / [`AxisAssignment`] types.
//!
//! This tier carries normal semver guarantees: breaking changes here go
//! through the usual deprecation cycle.
//!
//! ## Advanced tier — the composition API (build your own detector)
//!
//! [`shared`], [`topological`], [`lattice`], [`orient`], and [`cluster`]
//! expose the assembly engine the facade is built from, as a deliberate
//! **composition API**. It exists so a consumer with its own per-pattern
//! invariants — the in-workspace chessboard detector is the reference example,
//! but the contract is written for external consumers too — can drive the same
//! grid-growth and recovery machinery under its own policy instead of going
//! through the one-size-fits-all facade. This is intended product, not a
//! private engine that merely happens to be `pub`.
//!
//! The composition contract — what a consumer supplies and what the engine
//! guarantees in return:
//!
//! * **You supply a [`shared::grow::SquareAttachPolicy`].** This is the seam
//!   between the geometry-only growth machinery and your pattern's rules. The
//!   policy answers four questions per candidate — `is_eligible`,
//!   `required_label_at`, `accept_candidate`, `edge_ok` — letting you veto an
//!   attachment that is geometrically plausible but illegal for your pattern
//!   (e.g. a chessboard's alternating-colour parity, or a per-corner
//!   blacklist). The engine never relabels behind your back: every attachment
//!   it proposes has already passed your policy.
//! * **You drive the growth / recovery engine.** The geometry-only primitives
//!   live under [`shared`]: [`shared::grow`] (candidate search + ambiguity
//!   resolution + the per-edge cardinal gate), [`shared::fill`] (interior
//!   hole fill), [`shared::extension`] / [`shared::grow_extend`] (boundary
//!   extension by local homography then cardinal BFS), and
//!   [`shared::recovery_schedule`] — the [`RecoverySchedule`]
//!   fixed-point that composes extension + fill + revalidation + drop filters
//!   into one post-convergence pass. Tune it with [`RecoveryParams`].
//! * **You compose the shared back-half.** After your front-half has built
//!   integer-labelled components, the lattice-parameterised back-half reunites
//!   them with local geometry only ([`shared::merge`]), drops outliers with the
//!   structural-cue gate ([`shared::validate`]), and fits the model→image
//!   projective transform. The whole back-half is written against the
//!   [`Lattice`] trait, so it serves any implemented family.
//! * **The guarantees.** Every stage is *drop-only* with respect to labels: a
//!   corner whose geometry does not cohere is removed, never relabelled. The
//!   precision contract of the facade — **zero wrong `(i, j)` labels**, misses
//!   acceptable — therefore holds for a consumer that composes the engine
//!   under its own policy, because the policy gates and the validate/drop
//!   filters run on every attachment exactly as they do on the facade path.
//!   Returned components are rebased so the labelled bounding-box minimum is
//!   `(0, 0)`; quad / homography corner order is TL, TR, BR, BL.
//!
//! **Stability of the advanced tier.** This surface is *advanced, and may
//! evolve*: its shape tracks the engine's internal structure, so an item may
//! change between minor releases when the engine is refactored. That is a
//! deliberate product choice — the tier trades a slower-moving contract for
//! direct access to the engine — not a disclaimer that it is really private.
//! If you only need to detect a grid, depend on the facade; reach for the
//! composition tier when you are building a *new kind of detector* on top of
//! the engine. Engine items with no external consumer stay `pub(crate)`, so
//! the advanced surface is kept no wider than what a consumer actually
//! composes.
//!
//! # Core vs. extended surface
//!
//! Orthogonal to the stable/advanced split is a second distinction worth
//! making explicit, because the two halves are exercised very differently in
//! practice:
//!
//! * **Core path (exercised in production).** The square lattice
//!   ([`LatticeKind::Square`]) driven by two-axis evidence
//!   ([`Evidence::Oriented2`]). This is the path every in-workspace detector —
//!   chessboard, ChArUco, puzzleboard, marker board — actually runs, and it is
//!   the most heavily regression-tested surface in the crate.
//! * **Extended path (library-only breadth).** The hexagonal lattice
//!   ([`Hex`], the [`topological`] hex arm, [`D6_TRANSFORMS`]) and the
//!   orientation-free / single-axis synthesis ([`orient`],
//!   [`shared::recovery_schedule`], [`Evidence::Positions`] /
//!   [`Evidence::Oriented1`]). This breadth is intended external product — it
//!   is published precisely so a downstream user can detect a dot grid or a
//!   hex target — but no in-workspace detector exercises it today; it is
//!   covered by this crate's own tests. See `docs/DESIGN.md` for the full
//!   core-vs-extended map.

#![warn(missing_docs)]

pub mod check;
pub mod cluster;
pub mod detect;
pub mod error;
pub mod feature;
pub mod float;
pub mod geometry;
pub mod lattice;
pub mod orient;
pub mod result;

pub use crate::check::{check_consistency, ConsistencyParams, ConsistencyRequest};
pub use crate::cluster::{
    cluster_axes, AxisAssignment, AxisClusterCenters, AxisClusterDebug, AxisFeature,
    AxisObservation, ClusterParams,
};
pub use crate::detect::{
    detect_grid, detect_grid_all, DetectionParams, DetectionReport, DetectionRequest, Evidence,
    RecoveryParams, RecoverySchedule, TopologicalParams,
};
pub use crate::error::{EvidenceKind, GridError, GridTask};
pub use crate::feature::{CoordinateHypothesis, LocalAxis, OrientedFeature, PointFeature};
pub use crate::float::Float;
pub use crate::lattice::{
    Coord, GridDimensions, GridTransform, Hex, Lattice, LatticeKind, Square, D4_TRANSFORMS,
    D6_TRANSFORMS, HEX_AXIAL_OFFSETS, SQUARE_CARDINAL_OFFSETS,
};
pub use crate::orient::{
    synthesize_oriented2, synthesize_oriented2_from_oriented1, synthesize_oriented3,
};
pub use crate::result::{
    ConsistencyReport, GridEntry, GridSolution, LabelledGrid, LatticeFit, RejectedFeature,
    RejectionReason, ResidualSummary,
};

pub mod shared;
pub mod topological;
