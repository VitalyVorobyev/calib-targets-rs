//! Built-in [`SquareSeedPolicy`] + [`SquareAttachPolicy`] over
//! `&[OrientedFeature<2>]`.
//!
//! The facade has no parity / feature-class labels (unlike the chessboard
//! detector's `ChessAttachPolicy`), so this policy treats every corner as
//! eligible, imposes no required-label constraint, and reads each corner's
//! two local axes directly. It ports the seed-and-grow invariants that the
//! historical generic-`F` `square::grow` engine enforced inline:
//!
//! - **`accept_candidate`** — the candidate's two axes each align (within
//!   `axis_align_tol_rad`, undirected mod-π) with one of the grid axes,
//!   and the two candidate axes pick *distinct* grid axes (the axis-slot-
//!   swap parity check).
//! - **`edge_ok`** — the induced edge length is within
//!   `[1 - edge_length_tol, 1 + edge_length_tol] × cell_size`.
//!
//! The grid axes are derived once from the seed quad's `B-A` / `C-A`
//! chords (or supplied via `GrowParams::global_axis_u_v`), mirroring the
//! historical engine. Because the facade seed-and-grow path is exercised
//! only by synthetic tests (the dataset-gated chessboard path composes the
//! advanced engine directly with its own policy), this policy's job is to
//! reproduce the historical engine's accept/reject behaviour on those
//! tests, not to be tuned against real images.

use nalgebra::{Point2, Vector2};

use crate::detect::advanced::square::grow::{Admit, LabelledNeighbour, SquareAttachPolicy};
use crate::detect::advanced::square::seed::finder::SquareSeedPolicy;
use crate::feature::{LocalAxis, OrientedFeature};

/// Tolerances the policy enforces; ported from the historical
/// `square::grow::GrowParams` axis-alignment + edge-length knobs.
#[derive(Clone, Copy, Debug)]
pub(super) struct Oriented2Tolerances {
    /// Per-candidate axis-alignment tolerance in radians.
    pub axis_align_tol_rad: f32,
    /// Per-edge length tolerance (fraction of `cell_size`).
    pub edge_length_tol: f32,
    /// Scalar cell size in pixels (seed-derived).
    pub cell_size: f32,
    /// Grid `u` direction (unit vector) and `v` direction (unit vector).
    pub axis_u: Vector2<f32>,
    pub axis_v: Vector2<f32>,
}

/// Built-in facade policy over oriented-2 features.
pub(super) struct Oriented2Policy<'a> {
    features: &'a [OrientedFeature<2>],
    positions: &'a [Point2<f32>],
    tol: Oriented2Tolerances,
}

impl<'a> Oriented2Policy<'a> {
    pub(super) fn new(
        features: &'a [OrientedFeature<2>],
        positions: &'a [Point2<f32>],
        tol: Oriented2Tolerances,
    ) -> Self {
        Self {
            features,
            positions,
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
        _neighbours: &[LabelledNeighbour],
    ) -> Admit {
        if candidate_axes_align(
            self.axes(idx),
            self.tol.axis_u,
            self.tol.axis_v,
            self.tol.axis_align_tol_rad,
        ) {
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
        let ratio = len / self.tol.cell_size;
        let low = 1.0 - self.tol.edge_length_tol;
        let high = 1.0 + self.tol.edge_length_tol;
        ratio >= low && ratio <= high
    }
}

/// Each candidate axis must align with one of the grid axes (within
/// `tol`, undirected mod-π) and the two candidate axes must pick distinct
/// grid axes. Mirrors the historical `square::grow::candidate_axes_align`.
fn candidate_axes_align(
    axes: [LocalAxis; 2],
    axis_u: Vector2<f32>,
    axis_v: Vector2<f32>,
    tol: f32,
) -> bool {
    let alpha = wrap_pi(axes[0].angle_rad);
    let beta = wrap_pi(axes[1].angle_rad);
    let theta_u = wrap_pi(axis_u.y.atan2(axis_u.x));
    let theta_v = wrap_pi(axis_v.y.atan2(axis_v.x));

    let (alpha_u, alpha_v) = (
        angular_dist_pi(alpha, theta_u),
        angular_dist_pi(alpha, theta_v),
    );
    let (beta_u, beta_v) = (
        angular_dist_pi(beta, theta_u),
        angular_dist_pi(beta, theta_v),
    );

    let alpha_pick = if alpha_u <= tol && alpha_u <= alpha_v {
        Some(0)
    } else if alpha_v <= tol {
        Some(1)
    } else {
        None
    };
    let beta_pick = if beta_u <= tol && beta_u <= beta_v {
        Some(0)
    } else if beta_v <= tol {
        Some(1)
    } else {
        None
    };
    match (alpha_pick, beta_pick) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    }
}

/// Derive the seed quad's two grid-axis unit vectors from the `B-A` and
/// `C-A` chords, matching the historical engine's `derive_seed_axes`.
pub(super) fn derive_seed_axes(
    positions: &[Point2<f32>],
    a: usize,
    b: usize,
    c: usize,
) -> (Vector2<f32>, Vector2<f32>) {
    let eps = 1e-6_f32;
    let pa = positions[a];
    let pb = positions[b];
    let pc = positions[c];
    let raw_u = pb - pa;
    let raw_v = pc - pa;
    let nu = raw_u.norm().max(eps);
    let nv = raw_v.norm().max(eps);
    (raw_u / nu, raw_v / nv)
}

#[inline]
fn wrap_pi(theta: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let mut t = theta % pi;
    if t < 0.0 {
        t += pi;
    }
    if t >= pi {
        t -= pi;
    }
    t
}

#[inline]
fn angular_dist_pi(a: f32, b: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let mut diff = ((a - b) % pi + pi) % pi;
    let comp = pi - diff;
    if comp < diff {
        diff = comp;
    }
    diff
}
