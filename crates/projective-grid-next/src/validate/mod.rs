//! Post-growth validation gate for a labelled square grid.
//!
//! Five independent checks run over the labelled set:
//!
//! 1. **Line collinearity** ([`lines`]). For every row (`j = const`) and
//!    column (`i = const`) with at least
//!    [`ValidationParams::line_min_members`] labelled members, fit a
//!    least-squares line in pixel space and flag any member whose
//!    perpendicular residual exceeds `line_tol_rel * scale`.
//! 2. **Local-H residual** ([`local_h`]). For every labelled corner with at
//!    least 4 non-collinear labelled neighbours in `(i, j)`-space, fit a
//!    4-point local homography from the 4 grid-closest neighbours, predict
//!    the corner's pixel position, and measure the residual. Corners whose
//!    residual exceeds `local_h_tol_rel * scale` are flagged.
//! 3. **Per-edge length band** ([`edges`]). Collect every cardinal labelled
//!    edge length; drop any edge whose `length / median` falls outside
//!    `[1 / (1 + edge_length_band_rel), 1 + edge_length_band_rel]`. The
//!    endpoint with the higher edge-failure count is blacklisted.
//! 4. **Axis-slot parity** ([`edges`]). When the policy carries a chessboard
//!    parity rule and `enable_edge_parity_check` is set, every labelled pair
//!    must pick *opposite* axis slots for the edge direction; same-slot is
//!    a parity violation and the higher-`idx` endpoint is blacklisted.
//!
//! Flags from checks (1) and (2) are combined via the legacy attribution
//! rules (≥ 2 line flags, high local-H + line flag, base attribution).
//! Flags from checks (3) and (4) go to the blacklist *unconditionally* —
//! they are tighter than the legacy line check, and self-evidently bad
//! geometry has no safe recovery downstream.
//!
//! The orchestrator emits [`Event::StageStarted`] / [`Event::StageFinished`]
//! bookends for [`Stage::Validate`] and a
//! [`Event::ValidationDropped`] for every blacklisted coordinate.
//!
//! ## Pattern-agnostic surface
//!
//! The line and local-H checks have no dependency on
//! chessboard-specific vocabulary; any caller that can produce a
//! `(corner_index, position, grid_coord)` slice can use them. The axis-slot
//! parity check consults a [`LabelPolicy`] for its
//! [`ParityRule`](crate::policy::ParityRule) only — the policy is the single
//! place that decides whether parity is enforced.
//!
//! [`Event::StageStarted`]: crate::diagnostics::Event::StageStarted
//! [`Event::StageFinished`]: crate::diagnostics::Event::StageFinished
//! [`Event::ValidationDropped`]: crate::diagnostics::Event::ValidationDropped
//! [`Stage::Validate`]: crate::diagnostics::Stage::Validate
//! [`LabelPolicy`]: crate::policy::LabelPolicy

pub mod edges;
pub mod lines;
pub mod local_h;

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use nalgebra::Point2;

use crate::diagnostics::{DiagnosticSink, Event, Stage, ValidationReason};
use crate::feature::Observation;
use crate::float::{lit, Float};
use crate::lattice::Coord;
use crate::policy::LabelPolicy;

/// Tolerances and feature toggles for the validation pass.
///
/// All spatial tolerances are expressed as ratios of the caller-supplied
/// global `cell_size`. The struct is `#[non_exhaustive]` so new tuning knobs
/// can be added in a minor release; downstream construction goes through
/// [`ValidationParams::new`] plus the `with_*` builders.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ValidationParams<F: Float> {
    /// Straight-line fit collinearity tolerance (fraction of `cell_size`).
    pub line_tol_rel: F,
    /// Minimum members required to fit a row / column line.
    pub line_min_members: usize,
    /// Local-H prediction tolerance (fraction of `cell_size`).
    pub local_h_tol_rel: F,
    /// Per-edge length band. Edges with `len / median` outside
    /// `[1 / (1 + band), 1 + band]` are flagged. Default `0.35`.
    pub edge_length_band_rel: F,
    /// Enable the axis-slot parity check. The check only runs when the
    /// active [`ParityRule`](crate::policy::ParityRule) is
    /// [`Chessboard`](crate::policy::ParityRule::Chessboard); other parity
    /// rules silently skip it. Default `true`.
    pub enable_edge_parity_check: bool,
}

impl<F: Float> Default for ValidationParams<F> {
    fn default() -> Self {
        Self {
            line_tol_rel: lit::<F>(0.15_f32),
            line_min_members: 3,
            local_h_tol_rel: lit::<F>(0.20_f32),
            edge_length_band_rel: lit::<F>(0.35_f32),
            enable_edge_parity_check: true,
        }
    }
}

impl<F: Float> ValidationParams<F> {
    /// Construct the legacy line / local-H tolerances. The new edge-band and
    /// edge-parity knobs take their defaults; use `with_*` builders to
    /// override.
    pub fn new(line_tol_rel: F, line_min_members: usize, local_h_tol_rel: F) -> Self {
        Self {
            line_tol_rel,
            line_min_members,
            local_h_tol_rel,
            ..Self::default()
        }
    }

    /// Override the per-edge length band (fraction of the per-image median
    /// edge length).
    #[must_use]
    pub fn with_edge_length_band(mut self, band: F) -> Self {
        self.edge_length_band_rel = band;
        self
    }

    /// Toggle the axis-slot parity gate. The gate is a no-op when the active
    /// [`ParityRule`](crate::policy::ParityRule) is not
    /// [`Chessboard`](crate::policy::ParityRule::Chessboard); this flag
    /// disables it explicitly even under chessboard parity.
    #[must_use]
    pub fn with_edge_parity_check(mut self, enabled: bool) -> Self {
        self.enable_edge_parity_check = enabled;
        self
    }
}

/// A single labelled corner fed into [`validate`]: its caller-chosen index
/// (carried back in [`ValidationResult::blacklist`]), its pixel position, and
/// its integer grid coordinate.
///
/// `idx` is opaque to this module — callers may pick any scheme (direct slice
/// indices, corner struct fields, etc.) as long as the same scheme maps
/// blacklist entries back to their originals.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct LabelledEntry<F: Float> {
    /// Caller-chosen opaque index. Carried back in
    /// [`ValidationResult::blacklist`].
    pub idx: usize,
    /// The corner's position in image pixels.
    pub position: Point2<F>,
    /// The corner's integer `(i, j)` grid coordinate.
    pub coord: Coord,
}

impl<F: Float> LabelledEntry<F> {
    /// Construct a labelled entry from its three required fields.
    pub fn new(idx: usize, position: Point2<F>, coord: Coord) -> Self {
        Self {
            idx,
            position,
            coord,
        }
    }
}

/// Per-corner edge-failure record kept in [`ValidationResult::edge_failures`].
///
/// `neighbour_idx` is the *other* endpoint of the worst-offending edge from
/// the blamed corner's point of view; `ratio` is that edge's
/// `length / median`. `low` and `high` are the active acceptance-band bounds
/// at the time the failure was recorded — they are copied out of the
/// `ValidationParams` so downstream debuggers can interpret `ratio` without
/// re-deriving the band.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct EdgeFailure<F: Float> {
    /// The other endpoint of the worst-offending edge from the blamed
    /// corner's point of view.
    pub neighbour_idx: usize,
    /// `edge_length / per-image-median` for the worst-offending edge.
    pub ratio: F,
    /// Lower acceptance-band bound, copied from
    /// [`ValidationParams::edge_length_band_rel`] at the time of failure.
    pub low: F,
    /// Upper acceptance-band bound.
    pub high: F,
}

impl<F: Float> EdgeFailure<F> {
    /// Construct a record from its four required fields.
    pub fn new(neighbour_idx: usize, ratio: F, low: F, high: F) -> Self {
        Self {
            neighbour_idx,
            ratio,
            low,
            high,
        }
    }
}

/// Outcome of one validation pass.
#[derive(Debug)]
#[non_exhaustive]
pub struct ValidationResult<F: Float> {
    /// Corner indices to blacklist (attribution rules + edge-band +
    /// axis-slot parity have been applied).
    pub blacklist: HashSet<usize>,
    /// For each labelled corner with at least 4 non-collinear labelled
    /// neighbours, its local-H residual in pixels.
    pub local_h_residuals: HashMap<usize, F>,
    /// Per-corner edge-band failure records. Keyed by blamed-corner `idx`.
    pub edge_failures: HashMap<usize, EdgeFailure<F>>,
}

impl<F: Float> ValidationResult<F> {
    /// Construct an empty result.
    pub fn new() -> Self {
        Self {
            blacklist: HashSet::new(),
            local_h_residuals: HashMap::new(),
            edge_failures: HashMap::new(),
        }
    }
}

impl<F: Float> Default for ValidationResult<F> {
    fn default() -> Self {
        Self::new()
    }
}

/// Run every validation check and produce a blacklist plus per-corner
/// diagnostics.
///
/// Bookends emit [`Event::StageStarted`] / [`Event::StageFinished`] for
/// [`Stage::Validate`]; every dropped coordinate emits a
/// [`Event::ValidationDropped`].
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(
        level = "info",
        skip_all,
        fields(num_labelled = entries.len()),
    )
)]
pub fn validate<F, S>(
    entries: &[LabelledEntry<F>],
    observations: &[Observation<F>],
    cell_size: F,
    policy: &LabelPolicy<F>,
    params: &ValidationParams<F>,
    sink: &mut S,
) -> ValidationResult<F>
where
    F: Float,
    S: DiagnosticSink<F>,
{
    sink.emit(Event::StageStarted {
        stage: Stage::Validate,
    });
    let start = Instant::now();

    let result = run(entries, observations, cell_size, policy, params, sink);

    sink.emit(Event::StageFinished {
        stage: Stage::Validate,
        duration: start.elapsed(),
    });
    result
}

fn run<F, S>(
    entries: &[LabelledEntry<F>],
    observations: &[Observation<F>],
    cell_size: F,
    policy: &LabelPolicy<F>,
    params: &ValidationParams<F>,
    sink: &mut S,
) -> ValidationResult<F>
where
    F: Float,
    S: DiagnosticSink<F>,
{
    // Quick lookups, built once per call.
    let by_idx: HashMap<usize, &LabelledEntry<F>> = entries.iter().map(|e| (e.idx, e)).collect();
    let by_grid: HashMap<(i32, i32), usize> = entries.iter().map(|e| (e.coord, e.idx)).collect();

    // Uniform per-corner scale: this crate's new `ValidationParams` drops the
    // legacy `use_step_aware` knob (the step-aware machinery moved out to the
    // shared `crate::stats::local_step` module), so every corner gets the
    // caller-supplied global `cell_size`. Callers that want per-corner
    // scales should pre-compute them and either pass a tighter `cell_size`
    // or extend the API in a future minor release.
    let scale_at = |_idx: usize| -> F { cell_size };

    // --- Line collinearity --------------------------------------------------
    let line_flags = lines::line_collinearity_flags(&by_idx, &by_grid, params, &scale_at);

    // --- Local-H residual ---------------------------------------------------
    let two = lit::<F>(2.0_f32);
    let mut residuals: HashMap<usize, F> = HashMap::new();
    let mut local_h_flagged: HashMap<usize, F> = HashMap::new();
    let mut local_h_high: HashMap<usize, F> = HashMap::new();
    for entry in entries {
        let base = local_h::pick_local_h_base::<F>(&by_grid, entry.idx, entry.coord);
        if base.len() < 4 {
            continue;
        }
        let Some(resid) =
            local_h::local_h_residual(&by_idx, entry.idx, entry.coord, &base, &by_grid)
        else {
            continue;
        };
        residuals.insert(entry.idx, resid);
        let scale = scale_at(entry.idx);
        let local_h_tol_px = params.local_h_tol_rel * scale;
        if resid > local_h_tol_px {
            local_h_flagged.insert(entry.idx, resid);
            if resid > two * local_h_tol_px {
                local_h_high.insert(entry.idx, resid);
            }
        }
    }

    // --- Per-edge length + axis-slot parity ---------------------------------
    let edge_report = edges::edge_precision_flags(
        entries,
        &by_idx,
        &by_grid,
        observations,
        policy,
        params,
        sink,
    );

    // --- Attribution --------------------------------------------------------
    let mut blacklist: HashSet<usize> = HashSet::new();
    let emit_legacy_drop =
        |sink: &mut S, idx: usize, reason: ValidationReason<F>, dropped: &mut HashSet<usize>| {
            if dropped.insert(idx) {
                if let Some(entry) = by_idx.get(&idx) {
                    sink.emit(Event::ValidationDropped {
                        coord: entry.coord,
                        reason,
                    });
                }
            }
        };

    let mut legacy_dropped: HashSet<usize> = HashSet::new();

    // Rule 1: >= 2 line flags -> outlier.
    for (&idx, &count) in &line_flags {
        if count >= 2 {
            blacklist.insert(idx);
            let scale = scale_at(idx);
            emit_legacy_drop(
                sink,
                idx,
                ValidationReason::LineResidualExceeded {
                    residual: scale,
                    tol: params.line_tol_rel * scale,
                },
                &mut legacy_dropped,
            );
        }
    }
    // Rule 2: high local-H residual AND >= 1 line flag -> outlier.
    for (&idx, &resid) in &local_h_high {
        if line_flags.get(&idx).copied().unwrap_or(0) >= 1 {
            blacklist.insert(idx);
            let scale = scale_at(idx);
            emit_legacy_drop(
                sink,
                idx,
                ValidationReason::LocalHResidualExceeded {
                    residual: resid,
                    tol: params.local_h_tol_rel * scale,
                },
                &mut legacy_dropped,
            );
        }
    }
    // Rule 3: local-H flag with no line flag BUT a base neighbour flagged in
    // a line -> blacklist the worst base instead.
    for (&idx, &resid_drop) in &local_h_flagged {
        if line_flags.get(&idx).copied().unwrap_or(0) >= 1 {
            continue;
        }
        if blacklist.contains(&idx) {
            continue;
        }
        let Some(entry) = by_idx.get(&idx) else {
            continue;
        };
        let base = local_h::pick_local_h_base::<F>(&by_grid, idx, entry.coord);
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
            let scale = scale_at(base_idx);
            // Mirror the legacy reason: we blame the base because its line
            // flag is the cohort's strongest signal that something is wrong.
            emit_legacy_drop(
                sink,
                base_idx,
                ValidationReason::LineResidualExceeded {
                    residual: resid_drop,
                    tol: params.line_tol_rel * scale,
                },
                &mut legacy_dropped,
            );
        }
    }

    // Unconditional blacklist for the new edge gates.
    for &idx in edge_report.length_flags.keys() {
        blacklist.insert(idx);
    }
    for &idx in edge_report.parity_flags.keys() {
        blacklist.insert(idx);
    }

    ValidationResult {
        blacklist,
        local_h_residuals: residuals,
        edge_failures: edge_report.length_flags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::NoOpSink;
    use crate::feature::Observation;

    fn entry<F: Float>(idx: usize, x: F, y: F, i: i32, j: i32) -> LabelledEntry<F> {
        LabelledEntry::new(idx, Point2::new(x, y), (i, j))
    }

    fn clean_grid<F: Float>(rows: i32, cols: i32, s: F) -> Vec<LabelledEntry<F>> {
        let mut out = Vec::new();
        let mut idx = 0_usize;
        let origin = lit::<F>(50.0_f32);
        for j in 0..rows {
            for i in 0..cols {
                out.push(entry::<F>(
                    idx,
                    lit::<F>(i as f32) * s + origin,
                    lit::<F>(j as f32) * s + origin,
                    i,
                    j,
                ));
                idx += 1;
            }
        }
        out
    }

    fn make_unannotated_obs<F: Float>(entries: &[LabelledEntry<F>]) -> Vec<Observation<F>> {
        // The line / local-H paths do not consult observations; supply an
        // unannotated vector long enough that lookup-by-idx never fails.
        let n = entries.iter().map(|e| e.idx).max().unwrap_or(0) + 1;
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(Observation::<F>::new(Point2::new(F::zero(), F::zero())));
        }
        out
    }

    fn assert_clean_grid_has_empty_blacklist<F: Float>() {
        let entries = clean_grid::<F>(7, 7, lit::<F>(20.0_f32));
        let obs = make_unannotated_obs::<F>(&entries);
        let policy = LabelPolicy::<F>::builder(obs.len()).build();
        let mut sink = NoOpSink;
        let result = validate(
            &entries,
            &obs,
            lit::<F>(20.0_f32),
            &policy,
            &ValidationParams::default(),
            &mut sink,
        );
        assert!(result.blacklist.is_empty(), "{:?}", result.blacklist);
    }

    fn assert_displaced_interior_is_blacklisted<F: Float>() {
        let s = lit::<F>(20.0_f32);
        let mut entries = clean_grid::<F>(7, 7, s);
        // Displace (3, 3) by ~6 px in both directions — far enough to fail
        // line fits, local-H, and the new edge-length band.
        let target = entries
            .iter_mut()
            .find(|e| e.coord == (3, 3))
            .expect("(3,3) present");
        target.position.x += lit::<F>(6.0_f32);
        target.position.y += lit::<F>(6.0_f32);
        let target_idx = target.idx;

        let obs = make_unannotated_obs::<F>(&entries);
        let policy = LabelPolicy::<F>::builder(obs.len()).build();
        let mut sink = NoOpSink;
        let result = validate(
            &entries,
            &obs,
            s,
            &policy,
            &ValidationParams::default(),
            &mut sink,
        );
        assert!(
            result.blacklist.contains(&target_idx),
            "expected {target_idx} blacklisted: {:?}",
            result.blacklist
        );
    }

    fn assert_too_few_members_per_line_is_ignored<F: Float>() {
        let s = lit::<F>(20.0_f32);
        let entries = vec![
            entry::<F>(0, F::zero(), F::zero(), 0, 0),
            entry::<F>(1, s, F::zero(), 1, 0),
        ];
        let obs = make_unannotated_obs::<F>(&entries);
        let policy = LabelPolicy::<F>::builder(obs.len()).build();
        let mut sink = NoOpSink;
        let result = validate(
            &entries,
            &obs,
            s,
            &policy,
            &ValidationParams::default(),
            &mut sink,
        );
        assert!(result.blacklist.is_empty());
    }

    #[test]
    fn clean_grid_has_empty_blacklist_f32() {
        assert_clean_grid_has_empty_blacklist::<f32>();
    }
    #[test]
    fn clean_grid_has_empty_blacklist_f64() {
        assert_clean_grid_has_empty_blacklist::<f64>();
    }
    #[test]
    fn displaced_interior_is_blacklisted_f32() {
        assert_displaced_interior_is_blacklisted::<f32>();
    }
    #[test]
    fn displaced_interior_is_blacklisted_f64() {
        assert_displaced_interior_is_blacklisted::<f64>();
    }
    #[test]
    fn too_few_members_per_line_is_ignored_f32() {
        assert_too_few_members_per_line_is_ignored::<f32>();
    }
    #[test]
    fn too_few_members_per_line_is_ignored_f64() {
        assert_too_few_members_per_line_is_ignored::<f64>();
    }
}
