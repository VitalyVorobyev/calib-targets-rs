//! Compare a fresh detector output against a baseline.
//!
//! # Contract
//!
//! The baseline (GT) is a **minimum signal**, not a ceiling:
//!
//! 1. **No false positives.** Every GT corner must appear in the run at
//!    a byte-equal position (within [`crate::POSITION_DRIFT_EPS_PX`]).
//! 2. **`(i, j)` labels may shift uniformly.** A grid-builder change can
//!    legitimately rebase the labelling — e.g., finding a new column to
//!    the left of the old `(0, 0)` shifts every old label by `(+1, 0)`.
//!    We auto-derive the shift `Δ = run_grid − gt_grid` from any single
//!    matching pair and verify all matches agree on it.
//! 3. **Extras are improvements, not failures.** Run corners whose
//!    positions are not in the GT are reported as `extras` for visibility
//!    but do not flip the pass/fail bit.
//!
//! # Failure modes
//!
//! - `missing_labels`: GT corner whose position is *not* present in the
//!   run output. Hard regression.
//! - `wrong_position`: GT and run share an `(i+Δ, j+Δ)` pair but the run
//!   position drifted by more than `POSITION_DRIFT_EPS_PX`. The grid
//!   builder picked a different corner index for the same cell.
//! - `wrong_id`: same `(i+Δ, j+Δ)` and same position but different `id`
//!   field (relevant for ChArUco / marker boards, not chessboard).
//! - `inconsistent_shift`: matched pairs require different `Δ`. Indicates
//!   a non-rigid relabelling — the grid builder topology changed.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::baseline::{BaselineCorner, BaselineImage};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct WrongPosition {
    /// Baseline `(i, j)`.
    pub i: i32,
    pub j: i32,
    pub baseline: [f32; 2],
    pub run: [f32; 2],
    pub drift_px: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct BaselineDiff {
    /// GT corners with no matching position in the run output.
    pub missing_labels: Vec<[i32; 2]>,
    /// Run corners whose positions are not in the GT (improvements).
    /// Reported informationally; does NOT flip the pass/fail bit.
    pub extra_labels: Vec<[i32; 2]>,
    /// GT/run pairs whose positions disagree above the drift floor.
    pub wrong_position: Vec<WrongPosition>,
    /// GT/run pairs whose `id` field disagrees.
    pub wrong_id: Vec<[i32; 2]>,
    /// Detected `(Δi, Δj)` shift applied to GT labels before matching,
    /// or `None` when no matching pair was found / the run lost detection.
    pub shift: Option<[i32; 2]>,
    /// True when matched pairs disagreed on `Δ` (impossible-to-reconcile
    /// non-rigid relabelling). Always a hard failure.
    pub inconsistent_shift: bool,
    /// Run output had two or more `(i, j)` labels that quantise to the
    /// same pixel position — i.e., the same physical corner was
    /// labelled twice. Always a hard failure.
    pub duplicate_run_positions: Vec<DuplicatePosition>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DuplicatePosition {
    pub position: [f32; 2],
    /// Every run-grid label that quantised to the same position.
    pub labels: Vec<[i32; 2]>,
}

impl BaselineDiff {
    /// Run output passes the GT contract: no missing, no wrong positions,
    /// no wrong ids, no inconsistent shift, no duplicate-position labels.
    /// Extras allowed.
    pub fn passed(&self) -> bool {
        self.missing_labels.is_empty()
            && self.wrong_position.is_empty()
            && self.wrong_id.is_empty()
            && !self.inconsistent_shift
            && self.duplicate_run_positions.is_empty()
    }

    pub fn compute(baseline: &BaselineImage, run: &[BaselineCorner]) -> Self {
        // Index runs by quantised position. Multiple labels mapping to
        // the same position is a precision-contract violation (the same
        // physical corner labelled twice) — collect every such bucket.
        let mut by_pos_run: HashMap<(i32, i32), Vec<&BaselineCorner>> = HashMap::new();
        for c in run {
            by_pos_run
                .entry(quantise_pos(c.x, c.y))
                .or_default()
                .push(c);
        }

        let mut diff = BaselineDiff::default();
        for (_, bucket) in by_pos_run.iter() {
            if bucket.len() >= 2 {
                let mut labels: Vec<[i32; 2]> = bucket.iter().map(|c| [c.i, c.j]).collect();
                labels.sort();
                diff.duplicate_run_positions.push(DuplicatePosition {
                    position: [bucket[0].x, bucket[0].y],
                    labels,
                });
            }
        }

        let mut shift: Option<[i32; 2]> = None;
        let mut matched_run_keys: std::collections::HashSet<(i32, i32)> =
            std::collections::HashSet::new();

        for b in &baseline.corners {
            let key = quantise_pos(b.x, b.y);
            let Some(bucket) = by_pos_run.get(&key) else {
                diff.missing_labels.push([b.i, b.j]);
                continue;
            };
            // Even if the bucket is multi-valued, pick the first to
            // measure drift / ID; the duplication itself was already
            // recorded above.
            let r = bucket[0];

            // Position match found. Verify the (i, j) shift is consistent.
            let this_shift = [r.i - b.i, r.j - b.j];
            match shift {
                None => shift = Some(this_shift),
                Some(prev) if prev != this_shift => {
                    diff.inconsistent_shift = true;
                }
                _ => {}
            }

            // Position drift within the rounding floor.
            let dx = r.x - b.x;
            let dy = r.y - b.y;
            let drift = (dx * dx + dy * dy).sqrt();
            if drift > crate::POSITION_DRIFT_EPS_PX {
                diff.wrong_position.push(WrongPosition {
                    i: b.i,
                    j: b.j,
                    baseline: [b.x, b.y],
                    run: [r.x, r.y],
                    drift_px: drift,
                });
            }

            if r.id != b.id {
                diff.wrong_id.push([b.i, b.j]);
            }

            matched_run_keys.insert(key);
        }

        // Extras: every run corner whose position wasn't matched against
        // a GT corner. Duplicates within the bucket count once each
        // (they're already flagged as `duplicate_run_positions`).
        for c in run {
            let key = quantise_pos(c.x, c.y);
            if !matched_run_keys.contains(&key) {
                diff.extra_labels.push([c.i, c.j]);
            }
        }

        diff.shift = shift;
        diff
    }
}

/// Quantise a pixel position to the [`crate::POSITION_DRIFT_EPS_PX`] grid.
/// Two corners closer than that floor map to the same key.
fn quantise_pos(x: f32, y: f32) -> (i32, i32) {
    let scale = 1.0_f32 / crate::POSITION_DRIFT_EPS_PX;
    ((x * scale).round() as i32, (y * scale).round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corner(i: i32, j: i32, x: f32, y: f32) -> BaselineCorner {
        BaselineCorner {
            i,
            j,
            x,
            y,
            id: None,
            score: 0.0,
        }
    }

    fn baseline(corners: Vec<BaselineCorner>) -> BaselineImage {
        BaselineImage {
            labelled_count: corners.len(),
            cell_size_px: 10.0,
            corners,
        }
    }

    #[test]
    fn byte_exact_match_passes() {
        let bl = baseline(vec![corner(0, 0, 10.0, 10.0), corner(1, 0, 20.0, 10.0)]);
        let run = bl.corners.clone();
        let diff = BaselineDiff::compute(&bl, &run);
        assert!(diff.passed());
        assert_eq!(diff.shift, Some([0, 0]));
        assert!(diff.extra_labels.is_empty());
    }

    #[test]
    fn extras_are_informational_not_failures() {
        // GT has 2 corners; run has the same 2 plus one new one.
        let bl = baseline(vec![corner(0, 0, 10.0, 10.0), corner(1, 0, 20.0, 10.0)]);
        let run = vec![
            corner(0, 0, 10.0, 10.0),
            corner(1, 0, 20.0, 10.0),
            corner(2, 0, 30.0, 10.0), // new
        ];
        let diff = BaselineDiff::compute(&bl, &run);
        assert!(diff.passed(), "extras must not fail");
        assert_eq!(diff.extra_labels, vec![[2, 0]]);
    }

    #[test]
    fn uniform_shift_passes() {
        // GT labels (0, 0)..(2, 0); run rebased so the same positions
        // are now (1, 0)..(3, 0). Auto-Δ = (1, 0); contract holds.
        let bl = baseline(vec![
            corner(0, 0, 10.0, 10.0),
            corner(1, 0, 20.0, 10.0),
            corner(2, 0, 30.0, 10.0),
        ]);
        let run = vec![
            corner(1, 0, 10.0, 10.0),
            corner(2, 0, 20.0, 10.0),
            corner(3, 0, 30.0, 10.0),
        ];
        let diff = BaselineDiff::compute(&bl, &run);
        assert!(diff.passed());
        assert_eq!(diff.shift, Some([1, 0]));
    }

    #[test]
    fn missing_corner_fails() {
        let bl = baseline(vec![corner(0, 0, 10.0, 10.0), corner(1, 0, 20.0, 10.0)]);
        let run = vec![corner(0, 0, 10.0, 10.0)];
        let diff = BaselineDiff::compute(&bl, &run);
        assert!(!diff.passed());
        assert_eq!(diff.missing_labels, vec![[1, 0]]);
    }

    #[test]
    fn position_drift_fails() {
        let bl = baseline(vec![corner(0, 0, 10.0, 10.0)]);
        // Pure-position drift > epsilon: no match key found, registered as missing,
        // and the run corner shows up as an extra. Both signal a real regression.
        let run = vec![corner(0, 0, 10.05, 10.0)];
        let diff = BaselineDiff::compute(&bl, &run);
        assert!(!diff.passed());
        assert_eq!(diff.missing_labels.len(), 1);
        assert_eq!(diff.extra_labels.len(), 1);
    }

    #[test]
    fn duplicate_run_position_fails() {
        // Run output has two distinct (i, j) labels at the same physical
        // position — the precision-contract violation (one corner
        // labelled twice). Diff must fail.
        let bl = baseline(vec![corner(0, 0, 10.0, 10.0)]);
        let run = vec![
            corner(0, 0, 10.0, 10.0),
            corner(1, 0, 10.0, 10.0), // SAME position, different label
        ];
        let diff = BaselineDiff::compute(&bl, &run);
        assert!(!diff.passed());
        assert_eq!(diff.duplicate_run_positions.len(), 1);
        let dup = &diff.duplicate_run_positions[0];
        assert_eq!(dup.labels.len(), 2);
    }

    #[test]
    fn inconsistent_shift_fails() {
        // Two GT pairs that disagree on Δ — non-rigid relabelling.
        let bl = baseline(vec![corner(0, 0, 10.0, 10.0), corner(1, 0, 20.0, 10.0)]);
        let run = vec![
            corner(0, 0, 10.0, 10.0), // Δ = (0, 0)
            corner(2, 0, 20.0, 10.0), // Δ = (1, 0) — disagrees
        ];
        let diff = BaselineDiff::compute(&bl, &run);
        assert!(!diff.passed());
        assert!(diff.inconsistent_shift);
    }
}
