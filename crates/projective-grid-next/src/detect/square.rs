//! `detect_square_grid` and `detect_square_all`: composing seed → grow →
//! refine → merge → validate → cleanup into one user-facing call.
//!
//! Two algorithm strategies share the same return type:
//!
//! * [`DetectAlgorithm::SeedAndGrow`] — `find_quad → bfs_grow → refine → merge
//!   → validate → cleanup`. Battle-tested across all four target families;
//!   default.
//! * [`DetectAlgorithm::Topological`] — `build_grid_topological → refine →
//!   merge → validate → cleanup`. Image-free, faster on clean boards.
//!
//! Both terminate with the same precision gate ([`mod@crate::validate`]), so the
//! precision-by-construction contract holds regardless of strategy.
//!
//! `enable_refine`, `enable_merge`, `enable_validate`, `enable_cleanup` toggle
//! the post-grow stages. The defaults run the full pipeline.

use std::collections::HashMap;

use nalgebra::{Point2, Vector2};

use crate::diagnostics::DiagnosticSink;
use crate::error::DetectionError;
use crate::feature::Observation;
use crate::float::{lit, Float};
use crate::grow::{bfs_grow, GrowParams, GrowResult, SquareGrowContext};
use crate::lattice::{Coord, GridTransform, D4_TRANSFORMS};
use crate::merge::{
    merge_components_local, ComponentInput, MergeMode, MergeParams, MergedComponent,
};
use crate::policy::LabelPolicy;
use crate::refine::extend_global::{extend_via_global_homography, ExtensionParams};
use crate::refine::extend_local::{extend_via_local_homography, LocalExtensionParams};
use crate::refine::fill::{fill_grid_holes, FillParams};
use crate::seed::{find_quad, SeedQuadContext, SeedQuadParams};
use crate::topological::{build_grid_topological, TopologicalContext, TopologicalParams};
use crate::validate::{validate, LabelledEntry, ValidationParams};

/// Strategy selector for [`detect_square_grid`] / [`detect_square_all`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum DetectAlgorithm {
    /// Seed-and-grow BFS (`find_quad → bfs_grow → refine`). Default.
    #[default]
    SeedAndGrow,
    /// Topological pipeline (`build_grid_topological → refine`). Image-free
    /// and faster on clean boards; the topological cell-test currently
    /// regresses recall on ChArUco-style imagery — see
    /// `docs/projective_grid_overview.md` Gap 10.
    Topological,
}

/// Tuning knobs for [`detect_square_grid`] / [`detect_square_all`].
///
/// Composed of the per-stage parameter structs plus six toggles for the
/// post-grow stages. Field-by-field defaults are correct for zero-config
/// detection on a synthetic axis-aligned grid via the open
/// [`crate::grow::OpenContext`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct DetectParams<F: Float> {
    /// Which strategy to run.
    pub algorithm: DetectAlgorithm,
    /// Seed-quad finder tunables (`SeedAndGrow` only).
    pub seed: SeedQuadParams<F>,
    /// BFS grow tunables (`SeedAndGrow` only).
    pub grow: GrowParams<F>,
    /// Topological pipeline tunables (`Topological` only).
    pub topological: TopologicalParams<F>,
    /// Global-homography boundary extender tunables.
    pub extend_global: ExtensionParams<F>,
    /// Local-homography boundary extender tunables.
    pub extend_local: LocalExtensionParams<F>,
    /// Interior hole-fill tunables.
    pub fill: FillParams<F>,
    /// Component-merge tunables. Default mode is
    /// [`MergeMode::OverlapOnly`]; switch to
    /// [`MergeMode::OverlapAndPredicted`] to bridge disjoint components.
    pub merge: MergeParams<F>,
    /// Validation gate tunables.
    pub validate: ValidationParams<F>,
    /// Run the refine stages (extend-global → extend-local → fill).
    /// Default `true`.
    pub enable_refine: bool,
    /// Run component merge. Default `true`. When `false`, [`detect_square_grid`]
    /// still picks the single largest grown component.
    pub enable_merge: bool,
    /// Run the precision-gate validation pass. Default `true`.
    pub enable_validate: bool,
    /// Apply top-left canonicalisation (the D4 transform that puts `+i`
    /// pointing right and `+j` pointing down in pixel space). Default `true`.
    pub enable_cleanup: bool,
}

impl<F: Float> Default for DetectParams<F> {
    fn default() -> Self {
        Self {
            algorithm: DetectAlgorithm::default(),
            seed: SeedQuadParams::default(),
            grow: GrowParams::default(),
            topological: TopologicalParams::default(),
            extend_global: ExtensionParams::default(),
            extend_local: LocalExtensionParams::default(),
            fill: FillParams::default(),
            merge: MergeParams::default(),
            validate: ValidationParams::default(),
            enable_refine: true,
            enable_merge: true,
            enable_validate: true,
            enable_cleanup: true,
        }
    }
}

impl<F: Float> DetectParams<F> {
    /// Construct fully-default detection params for the chosen strategy.
    pub fn new(algorithm: DetectAlgorithm) -> Self {
        Self {
            algorithm,
            ..Self::default()
        }
    }

    /// Toggle the refine stage.
    #[must_use]
    pub fn with_refine(mut self, on: bool) -> Self {
        self.enable_refine = on;
        self
    }

    /// Toggle the merge stage.
    #[must_use]
    pub fn with_merge(mut self, on: bool) -> Self {
        self.enable_merge = on;
        self
    }

    /// Toggle the validate stage.
    #[must_use]
    pub fn with_validate(mut self, on: bool) -> Self {
        self.enable_validate = on;
        self
    }

    /// Toggle the top-left canonicalisation cleanup pass.
    #[must_use]
    pub fn with_cleanup(mut self, on: bool) -> Self {
        self.enable_cleanup = on;
        self
    }
}

/// One detected square grid component.
///
/// Carries the labelled `(i, j) → observation_idx` map (rebased so the
/// bbox-min is `(0, 0)`), the inferred cell-size, the mean per-step pixel
/// displacement for the two grid axes (useful for downstream rectification),
/// the component bounding box, and counts that report how many components the
/// pipeline saw and how many entries the validation gate dropped.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GridDetection<F: Float> {
    /// `(i, j) → observation_idx` map, rebased so the bbox minimum is
    /// `(0, 0)`.
    pub labelled: HashMap<Coord, usize>,
    /// Per-image cell size in pixels.
    pub cell_size: F,
    /// Mean pixel displacement for a `+i` step.
    pub axis_i: Vector2<F>,
    /// Mean pixel displacement for a `+j` step.
    pub axis_j: Vector2<F>,
    /// Inclusive bounding box `((min_i, min_j), (max_i, max_j))` after
    /// rebasing.
    pub bbox: (Coord, Coord),
    /// Number of grown components observed before merging.
    pub components_found: usize,
    /// Number of labelled entries the validation gate dropped.
    pub dropped_by_validation: usize,
}

impl<F: Float> GridDetection<F> {
    /// Construct a grid detection from its constituent fields.
    pub fn new(
        labelled: HashMap<Coord, usize>,
        cell_size: F,
        axis_i: Vector2<F>,
        axis_j: Vector2<F>,
        bbox: (Coord, Coord),
        components_found: usize,
        dropped_by_validation: usize,
    ) -> Self {
        Self {
            labelled,
            cell_size,
            axis_i,
            axis_j,
            bbox,
            components_found,
            dropped_by_validation,
        }
    }
}

/// Run the full square-grid detection pipeline and return the **largest**
/// detected component.
///
/// The pipeline composition depends on [`DetectParams::algorithm`]:
///
/// * [`DetectAlgorithm::SeedAndGrow`] — `find_quad → bfs_grow → refine →
///   merge → validate → cleanup`.
/// * [`DetectAlgorithm::Topological`] — `build_grid_topological → refine →
///   merge → validate → cleanup`.
///
/// Per-stage toggles in [`DetectParams`] skip individual phases. When
/// `enable_merge` is `false` and the underlying algorithm produced multiple
/// components, this function returns only the single largest one — use
/// [`detect_square_all`] to retrieve every surviving component.
///
/// # Errors
///
/// * [`DetectionError::NoSeedFound`] — the seed-and-grow path could not
///   produce a quad.
/// * [`DetectionError::NoComponentSatisfiesPolicy`] — the topological path
///   produced no labelled components that survive validation.
/// * Any [`DetectionError`] propagated from the underlying algorithm.
pub fn detect_square_grid<F, C, S>(
    observations: &[Observation<F>],
    policy: &LabelPolicy<F>,
    ctx: &C,
    params: &DetectParams<F>,
    sink: &mut S,
) -> Result<GridDetection<F>, DetectionError>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + SeedQuadContext<F> + TopologicalContext<F>,
    S: DiagnosticSink<F>,
{
    let mut all = detect_square_all(observations, policy, ctx, params, sink)?;
    if all.is_empty() {
        return Err(DetectionError::NoComponentSatisfiesPolicy);
    }
    // `detect_square_all` already sorts by descending label count.
    Ok(all.remove(0))
}

/// Same pipeline as [`detect_square_grid`] but returns **every** surviving
/// component (sorted by descending label count).
///
/// Useful when the caller knows their imagery contains multiple disjoint
/// boards, or when the topological pipeline naturally produces several
/// components. With `enable_merge = false` no merging is attempted and the
/// returned vector is one entry per grown component; with `enable_merge =
/// true` (default) the orchestrator collapses overlapping or — under
/// [`MergeMode::OverlapAndPredicted`] — extrapolated-overlap components.
pub fn detect_square_all<F, C, S>(
    observations: &[Observation<F>],
    policy: &LabelPolicy<F>,
    ctx: &C,
    params: &DetectParams<F>,
    sink: &mut S,
) -> Result<Vec<GridDetection<F>>, DetectionError>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + SeedQuadContext<F> + TopologicalContext<F>,
    S: DiagnosticSink<F>,
{
    // ---- Stage 1: produce one or more `GrowResult` components ----
    let mut components = match params.algorithm {
        DetectAlgorithm::SeedAndGrow => vec![run_seed_and_grow(observations, ctx, params, sink)?],
        DetectAlgorithm::Topological => run_topological(observations, ctx, params, sink)?,
    };
    let components_found = components.len();

    // ---- Stage 2: refine each component independently ----
    if params.enable_refine {
        for grow in components.iter_mut() {
            run_refine(observations, grow, ctx, params, sink);
        }
    }

    // ---- Stage 3: merge ----
    // Always go through the merger so the data shape collapses to
    // `MergedComponent<F>` even when the merge itself is a no-op (single
    // component, or `enable_merge = false` which forces `MergeMode::OverlapOnly`
    // with `min_overlap = usize::MAX` — a guaranteed-no-merge configuration).
    let merge_results = run_merge(observations, &components, params, sink)?;

    // ---- Stage 4: validate, cleanup, build `GridDetection` ----
    let mut out: Vec<GridDetection<F>> = Vec::with_capacity(merge_results.len());
    for merged in merge_results {
        let labelled = merged_to_labelled(observations, &merged);
        // Per-component cell size carried by the merger; fall back to a
        // per-component edge-length estimate when the merger could not
        // produce one (degenerate single-cell components).
        let cell_size = if merged.cell_size > F::zero() {
            merged.cell_size
        } else {
            estimate_cell_size_from_labels(observations, &labelled)
        };
        let (labelled, dropped) = if params.enable_validate {
            run_validate(observations, labelled, cell_size, policy, params, sink)
        } else {
            (labelled, 0)
        };
        if labelled.is_empty() {
            continue;
        }
        let labelled = if params.enable_cleanup {
            canonicalize_top_left(labelled, observations)
        } else {
            labelled
        };
        let (axis_i, axis_j) = mean_axis_steps(observations, &labelled);
        let bbox = compute_bbox(&labelled);
        out.push(GridDetection::new(
            labelled,
            cell_size,
            axis_i,
            axis_j,
            bbox,
            components_found,
            dropped,
        ));
    }
    out.sort_by_key(|g| std::cmp::Reverse(g.labelled.len()));
    Ok(out)
}

// ---- Stage 1 helpers ----

fn run_seed_and_grow<F, C, S>(
    observations: &[Observation<F>],
    ctx: &C,
    params: &DetectParams<F>,
    sink: &mut S,
) -> Result<GrowResult<F>, DetectionError>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + SeedQuadContext<F> + ?Sized,
    S: DiagnosticSink<F>,
{
    let seed =
        find_quad(observations, &params.seed, ctx, sink).ok_or(DetectionError::NoSeedFound)?;
    let grow = bfs_grow(observations, &seed, &params.grow, ctx, sink);
    Ok(grow)
}

fn run_topological<F, C, S>(
    observations: &[Observation<F>],
    ctx: &C,
    params: &DetectParams<F>,
    sink: &mut S,
) -> Result<Vec<GrowResult<F>>, DetectionError>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: TopologicalContext<F>,
    S: DiagnosticSink<F>,
{
    let grid = build_grid_topological(observations, &params.topological, ctx, sink)?;
    let mut out: Vec<GrowResult<F>> = Vec::with_capacity(grid.components.len());
    for comp in grid.components {
        let labelled = comp.labelled;
        if labelled.is_empty() {
            continue;
        }
        let cell_size = estimate_cell_size_from_labels(observations, &labelled);
        let bbox = comp.bbox;
        // `n_attached` is best-effort here — the topological walker doesn't
        // distinguish "seed" from "grown" the way BFS does. Counting all
        // labelled cells minus the four-corner conceptual seed reads as the
        // closest analogue.
        let n_attached = labelled.len().saturating_sub(4);
        out.push(GrowResult {
            labelled,
            cell_size,
            bbox,
            n_attached,
            n_rejected: 0,
        });
    }
    Ok(out)
}

// ---- Stage 2 helper ----

fn run_refine<F, C, S>(
    observations: &[Observation<F>],
    grow: &mut GrowResult<F>,
    ctx: &C,
    params: &DetectParams<F>,
    sink: &mut S,
) where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
    S: DiagnosticSink<F>,
{
    let _ = extend_via_global_homography(observations, grow, &params.extend_global, ctx, sink);
    let _ = extend_via_local_homography(observations, grow, &params.extend_local, ctx, sink);
    let _ = fill_grid_holes(observations, grow, &params.fill, ctx, sink);
}

// ---- Stage 3 helper ----

fn run_merge<F, S>(
    observations: &[Observation<F>],
    components: &[GrowResult<F>],
    params: &DetectParams<F>,
    sink: &mut S,
) -> Result<Vec<MergedComponent<F>>, DetectionError>
where
    F: Float + kiddo::float::kdtree::Axis,
    S: DiagnosticSink<F>,
{
    if components.is_empty() {
        return Ok(Vec::new());
    }
    let positions: Vec<Point2<F>> = observations.iter().map(|o| o.position).collect();
    // Build `ComponentInput` views over the (shared) observation positions.
    let views: Vec<ComponentInput<'_, F>> = components
        .iter()
        .map(|g| ComponentInput::new(&positions, g.labelled.clone(), g.cell_size))
        .collect();

    // When merge is disabled, configure a guaranteed-no-merge pass so the
    // orchestrator still produces a `MergedComponent` per input — keeping
    // the rest of the pipeline shape agnostic to the toggle.
    let merge_params = if params.enable_merge {
        params.merge
    } else {
        MergeParams {
            min_overlap: usize::MAX,
            mode: MergeMode::OverlapOnly,
            ..params.merge
        }
    };

    let report = merge_components_local(&views, &merge_params, sink)
        .map_err(|e| DetectionError::InconsistentInput(e.to_string()))?;
    Ok(report.merged_components)
}

// ---- Stage 4 helpers ----

fn merged_to_labelled<F: Float>(
    observations: &[Observation<F>],
    merged: &MergedComponent<F>,
) -> HashMap<Coord, usize> {
    // The merger returns `(Coord → Point2<F>)` (positions in the merged
    // frame). Map each merged label back to its closest observation index so
    // downstream callers can recover the original feature payload.
    let mut out: HashMap<Coord, usize> = HashMap::with_capacity(merged.labels.len());
    for (&coord, &pos) in &merged.labels {
        let mut best: Option<(usize, F)> = None;
        for (idx, obs) in observations.iter().enumerate() {
            let dx = obs.position.x - pos.x;
            let dy = obs.position.y - pos.y;
            let d2 = dx * dx + dy * dy;
            if best.map(|(_, bd)| d2 < bd).unwrap_or(true) {
                best = Some((idx, d2));
            }
        }
        if let Some((idx, _)) = best {
            out.insert(coord, idx);
        }
    }
    out
}

fn run_validate<F, S>(
    observations: &[Observation<F>],
    labelled: HashMap<Coord, usize>,
    cell_size: F,
    policy: &LabelPolicy<F>,
    params: &DetectParams<F>,
    sink: &mut S,
) -> (HashMap<Coord, usize>, usize)
where
    F: Float,
    S: DiagnosticSink<F>,
{
    let entries: Vec<LabelledEntry<F>> = labelled
        .iter()
        .map(|(&coord, &idx)| LabelledEntry::new(idx, observations[idx].position, coord))
        .collect();
    let result = validate(
        &entries,
        observations,
        cell_size,
        policy,
        &params.validate,
        sink,
    );
    let blacklist = result.blacklist;
    let kept: HashMap<Coord, usize> = labelled
        .into_iter()
        .filter(|(_, idx)| !blacklist.contains(idx))
        .collect();
    let dropped = blacklist.len();
    (kept, dropped)
}

fn estimate_cell_size_from_labels<F: Float>(
    observations: &[Observation<F>],
    labelled: &HashMap<Coord, usize>,
) -> F {
    let mut sum = F::zero();
    let mut count: u32 = 0;
    for (&(i, j), &idx) in labelled {
        let p = observations[idx].position;
        for (di, dj) in [(1, 0), (0, 1)] {
            if let Some(&n_idx) = labelled.get(&(i + di, j + dj)) {
                let q = observations[n_idx].position;
                let dx = q.x - p.x;
                let dy = q.y - p.y;
                let d = (dx * dx + dy * dy).sqrt();
                if d > F::zero() && d.is_finite() {
                    sum += d;
                    count += 1;
                }
            }
        }
    }
    if count == 0 {
        // Fall back to a non-zero sentinel so downstream relative-tolerance
        // computations don't divide by zero.
        return lit::<F>(1.0_f32);
    }
    sum / lit::<F>(count as f32)
}

fn mean_axis_steps<F: Float>(
    observations: &[Observation<F>],
    labelled: &HashMap<Coord, usize>,
) -> (Vector2<F>, Vector2<F>) {
    let mut sum_i = Vector2::new(F::zero(), F::zero());
    let mut sum_j = Vector2::new(F::zero(), F::zero());
    let mut n_i: u32 = 0;
    let mut n_j: u32 = 0;
    for (&(i, j), &idx) in labelled {
        let p = observations[idx].position;
        if let Some(&n_idx) = labelled.get(&(i + 1, j)) {
            let q = observations[n_idx].position;
            sum_i += Vector2::new(q.x - p.x, q.y - p.y);
            n_i += 1;
        }
        if let Some(&n_idx) = labelled.get(&(i, j + 1)) {
            let q = observations[n_idx].position;
            sum_j += Vector2::new(q.x - p.x, q.y - p.y);
            n_j += 1;
        }
    }
    let axis_i = if n_i > 0 {
        sum_i / lit::<F>(n_i as f32)
    } else {
        Vector2::new(F::zero(), F::zero())
    };
    let axis_j = if n_j > 0 {
        sum_j / lit::<F>(n_j as f32)
    } else {
        Vector2::new(F::zero(), F::zero())
    };
    (axis_i, axis_j)
}

fn compute_bbox(labelled: &HashMap<Coord, usize>) -> (Coord, Coord) {
    if labelled.is_empty() {
        return ((0, 0), (0, 0));
    }
    let (mut min_i, mut min_j) = (i32::MAX, i32::MAX);
    let (mut max_i, mut max_j) = (i32::MIN, i32::MIN);
    for &(i, j) in labelled.keys() {
        min_i = min_i.min(i);
        min_j = min_j.min(j);
        max_i = max_i.max(i);
        max_j = max_j.max(j);
    }
    ((min_i, min_j), (max_i, max_j))
}

fn rebase_to_origin(labelled: HashMap<Coord, usize>) -> HashMap<Coord, usize> {
    if labelled.is_empty() {
        return labelled;
    }
    let (min_i, min_j) = labelled
        .keys()
        .fold((i32::MAX, i32::MAX), |(a, b), &(i, j)| (a.min(i), b.min(j)));
    if min_i == 0 && min_j == 0 {
        return labelled;
    }
    labelled
        .into_iter()
        .map(|((i, j), idx)| ((i - min_i, j - min_j), idx))
        .collect()
}

/// Canonicalise a labelled grid so `+i` points right and `+j` points down in
/// pixel space. Ports the legacy
/// `projective_grid::square::cleanup::canonicalize_top_left` algorithm to
/// `F: Float`.
fn canonicalize_top_left<F: Float>(
    labelled: HashMap<Coord, usize>,
    observations: &[Observation<F>],
) -> HashMap<Coord, usize> {
    if labelled.is_empty() {
        return labelled;
    }
    let transform = top_left_transform(&labelled, observations);
    let transformed: HashMap<Coord, usize> = labelled
        .into_iter()
        .map(|(coord, idx)| (transform.apply(coord), idx))
        .collect();
    rebase_to_origin(transformed)
}

fn top_left_transform<F: Float>(
    labelled: &HashMap<Coord, usize>,
    observations: &[Observation<F>],
) -> GridTransform {
    // Mean pixel displacement per +i step and per +j step.
    let (mut di_x, mut di_y, mut di_n) = (F::zero(), F::zero(), 0_u32);
    let (mut dj_x, mut dj_y, mut dj_n) = (F::zero(), F::zero(), 0_u32);
    for (&(i, j), &idx) in labelled {
        let p = observations[idx].position;
        if let Some(&n) = labelled.get(&(i + 1, j)) {
            let q = observations[n].position;
            di_x += q.x - p.x;
            di_y += q.y - p.y;
            di_n += 1;
        }
        if let Some(&n) = labelled.get(&(i, j + 1)) {
            let q = observations[n].position;
            dj_x += q.x - p.x;
            dj_y += q.y - p.y;
            dj_n += 1;
        }
    }
    if di_n == 0 || dj_n == 0 {
        return D4_TRANSFORMS[0];
    }
    let u_x = di_x / lit::<F>(di_n as f32);
    let u_y = di_y / lit::<F>(di_n as f32);
    let v_x = dj_x / lit::<F>(dj_n as f32);
    let v_y = dj_y / lit::<F>(dj_n as f32);

    // For each D4 transform T, score how well T's inverse maps `+i` to `+x`
    // and `+j` to `+y`. For D4 matrices the inverse is the transpose.
    let mut best: Option<(F, GridTransform)> = None;
    for t in &D4_TRANSFORMS {
        let m = t.matrix;
        // `inv-T` applied to grid (1, 0) and (0, 1):
        //   inv * (1, 0)^T = (m[0][0], m[1][0])
        //   inv * (0, 1)^T = (m[0][1], m[1][1])
        let gi_i = m[0][0];
        let gi_j = m[1][0];
        let gj_i = m[0][1];
        let gj_j = m[1][1];
        let new_i_x = lit::<F>(gi_i as f32) * u_x + lit::<F>(gi_j as f32) * v_x;
        let new_j_y = lit::<F>(gj_i as f32) * u_y + lit::<F>(gj_j as f32) * v_y;
        let score = new_i_x + new_j_y;
        if best.map(|(b, _)| score > b).unwrap_or(true) {
            best = Some((score, *t));
        }
    }
    best.map(|(_, t)| t).unwrap_or(D4_TRANSFORMS[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::NoOpSink;
    use crate::grow::OpenContext as GrowOpenContext;

    fn axis_aligned_grid<F>(rows: i32, cols: i32, s: F, ox: F, oy: F) -> Vec<Observation<F>>
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let mut out = Vec::with_capacity((rows * cols) as usize);
        for j in 0..rows {
            for i in 0..cols {
                let x = lit::<F>(i as f32) * s + ox;
                let y = lit::<F>(j as f32) * s + oy;
                out.push(Observation::new(Point2::new(x, y)));
            }
        }
        out
    }

    fn assert_default_strategy_is_seed_and_grow<F: Float>() {
        let p: DetectParams<F> = DetectParams::default();
        assert_eq!(p.algorithm, DetectAlgorithm::SeedAndGrow);
        assert!(p.enable_refine);
        assert!(p.enable_merge);
        assert!(p.enable_validate);
        assert!(p.enable_cleanup);
    }

    fn assert_zero_config_detects_clean_5x5<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let s = lit::<F>(25.0_f32);
        let obs = axis_aligned_grid::<F>(5, 5, s, lit::<F>(50.0_f32), lit::<F>(50.0_f32));
        let ctx = GrowOpenContext::<F>::new(obs.len());
        let policy = LabelPolicy::<F>::builder(obs.len()).build();
        let params = DetectParams::default();
        let mut sink = NoOpSink;
        let det = detect_square_grid(&obs, &policy, &ctx, &params, &mut sink).unwrap();
        assert_eq!(det.labelled.len(), 25);
        assert_eq!(det.bbox, ((0, 0), (4, 4)));
    }

    #[test]
    fn default_strategy_is_seed_and_grow_f32() {
        assert_default_strategy_is_seed_and_grow::<f32>();
    }
    #[test]
    fn default_strategy_is_seed_and_grow_f64() {
        assert_default_strategy_is_seed_and_grow::<f64>();
    }
    #[test]
    fn zero_config_detects_clean_5x5_f32() {
        assert_zero_config_detects_clean_5x5::<f32>();
    }
    #[test]
    fn zero_config_detects_clean_5x5_f64() {
        assert_zero_config_detects_clean_5x5::<f64>();
    }
}
