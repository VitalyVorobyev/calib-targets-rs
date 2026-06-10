//! `(LatticeKind::Square, Evidence::Oriented2)` seed-and-grow wiring on
//! top of the advanced square engine.
//!
//! Pipeline (each stage is one bullet to keep the rustdoc list flat):
//!
//! - **build_components** — repeatedly run the advanced seed-quad finder + BFS
//!   grow over the still-unlabelled corners, collecting one `(i, j) →
//!   corner_idx` map per closed seed until no seed closes (returns at least one
//!   component).
//! - **merge** — `merge_components_local` reunites components in label space
//!   using local geometry only (radial-distortion safe).
//! - **validate** — the advanced post-grow validator (line collinearity +
//!   local-H) drops blacklisted corners per merged component.
//! - **fit** — the shared `fit_component` back-half (in `shared::fit`) fits a
//!   projective transform and reports residuals, dropping over-threshold
//!   corners once and refitting.
//!
//! The facade has no parity / feature-class labels, so the built-in
//! `Oriented2Policy` (the private `policy` submodule) treats every corner as
//! eligible. This path is exercised by synthetic tests only; the dataset-
//! gated chessboard seed-and-grow path composes the advanced engine
//! directly with its own policy.

use std::collections::{HashMap, HashSet};

use nalgebra::Point2;

use crate::detect::DetectionParams;
use crate::error::{GridError, Result};
use crate::feature::OrientedFeature;
use crate::lattice::{Coord, GridDimensions, LatticeKind};
use crate::result::{GridSolution, LabelledGrid, RejectedFeature, RejectionReason};
use crate::seed_and_grow::grow::{bfs_grow as adv_bfs_grow, GrowParams as AdvGrowParams};
use crate::seed_and_grow::pipeline::{
    run_convergence_loop, IterationDriver, IterationProduct, LoopParams,
};
use crate::seed_and_grow::seed::finder::find_quad as adv_find_quad;
use crate::seed_and_grow::seed::Seed as AdvSeed;
use crate::shared::merge::{merge_components_local, ComponentInput};
use crate::shared::validate as pg_validate;

use super::shared::{fit_component, FitComponentResult};
use crate::seed_and_grow::policy::{Oriented2Policy, Oriented2Tolerances};
use crate::seed_and_grow::positions_policy::{PositionsAttachPolicy, PositionsTolerances};
use crate::seed_and_grow::recovery::RecoveryParams;

/// Wide soft axis-alignment tolerance (radians) for the synthesized-axis
/// position path — 50°, double the `Oriented2Policy` 25°. Synthesized axes are
/// noisier than caller-supplied ones, so the position policy uses this only as
/// a soft cue (see [`positions_policy`]).
const POSITIONS_SOFT_AXIS_TOL_RAD: f32 = 0.872_664_6; // 50°
/// Per-edge length tolerance for the position policy. Slightly wider than the
/// oriented band so a foreshortened boundary edge is not rejected before the
/// recovery schedule's revalidation can judge it in context.
const POSITIONS_EDGE_LENGTH_TOL: f32 = 0.40;

/// Default per-candidate axis-alignment tolerance (radians) for the
/// facade policy — matches the historical seed-grow engine's 25°.
const FACADE_AXIS_ALIGN_TOL_RAD: f32 = 0.436_332_3; // 25°
/// Default per-edge length tolerance (fraction of cell size) for the
/// facade policy — matches the historical seed-grow engine's 0.35.
const FACADE_EDGE_LENGTH_TOL: f32 = 0.35;
/// Hard cap on the number of seed-and-grow components assembled per call,
/// so a pathological input can't spin forever.
const MAX_COMPONENTS: usize = 16;
/// Hard cap on the per-component seed → grow → validate convergence loop
/// iterations. Mirrors the chessboard tuning default (`6`); a clean grid
/// converges on the first iteration, so this only bounds the re-seed
/// behaviour on noisy inputs.
const FACADE_MAX_VALIDATION_ITERS: u32 = 6;

/// Seed → grow (multi-component) → merge → validate → fit pipeline for
/// square lattices with two-axis-per-feature evidence.
///
/// Returns one [`GridSolution`] per merged component, ordered by
/// labelled-count descending (ties by smallest source_index). At least
/// one solution is returned on success.
pub(crate) fn detect_square_oriented2_seed_grow(
    features: &[OrientedFeature<2>],
    dimensions: Option<GridDimensions>,
    params: &DetectionParams,
    synthesized_axes: bool,
) -> Result<Vec<GridSolution>> {
    if features.len() < 4 {
        return Err(GridError::InsufficientEvidence);
    }

    // Policy mode: native two-axis evidence uses the strict `Oriented2Policy`
    // (axis voucher is a hard gate); synthesized-axis evidence uses the
    // geometry-first `PositionsAttachPolicy` (axis voucher is a soft cue).
    let mode = if synthesized_axes {
        PolicyMode::Positions
    } else {
        PolicyMode::Oriented2
    };
    // Recovery schedule: enabled for the synthesized-axis path under `Auto`.
    let recovery = params.recovery.resolve(synthesized_axes);

    let positions: Vec<Point2<f32>> = features.iter().map(|f| f.point.position).collect();
    // Per-corner local pitch (nearest-neighbour distance). The attach policy's
    // per-edge length band gates against this local expectation rather than a
    // single seed-derived scalar, so growth tracks perspective foreshortening.
    let local_pitch = compute_local_pitch(&positions);

    // Stage 1: assemble one labelled component per closed seed.
    let raw_components = build_components(features, &positions, &local_pitch, params, mode);
    if raw_components.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }

    // Stage 2: local-geometry component merge over all raw components.
    let merged = merge_raw_components(&raw_components, &positions, params);
    if merged.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }

    // Stage 2.5: per-component geometry-only recovery schedule (extension +
    // fill + revalidate + drop filters) for the synthesized-axis path. Pushes
    // recall up to the dense-recovery level without any target vocabulary;
    // disabled for native `Oriented2` (recovery is `None`), preserving its
    // single-pass behaviour byte-for-byte.
    let merged = apply_recovery(
        merged,
        features,
        &positions,
        &local_pitch,
        params,
        mode,
        &recovery,
    );

    // Stage 3 + 4: validate + fit per merged component.
    let mut component_outputs: Vec<ComponentOutput> = Vec::new();
    for labelled in &merged {
        if let Some(out) = finish_component(labelled, features, &positions, params) {
            component_outputs.push(out);
        }
    }
    if component_outputs.is_empty() {
        return Err(GridError::DegenerateGeometry);
    }

    component_outputs.sort_by(|a, b| {
        b.kept_source_indices
            .len()
            .cmp(&a.kept_source_indices.len())
            .then_with(|| a.min_source_index.cmp(&b.min_source_index))
    });

    Ok(assemble_solutions(component_outputs, features, dimensions))
}

/// Which built-in attach policy the facade grows with. The native two-axis
/// path keeps the strict axis voucher; the synthesized-axis path uses the
/// geometry-first position policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PolicyMode {
    /// Caller-supplied two-axis evidence — strict axis voucher.
    Oriented2,
    /// Synthesized-axis evidence — soft axis cue, geometry-first.
    Positions,
}

/// Repeatedly find a seed quad over the still-unlabelled corners, grow it,
/// and collect the component. Stops when no seed closes or the component
/// cap is hit.
fn build_components(
    features: &[OrientedFeature<2>],
    positions: &[Point2<f32>],
    local_pitch: &[f32],
    params: &DetectionParams,
    mode: PolicyMode,
) -> Vec<HashMap<(i32, i32), usize>> {
    let mut out: Vec<HashMap<(i32, i32), usize>> = Vec::new();
    let mut used: HashSet<usize> = HashSet::new();

    while out.len() < MAX_COMPONENTS {
        // Remaining corner indices not yet claimed by an earlier component.
        let remaining: Vec<usize> = (0..features.len()).filter(|i| !used.contains(i)).collect();
        if remaining.len() < 4 {
            break;
        }

        // The advanced finder / grow operate on the global index space and
        // use the policy's `is_eligible` to mask. We restrict eligibility
        // to the remaining set by passing a fresh policy whose seed
        // candidate lists exclude already-used corners.
        let Some(component) =
            grow_one_component(features, positions, local_pitch, params, &used, mode)
        else {
            break;
        };
        if component.len() < 4 {
            break;
        }
        for &idx in component.values() {
            used.insert(idx);
        }
        out.push(component);
    }

    out
}

/// Assemble one labelled component over the not-yet-used corners by running
/// the shared seed → grow → validate → blacklist convergence loop. Returns
/// the converged labelled map, or `None` when no seed closes.
///
/// The loop (hosted in [`crate::seed_and_grow::pipeline`]) re-seeds and
/// re-grows the component whenever the post-grow validator flags outliers,
/// excluding them, until the validator stops flagging (or a small residual
/// soft-converges). On a clean grid the validator flags nothing, so the loop
/// converges on its first iteration and the output equals the historical
/// single-pass result.
fn grow_one_component(
    features: &[OrientedFeature<2>],
    positions: &[Point2<f32>],
    local_pitch: &[f32],
    params: &DetectionParams,
    used: &HashSet<usize>,
    mode: PolicyMode,
) -> Option<HashMap<(i32, i32), usize>> {
    let mut driver = FacadeComponentDriver {
        features,
        positions,
        local_pitch,
        params,
        used,
        mode,
    };
    // Mirror the chessboard loop's soft-convergence shape: `it + 1 >= 2`,
    // residual `<= 2`, labelled `>= 4` (the facade's minimum component size).
    let loop_params = LoopParams::new(FACADE_MAX_VALIDATION_ITERS, 2, 2, 4);
    let report = run_convergence_loop(&mut driver, loop_params);
    let conv = report.converged_record()?;
    if conv.labelled.len() < 4 {
        return None;
    }
    Some(conv.labelled.clone())
}

/// One iteration of the facade's per-component assembly: find a seed over the
/// `used ∪ blacklist`-masked corners, grow it, and validate the labelled set.
struct FacadeComponentDriver<'a> {
    features: &'a [OrientedFeature<2>],
    positions: &'a [Point2<f32>],
    local_pitch: &'a [f32],
    params: &'a DetectionParams,
    used: &'a HashSet<usize>,
    mode: PolicyMode,
}

impl IterationDriver for FacadeComponentDriver<'_> {
    fn run_iteration(&mut self, blacklist: &HashSet<usize>, _it: u32) -> IterationProduct {
        // The seed + attach masks exclude both the corners earlier components
        // claimed (`used`) and this component's running blacklist.
        let masked: HashSet<usize> = self.used.union(blacklist).copied().collect();

        let seed_policy = RestrictedSeedPolicy {
            features: self.features,
            positions: self.positions,
            used: &masked,
        };
        let Some(seed_out) = adv_find_quad(&seed_policy, &self.params.seed) else {
            return IterationProduct::seed_failed();
        };
        let seed = seed_out.seed;
        let cell_size = seed_out.cell_size;

        let inner = make_inner_policy(
            self.mode,
            self.features,
            self.positions,
            self.local_pitch,
            cell_size,
        );
        let attach_policy = RestrictedAttachPolicy {
            inner,
            used: &masked,
        };

        let adv_seed = AdvSeed {
            a: seed.a,
            b: seed.b,
            c: seed.c,
            d: seed.d,
        };
        let grow_params = AdvGrowParams::new(
            self.params.grow.attach_search_rel,
            self.params.grow.attach_ambiguity_factor,
        );
        let result = adv_bfs_grow(
            self.positions,
            adv_seed,
            cell_size,
            &grow_params,
            &attach_policy,
        );
        if result.labelled.len() < 4 {
            return IterationProduct::seed_failed();
        }

        // Validate the grown component so the loop can blacklist outliers and
        // re-seed. Use the grown set's mean cardinal-edge length as the
        // validation cell size (matches `finish_component`'s estimate).
        let validate_cell_size = estimate_cell_size(&result.labelled, self.positions);
        let ordered = sorted_labelled(&result.labelled);
        let entries: Vec<pg_validate::LabelledEntry> = ordered
            .iter()
            .map(|&(grid, idx)| pg_validate::LabelledEntry {
                idx,
                pixel: self.positions[idx],
                grid,
            })
            .collect();
        let validation = pg_validate::validate(&entries, validate_cell_size, &self.params.validate);

        IterationProduct {
            seed_found: true,
            labelled: result.labelled,
            validation_blacklist: validation.blacklist.iter().copied().collect(),
            cell_size: Some(cell_size),
            seed_indices: Some([seed.a, seed.b, seed.c, seed.d]),
        }
    }
}

/// Seed policy that restricts both candidate classes to the unused corner
/// set so successive `build_components` passes don't re-seed on already-
/// claimed corners.
struct RestrictedSeedPolicy<'a> {
    features: &'a [OrientedFeature<2>],
    positions: &'a [Point2<f32>],
    used: &'a HashSet<usize>,
}

impl crate::seed_and_grow::seed::finder::SquareSeedPolicy for RestrictedSeedPolicy<'_> {
    fn position(&self, idx: usize) -> Point2<f32> {
        self.positions[idx]
    }
    fn axes(&self, idx: usize) -> [crate::feature::LocalAxis; 2] {
        self.features[idx].axes
    }
    fn primary_candidates(&self) -> Vec<usize> {
        (0..self.features.len())
            .filter(|i| !self.used.contains(i))
            .collect()
    }
    fn secondary_candidates(&self) -> Vec<usize> {
        (0..self.features.len())
            .filter(|i| !self.used.contains(i))
            .collect()
    }
}

/// The facade's built-in attach policy, dispatched on [`PolicyMode`]. Both
/// variants implement [`SquareAttachPolicy`](crate::seed_and_grow::grow::SquareAttachPolicy);
/// this enum lets the driver and recovery schedule hold one of them without a
/// generic type parameter threading through every helper.
enum FacadeInner<'a> {
    Oriented2(Oriented2Policy<'a>),
    Positions(PositionsAttachPolicy<'a>),
}

/// Build the inner attach policy for the given mode, sharing the per-corner
/// local-pitch table and the seed-derived `cell_size` fallback.
fn make_inner_policy<'a>(
    mode: PolicyMode,
    features: &'a [OrientedFeature<2>],
    positions: &'a [Point2<f32>],
    local_pitch: &'a [f32],
    cell_size: f32,
) -> FacadeInner<'a> {
    match mode {
        PolicyMode::Oriented2 => {
            let tol = Oriented2Tolerances {
                axis_align_tol_rad: FACADE_AXIS_ALIGN_TOL_RAD,
                edge_length_tol: FACADE_EDGE_LENGTH_TOL,
                cell_size,
            };
            FacadeInner::Oriented2(Oriented2Policy::new(features, positions, local_pitch, tol))
        }
        PolicyMode::Positions => {
            let tol = PositionsTolerances {
                soft_axis_tol_rad: POSITIONS_SOFT_AXIS_TOL_RAD,
                edge_length_tol: POSITIONS_EDGE_LENGTH_TOL,
                cell_size,
            };
            FacadeInner::Positions(PositionsAttachPolicy::new(
                features,
                positions,
                local_pitch,
                tol,
            ))
        }
    }
}

impl crate::seed_and_grow::grow::SquareAttachPolicy for FacadeInner<'_> {
    fn is_eligible(&self, idx: usize) -> bool {
        match self {
            FacadeInner::Oriented2(p) => p.is_eligible(idx),
            FacadeInner::Positions(p) => p.is_eligible(idx),
        }
    }
    fn required_label_at(&self, i: i32, j: i32) -> Option<u8> {
        match self {
            FacadeInner::Oriented2(p) => p.required_label_at(i, j),
            FacadeInner::Positions(p) => p.required_label_at(i, j),
        }
    }
    fn label_of(&self, idx: usize) -> Option<u8> {
        match self {
            FacadeInner::Oriented2(p) => p.label_of(idx),
            FacadeInner::Positions(p) => p.label_of(idx),
        }
    }
    fn accept_candidate(
        &self,
        idx: usize,
        at: (i32, i32),
        prediction: Point2<f32>,
        neighbours: &[crate::seed_and_grow::grow::LabelledNeighbour],
    ) -> crate::seed_and_grow::grow::Admit {
        match self {
            FacadeInner::Oriented2(p) => p.accept_candidate(idx, at, prediction, neighbours),
            FacadeInner::Positions(p) => p.accept_candidate(idx, at, prediction, neighbours),
        }
    }
    fn edge_ok(&self, c: usize, n: usize, ac: (i32, i32), an: (i32, i32)) -> bool {
        match self {
            FacadeInner::Oriented2(p) => p.edge_ok(c, n, ac, an),
            FacadeInner::Positions(p) => p.edge_ok(c, n, ac, an),
        }
    }
}

/// Attach policy that wraps a [`FacadeInner`] and masks out corners that
/// earlier components already claimed.
struct RestrictedAttachPolicy<'a> {
    inner: FacadeInner<'a>,
    used: &'a HashSet<usize>,
}

impl crate::seed_and_grow::grow::SquareAttachPolicy for RestrictedAttachPolicy<'_> {
    fn is_eligible(&self, idx: usize) -> bool {
        !self.used.contains(&idx) && self.inner.is_eligible(idx)
    }
    fn required_label_at(&self, i: i32, j: i32) -> Option<u8> {
        self.inner.required_label_at(i, j)
    }
    fn label_of(&self, idx: usize) -> Option<u8> {
        self.inner.label_of(idx)
    }
    fn accept_candidate(
        &self,
        idx: usize,
        at: (i32, i32),
        prediction: Point2<f32>,
        neighbours: &[crate::seed_and_grow::grow::LabelledNeighbour],
    ) -> crate::seed_and_grow::grow::Admit {
        self.inner.accept_candidate(idx, at, prediction, neighbours)
    }
    fn edge_ok(
        &self,
        candidate_idx: usize,
        neighbour_idx: usize,
        at_candidate: (i32, i32),
        at_neighbour: (i32, i32),
    ) -> bool {
        self.inner
            .edge_ok(candidate_idx, neighbour_idx, at_candidate, at_neighbour)
    }
}

/// Rebase a labelled component so its bounding-box minimum sits at
/// `(0, 0)`, enforcing the workspace's non-negative `(i, j)` invariant.
/// `merge_components_local` already rebases its multi-component output;
/// this mirrors that for the single-component fast path (a component
/// grown leftward / upward of its seed can otherwise carry negative
/// coords).
fn rebase_to_origin(labelled: &HashMap<(i32, i32), usize>) -> HashMap<(i32, i32), usize> {
    let min_i = labelled.keys().map(|&(i, _)| i).min().unwrap_or(0);
    let min_j = labelled.keys().map(|&(_, j)| j).min().unwrap_or(0);
    if min_i == 0 && min_j == 0 {
        return labelled.clone();
    }
    labelled
        .iter()
        .map(|(&(i, j), &idx)| ((i - min_i, j - min_j), idx))
        .collect()
}

/// Run the local-geometry component merge over the raw components.
fn merge_raw_components(
    raw_components: &[HashMap<(i32, i32), usize>],
    positions: &[Point2<f32>],
    _params: &DetectionParams,
) -> Vec<HashMap<(i32, i32), usize>> {
    if raw_components.len() == 1 {
        return vec![rebase_to_origin(&raw_components[0])];
    }
    let inputs: Vec<ComponentInput<'_>> = raw_components
        .iter()
        .map(|labelled| ComponentInput {
            labelled,
            positions,
        })
        .collect();
    let merge_params = crate::shared::merge::LocalMergeParams::default();
    let merged = merge_components_local(&inputs, &merge_params);
    if merged.components.is_empty() {
        raw_components.iter().map(rebase_to_origin).collect()
    } else {
        merged.components
    }
}

/// Run the geometry-only recovery schedule over every merged component when
/// `recovery` is enabled. Each component is recovered against the corners NOT
/// claimed by any *other* component (so two components can't fight over the
/// same corner), using the same attach policy the grow loop used. Returns the
/// recovered (rebased) labelled maps; a no-op clone when `recovery` is `None`.
fn apply_recovery(
    merged: Vec<HashMap<(i32, i32), usize>>,
    features: &[OrientedFeature<2>],
    positions: &[Point2<f32>],
    local_pitch: &[f32],
    params: &DetectionParams,
    mode: PolicyMode,
    recovery: &Option<RecoveryParams>,
) -> Vec<HashMap<(i32, i32), usize>> {
    let Some(rec_params) = recovery else {
        return merged;
    };
    debug_assert_eq!(
        mode,
        PolicyMode::Positions,
        "recovery is `Auto`-enabled only for the synthesized-axis path"
    );
    crate::seed_and_grow::recovery::recover_components(
        merged,
        crate::seed_and_grow::recovery::RecoveryInputs {
            features,
            positions,
            local_pitch,
            params: rec_params,
            validate_params: &params.validate,
        },
    )
}

struct ComponentOutput {
    grid: LabelledGrid,
    fit: crate::result::LatticeFit,
    rejected: Vec<RejectedFeature>,
    kept_source_indices: HashSet<usize>,
    validation_drop_source_indices: HashSet<usize>,
    min_source_index: usize,
}

/// Validate + fit one merged component. Returns `None` when fewer than
/// four corners survive.
fn finish_component(
    labelled: &HashMap<(i32, i32), usize>,
    features: &[OrientedFeature<2>],
    positions: &[Point2<f32>],
    params: &DetectionParams,
) -> Option<ComponentOutput> {
    if labelled.len() < 4 {
        return None;
    }

    // Materialize the labelled set in deterministic `(i, j)`-sorted order.
    // The downstream validate / fit stages consume an ordered slice and their
    // tie-breaking can depend on input order, so iterating the HashMap in its
    // (nondeterministic) hash order would make the detection output flip
    // between runs on the same input. Sorting by coord pins it.
    let ordered: Vec<((i32, i32), usize)> = sorted_labelled(labelled);

    let validate_entries: Vec<pg_validate::LabelledEntry> = ordered
        .iter()
        .map(|&(grid, idx)| pg_validate::LabelledEntry {
            idx,
            pixel: positions[idx],
            grid,
        })
        .collect();
    let cell_size = estimate_cell_size(labelled, positions);
    let validation = pg_validate::validate(&validate_entries, cell_size, &params.validate);

    let mut kept: Vec<(Coord, usize)> = ordered
        .iter()
        .filter(|(_, idx)| !validation.blacklist.contains(idx))
        .map(|&((i, j), idx)| (Coord::new(i, j), idx))
        .collect();
    if kept.len() < 4 {
        return None;
    }

    let fit_result = run_fit_with_residual_drop(&mut kept, features, positions, params)?;
    let FitComponentResult {
        entries,
        fit,
        over_threshold,
    } = fit_result;

    let kept_source_indices: HashSet<usize> = kept
        .iter()
        .map(|&(_, idx)| features[idx].point.source_index)
        .collect();
    let validation_drop_source_indices: HashSet<usize> = validation
        .blacklist
        .iter()
        .map(|&idx| features[idx].point.source_index)
        .collect();

    let mut rejected: Vec<RejectedFeature> = Vec::new();
    for &src in &validation_drop_source_indices {
        rejected.push(RejectedFeature::new(
            src,
            None,
            None,
            RejectionReason::ValidationDropped,
        ));
    }
    for r in over_threshold {
        rejected.push(r);
    }

    let min_source_index = kept_source_indices
        .iter()
        .copied()
        .min()
        .unwrap_or(usize::MAX);
    let grid = LabelledGrid::new(LatticeKind::Square, entries, None);

    Some(ComponentOutput {
        grid,
        fit,
        rejected,
        kept_source_indices,
        validation_drop_source_indices,
        min_source_index,
    })
}

/// Materialize a labelled component as a `(i, j)`-sorted vector. Pins a
/// deterministic processing order for the validate / fit stages, which would
/// otherwise inherit the HashMap's nondeterministic iteration order.
fn sorted_labelled(labelled: &HashMap<(i32, i32), usize>) -> Vec<((i32, i32), usize)> {
    let mut out: Vec<((i32, i32), usize)> = labelled.iter().map(|(&k, &v)| (k, v)).collect();
    out.sort_unstable_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    out
}

/// Fit, drop over-threshold once, refit. Mutates `kept` to the survivors.
fn run_fit_with_residual_drop(
    kept: &mut Vec<(Coord, usize)>,
    features: &[OrientedFeature<2>],
    positions: &[Point2<f32>],
    params: &DetectionParams,
) -> Option<FitComponentResult> {
    let lattice = LatticeKind::Square;
    let first = fit_component(kept, features, positions, lattice, params).ok()?;
    if first.over_threshold.is_empty() {
        return Some(first);
    }
    let drop: HashSet<usize> = first
        .over_threshold
        .iter()
        .map(|r| r.source_index)
        .collect();
    kept.retain(|&(_, idx)| !drop.contains(&features[idx].point.source_index));
    if kept.len() < 4 {
        return None;
    }
    let refit = fit_component(kept, features, positions, lattice, params).ok()?;
    Some(FitComponentResult {
        entries: refit.entries,
        fit: refit.fit,
        over_threshold: first.over_threshold,
    })
}

/// Build the global "unlabelled" set and assemble per-component solutions,
/// attaching the global unlabelled set to the largest component.
fn assemble_solutions(
    component_outputs: Vec<ComponentOutput>,
    features: &[OrientedFeature<2>],
    dimensions: Option<GridDimensions>,
) -> Vec<GridSolution> {
    let mut globally_kept: HashSet<usize> = HashSet::new();
    let mut globally_validation_dropped: HashSet<usize> = HashSet::new();
    for out in &component_outputs {
        for &src in &out.kept_source_indices {
            globally_kept.insert(src);
        }
        for &src in &out.validation_drop_source_indices {
            globally_validation_dropped.insert(src);
        }
    }
    let mut global_unlabelled: Vec<RejectedFeature> = Vec::new();
    for feature in features {
        let src = feature.point.source_index;
        if globally_kept.contains(&src) {
            continue;
        }
        let reason = if globally_validation_dropped.contains(&src) {
            RejectionReason::ValidationDropped
        } else {
            RejectionReason::Unlabelled
        };
        global_unlabelled.push(RejectedFeature::new(src, None, None, reason));
    }

    let mut solutions: Vec<GridSolution> = Vec::with_capacity(component_outputs.len());
    for (idx, out) in component_outputs.into_iter().enumerate() {
        let ComponentOutput {
            mut grid,
            fit,
            mut rejected,
            ..
        } = out;
        // Re-attach caller dimensions (the per-component grid was built
        // with `None` so the bbox is computed from entries only).
        grid.dimensions = dimensions;
        if idx == 0 {
            rejected.extend(global_unlabelled.iter().copied());
        }
        solutions.push(GridSolution::new(grid, Some(fit), rejected));
    }
    solutions
}

/// Number of nearest neighbours pooled per corner for the robust local-pitch
/// estimate. On a square lattice a corner's four cardinal neighbours sit at the
/// pitch and the diagonals are strictly farther (≈√2·pitch), so pooling five
/// lands the median on the cardinal pitch while a single off-lattice point that
/// is *closer* than the pitch — a noise feature dropped inside a cell, a marker
/// corner — can only displace one or two slots and never the median.
const LOCAL_PITCH_NEIGHBOURS: usize = 5;

/// Per-corner local pitch: a robust order statistic of each corner's nearest
/// neighbour distances. Tracks perspective foreshortening (the projected cell
/// pitch shrinks toward the vanishing points), so the attach policy's per-edge
/// length band stays valid across the whole image instead of gating against one
/// seed scalar — yet, unlike the bare nearest-neighbour distance, it is not
/// corrupted by a minority of off-lattice points sitting closer than the pitch.
///
/// The bare nearest-neighbour distance was maximally sensitive to exactly that:
/// a noise feature at a cell centre is ≈0.71·pitch from each of the cell's four
/// corners, so it became those corners' "pitch", collapsing the per-edge band
/// and dropping real corners. Taking the median over the
/// [`LOCAL_PITCH_NEIGHBOURS`] nearest distances tolerates such a minority while
/// staying local.
///
/// Falls back to `0.0` for a corner with no finite neighbour; the policy treats
/// a non-positive local pitch as "use the seed scalar".
fn compute_local_pitch(positions: &[Point2<f32>]) -> Vec<f32> {
    use kiddo::{KdTree, SquaredEuclidean};
    let n = positions.len();
    if n < 2 {
        return vec![0.0; n];
    }
    let mut tree: KdTree<f32, 2> = KdTree::new();
    for (i, p) in positions.iter().enumerate() {
        tree.add(&[p.x, p.y], i as u64);
    }
    positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            // Pool the nearest neighbours (the `+ 1` covers the point itself,
            // which `nearest_n` returns at distance 0).
            let hits = tree.nearest_n::<SquaredEuclidean>(&[p.x, p.y], LOCAL_PITCH_NEIGHBOURS + 1);
            let mut dists: Vec<f32> = hits
                .into_iter()
                .filter(|nn| nn.item as usize != i)
                .map(|nn| nn.distance.sqrt())
                .filter(|d| d.is_finite() && *d > 1e-3)
                .collect();
            if dists.is_empty() {
                return 0.0;
            }
            dists.sort_by(|a, b| a.total_cmp(b));
            // Upper-median order statistic: robust to a minority of off-lattice
            // points closer than the pitch, and biases very slightly high (a
            // larger expected pitch can only *lower* an edge ratio, so it never
            // rejects a legitimate edge).
            dists[dists.len() / 2]
        })
        .collect()
}

/// Mean labelled-pair cardinal edge length, used as the validate
/// `cell_size`. Falls back to `1.0` when no cardinal pair exists.
fn estimate_cell_size(labelled: &HashMap<(i32, i32), usize>, positions: &[Point2<f32>]) -> f32 {
    let mut sum = 0.0_f32;
    let mut count = 0usize;
    for (&(i, j), &idx) in labelled {
        let here = positions[idx];
        for (di, dj) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            if let Some(&n_idx) = labelled.get(&(i + di, j + dj)) {
                let nb = positions[n_idx];
                let dx = nb.x - here.x;
                let dy = nb.y - here.y;
                sum += (dx * dx + dy * dy).sqrt();
                count += 1;
            }
        }
    }
    if count == 0 {
        1.0
    } else {
        sum / count as f32
    }
}

// Relocated submodules (were detect/advanced/square/* and detect/square/oriented2_policy).
mod angle;
pub mod extension;
pub mod fill;
pub mod grow;
pub mod grow_extend;
pub mod pipeline;
mod policy;
mod positions_policy;
pub mod recovery;
pub mod seed;
