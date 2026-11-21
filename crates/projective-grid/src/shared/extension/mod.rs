//! Boundary extension of a labelled grid via fitted homography.
//!
//! Once an inner block of `(i, j)` labels has been grown, this pass
//! attempts to extend the labelled set outward by fitting planar
//! homographies on the existing labels and projecting integer cells
//! beyond the labelled boundary.
//!
//! [`extend_via_local_homography`] fits a *per-candidate* H from the K
//! nearest labelled corners (by grid distance). Each cell gets a local
//! model that adapts to the local distortion regime, gated by a
//! per-candidate trust test — closer to APAP / moving-DLT in spirit than a
//! single global fit, which cannot hold under heavy radial distortion or
//! multi-region perspective.
//!
//! | Submodule | Responsibility |
//! |---|---|
//! | `common` (private) | Per-cell attachment ladder |
//! | [`local`] | [`extend_via_local_homography`] + deep cell enumeration + K-NN |
//!
//! # Precision contract
//!
//! Boundary-extension attachments must obey the same invariants as BFS
//! attachments (zero false-positive labels). Three layers of defence:
//!
//! 1. **Reprojection-residual gate.** The worst-case residual of the
//!    local fit `|H · (i, j) − pos(label)|` over its supports is measured;
//!    if it exceeds the configured threshold (× `cell_size`), the candidate
//!    cell is skipped.
//!
//! 2. **Same per-corner gates as BFS.** Candidate filtering uses the
//!    policy's `is_eligible` + `label_of` against `required_label_at`,
//!    `accept_candidate`, AND `edge_ok` against at least one already-
//!    labelled cardinal neighbour.
//!
//! 3. **Single-claim guarantee.** Each attachment updates `by_corner`
//!    immediately, so a corner index can only be claimed by one cell.

mod common;
pub mod local;

pub use local::extend_via_local_homography;

/// Attachment knobs of the boundary-extension ladder, nested in
/// [`LocalExtensionParams`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct ExtensionCommonParams {
    /// Search radius around each `H · (cell)` prediction, expressed as a
    /// fraction of `cell_size`.
    pub search_rel: f32,
    /// Ambiguity gate: when the second-nearest candidate is within
    /// `factor × nearest`, the attachment is skipped. Tighter than
    /// BFS's 1.5 because boundary errors are unrecoverable.
    pub ambiguity_factor: f32,
    /// Per-pass cap on iterations.
    pub max_iters: u32,
    /// Maximum allowed worst-case reprojection residual on the labelled
    /// support set of each candidate's local fit, expressed as a fraction of
    /// `cell_size`.
    pub max_residual_rel: f32,
}

impl Default for ExtensionCommonParams {
    fn default() -> Self {
        Self {
            search_rel: 0.40,
            ambiguity_factor: 2.5,
            max_iters: 5,
            max_residual_rel: 0.30,
        }
    }
}

/// Tuning knobs for [`extend_via_local_homography`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct LocalExtensionParams {
    /// Shared knobs — search radius, ambiguity, iteration cap, residual
    /// gate. See [`ExtensionCommonParams`].
    ///
    /// Note: the local-H default `max_iters` is 8 (vs the
    /// [`ExtensionCommonParams`] default of 5) because local-H typically
    /// needs more passes to propagate outward.
    pub common: ExtensionCommonParams,
    /// Number of nearest labelled corners (by grid Manhattan distance)
    /// used to fit each candidate cell's local H.
    pub k_nearest: usize,
    /// Minimum supports below which a candidate cell is skipped (the
    /// local H would be under-determined or noise-dominated). Must be
    /// `≥ 4` for DLT to be solvable.
    pub min_k: usize,
    /// Cell distance past the current bbox to enumerate per iter.
    /// `1` is the original behaviour (extend by one cell, iterate).
    /// Larger values let one iter reach further when the immediate
    /// neighbour cells are empty but cells further out have corners.
    pub extend_depth: u32,
}

impl Default for LocalExtensionParams {
    fn default() -> Self {
        Self {
            common: ExtensionCommonParams {
                max_iters: 8, // local-H default differs from the common default
                ..ExtensionCommonParams::default()
            },
            k_nearest: 12,
            min_k: 6,
            extend_depth: 3,
        }
    }
}

/// Diagnostic counters returned by [`extend_via_local_homography`].
///
/// `attached_indices` lets callers identify boundary-extension
/// attachments distinct from BFS-grow labels, e.g., for downstream
/// blacklist scoping or overlay rendering.
#[non_exhaustive]
#[derive(Clone, Debug, Default)]
pub struct ExtensionStats {
    /// Number of extension iterations actually run (≤ the configured cap).
    pub iterations: usize,
    /// `None` when the H wasn't fit. Pixel units.
    pub h_residual_median_px: Option<f32>,
    /// Maximum reprojection residual on the labelled set, in pixels.
    /// `None` when the H wasn't fit.
    pub h_residual_max_px: Option<f32>,
    /// `false` when the residual gate refused to extrapolate — the
    /// function is a no-op and `attached == 0`.
    pub h_trusted: bool,
    /// Number of corners successfully attached across all iterations.
    pub attached: usize,
    /// Candidate cells skipped because no corner sat near the predicted spot.
    pub rejected_no_candidate: usize,
    /// Candidate cells skipped because two corners were equally plausible.
    pub rejected_ambiguous: usize,
    /// Candidate cells skipped because the target `(i, j)` was already labelled.
    pub rejected_label: usize,
    /// Candidate corners rejected by the caller-supplied
    /// [`SquareAttachPolicy`](crate::shared::grow::SquareAttachPolicy).
    pub rejected_policy: usize,
    /// Candidate corners rejected by the induced-edge geometry check.
    pub rejected_edge: usize,
    /// Indices of the corners attached in this pass.
    pub attached_indices: Vec<usize>,
    /// `(i, j)` cells that survived to attachment.
    pub attached_cells: Vec<(i32, i32)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::grow::{Admit, GrowResult, LabelledNeighbour, SquareAttachPolicy};
    use nalgebra::Point2;
    use std::collections::HashMap;

    /// Trivial policy: every corner eligible, no label constraint, accept every candidate.
    struct OpenValidator;

    impl SquareAttachPolicy for OpenValidator {
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
            _idx: usize,
            _at: (i32, i32),
            _prediction: Point2<f32>,
            _neighbours: &[LabelledNeighbour],
        ) -> Admit {
            Admit::Accept
        }
    }

    /// Parity-aware policy: enforces a (i+j) % 2 == 0 → label 0,
    /// otherwise label 1 contract.
    struct AlternatingLabelPolicy {
        labels: Vec<u8>,
    }

    impl SquareAttachPolicy for AlternatingLabelPolicy {
        fn is_eligible(&self, _idx: usize) -> bool {
            true
        }
        fn required_label_at(&self, i: i32, j: i32) -> Option<u8> {
            Some(((i + j).rem_euclid(2)) as u8)
        }
        fn label_of(&self, idx: usize) -> Option<u8> {
            self.labels.get(idx).copied()
        }
        fn accept_candidate(
            &self,
            _idx: usize,
            _at: (i32, i32),
            _prediction: Point2<f32>,
            _neighbours: &[LabelledNeighbour],
        ) -> Admit {
            Admit::Accept
        }
    }

    /// Edge-aware policy: every edge involving `forbid_idx` is bad.
    struct EdgeRejectingValidator {
        forbid_idx: usize,
    }

    impl SquareAttachPolicy for EdgeRejectingValidator {
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
            _idx: usize,
            _at: (i32, i32),
            _prediction: Point2<f32>,
            _neighbours: &[LabelledNeighbour],
        ) -> Admit {
            Admit::Accept
        }
        fn edge_ok(
            &self,
            candidate_idx: usize,
            neighbour_idx: usize,
            _at_candidate: (i32, i32),
            _at_neighbour: (i32, i32),
        ) -> bool {
            candidate_idx != self.forbid_idx && neighbour_idx != self.forbid_idx
        }
    }

    fn synthetic_grid(rows: i32, cols: i32, scale: f32) -> Vec<Point2<f32>> {
        let mut pts = Vec::with_capacity((rows * cols) as usize);
        for j in 0..rows {
            for i in 0..cols {
                pts.push(Point2::new(
                    i as f32 * scale + 100.0,
                    j as f32 * scale + 50.0,
                ));
            }
        }
        pts
    }

    fn label_subgrid(
        positions: &[Point2<f32>],
        cols: i32,
        i_range: std::ops::Range<i32>,
        j_range: std::ops::Range<i32>,
    ) -> GrowResult {
        let mut labelled = HashMap::new();
        let mut by_corner = HashMap::new();
        for j in j_range {
            for i in i_range.clone() {
                let idx = (j * cols + i) as usize;
                labelled.insert((i, j), idx);
                by_corner.insert(idx, (i, j));
            }
        }
        let _ = positions;
        GrowResult {
            labelled,
            by_corner,
            ..Default::default()
        }
    }

    #[test]
    fn local_h_extends_clean_perspective_grid() {
        let cols = 6_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let positions = synthetic_grid(rows, cols, scale);
        let mut grow = label_subgrid(&positions, cols, 1..5, 1..3);
        let starting_count = grow.labelled.len();
        assert_eq!(starting_count, 8);

        let stats = extend_via_local_homography(
            &positions,
            &mut grow,
            scale,
            &LocalExtensionParams {
                min_k: 4,
                k_nearest: 8,
                ..Default::default()
            },
            &OpenValidator,
        );

        assert!(stats.h_trusted);
        assert!(
            grow.labelled.len() > starting_count,
            "local-H extension should add corners on a clean grid"
        );
    }

    #[test]
    fn local_h_reaches_further_than_global() {
        let cols = 8_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let positions = synthetic_grid(rows, cols, scale);
        let mut grow_local = label_subgrid(&positions, cols, 2..6, 0..rows);
        assert_eq!(grow_local.labelled.len(), 16);

        let stats = extend_via_local_homography(
            &positions,
            &mut grow_local,
            scale,
            &LocalExtensionParams {
                min_k: 4,
                ..Default::default()
            },
            &OpenValidator,
        );

        assert!(stats.iterations >= 2, "expected >= 2 iters");
        assert_eq!(
            grow_local.labelled.len(),
            (rows * cols) as usize,
            "local-H should reach every cell on a clean grid: {} of {}",
            grow_local.labelled.len(),
            rows * cols,
        );
    }

    #[test]
    fn local_h_no_op_when_too_few_labels() {
        let cols = 4_i32;
        let rows = 4_i32;
        let positions = synthetic_grid(rows, cols, 50.0);
        let mut grow = label_subgrid(&positions, cols, 0..2, 0..2);
        let stats = extend_via_local_homography(
            &positions,
            &mut grow,
            50.0,
            &LocalExtensionParams {
                min_k: 8,
                ..Default::default()
            },
            &OpenValidator,
        );
        assert_eq!(stats.attached, 0);
        assert!(!stats.h_trusted);
    }

    #[test]
    fn local_h_rejects_wrong_alternating_label() {
        let cols = 4_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let positions = synthetic_grid(rows, cols, scale);
        let mut grow = label_subgrid(&positions, cols, 1..3, 1..3);
        let labels: Vec<u8> = (0..(rows * cols))
            .map(|k| {
                let i = k % cols;
                let j = k / cols;
                ((i + j).rem_euclid(2)) as u8
            })
            .collect();
        let bad_idx = cols as usize;
        let mut labels = labels;
        labels[bad_idx] = 0;
        let policy = AlternatingLabelPolicy { labels };

        let stats = extend_via_local_homography(
            &positions,
            &mut grow,
            scale,
            &LocalExtensionParams {
                min_k: 4,
                ..Default::default()
            },
            &policy,
        );
        assert!(!grow.labelled.contains_key(&(0, 1)) || grow.labelled[&(0, 1)] != bad_idx);
        assert!(stats.rejected_label >= 1);
    }

    #[test]
    fn local_h_rejects_bad_edge_via_edge_ok_gate() {
        let cols = 4_i32;
        let rows = 4_i32;
        let scale = 50.0_f32;
        let positions = synthetic_grid(rows, cols, scale);
        let mut grow = label_subgrid(&positions, cols, 1..3, 1..3);

        let bad_candidate = cols as usize;
        let policy = EdgeRejectingValidator {
            forbid_idx: bad_candidate,
        };

        let stats = extend_via_local_homography(
            &positions,
            &mut grow,
            scale,
            &LocalExtensionParams {
                min_k: 4,
                ..Default::default()
            },
            &policy,
        );
        assert!(stats.rejected_edge >= 1);
        assert!(!grow.labelled.contains_key(&(0, 1)));
    }

    #[test]
    fn local_h_single_claim_prevents_double_attach() {
        let scale = 50.0_f32;
        let mut positions = Vec::new();
        for j in 0..3_i32 {
            for i in 0..3_i32 {
                positions.push(Point2::new(
                    i as f32 * scale + 100.0,
                    j as f32 * scale + 50.0,
                ));
            }
        }
        positions.push(Point2::new(250.0, 125.0));

        let mut labelled = HashMap::new();
        let mut by_corner = HashMap::new();
        for j in 0..3_i32 {
            for i in 0..3_i32 {
                let idx = (j * 3 + i) as usize;
                labelled.insert((i, j), idx);
                by_corner.insert(idx, (i, j));
            }
        }
        let mut grow = GrowResult {
            labelled,
            by_corner,
            ..Default::default()
        };

        let stats = extend_via_local_homography(
            &positions,
            &mut grow,
            scale,
            &LocalExtensionParams {
                min_k: 4,
                common: ExtensionCommonParams {
                    search_rel: 1.5,
                    ambiguity_factor: 1.01,
                    ..ExtensionCommonParams::default()
                },
                ..Default::default()
            },
            &OpenValidator,
        );
        assert!(
            stats.attached >= 1,
            "the extra corner must be attached once"
        );
        let attached_for_idx_9: Vec<&(i32, i32)> = grow
            .labelled
            .iter()
            .filter_map(|(k, &v)| if v == 9 { Some(k) } else { None })
            .collect();
        assert!(
            attached_for_idx_9.len() <= 1,
            "corner index 9 attached to {} cells: {:?}",
            attached_for_idx_9.len(),
            attached_for_idx_9
        );
        for (&cell, &idx) in &grow.labelled {
            assert_eq!(grow.by_corner.get(&idx), Some(&cell));
        }
    }
}
