//! Built-in [`SquareSeedPolicy`] + [`SquareAttachPolicy`] over
//! `&[OrientedFeature<2>]`.
//!
//! The facade has no parity / feature-class labels (unlike the chessboard
//! detector's `ChessAttachPolicy`), so this policy treats every corner as
//! eligible, imposes no required-label constraint, and reads each corner's
//! two local axes directly. It ports the seed-and-grow invariants that the
//! historical generic-`F` `square::grow` engine enforced inline:
//!
//! - **`accept_candidate`** — the candidate's two axes align (within
//!   `axis_align_tol_rad`, undirected mod-π) with the two axes of at least
//!   one already-labelled neighbour. Comparing against a **local** neighbour
//!   rather than a frozen global seed axis is what lets the facade track a
//!   perspective warp: under perspective the two grid directions are not
//!   orthogonal and rotate across the image, so a candidate far from the seed
//!   no longer matches the seed's axes — but it still matches its immediate
//!   neighbour's. A spurious corner with unrelated local axes finds no
//!   voucher and is rejected, preserving the precision contract.
//! - **`edge_ok`** — the induced edge length is within
//!   `[1 - edge_length_tol, 1 + edge_length_tol] × cell_size`.
//!
//! Because the facade seed-and-grow path is exercised only by synthetic tests
//! (the dataset-gated chessboard path composes the advanced engine directly
//! with its own policy), this policy's job is to reproduce sound accept/reject
//! behaviour on those tests and on the orientation-free position path, not to
//! be tuned against a specific real dataset.

use nalgebra::Point2;

use crate::feature::{LocalAxis, OrientedFeature};
use crate::seed_and_grow::angle::angular_dist_pi;
use crate::seed_and_grow::grow::{Admit, LabelledNeighbour, SquareAttachPolicy};
use crate::seed_and_grow::seed::finder::SquareSeedPolicy;

/// Tolerances the policy enforces: a per-candidate axis-alignment tolerance
/// (used to vouch a candidate against a labelled neighbour's axes) plus the
/// per-edge length band.
#[derive(Clone, Copy, Debug)]
pub(super) struct Oriented2Tolerances {
    /// Per-candidate axis-alignment tolerance in radians.
    pub axis_align_tol_rad: f32,
    /// Per-edge length tolerance (fraction of the local pitch).
    pub edge_length_tol: f32,
    /// Scalar fallback cell size in pixels (seed-derived); used only where a
    /// corner has no usable local-pitch estimate.
    pub cell_size: f32,
}

/// Built-in facade policy over oriented-2 features.
pub(super) struct Oriented2Policy<'a> {
    features: &'a [OrientedFeature<2>],
    positions: &'a [Point2<f32>],
    /// Per-corner local pitch in pixels (nearest-neighbour distance). Tracks
    /// perspective foreshortening so the per-edge length check stays local
    /// rather than gating against one global seed-derived scalar.
    local_pitch: &'a [f32],
    tol: Oriented2Tolerances,
}

impl<'a> Oriented2Policy<'a> {
    pub(super) fn new(
        features: &'a [OrientedFeature<2>],
        positions: &'a [Point2<f32>],
        local_pitch: &'a [f32],
        tol: Oriented2Tolerances,
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

impl SquareSeedPolicy for Oriented2Policy<'_> {
    fn position(&self, idx: usize) -> Point2<f32> {
        self.positions[idx]
    }

    fn axes(&self, idx: usize) -> [LocalAxis; 2] {
        self.features[idx].axes
    }

    fn primary_candidates(&self) -> Vec<usize> {
        // No feature classes: every corner is eligible to be a diagonal
        // corner. The finder's KD-tree + axis classification does the
        // geometric work.
        (0..self.features.len()).collect()
    }

    fn secondary_candidates(&self) -> Vec<usize> {
        (0..self.features.len()).collect()
    }
}

impl SquareAttachPolicy for Oriented2Policy<'_> {
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
        let cand = self.axes(idx);
        let tol = self.tol.axis_align_tol_rad;
        // Vouch the candidate against the LOCAL axes of an already-labelled
        // neighbour. This tracks a perspective warp (the grid directions
        // rotate across the image) where a frozen global seed axis would
        // reject everything far from the seed.
        for n in neighbours {
            if axis_pairs_align(cand, self.features[n.idx].axes, tol) {
                return Admit::Accept;
            }
        }
        // During normal grow there is always at least one labelled neighbour;
        // if somehow none, defer precision to the geometry + validate + fit
        // gates rather than block the attachment.
        if neighbours.is_empty() {
            Admit::Accept
        } else {
            Admit::Reject
        }
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
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let len = (dx * dx + dy * dy).sqrt();
        // Local expected pitch: the mean of the two endpoints' nearest-
        // neighbour distances. Under perspective the pitch foreshortens across
        // the image, so a *local* expectation keeps the band valid far from the
        // seed where a single global `cell_size` would reject legitimate edges.
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
/// The two grid directions need not be orthogonal (perspective), so this only
/// checks that each candidate axis matches a *distinct* neighbour axis.
fn axis_pairs_align(a: [LocalAxis; 2], b: [LocalAxis; 2], tol: f32) -> bool {
    let d = |x: f32, y: f32| angular_dist_pi(x, y);
    let direct =
        d(a[0].angle_rad, b[0].angle_rad) <= tol && d(a[1].angle_rad, b[1].angle_rad) <= tol;
    let swapped =
        d(a[0].angle_rad, b[1].angle_rad) <= tol && d(a[1].angle_rad, b[0].angle_rad) <= tol;
    direct || swapped
}
