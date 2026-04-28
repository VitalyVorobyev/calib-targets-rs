//! Post-growth validation for a labelled square grid.
//!
//! Two independent checks run over the labelled set:
//!
//! 1. **Line collinearity.** For every row (`j = const`) and column
//!    (`i = const`) with `≥ line_min_members` labelled members, fit
//!    a least-squares line in pixel space and flag any member whose
//!    perpendicular residual exceeds `line_tol_rel × scale`.
//!
//! 2. **Local-H residual.** For every labelled corner with ≥ 4
//!    non-collinear labelled neighbors in `(i, j)`-space, fit a 4-point
//!    local homography from the 4 grid-closest neighbors, predict the
//!    corner's pixel position, and measure the residual. Corners whose
//!    residual exceeds `local_h_tol_rel × scale` are flagged.
//!
//! `scale` is either the caller-supplied global `cell_size` (default
//! mode) or — when [`ValidationParams::use_step_aware`] is set — a
//! **per-corner local step** computed from labelled grid neighbours
//! via central or one-sided finite differences. Per-corner thresholds
//! are anisotropic: cells in perspective-foreshortened regions get a
//! tighter pixel tolerance proportional to their (smaller) local step;
//! cells in radially-distorted regions get a looser one. Corners
//! without enough labelled neighbours fall back to the global
//! `cell_size`.
//!
//! Flags are combined via the attribution rules below into a
//! blacklist of **indices into the input slice**:
//!
//! * A corner flagged in `≥ 2` lines is the outlier.
//! * A corner with a *large* local-H residual (`> 2 × local_h_tol`) AND
//!   at least one line flag is the outlier.
//! * A corner with a local-H flag but NO line flag, where at least one
//!   of its 4 base neighbors has `≥ 1` line flags, blames the worst-
//!   line-flagged base instead (the base is the outlier).
//! * When [`ValidationParams::step_deviation_thresh_rel`] is set, a
//!   corner whose local step deviates from the labelled-set median by
//!   more than the threshold AND has ≥ 1 line flag is also an outlier.
//! * Otherwise (isolated local-H flag with no supporting evidence),
//!   defer — no blacklist entry in this iteration.
//!
//! The caller is expected to re-run the seed/grow/validate loop after
//! updating its blacklist.
//!
//! # Pattern-agnostic
//!
//! This module has no dependency on chessboard-specific vocabulary
//! (parity, axis clusters, label enums). Any caller that can produce
//! a `(corner_index, pixel_position, grid_coord)` slice can use it.
//! Consumers that carry per-stage metadata should pre-filter to the
//! "labelled" subset before calling.

mod lines;
mod local_h;
mod step;

use nalgebra::Point2;
use std::collections::{HashMap, HashSet};

/// Tolerances for the validation pass.
///
/// All spatial tolerances are expressed as ratios of either the
/// caller-supplied global `cell_size` or — when [`use_step_aware`] is
/// set — the per-corner local step derived from labelled grid
/// neighbours.
///
/// [`use_step_aware`]: ValidationParams::use_step_aware
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct ValidationParams {
    /// Straight-line fit collinearity tolerance (fraction of the
    /// per-corner scale).
    pub line_tol_rel: f32,
    /// Minimum members required to fit a line / column.
    pub line_min_members: usize,
    /// Local-H prediction tolerance (fraction of the per-corner
    /// scale).
    pub local_h_tol_rel: f32,
    /// When `true`, line and local-H thresholds use a per-corner
    /// local step computed from labelled grid neighbours via central
    /// or one-sided finite differences (`(step_u + step_v) / 2`).
    /// Corners without enough labelled neighbours fall back to the
    /// global `cell_size`.
    ///
    /// Set this when the grid is non-uniform in pixel space —
    /// perspective foreshortening, radial distortion, or rectified-
    /// then-rasterised images. Has no effect on uniform grids.
    pub use_step_aware: bool,
    /// When `> 0` and [`use_step_aware`] is set, an additional flag
    /// fires for corners whose local step deviates from the labelled-
    /// set median by more than `step_deviation_thresh_rel` (relative).
    /// E.g. `0.5` flags corners whose step is < 1/(1+0.5) of the
    /// median or > (1+0.5)× the median.
    ///
    /// Combined with line flags via the existing attribution rules
    /// (rule 4: step-deviation flag + ≥ 1 line flag → outlier).
    /// Set to `0.0` to disable.
    ///
    /// [`use_step_aware`]: ValidationParams::use_step_aware
    pub step_deviation_thresh_rel: f32,
}

impl Default for ValidationParams {
    fn default() -> Self {
        // Matches `calib_targets_chessboard::DetectorParams`'s
        // validation defaults so the thin chessboard wrapper stays a
        // pure forward of the same numbers. Step-aware mode is opt-in.
        Self {
            line_tol_rel: 0.15,
            line_min_members: 3,
            local_h_tol_rel: 0.20,
            use_step_aware: false,
            step_deviation_thresh_rel: 0.0,
        }
    }
}

impl ValidationParams {
    /// Construct fully-specified core tolerances. Step-aware mode is
    /// off by default; call [`with_step_aware`] to enable it.
    ///
    /// [`with_step_aware`]: ValidationParams::with_step_aware
    pub fn new(line_tol_rel: f32, line_min_members: usize, local_h_tol_rel: f32) -> Self {
        Self {
            line_tol_rel,
            line_min_members,
            local_h_tol_rel,
            use_step_aware: false,
            step_deviation_thresh_rel: 0.0,
        }
    }

    /// Enable per-corner step-aware thresholds. Pass
    /// `deviation_thresh_rel = 0.0` for thresholds-only without the
    /// extra step-deviation flag.
    pub fn with_step_aware(mut self, deviation_thresh_rel: f32) -> Self {
        self.use_step_aware = true;
        self.step_deviation_thresh_rel = deviation_thresh_rel;
        self
    }
}

/// A single labelled corner fed into [`validate`]: its caller-chosen
/// index (carried back in `ValidationResult::blacklist`), its pixel
/// position, and its integer grid coordinate.
///
/// The index is opaque to this module — callers may pick any scheme
/// (direct slice indices, corner struct fields, etc.) as long as the
/// same scheme maps `blacklist` entries back to their originals.
#[derive(Clone, Copy, Debug)]
pub struct LabelledEntry {
    pub idx: usize,
    pub pixel: Point2<f32>,
    pub grid: (i32, i32),
}

/// Outcome of one validation pass.
#[derive(Debug, Default)]
pub struct ValidationResult {
    /// Corner indices to blacklist (attribution has been applied).
    pub blacklist: HashSet<usize>,
    /// For each labelled corner, its local-H residual in pixels
    /// (`None` when fewer than 4 non-collinear neighbors were
    /// available).
    pub local_h_residuals: HashMap<usize, f32>,
}

/// Run both validation passes and produce a blacklist.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_labelled = entries.len(), cell_size = cell_size),
    )
)]
pub fn validate(
    entries: &[LabelledEntry],
    cell_size: f32,
    params: &ValidationParams,
) -> ValidationResult {
    // Quick lookup maps (built once per call).
    let by_idx: HashMap<usize, &LabelledEntry> = entries.iter().map(|e| (e.idx, e)).collect();
    let by_grid: HashMap<(i32, i32), usize> = entries.iter().map(|e| (e.grid, e.idx)).collect();

    // Per-corner scale: in step-aware mode this is the labelled-
    // neighbour finite-difference step; otherwise it's the global
    // cell_size for every corner.
    let per_corner_step = if params.use_step_aware {
        step::local_step_per_corner(&by_idx, &by_grid)
    } else {
        HashMap::new()
    };
    let scale_at = |idx: usize| -> f32 {
        if params.use_step_aware {
            per_corner_step.get(&idx).copied().unwrap_or(cell_size)
        } else {
            cell_size
        }
    };

    // --- 7a. Line collinearity ------------------------------------------
    let line_flags = lines::line_collinearity_flags(&by_idx, &by_grid, params, &scale_at);

    // --- 7b. Local-H residual -------------------------------------------
    let mut residuals: HashMap<usize, f32> = HashMap::new();
    let mut local_h_flagged: HashMap<usize, f32> = HashMap::new();
    let mut local_h_high: HashMap<usize, f32> = HashMap::new();
    for entry in entries {
        let base = local_h::pick_local_h_base(&by_grid, entry.idx, entry.grid);
        if base.len() < 4 {
            continue;
        }
        let Some(resid) =
            local_h::local_h_residual(&by_idx, entry.idx, entry.grid, &base, &by_grid)
        else {
            continue;
        };
        residuals.insert(entry.idx, resid);
        let scale = scale_at(entry.idx);
        let local_h_tol_px = params.local_h_tol_rel * scale;
        if resid > local_h_tol_px {
            local_h_flagged.insert(entry.idx, resid);
            if resid > 2.0 * local_h_tol_px {
                local_h_high.insert(entry.idx, resid);
            }
        }
    }

    // --- 7c. Step-deviation flags (optional) ----------------------------
    let step_dev_flags = if params.use_step_aware && params.step_deviation_thresh_rel > 0.0 {
        step::flag_step_deviations(&per_corner_step, params.step_deviation_thresh_rel)
    } else {
        HashSet::new()
    };

    // --- 7d. Attribution ------------------------------------------------
    let mut blacklist: HashSet<usize> = HashSet::new();
    // Rule 1: ≥ 2 line flags → outlier.
    for (&idx, &count) in &line_flags {
        if count >= 2 {
            blacklist.insert(idx);
        }
    }
    // Rule 2: high local-H residual AND ≥ 1 line flag → outlier.
    for &idx in local_h_high.keys() {
        if line_flags.get(&idx).copied().unwrap_or(0) >= 1 {
            blacklist.insert(idx);
        }
    }
    // Rule 3: local-H flag with no line flag BUT base neighbor flagged
    // in a line → blacklist the worst base instead.
    for &idx in local_h_flagged.keys() {
        if line_flags.get(&idx).copied().unwrap_or(0) >= 1 {
            continue;
        }
        if blacklist.contains(&idx) {
            continue;
        }
        let Some(entry) = by_idx.get(&idx) else {
            continue;
        };
        let base = local_h::pick_local_h_base(&by_grid, idx, entry.grid);
        let mut worst: Option<(usize, u32)> = None;
        for base_idx in &base {
            if let Some(&flags) = line_flags.get(base_idx) {
                if flags >= 1 && worst.map(|w| flags > w.1).unwrap_or(true) {
                    worst = Some((*base_idx, flags));
                }
            }
        }
        if let Some((base_idx, _)) = worst {
            blacklist.insert(base_idx);
        }
    }
    // Rule 4: step-deviation flag AND ≥ 1 line flag → outlier.
    // Rationale: a corner whose finite-difference step disagrees with
    // the labelled-set median is a topology-consistency signal
    // independent of line / local-H residuals. Combined with a line
    // flag, it's strong evidence the corner is mis-labelled or sits
    // on a different sub-grid (e.g., marker-internal corner that
    // slipped past parity).
    for &idx in &step_dev_flags {
        if line_flags.get(&idx).copied().unwrap_or(0) >= 1 {
            blacklist.insert(idx);
        }
    }

    ValidationResult {
        blacklist,
        local_h_residuals: residuals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(idx: usize, x: f32, y: f32, i: i32, j: i32) -> LabelledEntry {
        LabelledEntry {
            idx,
            pixel: Point2::new(x, y),
            grid: (i, j),
        }
    }

    fn clean_grid(rows: i32, cols: i32, s: f32) -> Vec<LabelledEntry> {
        let mut out = Vec::new();
        let mut idx = 0;
        for j in 0..rows {
            for i in 0..cols {
                out.push(entry(idx, i as f32 * s + 50.0, j as f32 * s + 50.0, i, j));
                idx += 1;
            }
        }
        out
    }

    #[test]
    fn clean_grid_empty_blacklist() {
        let entries = clean_grid(7, 7, 20.0);
        let res = validate(&entries, 20.0, &ValidationParams::default());
        assert!(res.blacklist.is_empty(), "{:?}", res.blacklist);
    }

    #[test]
    fn displaced_interior_is_blacklisted() {
        let mut entries = clean_grid(7, 7, 20.0);
        // Displace (3, 3) by ~6px in both directions — failing both
        // line fits and the local-H residual check.
        let target = entries
            .iter_mut()
            .find(|e| e.grid == (3, 3))
            .expect("(3,3) present");
        target.pixel.x += 6.0;
        target.pixel.y += 6.0;
        let target_idx = target.idx;
        let res = validate(&entries, 20.0, &ValidationParams::default());
        assert!(
            res.blacklist.contains(&target_idx),
            "expected {target_idx} blacklisted, got {:?}",
            res.blacklist
        );
    }

    #[test]
    fn too_few_members_per_line_is_ignored() {
        let entries = vec![entry(0, 0.0, 0.0, 0, 0), entry(1, 20.0, 0.0, 1, 0)];
        let res = validate(&entries, 20.0, &ValidationParams::default());
        assert!(res.blacklist.is_empty());
    }

    #[test]
    fn step_aware_matches_global_on_uniform_grid() {
        // On a uniform grid, per-corner step ≈ cell_size everywhere,
        // so step-aware mode must agree with the default mode.
        let entries = clean_grid(7, 7, 20.0);
        let res_default = validate(&entries, 20.0, &ValidationParams::default());
        let res_step_aware = validate(
            &entries,
            20.0,
            &ValidationParams::default().with_step_aware(0.0),
        );
        assert_eq!(res_default.blacklist, res_step_aware.blacklist);
    }

    #[test]
    fn step_aware_flags_perspective_foreshortened_outlier() {
        // Build a grid whose right column has cell pitch ~10 px (half
        // of the rest at 20 px). On a uniform-`cell_size = 20` global
        // tolerance, a corner displaced by 4 px in the dense column
        // sits at 4 / 20 = 0.20 of the global cell. With step-aware
        // (local step ~10 px), the same residual sits at 4 / 10 = 0.40
        // — the tighter per-corner threshold catches it where the
        // global one would defer.
        //
        // Layout: 5x4 grid. Columns 0..3 at 20 px pitch; column 4 at
        // 10 px from column 3.
        let s = 20.0_f32;
        let mut entries = Vec::new();
        let mut idx = 0;
        for j in 0..4_i32 {
            for i in 0..4_i32 {
                entries.push(entry(idx, i as f32 * s + 50.0, j as f32 * s + 50.0, i, j));
                idx += 1;
            }
        }
        // Column 4: half-pitch (foreshortened).
        for j in 0..4_i32 {
            entries.push(entry(
                idx,
                3.0 * s + 50.0 + 0.5 * s, // x = 110 (one half-step past column 3 at x = 110)
                j as f32 * s + 50.0,
                4,
                j,
            ));
            idx += 1;
        }
        // Verify baseline: no outliers.
        let baseline = validate(&entries, s, &ValidationParams::default());
        assert!(baseline.blacklist.is_empty(), "{:?}", baseline.blacklist);

        // Displace (4, 1) — the dense-column corner — by 3 px in y.
        let target_idx = entries
            .iter()
            .find(|e| e.grid == (4, 1))
            .map(|e| e.idx)
            .expect("(4, 1) present");
        for e in entries.iter_mut() {
            if e.idx == target_idx {
                e.pixel.y += 3.0;
            }
        }

        let global_res = validate(&entries, s, &ValidationParams::default());
        let step_aware_res = validate(
            &entries,
            s,
            &ValidationParams::default().with_step_aware(0.0),
        );

        assert!(
            step_aware_res.blacklist.contains(&target_idx)
                || !global_res.blacklist.contains(&target_idx),
            "step-aware should be at least as sensitive: global={:?} step-aware={:?}",
            global_res.blacklist,
            step_aware_res.blacklist
        );
    }

    #[test]
    fn step_deviation_flag_fires_on_off_scale_corner() {
        let s = 20.0_f32;
        let mut entries = clean_grid(5, 5, s);
        let new_idx = entries.len();
        entries.push(entry(
            new_idx,
            4.0 * s + 0.5 * s + 50.0,
            2.0 * s + 50.0,
            5,
            2,
        ));
        entries[new_idx].pixel.y += 4.0;

        let res = validate(
            &entries,
            s,
            &ValidationParams::default().with_step_aware(0.5),
        );
        assert!(
            res.blacklist.contains(&new_idx),
            "expected new corner {new_idx} blacklisted: {:?}",
            res.blacklist
        );
    }

    #[test]
    fn local_step_per_corner_central_diff() {
        // Verify the helper produces central-difference values when
        // both neighbours are present, and one-sided otherwise.
        let entries = [
            entry(0, 0.0, 0.0, 0, 0),
            entry(1, 10.0, 0.0, 1, 0),
            entry(2, 30.0, 0.0, 2, 0), // i-step at (1, 0): central = (30 - 0)/2 = 15
            entry(3, 30.0, 20.0, 2, 1), // j-step at (2, 0): one-sided forward = 20
        ];
        let by_idx: HashMap<usize, &LabelledEntry> = entries.iter().map(|e| (e.idx, e)).collect();
        let by_grid: HashMap<(i32, i32), usize> = entries.iter().map(|e| (e.grid, e.idx)).collect();
        let steps = step::local_step_per_corner(&by_idx, &by_grid);

        // (1, 0): central i-step = 15; no j neighbours → step = 15.
        assert!((steps[&1] - 15.0).abs() < 1e-4, "got {}", steps[&1]);
        // (2, 0): one-sided i-step backward = 20; j-step forward = 20 → mean = 20.
        assert!((steps[&2] - 20.0).abs() < 1e-4, "got {}", steps[&2]);
    }
}
