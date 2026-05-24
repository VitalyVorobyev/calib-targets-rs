//! Lattice-aware component merger.
//!
//! Two-pass orchestrator: for every pair of input components, try the
//! overlap-only merger (`overlap::find_overlap_merge`) first, then — if
//! `mode == MergeMode::OverlapAndPredicted` — the predicted merger
//! (`predicted::find_predicted_merge`). Acceptance emits
//! [`Event::MergeAccepted`]; rejection emits [`Event::MergeRejected`].
//!
//! ## Lattice safety
//!
//! [`MergeParams::symmetry`] is `&'static [GridTransform]`. Every transform's
//! [`GridTransform::source_kind`] must equal `expected_lattice`, otherwise the
//! orchestrator returns [`MergeError::SymmetryLatticeMismatch`]. Concrete
//! failure mode this rules out: a future hex caller accidentally passing
//! `D4_TRANSFORMS` to a hex lattice, which would silently produce garbage
//! `(i, j)` pairs at runtime.
//!
//! ## Closes Gap 9
//!
//! The legacy `merge_components_local` required `min_overlap >= 1` shared
//! label between two components; disjoint label sets — common when an entire
//! row of corners is missing — never merged. The new design adds
//! [`MergeMode::OverlapAndPredicted`]: when overlap-only fails, fall through
//! to the predicted merger that compares one component's actual labels to
//! the other's predicted labels (walked outward from its bbox using local
//! cell-step vectors). See `docs/algorithmic_gaps.md` Gap 9.

pub mod overlap;
pub mod predicted;

use std::collections::HashMap;

use nalgebra::{ComplexField, Point2, RealField};

use crate::diagnostics::{DiagnosticSink, Event, MergeRejectReason};
use crate::error::MergeError;
use crate::float::{lit, Float};
use crate::lattice::{Coord, GridTransform, LatticeKind};

/// Which merge strategy the orchestrator runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum MergeMode {
    /// Legacy behaviour: require at least `min_overlap` shared labels between
    /// two components. Disjoint label sets are never merged.
    #[default]
    OverlapOnly,
    /// Try overlap-only first; if no merge is found, predict the other
    /// component's labels from a local cell-step extrapolation and accept
    /// when predicted positions match actual labels within tolerance.
    /// Closes Gap 9 in `docs/algorithmic_gaps.md`.
    OverlapAndPredicted,
}

/// Slim view over one component's data for merging.
///
/// `positions` is shared with the caller; only the slice indices referenced
/// by `labels` are read.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ComponentInput<'a, F: Float> {
    /// All observation positions in this component's frame.
    pub positions: &'a [Point2<F>],
    /// `(i, j) → idx` map (indices into `positions`).
    pub labels: HashMap<Coord, usize>,
    /// Per-component cell size used for the residual gate.
    pub cell_size: F,
}

impl<'a, F: Float> ComponentInput<'a, F> {
    /// Construct a component view from its component pieces.
    pub fn new(positions: &'a [Point2<F>], labels: HashMap<Coord, usize>, cell_size: F) -> Self {
        Self {
            positions,
            labels,
            cell_size,
        }
    }
}

/// Tuning knobs for [`merge_components_local`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct MergeParams<F: Float> {
    /// Lattice-aware symmetry table. Each transform's
    /// [`GridTransform::source_kind`] must match `expected_lattice` or the
    /// orchestrator returns [`MergeError::SymmetryLatticeMismatch`].
    pub symmetry: &'static [GridTransform],
    /// Lattice kind the caller's components live on. Sanity check; mismatches
    /// surface as `Err(MergeError::SymmetryLatticeMismatch)`.
    pub expected_lattice: LatticeKind,
    /// Strategy selector. Default `OverlapOnly`.
    pub mode: MergeMode,
    /// Minimum number of overlapping labels (overlap mode) or predicted
    /// matches (predicted mode) required to accept a merge. Default `2`.
    pub min_overlap: usize,
    /// Position residual tolerance for accepting a merge, expressed as a
    /// fraction of the mean per-component cell size. Default `0.20`.
    pub position_residual_max_rel: F,
    /// Cell-size agreement gate: `|s_a - s_b| / max(s_a, s_b) <= this`
    /// must hold before any merge is attempted. Default `0.20`.
    pub cell_size_disagreement_max: F,
    /// Upper bound on returned components after merging. Default `8`.
    pub max_components: usize,
}

impl<F: Float> Default for MergeParams<F> {
    fn default() -> Self {
        Self {
            symmetry: &crate::lattice::D4_TRANSFORMS,
            expected_lattice: LatticeKind::Square,
            mode: MergeMode::OverlapOnly,
            min_overlap: 2,
            position_residual_max_rel: lit::<F>(0.20_f32),
            cell_size_disagreement_max: lit::<F>(0.20_f32),
            max_components: 8,
        }
    }
}

impl<F: Float> MergeParams<F> {
    /// Construct params from the symmetry table and lattice kind; other
    /// knobs take their defaults.
    pub fn new(symmetry: &'static [GridTransform], expected_lattice: LatticeKind) -> Self {
        Self {
            symmetry,
            expected_lattice,
            ..Self::default()
        }
    }

    /// Override the merge strategy.
    #[must_use]
    pub fn with_mode(mut self, mode: MergeMode) -> Self {
        self.mode = mode;
        self
    }

    /// Override the minimum-overlap threshold.
    #[must_use]
    pub fn with_min_overlap(mut self, n: usize) -> Self {
        self.min_overlap = n;
        self
    }
}

/// Per-pair candidate produced by either sub-merger.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub(crate) struct MergeCandidate<F: Float> {
    pub transform: GridTransform,
    pub delta: Coord,
    pub overlap: usize,
    pub max_residual: F,
    pub merged_labels: HashMap<Coord, Point2<F>>,
}

/// One merged component returned from [`merge_components_local`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MergedComponent<F: Float> {
    /// `(i, j) → position` map, rebased so the bounding-box minimum is
    /// `(0, 0)`.
    pub labels: HashMap<Coord, Point2<F>>,
    /// Per-component cell size.
    pub cell_size: F,
    /// Indices (into the original input slice) of the source components
    /// that were folded into this merged component.
    pub source_indices: Vec<usize>,
}

/// Aggregate result of [`merge_components_local`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MergeReport<F: Float> {
    /// Surviving merged components. Sorted by descending label count.
    pub merged_components: Vec<MergedComponent<F>>,
    /// Number of pairwise merges that passed the geometry gate.
    pub n_merges_accepted: usize,
    /// Number of pairwise merges that failed the geometry gate.
    pub n_merges_rejected: usize,
}

/// Internal working component: owns its positions so the sub-mergers can
/// borrow a stable slice across iterations of the orchestrator.
struct WorkingComponent<F: Float> {
    positions: Vec<Point2<F>>,
    labels: HashMap<Coord, usize>,
    cell_size: F,
    source_indices: Vec<usize>,
}

impl<F: Float> WorkingComponent<F> {
    fn from_input(idx: usize, view: &ComponentInput<'_, F>) -> Self {
        // Compact positions to a tight slice indexed by a freshly-assigned
        // 0..N range; the labels map uses those tight indices.
        let mut positions: Vec<Point2<F>> = Vec::with_capacity(view.labels.len());
        let mut labels: HashMap<Coord, usize> = HashMap::with_capacity(view.labels.len());
        for (&coord, &src_idx) in &view.labels {
            let new_idx = positions.len();
            positions.push(view.positions[src_idx]);
            labels.insert(coord, new_idx);
        }
        Self {
            positions,
            labels,
            cell_size: view.cell_size,
            source_indices: vec![idx],
        }
    }

    fn from_merged(
        merged_labels: HashMap<Coord, Point2<F>>,
        cell_size: F,
        source_indices: Vec<usize>,
    ) -> Self {
        let mut positions: Vec<Point2<F>> = Vec::with_capacity(merged_labels.len());
        let mut labels: HashMap<Coord, usize> = HashMap::with_capacity(merged_labels.len());
        for (coord, pos) in merged_labels {
            let new_idx = positions.len();
            positions.push(pos);
            labels.insert(coord, new_idx);
        }
        Self {
            positions,
            labels,
            cell_size,
            source_indices,
        }
    }

    fn as_input(&self) -> ComponentInput<'_, F> {
        ComponentInput {
            positions: &self.positions,
            labels: self.labels.clone(),
            cell_size: self.cell_size,
        }
    }

    fn into_merged(mut self) -> MergedComponent<F> {
        let mut out_labels: HashMap<Coord, Point2<F>> = HashMap::with_capacity(self.labels.len());
        for (coord, idx) in self.labels.drain() {
            out_labels.insert(coord, self.positions[idx]);
        }
        MergedComponent {
            labels: out_labels,
            cell_size: self.cell_size,
            source_indices: self.source_indices,
        }
    }
}

/// Greedy component merger: iterates pairs largest-first and merges into the
/// larger of the two when the configured gates pass.
///
/// Returns `Err(MergeError::SymmetryLatticeMismatch)` when any transform in
/// `params.symmetry` belongs to a lattice family other than
/// `params.expected_lattice`.
#[cfg_attr(
    feature = "tracing",
    tracing::instrument(level = "info", skip_all, fields(n_components = views.len()))
)]
pub fn merge_components_local<F>(
    views: &[ComponentInput<'_, F>],
    params: &MergeParams<F>,
    sink: &mut impl DiagnosticSink<F>,
) -> Result<MergeReport<F>, MergeError>
where
    F: Float + kiddo::float::kdtree::Axis,
{
    // Lattice-mismatch check.
    for t in params.symmetry {
        if t.source_kind != params.expected_lattice {
            return Err(MergeError::SymmetryLatticeMismatch {
                expected: params.expected_lattice,
                got: t.source_kind,
            });
        }
    }

    if views.is_empty() {
        return Ok(MergeReport {
            merged_components: Vec::new(),
            n_merges_accepted: 0,
            n_merges_rejected: 0,
        });
    }

    let mut working: Vec<WorkingComponent<F>> = views
        .iter()
        .enumerate()
        .map(|(i, v)| WorkingComponent::from_input(i, v))
        .collect();
    let mut alive: Vec<bool> = vec![true; views.len()];
    let mut n_merges_accepted = 0usize;
    let mut n_merges_rejected = 0usize;
    let two = lit::<F>(2.0_f32);

    let mut changed = true;
    while changed {
        changed = false;
        let mut order: Vec<usize> = (0..working.len()).filter(|i| alive[*i]).collect();
        order.sort_by(|a, b| working[*b].labels.len().cmp(&working[*a].labels.len()));

        'outer: for &i in &order {
            for &j in &order {
                if i == j || !alive[i] || !alive[j] {
                    continue;
                }
                let s_i = RealField::max(working[i].cell_size, lit::<F>(1e-3_f32));
                let s_j = RealField::max(working[j].cell_size, lit::<F>(1e-3_f32));
                let denom = if s_i > s_j { s_i } else { s_j };
                let ratio = ComplexField::abs(s_i - s_j) / denom;
                if ratio > params.cell_size_disagreement_max {
                    sink.emit(Event::MergeRejected {
                        a: i,
                        b: j,
                        reason: MergeRejectReason::CellSizeDisagreement,
                    });
                    n_merges_rejected += 1;
                    continue;
                }
                let mean_cell = (s_i + s_j) / two;

                let a_view = working[i].as_input();
                let b_view = working[j].as_input();

                let candidate = match overlap::find_overlap_merge(&a_view, &b_view, params) {
                    Some(c) => Some(c),
                    None => {
                        if params.mode == MergeMode::OverlapAndPredicted {
                            predicted::find_predicted_merge(&a_view, &b_view, params)
                        } else {
                            None
                        }
                    }
                };

                let Some(cand) = candidate else {
                    sink.emit(Event::MergeRejected {
                        a: i,
                        b: j,
                        reason: MergeRejectReason::NoOverlap,
                    });
                    n_merges_rejected += 1;
                    continue;
                };

                sink.emit(Event::MergeAccepted {
                    a: i,
                    b: j,
                    overlap: cand.overlap,
                    max_residual: cand.max_residual,
                });
                n_merges_accepted += 1;

                let mut combined_sources = std::mem::take(&mut working[j].source_indices);
                combined_sources.extend(std::mem::take(&mut working[i].source_indices));
                let new_j =
                    WorkingComponent::from_merged(cand.merged_labels, mean_cell, combined_sources);
                working[j] = new_j;
                alive[i] = false;
                changed = true;
                continue 'outer;
            }
        }
    }

    let mut out: Vec<MergedComponent<F>> = working
        .into_iter()
        .zip(alive.iter().copied())
        .filter_map(|(m, a)| if a { Some(m.into_merged()) } else { None })
        .collect();
    out.sort_by_key(|m| std::cmp::Reverse(m.labels.len()));
    out.truncate(params.max_components);
    for m in &mut out {
        rebase(&mut m.labels);
    }

    Ok(MergeReport {
        merged_components: out,
        n_merges_accepted,
        n_merges_rejected,
    })
}

fn rebase<F: Float>(labels: &mut HashMap<Coord, Point2<F>>) {
    if labels.is_empty() {
        return;
    }
    let min_i = labels.keys().map(|(i, _)| *i).min().unwrap();
    let min_j = labels.keys().map(|(_, j)| *j).min().unwrap();
    if min_i == 0 && min_j == 0 {
        return;
    }
    let rebased: HashMap<Coord, Point2<F>> = labels
        .drain()
        .map(|((i, j), p)| ((i - min_i, j - min_j), p))
        .collect();
    *labels = rebased;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::NoOpSink;
    use crate::lattice::D4_TRANSFORMS;

    fn make_5x5<F: Float>(ox: F, oy: F, s: F) -> ComponentInput<'static, F> {
        let mut positions = Vec::new();
        let mut labels = HashMap::new();
        for j in 0..5 {
            for i in 0..5 {
                let idx = positions.len();
                labels.insert((i, j), idx);
                positions.push(Point2::new(
                    <F as From<f32>>::from(i as f32) * s + ox,
                    <F as From<f32>>::from(j as f32) * s + oy,
                ));
            }
        }
        let leaked: &'static [Point2<F>] = Box::leak(positions.into_boxed_slice());
        ComponentInput {
            positions: leaked,
            labels,
            cell_size: s,
        }
    }

    #[test]
    fn rejects_mismatched_symmetry_table_f32() {
        type F = f32;
        let s: F = 10.0;
        let a = make_5x5::<F>(0.0, 0.0, s);
        let b = make_5x5::<F>(2.0 * s, 0.0, s);
        let params = MergeParams::<F> {
            symmetry: &crate::lattice::D6_TRANSFORMS,
            expected_lattice: LatticeKind::Square,
            ..Default::default()
        };
        let mut sink = NoOpSink;
        let err =
            merge_components_local(&[a, b], &params, &mut sink).expect_err("lattice mismatch");
        assert!(matches!(
            err,
            MergeError::SymmetryLatticeMismatch {
                expected: LatticeKind::Square,
                ..
            }
        ));
    }

    #[test]
    fn overlap_only_merges_shifted_grids_into_one_f32() {
        type F = f32;
        let s: F = 10.0;
        let a = make_5x5::<F>(0.0, 0.0, s);
        let b = make_5x5::<F>(2.0 * s, 0.0, s);
        let params = MergeParams::<F> {
            symmetry: &D4_TRANSFORMS,
            expected_lattice: LatticeKind::Square,
            mode: MergeMode::OverlapOnly,
            min_overlap: 2,
            position_residual_max_rel: 0.20,
            cell_size_disagreement_max: 0.20,
            max_components: 8,
        };
        let mut sink = NoOpSink;
        let report = merge_components_local(&[a, b], &params, &mut sink).unwrap();
        assert_eq!(report.merged_components.len(), 1);
        // 5x5 each, shifted by 2 columns → union is a 5x7 = 35-label grid.
        assert_eq!(report.merged_components[0].labels.len(), 35);
        assert_eq!(report.n_merges_accepted, 1);
    }

    #[test]
    fn overlap_only_does_not_merge_disjoint_components_f32() {
        type F = f32;
        let s: F = 10.0;
        let a = make_5x5::<F>(0.0, 0.0, s);
        let b = make_5x5::<F>(0.0, 6.0 * s, s);
        let params = MergeParams::<F> {
            symmetry: &D4_TRANSFORMS,
            expected_lattice: LatticeKind::Square,
            mode: MergeMode::OverlapOnly,
            min_overlap: 4,
            position_residual_max_rel: 0.20,
            cell_size_disagreement_max: 0.20,
            max_components: 8,
        };
        let mut sink = NoOpSink;
        let report = merge_components_local(&[a, b], &params, &mut sink).unwrap();
        assert_eq!(
            report.merged_components.len(),
            2,
            "OverlapOnly must not merge disjoint components"
        );
    }
}
