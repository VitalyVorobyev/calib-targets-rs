//! `refine_grid`: post-grow task facade.
//!
//! Takes existing [`GrowResult`] components — produced by an external seed /
//! grow stage, or by an in-process call to [`crate::bfs_grow`] /
//! [`crate::build_grid_topological`] — and runs the post-grow stages
//! (refine → merge → validate → cleanup) over them.
//!
//! Use case: a consumer that has its own seed / grow path during migration
//! away from the legacy crate, and wants the refine + merge + validate +
//! cleanup composition without going through [`crate::detect_square_grid`].
//! See `docs/projective-grid-rewrite.md` Phase 5.

use std::collections::HashMap;

use nalgebra::{Point2, Vector2};

use crate::detect::square::GridDetection;
use crate::diagnostics::DiagnosticSink;
use crate::error::DetectionError;
use crate::feature::Observation;
use crate::float::{lit, Float};
use crate::grow::{GrowResult, SquareGrowContext};
use crate::lattice::{Coord, GridTransform, D4_TRANSFORMS};
use crate::merge::{
    merge_components_local, ComponentInput, MergeMode, MergeParams, MergedComponent,
};
use crate::policy::LabelPolicy;
use crate::refine::extend_global::{extend_via_global_homography, ExtensionParams};
use crate::refine::extend_local::{extend_via_local_homography, LocalExtensionParams};
use crate::refine::fill::{fill_grid_holes, FillParams};
use crate::validate::{validate, LabelledEntry, ValidationParams};

/// Tuning knobs for [`refine_grid`].
///
/// Splits per-stage parameters out into individual fields so callers can
/// substitute one extender without re-tuning the others. The toggles mirror
/// [`crate::detect::square::DetectParams`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct RefineParams<F: Float> {
    /// Global-homography boundary extender tunables.
    pub extend_global: ExtensionParams<F>,
    /// Local-homography boundary extender tunables.
    pub extend_local: LocalExtensionParams<F>,
    /// Interior hole-fill tunables.
    pub fill: FillParams<F>,
    /// Component-merge tunables.
    pub merge: MergeParams<F>,
    /// Precision-gate tunables.
    pub validate: ValidationParams<F>,
    /// Run the global-homography boundary extender. Default `true`.
    pub enable_extend_global: bool,
    /// Run the local-homography boundary extender. Default `true`.
    pub enable_extend_local: bool,
    /// Run the interior hole-fill stage. Default `true`.
    pub enable_fill: bool,
    /// Run component merge. Default `true`. Disabled merging yields one
    /// `GridDetection` per input component.
    pub enable_merge: bool,
    /// Run the validation gate. Default `true`.
    pub enable_validate: bool,
}

impl<F: Float> Default for RefineParams<F> {
    fn default() -> Self {
        Self {
            extend_global: ExtensionParams::default(),
            extend_local: LocalExtensionParams::default(),
            fill: FillParams::default(),
            merge: MergeParams::default(),
            validate: ValidationParams::default(),
            enable_extend_global: true,
            enable_extend_local: true,
            enable_fill: true,
            enable_merge: true,
            enable_validate: true,
        }
    }
}

impl<F: Float> RefineParams<F> {
    /// Construct fully-default refine params.
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle the global-homography extender.
    #[must_use]
    pub fn with_extend_global(mut self, on: bool) -> Self {
        self.enable_extend_global = on;
        self
    }

    /// Toggle the local-homography extender.
    #[must_use]
    pub fn with_extend_local(mut self, on: bool) -> Self {
        self.enable_extend_local = on;
        self
    }

    /// Toggle interior hole-fill.
    #[must_use]
    pub fn with_fill(mut self, on: bool) -> Self {
        self.enable_fill = on;
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

    /// Replace the merge parameters wholesale (useful to switch into
    /// `MergeMode::OverlapAndPredicted`).
    #[must_use]
    pub fn with_merge_params(mut self, merge: MergeParams<F>) -> Self {
        self.merge = merge;
        self
    }

    /// Replace the validation parameters wholesale.
    #[must_use]
    pub fn with_validate_params(mut self, validate: ValidationParams<F>) -> Self {
        self.validate = validate;
        self
    }
}

/// Run refine → merge → validate → cleanup over `components`.
///
/// `components` may carry one entry (one grown grid) or several disjoint
/// patches that the merger should attempt to join. The returned vector is
/// sorted by descending label count.
///
/// # Errors
///
/// * [`DetectionError::InconsistentInput`] when the merger's symmetry table
///   is mismatched with the lattice (propagated from
///   [`crate::merge::merge_components_local`]).
pub fn refine_grid<F, C, S>(
    observations: &[Observation<F>],
    mut components: Vec<GrowResult<F>>,
    policy: &LabelPolicy<F>,
    ctx: &C,
    params: &RefineParams<F>,
    sink: &mut S,
) -> Result<Vec<GridDetection<F>>, DetectionError>
where
    F: Float + kiddo::float::kdtree::Axis,
    C: SquareGrowContext<F> + ?Sized,
    S: DiagnosticSink<F>,
{
    let components_found = components.len();

    // Refine each component independently.
    for grow in components.iter_mut() {
        if params.enable_extend_global {
            let _ =
                extend_via_global_homography(observations, grow, &params.extend_global, ctx, sink);
        }
        if params.enable_extend_local {
            let _ =
                extend_via_local_homography(observations, grow, &params.extend_local, ctx, sink);
        }
        if params.enable_fill {
            let _ = fill_grid_holes(observations, grow, &params.fill, ctx, sink);
        }
    }

    // Merge (or trivially passthrough).
    let merge_results = run_merge(observations, &components, params, sink)?;

    let mut out: Vec<GridDetection<F>> = Vec::with_capacity(merge_results.len());
    for merged in merge_results {
        let labelled = merged_to_labelled(observations, &merged);
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
        let labelled = canonicalize_top_left(labelled, observations);
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

// ---- Private helpers ----
//
// Duplicated from `detect/square.rs` rather than re-exported because they are
// implementation details. Both functions consume the same `MergedComponent`
// shape and produce the same `GridDetection` shape, but they have different
// upstreams (detect runs seed/grow first; refine_grid takes the components
// directly). Keeping the helpers private to each task module avoids a
// premature cross-module abstraction.

fn run_merge<F, S>(
    observations: &[Observation<F>],
    components: &[GrowResult<F>],
    params: &RefineParams<F>,
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
    let views: Vec<ComponentInput<'_, F>> = components
        .iter()
        .map(|g| ComponentInput::new(&positions, g.labelled.clone(), g.cell_size))
        .collect();
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

fn merged_to_labelled<F: Float>(
    observations: &[Observation<F>],
    merged: &MergedComponent<F>,
) -> HashMap<Coord, usize> {
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
    params: &RefineParams<F>,
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

    let mut best: Option<(F, GridTransform)> = None;
    for t in &D4_TRANSFORMS {
        let m = t.matrix;
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
    use crate::grow::OpenContext;

    fn axis_aligned_grid<F: Float + kiddo::float::kdtree::Axis>(
        rows: i32,
        cols: i32,
        s: F,
        ox: F,
        oy: F,
    ) -> Vec<Observation<F>> {
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

    fn assert_refine_single_clean_component_passes_through<F>()
    where
        F: Float + kiddo::float::kdtree::Axis,
    {
        let s = lit::<F>(20.0_f32);
        let obs = axis_aligned_grid::<F>(4, 4, s, lit::<F>(50.0_f32), lit::<F>(50.0_f32));
        let mut labels: HashMap<Coord, usize> = HashMap::new();
        for j in 0..4_i32 {
            for i in 0..4_i32 {
                labels.insert((i, j), (j * 4 + i) as usize);
            }
        }
        let grow = GrowResult {
            labelled: labels,
            cell_size: s,
            bbox: ((0, 0), (3, 3)),
            n_attached: 12,
            n_rejected: 0,
        };
        let policy = LabelPolicy::<F>::builder(obs.len()).build();
        let ctx = OpenContext::<F>::new(obs.len());
        let mut sink = NoOpSink;
        let res = refine_grid(
            &obs,
            vec![grow],
            &policy,
            &ctx,
            &RefineParams::default(),
            &mut sink,
        )
        .unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].labelled.len(), 16);
        assert_eq!(res[0].bbox, ((0, 0), (3, 3)));
    }

    #[test]
    fn refine_single_clean_component_passes_through_f32() {
        assert_refine_single_clean_component_passes_through::<f32>();
    }
    #[test]
    fn refine_single_clean_component_passes_through_f64() {
        assert_refine_single_clean_component_passes_through::<f64>();
    }
}
