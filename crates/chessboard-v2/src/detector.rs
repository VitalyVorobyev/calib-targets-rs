//! Detector orchestrator: run the precision core end-to-end.
//!
//! Stages 5–7 loop with a blacklist until validation converges or
//! `max_validation_iters` is reached. The recall boosters (Stage 8)
//! are out of scope for this initial wiring — they will extend
//! the labelled set without compromising invariants once the core
//! is proven on the 120-snap dataset.

use crate::boosters::{apply_boosters, BoosterResult};
use crate::cluster::{cluster_axes, ClusterCenters};
use crate::corner::{CornerAug, CornerStage};
use crate::grow::{grow_from_seed, GrowResult};
use crate::params::DetectorParams;
use crate::seed::{find_seed, SeedOutput};
use crate::validate::{validate, ValidationResult};
use calib_targets_core::{Corner, GridCoords, LabeledCorner, TargetDetection, TargetKind};
use serde::Serialize;
use std::collections::HashSet;

/// Final detection output.
#[derive(Clone, Debug, Serialize)]
pub struct Detection {
    pub grid_directions: [f32; 2],
    pub cell_size: f32,
    pub target: TargetDetection,
    /// Indices into the input `corners` slice for the labelled corners,
    /// in the same order as `target.corners`.
    ///
    /// Consumers that need to map labelled grid points back to raw ChESS
    /// inputs (e.g., ChArUco marker alignment) should read this vector
    /// rather than reconstructing the mapping from positions.
    pub strong_indices: Vec<usize>,
}

/// Current [`DebugFrame`] schema version.
///
/// Bumped when fields are removed, renamed, or their semantics change.
/// Adding new optional fields does NOT bump the schema. Downstream
/// tooling (Python overlay script, etc.) should warn on mismatch.
pub const DEBUG_FRAME_SCHEMA: u32 = 1;

/// Compact debug payload — one per detection call.
///
/// Flat and serde-friendly so the Python overlay script can render
/// every decision stage.
#[derive(Clone, Debug, Serialize)]
pub struct DebugFrame {
    /// Schema version — see [`DEBUG_FRAME_SCHEMA`].
    pub schema: u32,
    pub input_count: usize,
    pub grid_directions: Option<[f32; 2]>,
    pub cell_size: Option<f32>,
    pub seed: Option<[usize; 4]>,
    pub iterations: Vec<IterationTrace>,
    /// Summary from the Stage-8 recall boosters (`None` when
    /// boosters didn't run — e.g., empty or Stage-5 failure).
    pub boosters: Option<BoosterResult>,
    pub detection: Option<Detection>,
    /// All corners carried through the pipeline (same indexing as
    /// the input slice). `stage` captures where each corner ended
    /// up.
    pub corners: Vec<CornerAug>,
}

#[derive(Clone, Debug, Serialize)]
pub struct IterationTrace {
    pub iter: u32,
    pub labelled_count: usize,
    pub new_blacklist: Vec<usize>,
    pub converged: bool,
}

/// Top-level detector.
pub struct Detector {
    pub params: DetectorParams,
}

impl Detector {
    pub fn new(params: DetectorParams) -> Self {
        Self { params }
    }

    /// Simple entry point: run the pipeline and return the best detection.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect(&self, corners: &[Corner]) -> Option<Detection> {
        self.detect_debug(corners).detection
    }

    /// Full-debug entry point for a single best detection.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_debug(&self, corners: &[Corner]) -> DebugFrame {
        self.detect_debug_excluding(corners, &HashSet::new())
    }

    /// Return every qualifying grid component from a single scene.
    ///
    /// Useful for ChArUco and similar setups where a single physical board
    /// can be split into multiple disconnected chessboard pieces by
    /// markers or occlusions. Each returned [`Detection`] carries its own
    /// locally-rebased `(i, j)` labels; alignment to a global frame is the
    /// caller's responsibility (ChArUco's marker decoding does this).
    ///
    /// Capped by [`DetectorParams::max_components`].
    ///
    /// Does NOT support scenes with multiple separate physical boards — one
    /// target per frame is the contract.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_all(&self, corners: &[Corner]) -> Vec<Detection> {
        self.detect_all_debug(corners)
            .into_iter()
            .filter_map(|f| f.detection)
            .collect()
    }

    /// Full-debug multi-component entry point. See [`Self::detect_all`].
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(
            level = "info",
            skip(self, corners),
            fields(num_corners = corners.len())
        )
    )]
    pub fn detect_all_debug(&self, corners: &[Corner]) -> Vec<DebugFrame> {
        let cap = self.params.max_components.max(1) as usize;
        let mut consumed: HashSet<usize> = HashSet::new();
        let mut frames: Vec<DebugFrame> = Vec::with_capacity(cap);

        for _ in 0..cap {
            let frame = self.detect_debug_excluding(corners, &consumed);
            let Some(detection) = frame.detection.as_ref() else {
                // No further detectable component — include the empty frame
                // so caller can introspect the failure stage if desired.
                if frames.is_empty() {
                    frames.push(frame);
                }
                break;
            };
            for &idx in &detection.strong_indices {
                consumed.insert(idx);
            }
            frames.push(frame);
        }

        frames
    }

    fn detect_debug_excluding(
        &self,
        corners: &[Corner],
        consumed: &HashSet<usize>,
    ) -> DebugFrame {
        let input_count = corners.len();
        let mut augs: Vec<CornerAug> = corners
            .iter()
            .enumerate()
            .map(|(i, c)| CornerAug::from_corner(i, c))
            .collect();

        let mut frame = DebugFrame {
            schema: DEBUG_FRAME_SCHEMA,
            input_count,
            grid_directions: None,
            cell_size: None,
            seed: None,
            iterations: Vec::new(),
            boosters: None,
            detection: None,
            corners: Vec::new(),
        };

        // Stage 1: pre-filter.
        // Corners already consumed by a previous `detect_all` iteration are
        // left at `Raw` stage, which makes them invisible to clustering,
        // seed search, grow, and validation.
        for aug in augs.iter_mut() {
            if consumed.contains(&aug.input_index) {
                continue;
            }
            if passes_strength(aug, &self.params) && passes_fit_quality(aug, &self.params) {
                aug.stage = CornerStage::Strong;
            }
        }
        if augs
            .iter()
            .filter(|a| matches!(a.stage, CornerStage::Strong))
            .count()
            < self.params.min_labeled_corners
        {
            frame.corners = augs;
            return frame;
        }

        // Stage 2 + 3: clustering.
        let centers = match cluster_axes(&mut augs, &self.params) {
            Some(c) => c,
            None => {
                frame.corners = augs;
                return frame;
            }
        };
        frame.grid_directions = Some([centers.theta0, centers.theta1]);

        // Stages 4+5 (fused): the seed finder is now self-consistent
        // — it finds a 4-corner quad that matches itself in edge
        // lengths, and reports `cell_size` as the mean seed-edge
        // length. This avoids the bimodal-histogram failure where
        // the old global cell-size estimator picked a too-small
        // mode (typically marker-internal spacing rather than true
        // board spacing), leaving the downstream edge-window
        // `[0.75s, 1.25s]` excluding every legitimate neighbor.
        //
        // The detector loops with a blacklist; each iteration re-
        // runs the seed + growth pair.
        let mut blacklist: HashSet<usize> = HashSet::new();
        let max_iters = self.params.max_validation_iters.max(1);

        for it in 0..max_iters {
            // Reset any Labeled stage on corners not in blacklist —
            // re-run means re-label from scratch in this iteration.
            for aug in augs.iter_mut() {
                if matches!(aug.stage, CornerStage::Labeled { .. })
                    && !blacklist.contains(&aug.input_index)
                {
                    // Stage-3 → Stage-5 invariant: every Labeled corner
                    // carries its cluster label. If it's somehow missing,
                    // leave the stage as-is rather than panicking — the
                    // next iteration's checks will re-handle this corner.
                    if let Some(label) = aug.label {
                        aug.stage = CornerStage::Clustered { label };
                    }
                }
            }

            let seed_out: SeedOutput = match find_seed(&augs, centers, &self.params) {
                Some(s) => s,
                None => break,
            };
            let seed = seed_out.seed;
            let cell_size = seed_out.cell_size;
            frame.cell_size = Some(cell_size);
            frame.seed = Some([seed.a, seed.b, seed.c, seed.d]);

            let grow_res: GrowResult = grow_from_seed(
                &mut augs,
                seed,
                centers,
                cell_size,
                &blacklist,
                &self.params,
            );
            let labelled_count = grow_res.labelled.len();

            let v: ValidationResult = validate(&augs, &grow_res.labelled, cell_size, &self.params);
            let new_blacklist: Vec<usize> = v
                .blacklist
                .iter()
                .filter(|idx| !blacklist.contains(idx))
                .copied()
                .collect();

            let converged = new_blacklist.is_empty();
            frame.iterations.push(IterationTrace {
                iter: it,
                labelled_count,
                new_blacklist: new_blacklist.clone(),
                converged,
            });

            if converged {
                let mut grow_mut = grow_res;

                // Phase E recall boosters: interior gap fill + line
                // extrapolation. Runs only after the precision core
                // has converged with no new blacklist entries, so
                // every candidate is validated against the same
                // attachment invariants as growth.
                let booster: BoosterResult = apply_boosters(
                    &mut augs,
                    &mut grow_mut,
                    centers,
                    cell_size,
                    &blacklist,
                    &self.params,
                );
                frame.boosters = Some(booster);

                // Write local-H residuals onto labelled corners.
                for (&c_idx, &resid) in &v.local_h_residuals {
                    if let CornerStage::Labeled { at, .. } = augs[c_idx].stage {
                        augs[c_idx].stage = CornerStage::Labeled {
                            at,
                            local_h_residual_px: Some(resid),
                        };
                    }
                }
                let final_count = grow_mut.labelled.len();
                if final_count >= self.params.min_labeled_corners {
                    frame.detection = Some(build_detection(&augs, &grow_mut, centers, cell_size));
                }
                break;
            }

            // Mark blacklisted corners and retry.
            for &idx in &new_blacklist {
                if let CornerStage::Labeled { at, .. } = augs[idx].stage {
                    augs[idx].stage = CornerStage::LabeledThenBlacklisted {
                        at,
                        reason: "post-validation outlier".into(),
                    };
                }
                blacklist.insert(idx);
            }
        }

        frame.corners = augs;
        frame
    }
}

fn passes_strength(aug: &CornerAug, params: &DetectorParams) -> bool {
    aug.strength >= params.min_corner_strength
}

fn passes_fit_quality(aug: &CornerAug, params: &DetectorParams) -> bool {
    if !params.max_fit_rms_ratio.is_finite() {
        return true;
    }
    if aug.contrast <= 0.0 {
        return true;
    }
    aug.fit_rms <= params.max_fit_rms_ratio * aug.contrast
}

fn build_detection(
    corners: &[CornerAug],
    grow: &GrowResult,
    centers: ClusterCenters,
    cell_size: f32,
) -> Detection {
    // Growth rebases (i, j) to non-negative already.
    // Sort by (j, i) before building LabeledCorner so the output order is
    // stable and we don't need a post-hoc unwrap on `grid`.
    let mut labelled_pairs: Vec<((i32, i32), usize)> =
        grow.labelled.iter().map(|(&k, &v)| (k, v)).collect();
    labelled_pairs.sort_by_key(|&((i, j), _)| (j, i));

    let mut labeled_corners: Vec<LabeledCorner> = Vec::with_capacity(labelled_pairs.len());
    let mut strong_indices: Vec<usize> = Vec::with_capacity(labelled_pairs.len());
    for ((i, j), c_idx) in labelled_pairs {
        let c = &corners[c_idx];
        labeled_corners.push(LabeledCorner {
            position: c.position,
            grid: Some(GridCoords { i, j }),
            id: None,
            target_position: None,
            score: c.strength,
        });
        strong_indices.push(c.input_index);
    }

    Detection {
        grid_directions: [centers.theta0, centers.theta1],
        cell_size,
        target: TargetDetection {
            kind: TargetKind::Chessboard,
            corners: labeled_corners,
        },
        strong_indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calib_targets_core::{AxisEstimate, Corner};
    use nalgebra::Point2;

    fn make_corner(idx: usize, x: f32, y: f32, swapped: bool) -> Corner {
        let (a0, a1) = if swapped {
            (std::f32::consts::FRAC_PI_2, 0.0)
        } else {
            (0.0, std::f32::consts::FRAC_PI_2)
        };
        let _ = idx;
        Corner {
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
        }
    }

    #[test]
    fn end_to_end_clean_grid() {
        let s = 20.0_f32;
        let mut corners = Vec::new();
        let mut k = 0;
        for j in 0..7_i32 {
            for i in 0..7_i32 {
                let x = i as f32 * s + 50.0;
                let y = j as f32 * s + 50.0;
                let swapped = (i + j).rem_euclid(2) == 1;
                corners.push(make_corner(k, x, y, swapped));
                k += 1;
            }
        }
        let det = Detector::new(DetectorParams::default());
        let d = det.detect(&corners).expect("detection");
        assert_eq!(d.target.corners.len(), 49);
    }

    #[test]
    fn rejects_when_too_few_corners() {
        let det = Detector::new(DetectorParams::default());
        assert!(det.detect(&[]).is_none());
    }
}
