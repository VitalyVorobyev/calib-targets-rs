//! Geometry-only [`SquareAttachPolicy`] for the orientation-free
//! (`Evidence::Positions`) and single-axis (`Evidence::Oriented1`) paths.
//!
//! A *hard* axis voucher (a candidate is accepted only if its two synthesized
//! axes align, within ~25°, with a labelled neighbour's) works when the caller
//! supplied real per-corner orientation (chess-corners DiskFit axes), but on
//! the orientation-free path the axes are *synthesized* from neighbour geometry
//! ([`crate::orient`]) and are systematically less reliable near the grid
//! boundary and under heavy foreshortening — exactly where recall matters. A
//! hard axis voucher there stalls the growth frontier and is the binding
//! constraint behind the position path's recall gap.
//!
//! [`PositionsAttachPolicy`] inverts the trust order: the **geometry** is the
//! gate and the synthesized axes are a *soft* cue with a wide tolerance.
//!
//! - **eligibility** — every corner (no feature classes).
//! - **parity hooks** (`required_label_at` / `label_of`) — `None` (no parity
//!   on a position-only grid).
//! - **`accept_candidate`** — geometry only. The grow engine has already
//!   restricted the candidate to a search radius around the local-prediction
//!   point, so the candidate is geometrically plausible; this gate adds the
//!   *soft* axis cue: if the candidate's axes align with a labelled neighbour's
//!   within the wide [`Self`]`::soft_axis_tol_rad`, accept outright; otherwise
//!   still accept (the prediction residual + the per-edge band + downstream
//!   revalidation carry the precision), so a corner with a noisy synthesized
//!   axis becomes a *missing* corner only if the geometry also fails — never a
//!   mislabel the gates would not catch.
//! - **`edge_ok`** — per-edge length band against the *local* pitch (tracks
//!   perspective foreshortening).
//!
//! # Precision contract
//!
//! The accept gate is *wide*, so the precision burden shifts onto the per-edge
//! band, the search-radius prediction gate, and the post-convergence
//! revalidation + drop filters ([`crate::shared::validate`] +
//! [`crate::shared::validate::recovery`]). The recovery schedule that wraps
//! this policy runs the line-collinearity + local-H + topological wrong-label +
//! largest-component filters on every sweep, so a geometrically-incoherent
//! attach is dropped, not mislabelled.
//!
//! What does NOT belong here: any parity / axis-cluster vocabulary, or the
//! recovery control flow ([`crate::shared::recovery`]).
//!
//! **Tier:** advanced engine — semver-exempt pre-1.0.

use nalgebra::Point2;

use crate::cluster::angular_dist_pi;
use crate::feature::{LocalAxis, OrientedFeature};
use crate::shared::grow::{Admit, LabelledNeighbour, SquareAttachPolicy};

/// Tolerances for [`PositionsAttachPolicy`].
#[derive(Clone, Copy, Debug)]
pub(super) struct PositionsTolerances {
    /// Wide axis-alignment tolerance (radians) for the *soft* synthesized-axis
    /// cue. Synthesized axes are noisier than caller-supplied ones, so this is
    /// deliberately wide (the facade passes 50°, double a strict hard-voucher
    /// 25°).
    pub soft_axis_tol_rad: f32,
    /// Per-edge length tolerance (fraction of the local pitch).
    pub edge_length_tol: f32,
    /// Scalar fallback cell size in pixels (seed-derived); used only where a
    /// corner has no usable local-pitch estimate.
    pub cell_size: f32,
}

/// Geometry-first facade policy over oriented-2 features whose axes were
/// synthesized from positions.
pub(super) struct PositionsAttachPolicy<'a> {
    features: &'a [OrientedFeature<2>],
    positions: &'a [Point2<f32>],
    /// Per-corner local pitch in pixels (robust nearest-neighbour distance).
    local_pitch: &'a [f32],
    tol: PositionsTolerances,
}

impl<'a> PositionsAttachPolicy<'a> {
    pub(super) fn new(
        features: &'a [OrientedFeature<2>],
        positions: &'a [Point2<f32>],
        local_pitch: &'a [f32],
        tol: PositionsTolerances,
    ) -> Self {
        Self {
            features,
            positions,
            local_pitch,
            tol,
        }
    }

    fn axes(&self, idx: usize) -> [LocalAxis; 2] {
        self.features[idx].axes
    }
}

impl SquareAttachPolicy for PositionsAttachPolicy<'_> {
    fn is_eligible(&self, _idx: usize) -> bool {
        true
    }

    fn required_label_at(&self, _i: i32, _j: i32) -> Option<u8> {
        None
    }

    fn label_of(&self, _idx: usize) -> Option<u8> {
        None
    }

    fn accept_candidate(
        &self,
        idx: usize,
        _at: (i32, i32),
        _prediction: Point2<f32>,
        neighbours: &[LabelledNeighbour],
    ) -> Admit {
        // Geometry-first: the grow engine already restricted this candidate to
        // a search radius around the local prediction, so it is positionally
        // plausible. Accept it; the synthesized-axis cue is *soft* — it only
        // ever *widens* acceptance (it never hard-rejects), because a noisy
        // boundary axis must not collapse recall. Precision is carried by the
        // per-edge band (`edge_ok`), the prediction-residual search gate, and
        // the post-convergence revalidation + drop filters.
        //
        // The soft cue is still computed so a future tightening can flip it to
        // a vote; today both branches accept, mirroring the documented contract
        // (a wrong synthesized axis yields a *missing* corner, not a mislabel).
        let cand = self.axes(idx);
        let tol = self.tol.soft_axis_tol_rad;
        let _aligned = neighbours
            .iter()
            .any(|n| axis_pairs_align(cand, self.features[n.idx].axes, tol));
        Admit::Accept
    }

    fn edge_ok(
        &self,
        candidate_idx: usize,
        neighbour_idx: usize,
        _at_candidate: (i32, i32),
        _at_neighbour: (i32, i32),
    ) -> bool {
        let to = self.positions[candidate_idx];
        let from = self.positions[neighbour_idx];
        let len = (to - from).norm();
        let local = 0.5 * (self.local_pitch[candidate_idx] + self.local_pitch[neighbour_idx]);
        let expected = if local > 1e-3 {
            local
        } else {
            self.tol.cell_size
        };
        let ratio = len / expected;
        let low = 1.0 - self.tol.edge_length_tol;
        let high = 1.0 + self.tol.edge_length_tol;
        ratio >= low && ratio <= high
    }
}

/// True iff the candidate's two axes align (undirected, mod-π, within `tol`)
/// with the two axes of a labelled neighbour, under either slot assignment.
fn axis_pairs_align(a: [LocalAxis; 2], b: [LocalAxis; 2], tol: f32) -> bool {
    let d = |x: f32, y: f32| angular_dist_pi(x, y);
    let direct =
        d(a[0].angle_rad, b[0].angle_rad) <= tol && d(a[1].angle_rad, b[1].angle_rad) <= tol;
    let swapped =
        d(a[0].angle_rad, b[1].angle_rad) <= tol && d(a[1].angle_rad, b[0].angle_rad) <= tol;
    direct || swapped
}
