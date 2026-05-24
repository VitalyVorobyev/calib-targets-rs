//! [`SquareGrowContext<F>`] trait, [`EdgeCtx<F>`] payload, and the permissive
//! [`OpenContext<F>`] used for zero-config detection.
//!
//! Kept in a separate file from `engine.rs` so the BFS loop's source size
//! stays under the workspace's per-file budget (Phase 2 design goal). The
//! [`OpenContext`] type also implements
//! [`crate::seed::SeedQuadContext`] so the seed finder and the grow engine
//! share one zero-config object.

use nalgebra::Point2;

use crate::feature::AxisEstimate;
use crate::float::Float;
use crate::lattice::Coord;
use crate::policy::LabelPolicy;

/// Per-edge context passed to [`SquareGrowContext::edge_ok`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct EdgeCtx<F: Float> {
    /// The labelled neighbour's `(i, j)`.
    pub from_coord: Coord,
    /// The candidate's tentative `(i, j)`.
    pub to_coord: Coord,
    /// The labelled neighbour's position.
    pub from_position: Point2<F>,
    /// The candidate's position.
    pub to_position: Point2<F>,
    /// The labelled neighbour's observation index.
    pub from_idx: usize,
    /// The candidate's observation index.
    pub to_idx: usize,
    /// Scalar cell-size fallback (mean of the seed's edges).
    pub global_cell_size: F,
}

/// Pattern-aware hooks consulted by [`crate::grow::engine::bfs_grow`].
///
/// Default impls keep the BFS permissive:
///
/// * `axes_at` returns `None` (the engine still grows; it just cannot consult
///   axis-slot constraints).
/// * `edge_ok` accepts every edge.
/// * `accept_candidate` accepts every candidate.
///
/// Consumers tighten the gates by overriding. Eligibility and parity ride on
/// `ctx.label_policy()` and are checked at attach time by the engine itself.
pub trait SquareGrowContext<F: Float> {
    /// The active [`LabelPolicy`].
    fn label_policy(&self) -> &LabelPolicy<F>;

    /// Per-observation axes (or `None` when axis data is missing).
    #[allow(unused_variables)]
    fn axes_at(&self, idx: usize) -> Option<[AxisEstimate<F>; 2]> {
        None
    }

    /// Per-edge invariant. Called after the candidate has been selected but
    /// before it is committed.
    #[allow(unused_variables)]
    fn edge_ok(&self, edge: EdgeCtx<F>) -> bool {
        true
    }

    /// Per-candidate veto. Called after `edge_ok` passes on all labelled
    /// cardinal neighbours.
    #[allow(unused_variables)]
    fn accept_candidate(&self, coord: Coord, idx: usize) -> bool {
        true
    }
}

/// Zero-config context: only a [`LabelPolicy`] (permissive), nothing else.
///
/// Suitable for zero-config detection on synthetic grids — every observation
/// eligible, no parity rule, no per-edge or per-candidate veto. The same impl
/// is reused for [`crate::seed::SeedQuadContext`], so a single `OpenContext`
/// covers both the seed finder and the BFS engine.
#[derive(Debug, Clone)]
pub struct OpenContext<F: Float> {
    policy: LabelPolicy<F>,
}

impl<F: Float> OpenContext<F> {
    /// Construct a context covering `n_observations` features, all eligible,
    /// with no parity rule.
    pub fn new(n_observations: usize) -> Self {
        Self {
            policy: LabelPolicy::builder(n_observations).build(),
        }
    }

    /// Borrow the underlying policy.
    pub fn policy(&self) -> &LabelPolicy<F> {
        &self.policy
    }
}

impl<F: Float> SquareGrowContext<F> for OpenContext<F> {
    fn label_policy(&self) -> &LabelPolicy<F> {
        &self.policy
    }
}

impl<F: Float> crate::seed::SeedQuadContext<F> for OpenContext<F> {
    fn label_policy(&self) -> &LabelPolicy<F> {
        &self.policy
    }
    fn axes_at(&self, _idx: usize) -> Option<[AxisEstimate<F>; 2]> {
        // Synthetic clean grids need axis hints to seed; for OpenContext we
        // assume an axis-aligned lattice and supply `(0, π/2)` so the seed
        // finder's chord classifier can run. Pattern-aware consumers override
        // this with their own per-observation axes.
        Some([
            AxisEstimate::from_angle(F::zero()),
            AxisEstimate::from_angle(F::frac_pi_2()),
        ])
    }
}

impl<F: Float> crate::topological::TopologicalContext<F> for OpenContext<F> {
    fn label_policy(&self) -> &LabelPolicy<F> {
        &self.policy
    }
    fn axes_at(&self, _idx: usize) -> Option<[AxisEstimate<F>; 2]> {
        // Mirrors the `SeedQuadContext` default above: zero-config grids carry
        // no per-corner axes on their `Observation` slices, so the topological
        // pipeline gets an axis-aligned override for the same reason the seed
        // finder does. Pattern-aware consumers override this on their own
        // context type.
        Some([
            AxisEstimate::from_angle(F::zero()),
            AxisEstimate::from_angle(F::frac_pi_2()),
        ])
    }
}
