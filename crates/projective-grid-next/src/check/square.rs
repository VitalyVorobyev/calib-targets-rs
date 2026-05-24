//! `check_square_labels`: consistency-check task for an existing labelled
//! square grid.
//!
//! Given a caller-supplied `(i, j) → observation_idx` map, run the
//! [`mod@crate::validate`] precision gate (line collinearity, local-H residuals,
//! per-edge length band, axis-slot parity) and report which entries the gate
//! drops. Optionally enforces a single-component constraint via cardinal-
//! adjacency BFS over the labelled set.
//!
//! `check` is the orthogonal task to `detect`: where `detect` produces labels
//! from scratch, `check` verifies labels produced elsewhere (a deterministic
//! decoder, a manually-curated overlay, a previous run of `detect`). The
//! shared validate gate guarantees the two tasks make precision decisions
//! against the same predicates.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::diagnostics::DiagnosticSink;
use crate::error::ConsistencyError;
use crate::feature::Observation;
use crate::float::Float;
use crate::lattice::Coord;
use crate::policy::LabelPolicy;
use crate::validate::{validate, EdgeFailure, LabelledEntry, ValidationParams};

/// Tuning knobs for [`check_square_labels`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct CheckParams<F: Float> {
    /// Precision-gate tunables. See [`ValidationParams`].
    pub validate: ValidationParams<F>,
    /// Require the labelled set to form a single 4-connected component under
    /// cardinal `(i, j)` adjacency. Default `true` — multiple disconnected
    /// label patches indicate a labelling error in most calibration
    /// pipelines.
    pub require_one_component: bool,
}

impl<F: Float> Default for CheckParams<F> {
    fn default() -> Self {
        Self {
            validate: ValidationParams::default(),
            require_one_component: true,
        }
    }
}

impl<F: Float> CheckParams<F> {
    /// Construct check params; pass the validation gate explicitly and let
    /// the single-component toggle default to `true`.
    pub fn new(validate: ValidationParams<F>) -> Self {
        Self {
            validate,
            require_one_component: true,
        }
    }

    /// Override the single-component requirement.
    #[must_use]
    pub fn with_require_one_component(mut self, on: bool) -> Self {
        self.require_one_component = on;
        self
    }
}

/// Outcome of [`check_square_labels`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CheckReport<F: Float> {
    /// `true` iff the labels pass every active gate. Equivalent to
    /// `blacklist.is_empty() && (n_components == 1 || !require_one_component)`.
    pub passed: bool,
    /// Observation indices that fail the validation gate.
    pub blacklist: HashSet<usize>,
    /// Per-corner local-H residuals (only computed for corners with at
    /// least four non-collinear labelled neighbours).
    pub local_h_residuals: HashMap<usize, F>,
    /// Per-corner edge-length-band failure records.
    pub edge_failures: HashMap<usize, EdgeFailure<F>>,
    /// Number of 4-connected components in the labelled set.
    pub n_components: usize,
}

impl<F: Float> CheckReport<F> {
    /// Construct a check report from its constituent fields.
    pub fn new(
        passed: bool,
        blacklist: HashSet<usize>,
        local_h_residuals: HashMap<usize, F>,
        edge_failures: HashMap<usize, EdgeFailure<F>>,
        n_components: usize,
    ) -> Self {
        Self {
            passed,
            blacklist,
            local_h_residuals,
            edge_failures,
            n_components,
        }
    }
}

/// Verify a caller-supplied square-grid labelling.
///
/// Runs the [`mod@crate::validate`] gate and (when `params.require_one_component`)
/// the 4-connectivity check over `labelled`. Empty labels are treated as
/// vacuously consistent and return `passed = true, n_components = 0`.
///
/// # Errors
///
/// * [`ConsistencyError::CountMismatch`] when `labelled` references an
///   observation index outside `0..observations.len()`.
pub fn check_square_labels<F, S>(
    observations: &[Observation<F>],
    labelled: &HashMap<Coord, usize>,
    cell_size: F,
    policy: &LabelPolicy<F>,
    params: &CheckParams<F>,
    sink: &mut S,
) -> Result<CheckReport<F>, ConsistencyError>
where
    F: Float,
    S: DiagnosticSink<F>,
{
    // Sanity: every referenced observation index must be in range.
    for &idx in labelled.values() {
        if idx >= observations.len() {
            return Err(ConsistencyError::CountMismatch {
                positions: observations.len(),
                labels: labelled.len(),
            });
        }
    }

    if labelled.is_empty() {
        return Ok(CheckReport::new(
            true,
            HashSet::new(),
            HashMap::new(),
            HashMap::new(),
            0,
        ));
    }

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

    let n_components = count_components(labelled);
    let component_ok = !params.require_one_component || n_components == 1;
    let passed = result.blacklist.is_empty() && component_ok;

    Ok(CheckReport::new(
        passed,
        result.blacklist,
        result.local_h_residuals,
        result.edge_failures,
        n_components,
    ))
}

fn count_components(labelled: &HashMap<Coord, usize>) -> usize {
    if labelled.is_empty() {
        return 0;
    }
    let cells: HashSet<Coord> = labelled.keys().copied().collect();
    let mut visited: HashSet<Coord> = HashSet::with_capacity(cells.len());
    let mut n = 0usize;
    for &start in &cells {
        if visited.contains(&start) {
            continue;
        }
        n += 1;
        let mut q: VecDeque<Coord> = VecDeque::new();
        q.push_back(start);
        visited.insert(start);
        while let Some((i, j)) = q.pop_front() {
            for (di, dj) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let next = (i + di, j + dj);
                if cells.contains(&next) && visited.insert(next) {
                    q.push_back(next);
                }
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::NoOpSink;
    use crate::feature::Observation;
    use crate::float::lit;
    use nalgebra::Point2;

    fn clean_grid<F: Float>(
        rows: i32,
        cols: i32,
        s: F,
    ) -> (Vec<Observation<F>>, HashMap<Coord, usize>) {
        let origin = lit::<F>(50.0_f32);
        let mut obs = Vec::with_capacity((rows * cols) as usize);
        let mut labels: HashMap<Coord, usize> = HashMap::new();
        for j in 0..rows {
            for i in 0..cols {
                let idx = obs.len();
                obs.push(Observation::new(Point2::new(
                    lit::<F>(i as f32) * s + origin,
                    lit::<F>(j as f32) * s + origin,
                )));
                labels.insert((i, j), idx);
            }
        }
        (obs, labels)
    }

    fn assert_clean_grid_passes<F: Float>() {
        let s = lit::<F>(25.0_f32);
        let (obs, labels) = clean_grid::<F>(5, 5, s);
        let policy = LabelPolicy::<F>::builder(obs.len()).build();
        let mut sink = NoOpSink;
        let report = check_square_labels(
            &obs,
            &labels,
            s,
            &policy,
            &CheckParams::default(),
            &mut sink,
        )
        .unwrap();
        assert!(report.passed, "{:?}", report.blacklist);
        assert!(report.blacklist.is_empty());
        assert_eq!(report.n_components, 1);
    }

    fn assert_empty_labels_pass_vacuously<F: Float>() {
        let obs: Vec<Observation<F>> = Vec::new();
        let labels: HashMap<Coord, usize> = HashMap::new();
        let policy = LabelPolicy::<F>::builder(0).build();
        let mut sink = NoOpSink;
        let report = check_square_labels(
            &obs,
            &labels,
            lit::<F>(1.0_f32),
            &policy,
            &CheckParams::default(),
            &mut sink,
        )
        .unwrap();
        assert!(report.passed);
        assert_eq!(report.n_components, 0);
    }

    fn assert_out_of_range_index_returns_count_mismatch<F: Float>() {
        let (obs, _) = clean_grid::<F>(3, 3, lit::<F>(10.0_f32));
        let mut labels: HashMap<Coord, usize> = HashMap::new();
        labels.insert((0, 0), obs.len() + 1); // out of range
        let policy = LabelPolicy::<F>::builder(obs.len()).build();
        let mut sink = NoOpSink;
        let err = check_square_labels(
            &obs,
            &labels,
            lit::<F>(10.0_f32),
            &policy,
            &CheckParams::default(),
            &mut sink,
        )
        .expect_err("out-of-range index");
        assert!(matches!(err, ConsistencyError::CountMismatch { .. }));
    }

    fn assert_disconnected_labels_fail_when_required<F: Float>() {
        let (obs, mut labels) = clean_grid::<F>(3, 3, lit::<F>(10.0_f32));
        // Drop the bridge so the labels split into two components.
        labels.remove(&(1, 0));
        labels.remove(&(1, 1));
        labels.remove(&(1, 2));
        let policy = LabelPolicy::<F>::builder(obs.len()).build();
        let mut sink = NoOpSink;
        let report = check_square_labels(
            &obs,
            &labels,
            lit::<F>(10.0_f32),
            &policy,
            &CheckParams::default(),
            &mut sink,
        )
        .unwrap();
        assert_eq!(report.n_components, 2);
        assert!(!report.passed);
    }

    #[test]
    fn clean_grid_passes_f32() {
        assert_clean_grid_passes::<f32>();
    }
    #[test]
    fn clean_grid_passes_f64() {
        assert_clean_grid_passes::<f64>();
    }
    #[test]
    fn empty_labels_pass_vacuously_f32() {
        assert_empty_labels_pass_vacuously::<f32>();
    }
    #[test]
    fn empty_labels_pass_vacuously_f64() {
        assert_empty_labels_pass_vacuously::<f64>();
    }
    #[test]
    fn out_of_range_index_returns_count_mismatch_f32() {
        assert_out_of_range_index_returns_count_mismatch::<f32>();
    }
    #[test]
    fn out_of_range_index_returns_count_mismatch_f64() {
        assert_out_of_range_index_returns_count_mismatch::<f64>();
    }
    #[test]
    fn disconnected_labels_fail_when_required_f32() {
        assert_disconnected_labels_fail_when_required::<f32>();
    }
    #[test]
    fn disconnected_labels_fail_when_required_f64() {
        assert_disconnected_labels_fail_when_required::<f64>();
    }
}
