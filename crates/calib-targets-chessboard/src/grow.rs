//! Stage 6 — BFS-style growth over the labelled `(i, j)` set.
//!
//! The pattern-agnostic machinery (BFS queue, KD-tree, prediction,
//! ambiguity resolution, rebase-to-origin) lives in
//! [`projective_grid::square::grow`]. This module supplies the
//! chessboard-specific
//! [`GrowValidator`](projective_grid::square::grow::GrowValidator)
//! implementation — parity rules, axis-cluster matching, axis-slot-
//! swap edge invariant — and carries the pipeline's per-corner stage
//! updates.
//!
//! See the hoisted module for the algorithm description and
//! `book/src/chessboard.md` for the full pipeline context.

use crate::cluster::{angular_dist_pi, wrap_pi, ClusterCenters};
use crate::corner::{ClusterLabel, CornerAug, CornerStage};
use crate::params::DetectorParams;
use crate::seed::Seed;
use calib_targets_core::AxisEstimate;
use nalgebra::{Point2, Vector2};
use projective_grid::square::grow as pg_grow;
use std::collections::{HashMap, HashSet};

pub use pg_grow::{GrowParams, GrowResult as PgGrowResult};

/// Outcome of a grow pass.
///
/// Mirrors [`pg_grow::GrowResult`] but keeps the chessboard-local
/// field names overlays and boosters already depend on.
pub struct GrowResult {
    pub labelled: HashMap<(i32, i32), usize>,
    pub by_corner: HashMap<usize, (i32, i32)>,
    pub ambiguous: HashSet<(i32, i32)>,
    pub holes: HashSet<(i32, i32)>,
    pub grid_u: Vector2<f32>,
    pub grid_v: Vector2<f32>,
}

impl From<pg_grow::GrowResult> for GrowResult {
    fn from(r: pg_grow::GrowResult) -> Self {
        Self {
            labelled: r.labelled,
            by_corner: r.by_corner,
            ambiguous: r.ambiguous,
            holes: r.holes,
            grid_u: r.grid_u,
            grid_v: r.grid_v,
        }
    }
}

/// Grow from the seed. Returns accepted `(i, j) → index` labels.
///
/// `blacklist` — corner indices to exclude from candidate searches.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "debug",
        skip_all,
        fields(num_corners = corners.len(), cell_size = cell_size, blacklist_size = blacklist.len())
    )
)]
pub fn grow_from_seed(
    corners: &mut [CornerAug],
    seed: Seed,
    centers: ClusterCenters,
    cell_size: f32,
    blacklist: &HashSet<usize>,
    params: &DetectorParams,
) -> GrowResult {
    let positions: Vec<Point2<f32>> = corners.iter().map(|c| c.position).collect();
    let pg_seed = pg_grow::Seed {
        a: seed.a,
        b: seed.b,
        c: seed.c,
        d: seed.d,
    };
    let pg_params =
        pg_grow::GrowParams::new(params.attach_search_rel, params.attach_ambiguity_factor);
    let validator = ChessboardGrowValidator::new(corners, blacklist, centers, cell_size, params);

    let pg_result = pg_grow::bfs_grow(&positions, pg_seed, cell_size, &pg_params, &validator);

    // After BFS: update each labelled corner's stage so downstream
    // modules (validate, boosters, detection output) see the Labeled
    // marker and the chosen `(i, j)`.
    for (&at, &idx) in &pg_result.labelled {
        corners[idx].stage = CornerStage::Labeled {
            at,
            local_h_residual_px: None,
        };
    }

    pg_result.into()
}

/// Chessboard's plug-in for [`pg_grow::GrowValidator`].
///
/// Holds immutable references into the caller's corner array + the
/// clustering output + per-call tolerances. The BFS does not mutate
/// per-corner state via this trait — `grow_from_seed` does that after
/// the generic walk returns (see above).
pub(crate) struct ChessboardGrowValidator<'a> {
    pub(crate) corners: &'a [CornerAug],
    pub(crate) blacklist: &'a HashSet<usize>,
    pub(crate) centers: ClusterCenters,
    pub(crate) cell_size: f32,
    pub(crate) attach_tol_rad: f32,
    pub(crate) edge_tol_rad: f32,
    pub(crate) step_tol: f32,
}

impl<'a> ChessboardGrowValidator<'a> {
    /// Construct from chessboard `DetectorParams` + the same inputs the
    /// BFS-grow validator uses. Re-used by Stage-6
    /// `grow_extension::extend_via_global_homography` to keep parity /
    /// axis-cluster gates identical between BFS and boundary
    /// extrapolation.
    pub(crate) fn new(
        corners: &'a [CornerAug],
        blacklist: &'a HashSet<usize>,
        centers: ClusterCenters,
        cell_size: f32,
        params: &DetectorParams,
    ) -> Self {
        Self {
            corners,
            blacklist,
            centers,
            cell_size,
            attach_tol_rad: params.attach_axis_tol_deg.to_radians(),
            edge_tol_rad: params.edge_axis_tol_deg.to_radians(),
            step_tol: params.step_tol,
        }
    }
}

impl<'a> pg_grow::GrowValidator for ChessboardGrowValidator<'a> {
    fn is_eligible(&self, idx: usize) -> bool {
        if self.blacklist.contains(&idx) {
            return false;
        }
        matches!(self.corners[idx].stage, CornerStage::Clustered { .. })
    }

    fn required_label_at(&self, i: i32, j: i32) -> Option<u8> {
        Some(label_to_u8(required_label_at(i, j)))
    }

    fn label_of(&self, idx: usize) -> Option<u8> {
        if let CornerStage::Clustered { label } = self.corners[idx].stage {
            Some(label_to_u8(label))
        } else {
            None
        }
    }

    fn accept_candidate(
        &self,
        idx: usize,
        _at: (i32, i32),
        _prediction: Point2<f32>,
        _neighbours: &[pg_grow::LabelledNeighbour],
    ) -> pg_grow::Admit {
        let c = &self.corners[idx];
        if axes_match_centers(&c.axes, self.centers, self.attach_tol_rad) {
            pg_grow::Admit::Accept
        } else {
            pg_grow::Admit::Reject
        }
    }

    fn edge_ok(
        &self,
        candidate_idx: usize,
        neighbour_idx: usize,
        _at_candidate: (i32, i32),
        _at_neighbour: (i32, i32),
    ) -> bool {
        let c = &self.corners[candidate_idx];
        let n = &self.corners[neighbour_idx];
        let min_len = (1.0 - self.step_tol) * self.cell_size;
        let max_len = (1.0 + self.step_tol) * self.cell_size;
        let off = n.position - c.position;
        let dist = off.norm();
        if dist < min_len || dist > max_len {
            return false;
        }
        let ang = wrap_pi(off.y.atan2(off.x));
        let d_c0 = angular_dist_pi(ang, wrap_pi(c.axes[0].angle));
        let d_c1 = angular_dist_pi(ang, wrap_pi(c.axes[1].angle));
        let (slot_c, d_c) = if d_c0 <= d_c1 { (0, d_c0) } else { (1, d_c1) };
        if d_c > self.edge_tol_rad {
            return false;
        }
        let d_n0 = angular_dist_pi(ang, wrap_pi(n.axes[0].angle));
        let d_n1 = angular_dist_pi(ang, wrap_pi(n.axes[1].angle));
        let (slot_n, d_n) = if d_n0 <= d_n1 { (0, d_n0) } else { (1, d_n1) };
        if d_n > self.edge_tol_rad {
            return false;
        }
        slot_c != slot_n
    }
}

/// Parity of a labelled cell under the seed convention (seed `A` at
/// `(0, 0)` is `Canonical`).
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

fn axes_match_centers(axes: &[AxisEstimate; 2], centers: ClusterCenters, tol: f32) -> bool {
    let a0 = wrap_pi(axes[0].angle);
    let a1 = wrap_pi(axes[1].angle);
    let canon_max = angular_dist_pi(a0, centers.theta0).max(angular_dist_pi(a1, centers.theta1));
    let swap_max = angular_dist_pi(a0, centers.theta1).max(angular_dist_pi(a1, centers.theta0));
    canon_max.min(swap_max) <= tol
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::cluster_axes;
    use crate::seed::find_seed;
    use calib_targets_core::{AxisEstimate, Corner};
    use nalgebra::Point2;

    fn make_corner(
        idx: usize,
        x: f32,
        y: f32,
        axis_u: f32,
        axis_v: f32,
        swapped: bool,
    ) -> CornerAug {
        let _ = (axis_u, axis_v);
        let (a0, a1) = if swapped {
            (std::f32::consts::FRAC_PI_2, 0.0)
        } else {
            (0.0, std::f32::consts::FRAC_PI_2)
        };
        let c = Corner {
            position: Point2::new(x, y),
            orientation_cluster: None,
            axes: [
                AxisEstimate {
                    angle: a0,
                    sigma: 0.01,
                },
                AxisEstimate {
                    angle: a1,
                    sigma: 0.01,
                },
            ],
            contrast: 10.0,
            fit_rms: 1.0,
            strength: 1.0,
        };
        let mut aug = CornerAug::from_corner(idx, &c);
        aug.stage = CornerStage::Strong;
        aug
    }

    fn clean_grid(rows: i32, cols: i32, s: f32) -> Vec<CornerAug> {
        let mut out = Vec::new();
        let mut idx = 0;
        for j in 0..rows {
            for i in 0..cols {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let swapped = (i + j).rem_euclid(2) == 1;
                out.push(make_corner(
                    idx,
                    x,
                    y,
                    0.0,
                    std::f32::consts::FRAC_PI_2,
                    swapped,
                ));
                idx += 1;
            }
        }
        out
    }

    #[test]
    fn grow_labels_full_7x7() {
        let mut corners = clean_grid(7, 7, 20.0);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let seed = find_seed(&corners, centers, &params).expect("seed").seed;
        let blacklist = HashSet::new();
        let res = grow_from_seed(&mut corners, seed, centers, 20.0, &blacklist, &params);
        assert_eq!(res.labelled.len(), 49);
        let labelled_count = corners
            .iter()
            .filter(|c| matches!(c.stage, CornerStage::Labeled { .. }))
            .count();
        assert_eq!(labelled_count, 49);
        // Rebased to non-negative.
        assert!(res.labelled.keys().all(|(i, j)| *i >= 0 && *j >= 0));
    }

    #[test]
    fn rejects_parity_wrong_false_corner() {
        let mut corners = clean_grid(5, 5, 20.0);
        // Flip the center corner's axes so its Stage-3 label flips.
        let center_idx = 2 * 5 + 2;
        corners[center_idx].axes = [
            AxisEstimate {
                angle: std::f32::consts::FRAC_PI_2,
                sigma: 0.01,
            },
            AxisEstimate {
                angle: 0.0,
                sigma: 0.01,
            },
        ];
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let seed = find_seed(&corners, centers, &params).expect("seed").seed;
        let blacklist = HashSet::new();
        let res = grow_from_seed(&mut corners, seed, centers, 20.0, &blacklist, &params);
        assert!(
            !res.by_corner.contains_key(&center_idx),
            "parity-wrong corner was labelled"
        );
        assert!(res.labelled.len() >= 20);
    }

    #[test]
    fn grows_along_single_column_when_neighbours_are_collinear() {
        // 7-row by 2-column strip — at every position along j, the
        // labelled neighbours when predicting (0, j+1) sit on the same
        // column. The single-neighbour prediction path must handle
        // this without the old affine-from-3 singularity.
        let s = 25.0;
        let mut corners = clean_grid(7, 2, s);
        let params = DetectorParams::default();
        let centers = cluster_axes(&mut corners, &params).expect("centers");
        let seed = find_seed(&corners, centers, &params).expect("seed").seed;
        let blacklist = HashSet::new();
        let res = grow_from_seed(&mut corners, seed, centers, s, &blacklist, &params);
        assert_eq!(res.labelled.len(), 14, "got {}", res.labelled.len());
    }
}
