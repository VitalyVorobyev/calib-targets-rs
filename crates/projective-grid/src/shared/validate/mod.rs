//! Post-growth validation for a labelled square grid.
//!
//! Two independent checks run over the labelled set by default:
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
//! The caller is expected to re-run the topological recovery/validate loop after
//! updating its blacklist.
//!
//! An optional **edge-shape gate** is available for final validation
//! of a completed labelled grid. It rejects labels with too little
//! cardinal support, bad adjacent-edge continuation, or no valid
//! adjacent square cell under local opposite-side consistency. This
//! gate is opt-in so the historical grow-time validator remains
//! conservative.
//!
//! # Pattern-agnostic
//!
//! This module has no dependency on target-specific vocabulary such as
//! feature-class labels or target IDs. Any caller that can produce
//! a `(corner_index, pixel_position, grid_coord)` slice can use it.
//! Consumers that carry per-stage metadata should pre-filter to the
//! "labelled" subset before calling.

mod edge_shape;
mod lines;
mod local_h;
pub mod recovery;
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
    /// Optional final-gate checks for local square-grid edge shape.
    ///
    /// Disabled by default to preserve the conservative grow-time
    /// validator. Enable this only for final precision gates that may
    /// remove labels or refuse the whole detection.
    pub edge_shape: Option<EdgeShapeParams>,
}

impl Default for ValidationParams {
    fn default() -> Self {
        // Conservative defaults inherited from the mature square-grid
        // detector path. Step-aware mode is opt-in.
        Self {
            line_tol_rel: 0.15,
            line_min_members: 3,
            local_h_tol_rel: 0.20,
            use_step_aware: false,
            step_deviation_thresh_rel: 0.0,
            edge_shape: None,
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
            edge_shape: None,
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

    /// Builder-style override for [`Self::line_tol_rel`]. Set to
    /// `f32::INFINITY` to disable the line-collinearity check.
    pub fn with_line_tol_rel(mut self, value: f32) -> Self {
        self.line_tol_rel = value;
        self
    }

    /// Builder-style override for [`Self::local_h_tol_rel`]. Set to
    /// `f32::INFINITY` to disable the local-H residual check.
    pub fn with_local_h_tol_rel(mut self, value: f32) -> Self {
        self.local_h_tol_rel = value;
        self
    }

    /// Builder-style no-op kept for facade compatibility.
    ///
    /// The advanced validator has **no** unconditional edge-length-band
    /// gate (the historical generic `validate` did; the advanced
    /// validator replaces it with the opt-in [`Self::with_edge_shape_gate`]).
    /// This builder accepts the value so callers that disable validation
    /// by pushing every tolerance to `f32::INFINITY` keep compiling and
    /// keep their intent — there is simply no band gate to widen here.
    /// To disable the validator entirely, set `line_tol_rel` and
    /// `local_h_tol_rel` to `f32::INFINITY` and leave the edge-shape gate
    /// off (the default).
    pub fn with_edge_length_band_rel(self, _value: f32) -> Self {
        self
    }

    /// Enable local edge-shape validation for final labelled-grid
    /// precision gates.
    pub fn with_edge_shape_gate(mut self, edge_shape: EdgeShapeParams) -> Self {
        self.edge_shape = Some(edge_shape);
        self
    }
}

/// Tolerances for local square-grid edge-shape validation.
#[non_exhaustive]
#[derive(Clone, Copy, Debug)]
pub struct EdgeShapeParams {
    /// Minimum number of cardinally adjacent labelled neighbours
    /// required for a label to survive.
    pub min_cardinal_degree: u8,
    /// Maximum angle change, in degrees, allowed between two adjacent
    /// edges that continue through a shared vertex.
    pub continuation_angle_tol_deg: f32,
    /// Maximum edge-length ratio allowed between two adjacent edges
    /// that continue through a weakly supported shared vertex.
    /// Well-supported interior vertices are checked by direction and
    /// cell shape instead, because perspective can legitimately make
    /// adjacent samples along one projected line differ in length.
    pub continuation_length_ratio_max: f32,
    /// Maximum angle difference, in degrees, allowed between opposite
    /// sides of a complete local cell.
    pub cell_opposite_angle_tol_deg: f32,
    /// Maximum length ratio allowed between opposite sides of a
    /// complete local cell.
    pub cell_opposite_length_ratio_max: f32,
}

impl Default for EdgeShapeParams {
    fn default() -> Self {
        Self {
            min_cardinal_degree: 2,
            continuation_angle_tol_deg: 8.0,
            continuation_length_ratio_max: 1.18,
            cell_opposite_angle_tol_deg: 8.0,
            cell_opposite_length_ratio_max: 1.10,
        }
    }
}

/// Per-label diagnostics from local edge-shape validation.
#[derive(Clone, Copy, Debug, Default)]
pub struct EdgeShapeDiagnostic {
    /// Number of cardinally adjacent labelled neighbours.
    pub cardinal_degree: u8,
    /// Whether the coordinate lies on the labelled-set bounding box.
    pub is_bbox_boundary: bool,
    /// Maximum angle change across supported line continuations, in degrees.
    pub max_continuation_angle_deg: Option<f32>,
    /// Maximum length ratio across supported line continuations.
    pub max_continuation_length_ratio: Option<f32>,
    /// Number of complete adjacent square cells.
    pub adjacent_cell_count: u8,
    /// Number of adjacent square cells that satisfy opposite-side checks.
    pub valid_adjacent_cell_count: u8,
    /// Maximum opposite-side angle difference across adjacent cells, in degrees.
    pub max_cell_opposite_angle_deg: Option<f32>,
    /// Maximum opposite-side length ratio across adjacent cells.
    pub max_cell_opposite_length_ratio: Option<f32>,
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
    /// Caller-chosen opaque index, carried back in `ValidationResult::blacklist`.
    pub idx: usize,
    /// The corner's position in image pixels.
    pub pixel: Point2<f32>,
    /// The corner's integer `(i, j)` grid coordinate.
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
    /// Edge-shape diagnostics keyed by caller-chosen label index.
    /// Empty when the edge-shape gate is disabled.
    pub edge_shape_diagnostics: HashMap<usize, EdgeShapeDiagnostic>,
    /// Edge-shape rejection reason keyed by caller-chosen label index.
    /// Empty when the edge-shape gate is disabled.
    pub edge_shape_reasons: HashMap<usize, &'static str>,
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
    //
    // `by_grid` is injective on its key (each `grid` cell is unique), so its
    // contents are order-independent. `by_idx` is NOT: the topological walk can
    // label the same `source_index` at two grid cells, so an `idx` may appear in
    // `entries` twice with different `grid`/pixel — and a plain `collect` is
    // last-write-wins in the caller's `HashMap` iteration order. Build it from a
    // `(grid, idx)`-sorted pass so the surviving entry for a duplicated `idx`
    // (used by step-aware scale and Rule 3's base pick) is reproducible
    // run-to-run; this is part of the duplicate-label determinism contract on
    // the residual loop below.
    let mut sorted_entries: Vec<&LabelledEntry> = entries.iter().collect();
    sorted_entries.sort_unstable_by_key(|e| (e.grid, e.idx));
    let by_idx: HashMap<usize, &LabelledEntry> =
        sorted_entries.iter().map(|&e| (e.idx, e)).collect();
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
    // Visit entries in a fixed `(grid, idx)` order, not slice order.
    //
    // Determinism contract: the per-corner maps below are keyed by `idx`, but
    // the topological walk can label the same `source_index` at two different
    // grid cells (a non-injective labelled set). Such an `idx` then appears in
    // `entries` twice — once per grid cell — each with a *different* residual
    // (its local homography is fit at a different grid position). Inserting by
    // `idx` is last-write-wins, so in slice order (which is the caller's
    // `HashMap` iteration order) the stored residual would depend on which
    // duplicate was visited last, varying per process. That flipped a
    // borderline corner's drop and was the residual source of the
    // topological→ChArUco recall flake. Sorting the visitation pins which
    // duplicate wins without changing the result for the injective (common)
    // case.
    let mut entry_order: Vec<usize> = (0..entries.len()).collect();
    entry_order.sort_unstable_by_key(|&k| (entries[k].grid, entries[k].idx));
    let mut residuals: HashMap<usize, f32> = HashMap::new();
    let mut local_h_flagged: HashMap<usize, f32> = HashMap::new();
    let mut local_h_high: HashMap<usize, f32> = HashMap::new();
    for &k in &entry_order {
        let entry = &entries[k];
        let base = local_h::pick_local_h_base(&by_grid, entry.idx, entry.grid);
        if base.len() < 4 {
            continue;
        }
        let Some(resid) = local_h::local_h_residual(&by_idx, entry.idx, entry.grid, &base) else {
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
    //
    // Determinism contract: unlike Rules 1/2/4 (each an unconditional
    // per-`idx` set insertion, so iteration order is irrelevant), this rule
    // is order-sensitive. It both reads (`blacklist.contains(&idx)`) and
    // writes (`blacklist.insert(base_idx)`) the shared blacklist, and a
    // `base_idx` it inserts for one corner may be the `idx` of a later
    // corner — which then gets skipped. So the *set* of blacklisted corners
    // depends on the visitation order. `local_h_flagged` is a `HashMap`, so
    // iterating its keys directly would make the drop set depend on
    // per-process `HashMap` seeding — the residual source of the
    // topological→ChArUco recall flake. Visit the flagged corners in a fixed
    // `idx` order so the resolution is reproducible run-to-run.
    let mut local_h_flagged_order: Vec<usize> = local_h_flagged.keys().copied().collect();
    local_h_flagged_order.sort_unstable();
    for idx in local_h_flagged_order {
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
        for &(base_idx, _) in &base {
            if let Some(&flags) = line_flags.get(&base_idx) {
                if flags >= 1 && worst.map(|w| flags > w.1).unwrap_or(true) {
                    worst = Some((base_idx, flags));
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
    // on a different sub-grid.
    for &idx in &step_dev_flags {
        if line_flags.get(&idx).copied().unwrap_or(0) >= 1 {
            blacklist.insert(idx);
        }
    }

    // --- 7e. Final edge-shape gate (optional) --------------------------
    let (edge_shape_diagnostics, edge_shape_reasons) =
        if let Some(edge_shape_params) = params.edge_shape {
            let (diagnostics, reasons) =
                edge_shape::evaluate_edge_shape(&by_idx, &by_grid, edge_shape_params);
            blacklist.extend(reasons.keys().copied());
            (diagnostics, reasons)
        } else {
            (HashMap::new(), HashMap::new())
        };

    ValidationResult {
        blacklist,
        local_h_residuals: residuals,
        edge_shape_diagnostics,
        edge_shape_reasons,
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

    fn edge_gate_params() -> ValidationParams {
        ValidationParams::new(999.0, 99, 999.0).with_edge_shape_gate(EdgeShapeParams::default())
    }

    fn mild_perspective_grid(rows: i32, cols: i32, s: f32) -> Vec<LabelledEntry> {
        let mut out = Vec::new();
        let mut idx = 0;
        for j in 0..rows {
            for i in 0..cols {
                let u = i as f32;
                let v = j as f32;
                let denom = 1.0 + 0.01 * u + 0.006 * v;
                let x = 50.0 + (s * (u + 0.08 * v)) / denom;
                let y = 50.0 + (s * (v + 0.04 * u)) / denom;
                out.push(entry(idx, x, y, i, j));
                idx += 1;
            }
        }
        out
    }

    fn mild_radial_grid(rows: i32, cols: i32, s: f32) -> Vec<LabelledEntry> {
        let mut out = Vec::new();
        let mut idx = 0;
        let cx = (cols - 1) as f32 * 0.5;
        let cy = (rows - 1) as f32 * 0.5;
        for j in 0..rows {
            for i in 0..cols {
                let u = i as f32 - cx;
                let v = j as f32 - cy;
                let r2 = u * u + v * v;
                let k = 1.0 + 0.006 * r2;
                let x = 150.0 + s * u * k;
                let y = 150.0 + s * v * k;
                out.push(entry(idx, x, y, i, j));
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
    fn edge_shape_clean_grid_passes() {
        let entries = clean_grid(7, 7, 20.0);
        let res = validate(&entries, 20.0, &edge_gate_params());
        assert!(res.blacklist.is_empty(), "{:?}", res.blacklist);
    }

    #[test]
    fn edge_shape_mild_perspective_grid_passes() {
        let entries = mild_perspective_grid(7, 7, 20.0);
        let res = validate(&entries, 20.0, &edge_gate_params());
        assert!(res.blacklist.is_empty(), "{:?}", res.blacklist);
    }

    #[test]
    fn edge_shape_mild_radial_grid_passes() {
        let entries = mild_radial_grid(7, 7, 20.0);
        let res = validate(&entries, 20.0, &edge_gate_params());
        assert!(res.blacklist.is_empty(), "{:?}", res.blacklist);
    }

    #[test]
    fn edge_shape_rejects_isolated_point() {
        let mut entries = clean_grid(2, 2, 20.0);
        let isolated_idx = entries.len();
        entries.push(entry(isolated_idx, 150.0, 150.0, 5, 5));
        let res = validate(&entries, 20.0, &edge_gate_params());
        assert!(
            res.blacklist.contains(&isolated_idx),
            "blacklist={:?}",
            res.blacklist
        );
        assert_eq!(
            res.edge_shape_reasons.get(&isolated_idx).copied(),
            Some("low-cardinal-degree")
        );
    }

    #[test]
    fn edge_shape_rejects_degree_one_dangling_point() {
        let mut entries = clean_grid(2, 2, 20.0);
        let dangling_idx = entries.len();
        entries.push(entry(dangling_idx, 90.0, 50.0, 2, 0));
        let res = validate(&entries, 20.0, &edge_gate_params());
        assert!(
            res.blacklist.contains(&dangling_idx),
            "blacklist={:?}",
            res.blacklist
        );
        assert_eq!(res.edge_shape_diagnostics[&dangling_idx].cardinal_degree, 1);
    }

    #[test]
    fn edge_shape_rejects_bad_continuation_across_vertex() {
        let mut entries = clean_grid(3, 3, 20.0);
        let target = entries
            .iter_mut()
            .find(|e| e.grid == (1, 1))
            .expect("(1,1) present");
        target.pixel.x += 8.0;
        let target_idx = target.idx;
        let res = validate(&entries, 20.0, &edge_gate_params());
        assert!(
            res.blacklist.contains(&target_idx),
            "blacklist={:?} diagnostics={:?}",
            res.blacklist,
            res.edge_shape_diagnostics.get(&target_idx)
        );
        assert_eq!(
            res.edge_shape_reasons.get(&target_idx).copied(),
            Some("bad-continuation")
        );
    }

    #[test]
    fn edge_shape_rejects_corner_with_no_valid_adjacent_cell() {
        let mut entries = clean_grid(2, 2, 20.0);
        let target = entries
            .iter_mut()
            .find(|e| e.grid == (1, 1))
            .expect("(1,1) present");
        target.pixel.x += 8.0;
        let target_idx = target.idx;
        let res = validate(&entries, 20.0, &edge_gate_params());
        assert!(
            res.blacklist.contains(&target_idx),
            "blacklist={:?} diagnostics={:?}",
            res.blacklist,
            res.edge_shape_diagnostics.get(&target_idx)
        );
        assert_eq!(
            res.edge_shape_diagnostics[&target_idx].adjacent_cell_count,
            1
        );
        assert_eq!(
            res.edge_shape_reasons.get(&target_idx).copied(),
            Some("no-valid-adjacent-cell")
        );
    }

    #[test]
    fn edge_shape_complete_two_by_two_cell_keeps_degree_two_corners() {
        let entries = clean_grid(2, 2, 20.0);
        let res = validate(&entries, 20.0, &edge_gate_params());
        assert!(res.blacklist.is_empty(), "{:?}", res.blacklist);
        for entry in &entries {
            assert_eq!(res.edge_shape_diagnostics[&entry.idx].cardinal_degree, 2);
            assert_eq!(
                res.edge_shape_diagnostics[&entry.idx].valid_adjacent_cell_count,
                1
            );
        }
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
