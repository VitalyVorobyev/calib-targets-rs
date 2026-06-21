//! Grow policy trait, tuning, and result types.
//!
//! This module owns the growth *contract*: the [`SquareAttachPolicy`] trait
//! that lets a caller plug pattern-specific invariants into the otherwise
//! pure-geometry candidate search, the per-candidate [`Admit`] /
//! [`LabelledNeighbour`] / [`FillEdgeCtx`] data carriers, the [`GrowParams`]
//! tolerances, and the [`GrowResult`] output container. It deliberately holds
//! no algorithm — the candidate-search / ambiguity helpers live in
//! [`super`](crate::shared::grow) and the prediction geometry in
//! [`super::predict`] — so the policy contract can be read without wading
//! through the search loop. Tier: advanced engine (semver-exempt pre-1.0).

use nalgebra::{Point2, Vector2};
use std::collections::{HashMap, HashSet};

/// Per-candidate decision from a [`SquareAttachPolicy`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admit {
    /// Accept this candidate at the given grid cell.
    Accept,
    /// Reject this candidate; the generic code may move on to the
    /// next nearest (if any).
    Reject,
}

/// Information about an existing labelled neighbour, passed to the
/// policy during candidate evaluation.
#[derive(Clone, Copy, Debug)]
pub struct LabelledNeighbour {
    /// Index of the neighbour corner in the caller's position array.
    pub idx: usize,
    /// The neighbour's `(i, j)` grid cell.
    pub at: (i32, i32),
    /// The neighbour's position in image pixels.
    pub position: Point2<f32>,
}

/// Caller-supplied attachment policy for the square-lattice growth helpers.
///
/// Implementations typically hold references to the caller's feature
/// data (axes, labels, strengths) plus tuning parameters, and use `idx`
/// to look up the relevant per-feature record inside each callback.
pub trait SquareAttachPolicy {
    /// Is this corner index a possible candidate at all? Called
    /// once per corner when the KD-tree is built.
    fn is_eligible(&self, idx: usize) -> bool;

    /// Optional caller-defined label required at grid cell `(i, j)`.
    /// Return `None` for no constraint.
    fn required_label_at(&self, i: i32, j: i32) -> Option<u8>;

    /// Return the label of the corner at `idx`. Must agree with
    /// `required_label_at` at attachment time. Called during
    /// candidate filtering.
    fn label_of(&self, idx: usize) -> Option<u8>;

    /// Accept or reject a candidate for attachment at grid cell
    /// `at` given its geometric prediction and existing labelled
    /// neighbours. Called per candidate in order of increasing
    /// distance to `prediction`.
    fn accept_candidate(
        &self,
        idx: usize,
        at: (i32, i32),
        prediction: Point2<f32>,
        neighbours: &[LabelledNeighbour],
    ) -> Admit;

    /// Soft per-edge check: is the induced edge between the just-
    /// attached candidate and one of its cardinal-labelled neighbours
    /// admissible? At least one cardinal edge must pass for the
    /// attachment to stick; otherwise the position is marked a hole
    /// and the candidate is rolled back.
    ///
    /// Default: accept all edges (no soft check).
    fn edge_ok(
        &self,
        _candidate_idx: usize,
        _neighbour_idx: usize,
        _at_candidate: (i32, i32),
        _at_neighbour: (i32, i32),
    ) -> bool {
        true
    }

    /// Optional widened eligibility used by the fill-pass booster.
    ///
    /// Defaults to [`Self::is_eligible`]; patterns whose precision
    /// core admits only `Clustered` corners but want to admit a few
    /// near-cluster corners during the booster pass override this to
    /// expand the admissible set. The fill pass calls this when
    /// building its KD-tree; the regular grow / boundary-extension
    /// passes ignore it.
    fn eligible_for_fill(&self, idx: usize) -> bool {
        self.is_eligible(idx)
    }

    /// Optional fill-pass edge check that has access to the full
    /// labelled set and the position table via [`FillEdgeCtx`].
    ///
    /// The default delegates to [`Self::edge_ok`], ignoring the extra
    /// context. Pattern implementations that need a directional edge
    /// metric (e.g., a strongly anisotropic component where the
    /// horizontal pitch is much larger than the vertical pitch and a
    /// scalar `cell_size` rejects legitimate vertical extrapolations)
    /// override this to consult the labelled set when computing the
    /// expected edge length.
    ///
    /// Only invoked by [`crate::shared::fill::fill_grid_holes`]; the
    /// regular grow and boundary-extension passes call [`Self::edge_ok`]
    /// directly.
    fn fill_edge_ok(&self, ctx: FillEdgeCtx<'_>) -> bool {
        self.edge_ok(
            ctx.candidate_idx,
            ctx.neighbour_idx,
            ctx.at_candidate,
            ctx.at_neighbour,
        )
    }
}

/// Context passed to [`SquareAttachPolicy::fill_edge_ok`].
///
/// Bundles every piece of state the policy needs to make a
/// labelled-set-aware edge decision: the candidate + cardinal
/// neighbour indices, their `(i, j)` cells, the full labelled map,
/// the corner position array, and the scalar fallback cell size.
#[non_exhaustive]
#[derive(Clone, Copy)]
pub struct FillEdgeCtx<'a> {
    /// Index of the candidate corner being evaluated.
    pub candidate_idx: usize,
    /// Index of the already-labelled cardinal neighbour.
    pub neighbour_idx: usize,
    /// The candidate's prospective `(i, j)` cell.
    pub at_candidate: (i32, i32),
    /// The cardinal neighbour's `(i, j)` cell.
    pub at_neighbour: (i32, i32),
    /// The full `(i, j) → corner_idx` labelled map at this point in the grow.
    pub labelled: &'a HashMap<(i32, i32), usize>,
    /// Corner positions in image pixels, indexed by the values of `labelled`.
    pub positions: &'a [Point2<f32>],
    /// Scalar fallback cell size in pixels, used when no local estimate exists.
    pub cell_size: f32,
}

/// Tolerances for the square-lattice growth helpers.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct GrowParams {
    /// Candidate-search radius (fraction of `cell_size`) around each
    /// prediction. Applies when the target is being **interpolated**
    /// between labelled neighbours on opposite sides.
    pub attach_search_rel: f32,
    /// Ambiguity factor: if the second-nearest candidate is within
    /// `factor × nearest_distance`, the attachment is skipped.
    pub attach_ambiguity_factor: f32,
    /// Multiplier on `attach_search_rel` when the target is being
    /// **extrapolated** outward from the labelled set (every labelled
    /// neighbour sits on the same side of the target along at least one
    /// axis). Defaults to 2.0 — opens the search up enough to absorb
    /// the perspective-foreshortening overshoot at the image edge while
    /// still rejecting off-lattice candidates that sit several cell-
    /// widths away.
    pub boundary_search_factor: f32,
}

impl Default for GrowParams {
    fn default() -> Self {
        Self {
            attach_search_rel: 0.35,
            attach_ambiguity_factor: 1.5,
            boundary_search_factor: 2.0,
        }
    }
}

impl GrowParams {
    /// Construct grow parameters from the interpolation search radius and
    /// ambiguity factor; `boundary_search_factor` keeps its default.
    pub fn new(attach_search_rel: f32, attach_ambiguity_factor: f32) -> Self {
        Self {
            attach_search_rel,
            attach_ambiguity_factor,
            ..Self::default()
        }
    }
}

/// Outcome of a grow pass.
#[derive(Debug, Default)]
pub struct GrowResult {
    /// `(i, j) → corner_index` map of accepted labels. Rebased so the
    /// bounding-box minimum is `(0, 0)`.
    pub labelled: HashMap<(i32, i32), usize>,
    /// Inverse map.
    pub by_corner: HashMap<usize, (i32, i32)>,
    /// Positions with ≥ 2 candidates inside the ambiguity window.
    pub ambiguous: HashSet<(i32, i32)>,
    /// Positions with no accepted candidate.
    pub holes: HashSet<(i32, i32)>,
    /// Grid `i`-axis vector (pixels per cell) carried forward — overlays
    /// and boosters use it.
    pub axis_i: Vector2<f32>,
    /// Grid `j`-axis vector (pixels per cell) carried forward — overlays
    /// and boosters use it.
    pub axis_j: Vector2<f32>,
    /// Mod-2 `i` component of the coordinate shift removed by the
    /// final rebase.
    ///
    /// `bfs_grow` walks in seed-local coordinates, then subtracts the
    /// labelled bounding-box minimum so output coordinates start at
    /// `(0, 0)`. Callers with an alternating label rule can add these
    /// mod-2 components back when evaluating labels in post-rebase
    /// coordinates. Callers without an alternating rule can ignore
    /// these fields.
    pub rebase_i_mod2: i32,
    /// See [`Self::rebase_i_mod2`].
    pub rebase_j_mod2: i32,
}
