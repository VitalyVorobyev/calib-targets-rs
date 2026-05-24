//! Per-edge length and axis-slot parity precision gate.
//!
//! This is the *mandatory* geometry check called out in the workspace
//! `.claude/CLAUDE.md` (section "Evidence-driven detector debugging" /
//! "Geometry check is mandatory before returning a detection"). Detect
//! first, then verify by an independent geometric predicate that the
//! BFS / rescue paths did not already enforce; false detections are
//! unrecoverable for downstream calibration, missing corners are not.
//!
//! Two checks run against the labelled set:
//!
//! 1. **Per-edge length band.** Collect every cardinal labelled-pair edge
//!    length. Compute the median. Drop any edge whose `length / median`
//!    falls outside `[1 / (1 + band), 1 + band]`. The offending corner is
//!    chosen by taking the endpoint with the *higher* total edge-failure
//!    count (ties broken by the higher `idx`) — the endpoint that
//!    participates in the most bad edges is the most likely outlier.
//! 2. **Axis-slot parity.** Only meaningful when the active
//!    [`ParityRule`] is
//!    [`Chessboard`](ParityRule::Chessboard). Adjacent
//!    chessboard corners have opposite axis-slot assignments: if at one
//!    endpoint the closer axis (to the edge direction) is slot 0, then at
//!    the other endpoint it must be slot 1. When both endpoints pick the
//!    *same* slot for the same edge, the higher-`idx` endpoint is flagged
//!    as parity-broken.
//!
//! Uninformative axes (`sigma >= pi - eps`) skip the parity check for that
//! edge — the corner carries no axis information so there is nothing to
//! compare against.

use std::collections::HashMap;

use nalgebra::{ComplexField, RealField};

use crate::diagnostics::{DiagnosticSink, Event, ValidationReason};
use crate::feature::Observation;
use crate::float::{lit, Float};
use crate::lattice::SQUARE_CARDINAL_OFFSETS;
use crate::policy::{LabelPolicy, ParityRule};

use super::{EdgeFailure, LabelledEntry, ValidationParams};

/// Outcome of the per-edge precision gate.
///
/// `length_flags` lists corners blacklisted by the length-band check, paired
/// with the [`EdgeFailure`] describing the worst-offending edge. `parity_flags`
/// lists corners blacklisted by the axis-slot parity check. The same corner
/// may appear in both maps when independent edges trigger different reasons;
/// the orchestrator unions them into a single blacklist.
pub(super) struct EdgeReport<F: Float> {
    pub length_flags: HashMap<usize, EdgeFailure<F>>,
    pub parity_flags: HashMap<usize, ()>,
}

/// Run both edge-level checks and emit `ValidationDropped` events for every
/// flagged corner.
pub(super) fn edge_precision_flags<F, S>(
    entries: &[LabelledEntry<F>],
    by_idx: &HashMap<usize, &LabelledEntry<F>>,
    by_grid: &HashMap<(i32, i32), usize>,
    observations: &[Observation<F>],
    policy: &LabelPolicy<F>,
    params: &ValidationParams<F>,
    sink: &mut S,
) -> EdgeReport<F>
where
    F: Float,
    S: DiagnosticSink<F>,
{
    let length_flags = if entries.is_empty() {
        HashMap::new()
    } else {
        edge_length_flags(entries, by_idx, by_grid, params, sink)
    };

    let parity_flags = if params.enable_edge_parity_check
        && matches!(policy.parity_rule(), ParityRule::Chessboard { .. })
    {
        axis_slot_parity_flags(entries, by_grid, observations, policy, sink)
    } else {
        HashMap::new()
    };

    EdgeReport {
        length_flags,
        parity_flags,
    }
}

/// Enumerate every cardinal labelled edge, compute the per-image length
/// median, and blacklist the endpoint that participates in the most
/// out-of-band edges.
fn edge_length_flags<F, S>(
    entries: &[LabelledEntry<F>],
    by_idx: &HashMap<usize, &LabelledEntry<F>>,
    by_grid: &HashMap<(i32, i32), usize>,
    params: &ValidationParams<F>,
    sink: &mut S,
) -> HashMap<usize, EdgeFailure<F>>
where
    F: Float,
    S: DiagnosticSink<F>,
{
    // Collect (c_idx, n_idx, length) for every cardinal labelled pair, and
    // keep the lengths separately for the median. Each undirected edge is
    // visited twice (once from each endpoint); the duplicate is harmless for
    // the median and the ratio computation.
    let mut edges: Vec<(usize, usize, F)> = Vec::new();
    let mut lengths: Vec<F> = Vec::new();
    for entry in entries {
        let c_idx = entry.idx;
        for &(di, dj) in &SQUARE_CARDINAL_OFFSETS {
            let neigh = (entry.coord.0 + di, entry.coord.1 + dj);
            let Some(&n_idx) = by_grid.get(&neigh) else {
                continue;
            };
            if n_idx == c_idx {
                continue;
            }
            let Some(n_entry) = by_idx.get(&n_idx) else {
                continue;
            };
            let dx = n_entry.position.x - entry.position.x;
            let dy = n_entry.position.y - entry.position.y;
            let len = (dx * dx + dy * dy).sqrt();
            edges.push((c_idx, n_idx, len));
            lengths.push(len);
        }
    }

    if lengths.is_empty() {
        return HashMap::new();
    }

    // Median by sort-and-pick. Edges shorter than `eps` are ignored to avoid
    // division by zero downstream.
    lengths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = lengths[lengths.len() / 2];
    if median <= F::default_epsilon() {
        return HashMap::new();
    }

    let band = params.edge_length_band_rel;
    let one = F::one();
    let low = one / (one + band);
    let high = one + band;

    // First pass: flag every out-of-band edge with its ratio, and keep a
    // count of how many out-of-band edges each endpoint participates in.
    let mut bad_edges: Vec<(usize, usize, F)> = Vec::new();
    let mut bad_count: HashMap<usize, u32> = HashMap::new();
    for &(c_idx, n_idx, len) in &edges {
        let ratio = len / median;
        if ratio < low || ratio > high {
            bad_edges.push((c_idx, n_idx, ratio));
            *bad_count.entry(c_idx).or_insert(0) += 1;
        }
    }

    if bad_edges.is_empty() {
        return HashMap::new();
    }

    // Second pass: pick the worse endpoint per bad edge and track the
    // worst-deviation edge for each blacklisted corner. The "worst" ratio is
    // the one furthest from `1` in either direction.
    let mut out: HashMap<usize, EdgeFailure<F>> = HashMap::new();
    let mut dropped: HashMap<usize, ()> = HashMap::new();
    for (c_idx, n_idx, ratio) in bad_edges {
        let c_bad = bad_count.get(&c_idx).copied().unwrap_or(0);
        let n_bad = bad_count.get(&n_idx).copied().unwrap_or(0);
        let blame_idx = pick_endpoint_to_blame(c_idx, c_bad, n_idx, n_bad);

        // Track the worst ratio per blamed corner (largest |ratio - 1|).
        let deviation = ComplexField::abs(ratio - one);
        let take = match out.get(&blame_idx) {
            None => true,
            Some(prev) => ComplexField::abs(prev.ratio - one) < deviation,
        };
        if take {
            out.insert(
                blame_idx,
                EdgeFailure::new(
                    if blame_idx == c_idx { n_idx } else { c_idx },
                    ratio,
                    low,
                    high,
                ),
            );
        }

        // Emit one drop event per blamed corner — the first time we touch it.
        if dropped.insert(blame_idx, ()).is_none() {
            if let Some(entry) = by_idx.get(&blame_idx) {
                sink.emit(Event::ValidationDropped {
                    coord: entry.coord,
                    reason: ValidationReason::EdgeLengthOutOfBand { ratio, low, high },
                });
            }
        }
    }

    out
}

/// Tie-break helper. Pick the endpoint whose total edge-failure count is
/// higher; ties go to the higher `idx`.
#[inline]
fn pick_endpoint_to_blame(c_idx: usize, c_bad: u32, n_idx: usize, n_bad: u32) -> usize {
    match c_bad.cmp(&n_bad) {
        std::cmp::Ordering::Greater => c_idx,
        std::cmp::Ordering::Less => n_idx,
        std::cmp::Ordering::Equal => c_idx.max(n_idx),
    }
}

/// Axis-slot parity gate.
///
/// For every labelled corner pair `(c, n)` linked by a cardinal offset where
/// both endpoints carry informative axes, compute the edge direction (mod π)
/// and pick the closer of the two axes at each endpoint. Adjacent chessboard
/// corners must pick *opposite* slots; same-slot is a parity violation.
fn axis_slot_parity_flags<F, S>(
    entries: &[LabelledEntry<F>],
    by_grid: &HashMap<(i32, i32), usize>,
    observations: &[Observation<F>],
    policy: &LabelPolicy<F>,
    sink: &mut S,
) -> HashMap<usize, ()>
where
    F: Float,
    S: DiagnosticSink<F>,
{
    let mut flags: HashMap<usize, ()> = HashMap::new();

    // Enumerate undirected cardinal pairs only once: we visit each entry once
    // and only consider neighbours with strictly larger `idx`.
    for entry in entries {
        let c_idx = entry.idx;
        if c_idx >= observations.len() || !policy.is_eligible(c_idx) {
            continue;
        }
        let c_obs = &observations[c_idx];
        if !is_informative(c_obs) {
            continue;
        }
        for &(di, dj) in &SQUARE_CARDINAL_OFFSETS {
            let neigh = (entry.coord.0 + di, entry.coord.1 + dj);
            let Some(&n_idx) = by_grid.get(&neigh) else {
                continue;
            };
            if n_idx <= c_idx {
                // Already visited from the other side, or self-loop.
                continue;
            }
            if n_idx >= observations.len() || !policy.is_eligible(n_idx) {
                continue;
            }
            let n_obs = &observations[n_idx];
            if !is_informative(n_obs) {
                continue;
            }

            let n_pos = match by_grid
                .get(&neigh)
                .and_then(|ix| entries.iter().find(|e| e.idx == *ix).map(|e| e.position))
            {
                Some(p) => p,
                None => continue,
            };

            let dx = n_pos.x - entry.position.x;
            let dy = n_pos.y - entry.position.y;
            if ComplexField::abs(dx) <= F::default_epsilon()
                && ComplexField::abs(dy) <= F::default_epsilon()
            {
                continue;
            }
            let theta_edge = wrap_undirected::<F>(dy.atan2(dx));

            let slot_c =
                closer_axis_slot::<F>(theta_edge, c_obs.axes[0].angle, c_obs.axes[1].angle);
            let slot_n =
                closer_axis_slot::<F>(theta_edge, n_obs.axes[0].angle, n_obs.axes[1].angle);

            if slot_c == slot_n {
                // Both endpoints picked the same slot — chessboard parity
                // says adjacent corners must pick opposite slots.
                let blame_idx = c_idx.max(n_idx);
                if flags.insert(blame_idx, ()).is_none() {
                    // Find the entry for the blamed corner so we can emit
                    // the event with the right coordinate.
                    if let Some(blame_entry) = entries.iter().find(|e| e.idx == blame_idx) {
                        sink.emit(Event::ValidationDropped {
                            coord: blame_entry.coord,
                            reason: ValidationReason::AxisSlotParityMismatch,
                        });
                    }
                }
            }
        }
    }

    flags
}

#[inline]
fn is_informative<F: Float>(obs: &Observation<F>) -> bool {
    let pi = F::pi();
    let eps = F::default_epsilon();
    let threshold = pi - eps;
    obs.axes[0].sigma < threshold && obs.axes[1].sigma < threshold
}

/// Normalise an angle into `[0, pi)` (undirected axis convention).
#[inline]
fn wrap_undirected<F: Float>(angle: F) -> F {
    let pi = F::pi();
    let mut a = angle;
    // Bring into (-pi, pi].
    while a <= -pi {
        a += pi + pi;
    }
    while a > pi {
        a -= pi + pi;
    }
    // Map to [0, pi).
    if a < F::zero() {
        a += pi;
    }
    if a >= pi {
        a -= pi;
    }
    a
}

/// Return the slot (0 or 1) of the axis whose undirected angular distance to
/// `theta` is smallest.
#[inline]
fn closer_axis_slot<F: Float>(theta: F, alpha0: F, alpha1: F) -> u8 {
    let d0 = undirected_angle_distance::<F>(theta, alpha0);
    let d1 = undirected_angle_distance::<F>(theta, alpha1);
    if d0 <= d1 {
        0
    } else {
        1
    }
}

/// Undirected angular distance between `alpha` and `beta`, in `[0, pi/2]`.
///
/// `d(α, β) = min(|α - β|, π - |α - β|)` where the difference is taken modulo
/// `π` first (axes are equivalent under `θ ≡ θ + π`).
#[inline]
fn undirected_angle_distance<F: Float>(alpha: F, beta: F) -> F {
    let pi = F::pi();
    let two = lit::<F>(2.0_f32);
    let pi_over_two = pi / two;
    let mut diff = ComplexField::abs(alpha - beta);
    // Reduce modulo pi so the result is in [0, pi].
    while diff >= pi {
        diff -= pi;
    }
    // Map [0, pi] -> [0, pi/2] via the undirected min.
    if diff > pi_over_two {
        diff = pi - diff;
    }
    RealField::max(diff, F::zero())
}
