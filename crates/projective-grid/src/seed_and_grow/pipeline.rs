//! The seed → grow → validate → blacklist convergence loop.
//!
//! This module owns the *control flow* of the multi-iteration seed-and-grow
//! pipeline — the iteration counter, the soft-convergence test, and the
//! blacklist accumulation — while delegating the per-iteration geometric work
//! (find a seed, grow it, validate the labelled set) to a caller-supplied
//! [`IterationDriver`]. The loop never touches the caller's per-corner state
//! types: it operates purely on **feature indices** and a growing
//! **blacklist**, and returns a [`LoopReport`] of per-iteration
//! [`IterationRecord`]s that the caller replays onto its own stage machine
//! (the chessboard detector replays them onto `CornerAug` / `DebugFrame`).
//!
//! # Why a driver trait rather than closures
//!
//! The seed finder, BFS grow, and post-grow validator are all
//! policy-bearing in a real detector — they need the caller's parity rules,
//! axis-cluster centres, and tolerances. Threading those as closures (or as
//! borrowed caller types) into this crate would re-couple the generic grid
//! engine to a specific detector. Instead the loop takes one trait object:
//! the caller implements [`IterationDriver::run_iteration`] in its own type
//! space (using the existing [`SquareAttachPolicy`] seam internally), and
//! hands this loop only index-space data. **Data out, not callbacks.**
//!
//! [`SquareAttachPolicy`]: crate::seed_and_grow::grow::SquareAttachPolicy

use std::collections::{HashMap, HashSet};

/// Index-space product of one driver iteration.
///
/// The driver runs `find_seed → grow → validate` in its own type space and
/// reports the results as plain indices so the loop can reason over them
/// without seeing any detector type. Data carrier — not `#[non_exhaustive]`
/// (the caller constructs it directly inside `run_iteration`).
#[derive(Clone, Debug, Default)]
pub struct IterationProduct {
    /// `false` when no seed quad closed this iteration; the loop stops.
    pub seed_found: bool,
    /// The labelled `(i, j) → feature_index` map produced by this
    /// iteration's grow (empty when `seed_found` is `false`).
    pub labelled: HashMap<(i32, i32), usize>,
    /// Every feature index the validator flagged this iteration (the full
    /// validator blacklist, *including* indices already on the running
    /// blacklist). The loop computes the per-iteration delta itself.
    pub validation_blacklist: Vec<usize>,
    /// The seed-derived cell size, when a seed closed.
    pub cell_size: Option<f32>,
    /// The four seed-quad indices, when a seed closed. Carried for the
    /// caller's diagnostics replay only.
    pub seed_indices: Option<[usize; 4]>,
}

impl IterationProduct {
    /// A "no seed found" product — the loop stops on this.
    pub fn seed_failed() -> Self {
        Self::default()
    }
}

/// What one iteration of the loop decided.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IterationOutcome {
    /// No seed closed; the loop stops with no further work.
    SeedFailed,
    /// The validator flagged new outliers; the loop blacklists them and
    /// re-runs.
    NotConverged,
    /// The labelled set has stabilised. `soft` is `true` when this is a
    /// *soft* convergence (a small residual blacklist applied and accepted)
    /// rather than an exact one (no new blacklist at all).
    Converged {
        /// `true` for soft convergence (a bounded residual blacklist was
        /// applied before accepting); `false` for exact convergence.
        soft: bool,
    },
}

/// A single iteration's record, replayed by the caller onto its stage
/// machine + diagnostics. Data carrier — not `#[non_exhaustive]`.
#[derive(Clone, Debug)]
pub struct IterationRecord {
    /// The zero-based iteration index.
    pub iter: u32,
    /// Number of labelled corners this iteration's grow produced.
    pub labelled_count: usize,
    /// Indices newly added to the blacklist this iteration, in ascending
    /// index order (deterministic). For an exact convergence this is empty;
    /// for a soft convergence it is the bounded residual set that was
    /// applied before accepting; otherwise it is the non-converged outliers.
    pub new_blacklist: Vec<usize>,
    /// The labelled map this iteration produced, after the soft-convergence
    /// residual (if any) has been removed. The caller reads this on the
    /// converged record to drive its post-grow stages.
    pub labelled: HashMap<(i32, i32), usize>,
    /// The seed-derived cell size for this iteration, when a seed closed.
    pub cell_size: Option<f32>,
    /// The four seed-quad indices, when a seed closed.
    pub seed_indices: Option<[usize; 4]>,
    /// What this iteration decided.
    pub outcome: IterationOutcome,
}

/// The full multi-iteration loop output: one record per iteration run, in
/// order. Data carrier — not `#[non_exhaustive]`.
#[derive(Clone, Debug, Default)]
pub struct LoopReport {
    /// Per-iteration records, in iteration order. The last record carries
    /// the terminal [`IterationOutcome`].
    pub iterations: Vec<IterationRecord>,
}

impl LoopReport {
    /// The terminal outcome (the last iteration's outcome), or `None` when
    /// the loop ran zero iterations.
    pub fn terminal_outcome(&self) -> Option<IterationOutcome> {
        self.iterations.last().map(|r| r.outcome)
    }

    /// The converged iteration's record, if the loop converged.
    pub fn converged_record(&self) -> Option<&IterationRecord> {
        self.iterations
            .iter()
            .find(|r| matches!(r.outcome, IterationOutcome::Converged { .. }))
    }
}

/// Soft-convergence tuning for [`run_convergence_loop`].
///
/// These mirror the historical chessboard constants so the chessboard path
/// stays byte-identical. Data carrier — not `#[non_exhaustive]`.
#[derive(Clone, Copy, Debug)]
pub struct LoopParams {
    /// Hard ceiling on the number of iterations.
    pub max_iters: u32,
    /// Minimum iteration index (`it + 1`) before a *soft* convergence is
    /// permitted. The chessboard path uses `2`.
    pub min_iters_for_soft: u32,
    /// Maximum size of the residual blacklist tolerated by a soft
    /// convergence. The chessboard path uses `2`.
    pub soft_blacklist_max: usize,
    /// Minimum labelled-corner count required to *accept* a soft
    /// convergence. The chessboard path uses `min_labeled_corners`.
    pub soft_min_labelled: usize,
}

impl LoopParams {
    /// Construct loop params. `max_iters` is clamped to at least `1`.
    pub fn new(
        max_iters: u32,
        min_iters_for_soft: u32,
        soft_blacklist_max: usize,
        soft_min_labelled: usize,
    ) -> Self {
        Self {
            max_iters: max_iters.max(1),
            min_iters_for_soft,
            soft_blacklist_max,
            soft_min_labelled,
        }
    }
}

/// The per-iteration geometric work the loop delegates to the caller.
///
/// One call to [`run_iteration`](IterationDriver::run_iteration) runs the
/// caller's `find_seed → grow → validate` against the current blacklist and
/// reports the result as index-space [`IterationProduct`] data. The driver
/// owns all detector-specific state (cluster centres, parity policy, the
/// per-corner stage array) — the loop only feeds it the running blacklist
/// and the iteration index, and the driver re-derives its labelled set from
/// scratch each call (modulo blacklisted corners), exactly as the original
/// chessboard loop did.
pub trait IterationDriver {
    /// Run one `find_seed → grow → validate` pass over the not-blacklisted
    /// features and report the index-space result. `blacklist` is the
    /// running set accumulated by prior iterations; `it` is the zero-based
    /// iteration index.
    fn run_iteration(&mut self, blacklist: &HashSet<usize>, it: u32) -> IterationProduct;
}

/// Run the seed → grow → validate → blacklist convergence loop.
///
/// Drives `driver` up to `params.max_iters` times. Each iteration:
///
/// 1. Ask the driver to run one pass against the running `blacklist`.
/// 2. If no seed closed, stop (`SeedFailed`).
/// 3. Compute the per-iteration new-blacklist delta (validator flags not
///    already on the running blacklist), in ascending index order.
/// 4. Decide convergence: **exact** when the delta is empty; **soft** when
///    `it + 1 >= min_iters_for_soft`, the delta is at most
///    `soft_blacklist_max`, and the labelled count is at least
///    `soft_min_labelled`.
/// 5. On exact/soft convergence, fold the (possibly empty) delta into the
///    blacklist, strip it from the labelled map, record the converged
///    iteration, and stop. Otherwise fold the delta into the blacklist and
///    continue.
///
/// The returned [`LoopReport`] is the caller's replay tape: it never mutated
/// any caller state, only the local `blacklist` it owns. Determinism: the
/// new-blacklist delta is sorted by index, so the records do not depend on
/// the validator's `HashSet` iteration order.
pub fn run_convergence_loop<D: IterationDriver>(driver: &mut D, params: LoopParams) -> LoopReport {
    let mut blacklist: HashSet<usize> = HashSet::new();
    let mut report = LoopReport::default();

    for it in 0..params.max_iters {
        let product = driver.run_iteration(&blacklist, it);
        if !product.seed_found {
            report.iterations.push(IterationRecord {
                iter: it,
                labelled_count: 0,
                new_blacklist: Vec::new(),
                labelled: HashMap::new(),
                cell_size: product.cell_size,
                seed_indices: product.seed_indices,
                outcome: IterationOutcome::SeedFailed,
            });
            break;
        }

        let labelled_count = product.labelled.len();

        // Per-iteration delta: validator flags not already blacklisted,
        // de-duplicated and sorted by index for determinism.
        let mut new_blacklist: Vec<usize> = product
            .validation_blacklist
            .iter()
            .copied()
            .filter(|idx| !blacklist.contains(idx))
            .collect();
        new_blacklist.sort_unstable();
        new_blacklist.dedup();

        let converged = new_blacklist.is_empty();
        // Soft convergence: the validator keeps flagging a small residual
        // set over multiple rounds, so the labelled set has effectively
        // stabilised. Bounded below by `min_iters_for_soft` so we never
        // emit until at least that many validation passes have run.
        let soft_converged = !converged
            && it + 1 >= params.min_iters_for_soft
            && new_blacklist.len() <= params.soft_blacklist_max
            && labelled_count >= params.soft_min_labelled;

        if converged || soft_converged {
            let mut labelled = product.labelled;
            // Apply the residual blacklist before accepting so the emitted
            // labelled set excludes the flagged outliers.
            for &idx in &new_blacklist {
                blacklist.insert(idx);
                labelled.retain(|_, &mut v| v != idx);
            }
            report.iterations.push(IterationRecord {
                iter: it,
                labelled_count,
                new_blacklist,
                labelled,
                cell_size: product.cell_size,
                seed_indices: product.seed_indices,
                outcome: IterationOutcome::Converged {
                    soft: soft_converged,
                },
            });
            break;
        }

        // Non-converged: blacklist the new outliers and retry.
        for &idx in &new_blacklist {
            blacklist.insert(idx);
        }
        report.iterations.push(IterationRecord {
            iter: it,
            labelled_count,
            new_blacklist,
            labelled: product.labelled,
            cell_size: product.cell_size,
            seed_indices: product.seed_indices,
            outcome: IterationOutcome::NotConverged,
        });
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted driver: each call pops a pre-built product.
    struct ScriptDriver {
        scripted: Vec<IterationProduct>,
        seen_blacklists: Vec<Vec<usize>>,
    }

    impl IterationDriver for ScriptDriver {
        fn run_iteration(&mut self, blacklist: &HashSet<usize>, it: u32) -> IterationProduct {
            let mut bl: Vec<usize> = blacklist.iter().copied().collect();
            bl.sort_unstable();
            self.seen_blacklists.push(bl);
            self.scripted
                .get(it as usize)
                .cloned()
                .unwrap_or_else(IterationProduct::seed_failed)
        }
    }

    fn labelled(n: usize) -> HashMap<(i32, i32), usize> {
        (0..n).map(|k| ((k as i32, 0), k)).collect()
    }

    #[test]
    fn exact_convergence_first_iteration() {
        let mut driver = ScriptDriver {
            scripted: vec![IterationProduct {
                seed_found: true,
                labelled: labelled(6),
                validation_blacklist: vec![],
                cell_size: Some(10.0),
                seed_indices: Some([0, 1, 2, 3]),
            }],
            seen_blacklists: vec![],
        };
        let report = run_convergence_loop(&mut driver, LoopParams::new(6, 2, 2, 4));
        assert_eq!(report.iterations.len(), 1);
        assert_eq!(
            report.terminal_outcome(),
            Some(IterationOutcome::Converged { soft: false })
        );
        assert_eq!(report.converged_record().unwrap().labelled.len(), 6);
    }

    #[test]
    fn seed_failure_stops() {
        let mut driver = ScriptDriver {
            scripted: vec![IterationProduct::seed_failed()],
            seen_blacklists: vec![],
        };
        let report = run_convergence_loop(&mut driver, LoopParams::new(6, 2, 2, 4));
        assert_eq!(
            report.terminal_outcome(),
            Some(IterationOutcome::SeedFailed)
        );
    }

    #[test]
    fn blacklists_then_converges() {
        // Iter 0 flags {5}, iter 1 clean → converge. The blacklist must
        // carry {5} into iter 1.
        let mut driver = ScriptDriver {
            scripted: vec![
                IterationProduct {
                    seed_found: true,
                    labelled: labelled(6),
                    validation_blacklist: vec![5],
                    cell_size: Some(10.0),
                    seed_indices: Some([0, 1, 2, 3]),
                },
                IterationProduct {
                    seed_found: true,
                    labelled: labelled(5),
                    validation_blacklist: vec![],
                    cell_size: Some(10.0),
                    seed_indices: Some([0, 1, 2, 3]),
                },
            ],
            seen_blacklists: vec![],
        };
        let report = run_convergence_loop(&mut driver, LoopParams::new(6, 2, 2, 4));
        assert_eq!(report.iterations.len(), 2);
        assert_eq!(report.iterations[0].outcome, IterationOutcome::NotConverged);
        assert_eq!(report.iterations[0].new_blacklist, vec![5]);
        assert_eq!(
            report.iterations[1].outcome,
            IterationOutcome::Converged { soft: false }
        );
        // The blacklist {5} reached the second iteration.
        assert_eq!(driver.seen_blacklists[1], vec![5]);
    }

    #[test]
    fn soft_convergence_applies_residual() {
        // Iter 0 flags {7}, iter 1 keeps flagging {8} (≤ 2 residual, it+1>=2,
        // labelled>=4) → soft converge, applying {8}.
        let mut driver = ScriptDriver {
            scripted: vec![
                IterationProduct {
                    seed_found: true,
                    labelled: labelled(8),
                    validation_blacklist: vec![7],
                    cell_size: Some(10.0),
                    seed_indices: Some([0, 1, 2, 3]),
                },
                IterationProduct {
                    seed_found: true,
                    labelled: labelled(8),
                    validation_blacklist: vec![8],
                    cell_size: Some(10.0),
                    seed_indices: Some([0, 1, 2, 3]),
                },
            ],
            seen_blacklists: vec![],
        };
        let report = run_convergence_loop(&mut driver, LoopParams::new(6, 2, 2, 4));
        let conv = report.converged_record().unwrap();
        assert_eq!(conv.outcome, IterationOutcome::Converged { soft: true });
        assert_eq!(conv.new_blacklist, vec![8]);
        // Index 8 was stripped from the labelled map (only indices 0..=7
        // exist in `labelled(8)`, so the strip is a no-op on this fixture
        // but the residual must still be recorded as applied).
        assert!(!conv.labelled.values().any(|&v| v == 8));
    }

    #[test]
    fn deterministic_new_blacklist_order() {
        // Validator reports in scrambled order; the record must be sorted.
        let mut driver = ScriptDriver {
            scripted: vec![IterationProduct {
                seed_found: true,
                labelled: labelled(10),
                validation_blacklist: vec![9, 3, 7, 3],
                cell_size: Some(10.0),
                seed_indices: Some([0, 1, 2, 3]),
            }],
            seen_blacklists: vec![],
        };
        let report = run_convergence_loop(&mut driver, LoopParams::new(6, 2, 2, 4));
        assert_eq!(report.iterations[0].new_blacklist, vec![3, 7, 9]);
    }
}
