//! Recall boosters that run after the precision core converges.
//!
//! Run **after** the precision core (seed + grow + validate) has
//! converged with no new blacklist entries. Boosters ADD labelled
//! corners without compromising the precision contract — they reuse
//! the same attachment invariants as growth.
//!
//! The structural skeleton (cell enumeration, KD-tree, per-cell
//! attachment ladder, fixed-point iteration) lives in
//! [`projective_grid::seed_and_grow::fill::fill_grid_holes`]; this module
//! wraps it with a chessboard-specific [`SquareAttachPolicy`] that adds:
//!
//! - **Weak-cluster rescue**: admit `NoCluster` corners whose
//!   `max_d_deg` is within `weak_cluster_tol_deg`. These corners
//!   failed the strict cluster-admission gate by a hair — the booster
//!   pass re-assigns them a label at attachment time.
//! - **Directional edge scale (optional)**: replace the scalar
//!   `cell_size` in the edge-length check with a per-axis median over
//!   already-labelled cardinal edges. Used by the topological path,
//!   whose visible component can be strongly anisotropic before final
//!   recovery has filled the boundary.

use crate::cluster::{angular_dist_pi, wrap_pi, ClusterCenters};
use crate::corner::{ClusterLabel, CornerAug, CornerStage};
use crate::grow::GrowResult;
use crate::params::DetectorParams;
use calib_targets_core::AxisEstimate;
use nalgebra::Point2;
use projective_grid::seed_and_grow::fill::{fill_grid_holes, FillParams};
use projective_grid::seed_and_grow::grow::{
    Admit, FillEdgeCtx, LabelledNeighbour, SquareAttachPolicy,
};
use std::collections::{HashMap, HashSet};

/// Diagnostic returned by the recall-booster stage.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct BoosterResult {
    /// Corners added to the labelled set across all booster passes.
    pub added: usize,
    /// Positions tried and not attached (interior holes that
    /// couldn't find a passing candidate; or line extensions that
    /// failed).
    pub holes_untouched: usize,
}

/// Extend the labelled set via interior gap fill + line extrapolation.
/// Mutates `grow.labelled` and `corners[*].stage`.
///
/// Used by the topological recovery path, whose visible components can be
/// strongly anisotropic before final recovery has filled the boundary — the
/// edge-length check uses a per-axis directional scale rather than a single
/// scalar cell size.
///
/// `blacklist` — corner indices to keep excluded from candidate searches.
pub(crate) fn apply_boosters_with_directional_edge_scale(
    corners: &mut [CornerAug],
    grow: &mut GrowResult,
    centers: ClusterCenters,
    cell_size: f32,
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
) -> BoosterResult {
    apply_boosters_impl(corners, grow, centers, cell_size, blacklist, params, true)
}

fn apply_boosters_impl(
    corners: &mut [CornerAug],
    grow: &mut GrowResult,
    centers: ClusterCenters,
    cell_size: f32,
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
    use_directional_edge_scale: bool,
) -> BoosterResult {
    let positions: Vec<Point2<f32>> = corners.iter().map(|c| c.position).collect();
    let parity_shift = (grow.rebase_i_mod2 + grow.rebase_j_mod2).rem_euclid(2);
    let tuning = params.effective_tuning();
    let validator = ChessboardFillValidator {
        corners,
        blacklist,
        centers,
        cell_size,
        attach_tol_rad: tuning.attach_axis_tol_deg.to_radians(),
        edge_tol_rad: tuning.edge_axis_tol_deg.to_radians(),
        weak_attach_tol_rad: tuning.weak_cluster_tol_deg.to_radians(),
        weak_cluster_tol_deg: tuning.weak_cluster_tol_deg,
        step_tol: tuning.step_tol,
        enable_weak_cluster_rescue: tuning.enable_weak_cluster_rescue,
        use_directional_edge_scale,
        parity_shift,
    };

    let fill_params = FillParams::new(
        tuning.attach_search_rel,
        tuning.attach_ambiguity_factor,
        tuning.max_booster_iters.max(1) as usize,
    );

    let stats = fill_grid_holes(&positions, grow, cell_size, &fill_params, &validator);

    // Promote each attached corner to `Labeled` so downstream stages
    // (validate, geometry-check, detection output) see the correct
    // stage marker. The fill pass cannot do this directly because it
    // doesn't know about `CornerStage`.
    for (k, &idx) in stats.attached_indices.iter().enumerate() {
        let at = stats.attached_cells[k];
        corners[idx].stage = CornerStage::Labeled {
            at,
            local_h_residual_px: None,
        };
    }

    BoosterResult {
        added: stats.added,
        holes_untouched: 0,
    }
}

/// Chessboard's plug-in for the fill pass.
///
/// Holds immutable references into the caller's corner array + the
/// clustering output + per-call tolerances, plus two booster-specific
/// switches (weak-cluster rescue, directional edge scale).
struct ChessboardFillValidator<'a> {
    corners: &'a [CornerAug],
    blacklist: &'a HashSet<usize>,
    centers: ClusterCenters,
    cell_size: f32,
    attach_tol_rad: f32,
    edge_tol_rad: f32,
    weak_attach_tol_rad: f32,
    weak_cluster_tol_deg: f32,
    step_tol: f32,
    enable_weak_cluster_rescue: bool,
    use_directional_edge_scale: bool,
    /// `(rebase_i_mod2 + rebase_j_mod2) % 2` from the BFS rebase.
    /// See `GrowResult::rebase_i_mod2` for the full discussion.
    parity_shift: i32,
}

impl<'a> SquareAttachPolicy for ChessboardFillValidator<'a> {
    fn is_eligible(&self, idx: usize) -> bool {
        if self.blacklist.contains(&idx) {
            return false;
        }
        matches!(self.corners[idx].stage, CornerStage::Clustered { .. })
    }

    /// Widened eligibility for the booster: admit `NoCluster` corners
    /// within `weak_cluster_tol_deg` when weak-cluster rescue is on.
    fn eligible_for_fill(&self, idx: usize) -> bool {
        if self.blacklist.contains(&idx) {
            return false;
        }
        let c = &self.corners[idx];
        if matches!(c.stage, CornerStage::Clustered { .. }) {
            return true;
        }
        if self.enable_weak_cluster_rescue {
            if let CornerStage::NoCluster { max_d_deg } = c.stage {
                return max_d_deg <= self.weak_cluster_tol_deg;
            }
        }
        false
    }

    fn required_label_at(&self, i: i32, j: i32) -> Option<u8> {
        // Apply the post-rebase parity shift so the chessboard parity
        // at `(i, j)` matches the BFS pre-rebase convention. See
        // `GrowResult::rebase_i_mod2` for the full discussion.
        Some(label_to_u8(required_label_at(i + self.parity_shift, j)))
    }

    fn label_of(&self, idx: usize) -> Option<u8> {
        let c = &self.corners[idx];
        match c.stage {
            CornerStage::Clustered { label } => Some(label_to_u8(label)),
            CornerStage::NoCluster { .. } => {
                infer_label_from_axes(&c.axes, self.centers, self.weak_attach_tol_rad)
                    .map(label_to_u8)
            }
            _ => None,
        }
    }

    fn accept_candidate(
        &self,
        idx: usize,
        _at: (i32, i32),
        _prediction: Point2<f32>,
        _neighbours: &[LabelledNeighbour],
    ) -> Admit {
        let c = &self.corners[idx];
        // Use the wider rescue tolerance when accepting NoCluster
        // corners; otherwise use the standard attach tolerance.
        let tol = match c.stage {
            CornerStage::NoCluster { .. } => self.weak_attach_tol_rad,
            _ => self.attach_tol_rad,
        };
        if axes_match_centers(&c.axes, self.centers, tol) {
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
        edge_ok_with_metric(
            self.corners,
            candidate_idx,
            neighbour_idx,
            self.cell_size,
            self.step_tol,
            self.edge_tol_rad,
        )
    }

    fn fill_edge_ok(&self, ctx: FillEdgeCtx<'_>) -> bool {
        let expected_len = if self.use_directional_edge_scale {
            expected_cardinal_edge_len(
                ctx.at_candidate,
                ctx.at_neighbour,
                ctx.labelled,
                ctx.positions,
                ctx.cell_size,
            )
        } else {
            ctx.cell_size
        };
        edge_ok_with_metric(
            self.corners,
            ctx.candidate_idx,
            ctx.neighbour_idx,
            expected_len,
            self.step_tol,
            self.edge_tol_rad,
        )
    }
}

/// Required parity-derived chessboard label at `(i, j)` under the seed
/// convention (seed `A` at `(0, 0)` is `Canonical`).
fn required_label_at(i: i32, j: i32) -> ClusterLabel {
    if (i + j).rem_euclid(2) == 0 {
        ClusterLabel::Canonical
    } else {
        ClusterLabel::Swapped
    }
}

fn label_to_u8(label: ClusterLabel) -> u8 {
    match label {
        ClusterLabel::Canonical => 0,
        ClusterLabel::Swapped => 1,
    }
}

/// Infer cluster label for a weakly-clustered corner: pick the
/// assignment (canonical vs swapped) whose worst per-axis
/// distance is smaller; require it to be within `tol`.
fn infer_label_from_axes(
    axes: &[AxisEstimate; 2],
    centers: ClusterCenters,
    tol: f32,
) -> Option<ClusterLabel> {
    let a0 = wrap_pi(axes[0].angle);
    let a1 = wrap_pi(axes[1].angle);
    let canon_max = angular_dist_pi(a0, centers.theta0).max(angular_dist_pi(a1, centers.theta1));
    let swap_max = angular_dist_pi(a0, centers.theta1).max(angular_dist_pi(a1, centers.theta0));
    if canon_max <= swap_max {
        if canon_max <= tol {
            Some(ClusterLabel::Canonical)
        } else {
            None
        }
    } else if swap_max <= tol {
        Some(ClusterLabel::Swapped)
    } else {
        None
    }
}

fn axes_match_centers(axes: &[AxisEstimate; 2], centers: ClusterCenters, tol: f32) -> bool {
    let a0 = wrap_pi(axes[0].angle);
    let a1 = wrap_pi(axes[1].angle);
    let canon_max = angular_dist_pi(a0, centers.theta0).max(angular_dist_pi(a1, centers.theta1));
    let swap_max = angular_dist_pi(a0, centers.theta1).max(angular_dist_pi(a1, centers.theta0));
    canon_max.min(swap_max) <= tol
}

fn edge_ok_with_metric(
    corners: &[CornerAug],
    c_idx: usize,
    n_idx: usize,
    expected_len: f32,
    step_tol: f32,
    edge_tol_rad: f32,
) -> bool {
    let c = &corners[c_idx];
    let n = &corners[n_idx];
    let off = n.position - c.position;
    let dist = off.norm();
    let min_len = (1.0 - step_tol) * expected_len;
    let max_len = (1.0 + step_tol) * expected_len;
    if dist < min_len || dist > max_len {
        return false;
    }
    let ang = wrap_pi(off.y.atan2(off.x));
    let d_c0 = angular_dist_pi(ang, wrap_pi(c.axes[0].angle));
    let d_c1 = angular_dist_pi(ang, wrap_pi(c.axes[1].angle));
    let (slot_c, d_c) = if d_c0 <= d_c1 { (0, d_c0) } else { (1, d_c1) };
    if d_c > edge_tol_rad {
        return false;
    }
    let d_n0 = angular_dist_pi(ang, wrap_pi(n.axes[0].angle));
    let d_n1 = angular_dist_pi(ang, wrap_pi(n.axes[1].angle));
    let (slot_n, d_n) = if d_n0 <= d_n1 { (0, d_n0) } else { (1, d_n1) };
    if d_n > edge_tol_rad {
        return false;
    }
    slot_c != slot_n
}

fn corner_distance(positions: &[Point2<f32>], a: usize, b: usize) -> f32 {
    (positions[b] - positions[a]).norm()
}

fn median_len(values: &mut [f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    Some(values[values.len() / 2])
}

/// Expected cardinal-edge length between `pos` and `neigh`, used by
/// the directional-edge-scale variant.
///
/// Prefers the next already-labelled edge along the same local line
/// (handles perspective / optical anisotropy at the boundary). Falls
/// back to the component's directional median.
fn expected_cardinal_edge_len(
    pos: (i32, i32),
    neigh: (i32, i32),
    labelled: &HashMap<(i32, i32), usize>,
    positions: &[Point2<f32>],
    fallback_cell_size: f32,
) -> f32 {
    let step = (neigh.0 - pos.0, neigh.1 - pos.1);
    debug_assert!(matches!(step, (1, 0) | (-1, 0) | (0, 1) | (0, -1)));

    let far = (neigh.0 + step.0, neigh.1 + step.1);
    if let (Some(&n_idx), Some(&far_idx)) = (labelled.get(&neigh), labelled.get(&far)) {
        return corner_distance(positions, n_idx, far_idx);
    }

    let axis = if step.0 != 0 { (1, 0) } else { (0, 1) };
    let mut lengths = Vec::new();
    for (&ij, &idx) in labelled {
        let next = (ij.0 + axis.0, ij.1 + axis.1);
        if let Some(&next_idx) = labelled.get(&next) {
            lengths.push(corner_distance(positions, idx, next_idx));
        }
    }
    median_len(&mut lengths).unwrap_or(fallback_cell_size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::cluster_axes;
    use crate::corner::ChessCorner;
    use calib_targets_core::AxisEstimate;
    use nalgebra::Point2;

    fn make_corner(
        idx: usize,
        x: f32,
        y: f32,
        axis_u: f32,
        axis_v: f32,
        label: ClusterLabel,
    ) -> CornerAug {
        let axes = match label {
            ClusterLabel::Canonical => [axis_u, axis_v],
            ClusterLabel::Swapped => [axis_v, axis_u],
        };
        let c = ChessCorner {
            position: Point2::new(x, y),
            axes: [
                AxisEstimate {
                    angle: wrap_pi(axes[0]),
                    sigma: 0.01,
                },
                AxisEstimate {
                    angle: wrap_pi(axes[1]),
                    sigma: 0.01,
                },
            ],
            contrast: 10.0,
            fit_rms: 1.0,
            strength: 1.0,
        };
        let mut aug = CornerAug::from_chess_corner(idx, &c);
        aug.stage = CornerStage::Strong;
        aug
    }

    #[test]
    fn line_extrapolation_uses_directional_edge_scale() {
        // Topological recovery can start from an anisotropic component: the
        // horizontal pitch is much larger than the vertical pitch. The booster
        // edge invariant must compare candidate vertical edges against the
        // local vertical step, not against the scalar recovery cell size.
        let axis_u = 0.0_f32;
        let axis_v = std::f32::consts::FRAC_PI_2;
        let mut corners = Vec::new();
        let cols = 2usize;
        let rows = 5usize;
        for j in 0..rows {
            for i in 0..cols {
                let label = if (i as i32 + j as i32).rem_euclid(2) == 0 {
                    ClusterLabel::Canonical
                } else {
                    ClusterLabel::Swapped
                };
                corners.push(make_corner(
                    j * cols + i,
                    100.0 + i as f32 * 60.0,
                    100.0 + j as f32 * 32.0,
                    axis_u,
                    axis_v,
                    label,
                ));
            }
        }

        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let mut grow = GrowResult {
            labelled: Default::default(),
            by_corner: Default::default(),
            ambiguous: Default::default(),
            holes: Default::default(),
            axis_i: nalgebra::Vector2::new(1.0, 0.0),
            axis_j: nalgebra::Vector2::new(0.0, 1.0),
            rebase_i_mod2: 0,
            rebase_j_mod2: 0,
        };

        for j in 1..=3 {
            for i in 0..cols {
                let idx = j * cols + i;
                let at = (i as i32, j as i32);
                grow.labelled.insert(at, idx);
                grow.by_corner.insert(idx, at);
                corners[idx].stage = CornerStage::Labeled {
                    at,
                    local_h_residual_px: None,
                };
            }
        }

        let blacklist = HashSet::new();
        let result = apply_boosters_with_directional_edge_scale(
            &mut corners,
            &mut grow,
            centers,
            60.0,
            &blacklist,
            &params,
        );

        assert!(result.added >= 4, "expected both extrapolated rows");
        for i in 0..cols {
            assert!(grow.labelled.contains_key(&(i as i32, 0)));
            assert!(grow.labelled.contains_key(&(i as i32, 4)));
        }
    }
}
