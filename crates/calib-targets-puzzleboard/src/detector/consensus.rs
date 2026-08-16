//! Period-3 edge consensus — the master code's redundancy, read before
//! registration.
//!
//! # The structure
//!
//! Both shipped maps are cyclic with a period of **3** on their short axis
//! (see [`crate::code_maps`]):
//!
//! | edge family | bit at `(mr, mc)` | row period | col period |
//! |---|---|---|---|
//! | vertical | `map_a[mr % 3][mc % 167]` | **3** | 167 |
//! | horizontal | `map_b[mr % 167][mc % 3]` | 167 | **3** |
//!
//! A horizontal edge therefore carries the same bit as the horizontal edge
//! three columns away, and a vertical edge the same as the vertical edge three
//! rows away — in both cases, **three steps along the edge's own direction**.
//! The paper (arXiv:2409.20127) names this as the intended error-correction
//! mechanism: *"As each bit is repeated every three rows or columns, its
//! correct value can be derived by majority voting when more rows and columns
//! are visible."*
//!
//! # Why this runs before registration
//!
//! Two invariants make the partition computable from the *local* grid alone,
//! with no origin and no orientation hypothesis. Both are pinned by tests
//! below.
//!
//! 1. **Origin-independence.** Local coordinates reach the master through
//!    `master = transform(local) + origin`. A translation shifts every member
//!    of a class equally, so it only *renames* classes — it never moves an
//!    observation from one class to another.
//! 2. **Rotation-independence.** A 90° rotation carries a local horizontal edge
//!    onto a master *vertical* edge, and carries a `+3` step along the edge's
//!    own direction onto a `±3` step along the target edge's own direction —
//!    which is exactly the period the target family is cyclic in. So the
//!    partition is identical under every element of D4 (and a fortiori of C4).
//!
//! Together: whatever the true pose, members of one class predict the *same*
//! master bit under *every* hypothesis. That is what licenses both uses below.
//!
//! # How the two scoring paths use it differently
//!
//! - The **hard** scorer counts matched dots, having already discarded the
//!   per-dot confidence distribution. Feeding it the voted classes is strictly
//!   better input, and it is the paper's prescription applied where it belongs:
//!   the fragment's `~2w²` dots collapse to the `~6w` code bits they actually
//!   carry.
//! - The **soft** scorer must *not* be pre-voted. Because class members predict
//!   the same bit under every hypothesis, their per-dot log-likelihoods always
//!   add — the existing sum already *is* the optimal combination of the
//!   replicas. Voting first would be a hard decision taken too early, throwing
//!   away exactly the information that path exists to use.
//!
//! Both paths do share the class *count*: it is the fragment's true code
//! length, and the accept/reject gates are statements about that length rather
//! than about the physical dot count.

use std::collections::BTreeMap;

use crate::code_maps::{EdgeOrientation, PuzzleBoardObservedEdge};

/// Cyclic period shared by both maps on their short axis.
///
/// `map_a` is `3 × 167` and `map_b` is `167 × 3`; this is the `3`.
const CODE_PERIOD: i32 = 3;

/// Class key: the orientation plus the two coordinates that survive the
/// reduction — one exact, one taken modulo [`CODE_PERIOD`].
///
/// Ordered (rather than hashed) so the emitted class order is a pure function
/// of the observation *set*, never of hash iteration order.
type ClassKey = (u8, i32, i32);

/// Reduce an observation to the logical code bit it reads.
///
/// Horizontal edges are periodic in `col`, vertical edges in `row` — see the
/// module docs for why that is "three steps along the edge's own direction" in
/// both cases.
#[inline]
fn class_key(e: &PuzzleBoardObservedEdge) -> ClassKey {
    match e.orientation {
        EdgeOrientation::Horizontal => (0, e.row, e.col.rem_euclid(CODE_PERIOD)),
        EdgeOrientation::Vertical => (1, e.row.rem_euclid(CODE_PERIOD), e.col),
    }
}

/// One logical code bit, with every physical dot that reads it.
#[derive(Clone, Debug)]
pub(crate) struct EdgeClass {
    /// The first member, in observation order. Any member serves: they all look
    /// up the same master bit under every origin and every searched transform,
    /// so the representative's `(row, col)` is a valid stand-in for the class.
    representative: PuzzleBoardObservedEdge,
    /// Dot counts supporting bit 0 / bit 1.
    count: [u32; 2],
    /// Summed confidence supporting bit 0 / bit 1.
    weight: [f32; 2],
}

impl EdgeClass {
    /// The confidence-weighted majority bit, or `None` when the vote is an
    /// exact tie.
    ///
    /// **A tie is an erasure, not a guess.** Every downstream consumer of a
    /// voted bit — the hard scorer, the BER budget, the `margin > k_winner`
    /// uniqueness proof — counts it as *one observation*. A class whose two
    /// sides carry identical confidence mass observed nothing, so emitting
    /// either bit would feed a coin flip into a count that is supposed to be
    /// evidence. Measured: at the minimum window each bit is read exactly
    /// twice, so one corrupted dot ties its class, and guessing produced
    /// wrong-origin decodes at 10 % and 25 % dot corruption
    /// (`consensus_noise_tolerance_report`). Dropping the class instead makes
    /// those misses, which the detection contract permits.
    ///
    /// The line is drawn at *exactly* zero net evidence because that is the one
    /// place it can be drawn without inventing a constant: "did this class
    /// observe anything" is constant-free, while "did it observe *enough*"
    /// would need a threshold. Weak-but-nonzero evidence is already discounted
    /// — through the confidence carried by [`Self::as_observation`], and on the
    /// soft path through the per-dot log-likelihoods, which never take a hard
    /// decision at all.
    #[inline]
    pub(crate) fn voted_bit(&self) -> Option<u8> {
        match self.weight[1].partial_cmp(&self.weight[0])? {
            std::cmp::Ordering::Greater => Some(1),
            std::cmp::Ordering::Less => Some(0),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// How many physical dots read this bit.
    #[inline]
    pub(crate) fn multiplicity(&self) -> u32 {
        self.count[0] + self.count[1]
    }

    /// Confidence mass on the losing side of the vote — zero for a unanimous
    /// class, and zero by construction for a singleton.
    #[inline]
    fn dissent_weight(&self) -> f32 {
        self.weight[0].min(self.weight[1])
    }

    /// Total confidence mass over all members.
    #[inline]
    fn total_weight(&self) -> f32 {
        self.weight[0] + self.weight[1]
    }

    /// The class as a single observation carrying the voted bit, or `None` for
    /// an erased (tied) class.
    ///
    /// The confidence is the *net* evidence per member,
    /// `(w_win − w_lose) / multiplicity`: a unanimous class keeps the mean
    /// confidence of its dots, a split class is discounted in proportion to how
    /// split it is, and a singleton is returned unchanged.
    fn as_observation(&self) -> Option<PuzzleBoardObservedEdge> {
        let bit = self.voted_bit()?;
        let (win, lose) = (self.weight[bit as usize], self.weight[1 - bit as usize]);
        let confidence = ((win - lose) / self.multiplicity() as f32).clamp(0.0, 1.0);
        Some(PuzzleBoardObservedEdge {
            bit,
            confidence,
            ..self.representative
        })
    }
}

/// The period-3 reduction of an observation set.
#[derive(Clone, Debug)]
pub(crate) struct EdgeConsensus {
    classes: Vec<EdgeClass>,
    dissent_weight: f32,
    replicated_weight: f32,
}

impl EdgeConsensus {
    /// Partition `observed` into logical code bits and vote within each.
    ///
    /// `O(N log k)` in the number of dots, for `k ≈ 6w` classes. Runs on the
    /// *local* grid frame — no origin, no orientation. See the module docs.
    pub(crate) fn build(observed: &[PuzzleBoardObservedEdge]) -> Self {
        let mut by_key: BTreeMap<ClassKey, EdgeClass> = BTreeMap::new();
        for e in observed {
            let slot = by_key.entry(class_key(e)).or_insert_with(|| EdgeClass {
                representative: *e,
                count: [0; 2],
                weight: [0.0; 2],
            });
            let b = usize::from(e.bit != 0);
            slot.count[b] += 1;
            slot.weight[b] += e.confidence;
        }
        let classes: Vec<EdgeClass> = by_key.into_values().collect();
        let (dissent_weight, replicated_weight) = classes
            .iter()
            .filter(|c| c.multiplicity() >= 2)
            .fold((0.0, 0.0), |(d, t), c| {
                (d + c.dissent_weight(), t + c.total_weight())
            });
        Self {
            classes,
            dissent_weight,
            replicated_weight,
        }
    }

    /// Fraction of confidence mass that lost its class vote, over the classes
    /// that had a vote to lose (multiplicity ≥ 2).
    ///
    /// A **hypothesis-free** read-quality meter: it needs no origin, no
    /// orientation and no successful decode, so it is defined even when the
    /// decode fails. Near zero on a clean board; large when the dots are noisy
    /// *or* when the grid labelling is wrong, since a mislabelled grid mixes
    /// observations that read different master bits into one class.
    ///
    /// It is a monotone proxy for the raw dot error rate, not a calibrated
    /// estimate of it: the map from error rate to dissent depends on class
    /// multiplicity, and it saturates well below `1` (a two-member class can
    /// never put more than half its mass on the losing side). Read it as
    /// "clean" versus "not clean", not as a probability.
    #[inline]
    pub(crate) fn dissent_rate(&self) -> f32 {
        if self.replicated_weight <= 0.0 {
            0.0
        } else {
            self.dissent_weight / self.replicated_weight
        }
    }

    /// The voted classes as an observation set, one entry per logical bit that
    /// the fragment actually determined.
    ///
    /// This is what the hard scorer decodes and what both gates are judged
    /// over. Tied classes are **erased** rather than guessed — see
    /// [`EdgeClass::voted_bit`] — so this can be shorter than the class count,
    /// and the difference is bits the fragment read but could not resolve.
    ///
    /// Deterministic in the input set: classes come out in key order, never in
    /// hash order.
    pub(crate) fn class_observations(&self) -> Vec<PuzzleBoardObservedEdge> {
        self.classes
            .iter()
            .filter_map(EdgeClass::as_observation)
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn classes(&self) -> &[EdgeClass] {
        &self.classes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_maps::{horizontal_edge_bit, vertical_edge_bit};
    use crate::detector::decode::transform_edge_lookup;
    use calib_targets_core::{GridTransform, GRID_TRANSFORMS_D4};
    use std::collections::BTreeSet;

    fn edge(
        orientation: EdgeOrientation,
        row: i32,
        col: i32,
        bit: u8,
        confidence: f32,
    ) -> PuzzleBoardObservedEdge {
        PuzzleBoardObservedEdge {
            row,
            col,
            orientation,
            bit,
            confidence,
        }
    }

    /// Build the exact edge set a `w × w`-square fragment at `(origin_row,
    /// origin_col)` presents to the sampler: interior edges only, because a dot
    /// is read against the two squares flanking it and the outermost ring has
    /// only one.
    fn perfect_fragment(w: i32, origin_row: i32, origin_col: i32) -> Vec<PuzzleBoardObservedEdge> {
        let mut out = Vec::new();
        for r in 0..w {
            for c in 0..w {
                let (mr, mc) = (origin_row + r, origin_col + c);
                if r + 1 < w {
                    out.push(edge(
                        EdgeOrientation::Horizontal,
                        r,
                        c,
                        horizontal_edge_bit(mr, mc),
                        1.0,
                    ));
                }
                if c + 1 < w {
                    out.push(edge(
                        EdgeOrientation::Vertical,
                        r,
                        c,
                        vertical_edge_bit(mr, mc),
                        1.0,
                    ));
                }
            }
        }
        out
    }

    /// The partition as a canonical set-of-sets over observation indices.
    fn partition(observed: &[PuzzleBoardObservedEdge]) -> BTreeSet<Vec<usize>> {
        let mut groups: BTreeMap<ClassKey, Vec<usize>> = BTreeMap::new();
        for (i, e) in observed.iter().enumerate() {
            groups.entry(class_key(e)).or_default().push(i);
        }
        groups.into_values().collect()
    }

    /// The class an observation lands in *after* the decoder has resolved it
    /// into a transform's master frame — the same reduction [`class_key`]
    /// performs locally, but applied to the production lookup cell.
    fn master_class_key(e: &PuzzleBoardObservedEdge, t: &GridTransform) -> ClassKey {
        let l = transform_edge_lookup(e, t);
        match l.orientation {
            EdgeOrientation::Horizontal => (0, l.lookup_row, l.lookup_col.rem_euclid(CODE_PERIOD)),
            EdgeOrientation::Vertical => (1, l.lookup_row.rem_euclid(CODE_PERIOD), l.lookup_col),
        }
    }

    /// The claim the whole module rests on: grouping observations *locally*,
    /// with no pose at all, yields exactly the grouping the decoder would get
    /// after resolving them into any transform's master frame.
    ///
    /// Checked against the production [`transform_edge_lookup`] rather than a
    /// re-derivation of it, so a change to the edge-anchoring convention breaks
    /// this test instead of silently invalidating the reduction.
    #[test]
    fn local_partition_equals_the_master_partition_under_every_d4_transform() {
        let observed = perfect_fragment(8, 40, 17);
        let local = partition(&observed);
        for t in GRID_TRANSFORMS_D4 {
            let mut groups: BTreeMap<ClassKey, Vec<usize>> = BTreeMap::new();
            for (i, e) in observed.iter().enumerate() {
                groups.entry(master_class_key(e, &t)).or_default().push(i);
            }
            let in_master: BTreeSet<Vec<usize>> = groups.into_values().collect();
            assert_eq!(
                in_master,
                local,
                "D4 transform {:?} repartitioned the observations",
                t.matrix()
            );
        }
    }

    /// Members of one class predict the same master bit under every transform
    /// and every origin — the property that lets a single representative stand
    /// in for the class.
    #[test]
    fn class_members_agree_on_the_expected_bit_under_every_transform() {
        let observed = perfect_fragment(8, 40, 17);
        for t in GRID_TRANSFORMS_D4 {
            for (origin_row, origin_col) in [(0, 0), (40, 17), (1, 2), (166, 300)] {
                let mut expected: BTreeMap<ClassKey, u8> = BTreeMap::new();
                for e in &observed {
                    let l = transform_edge_lookup(e, &t);
                    let (mr, mc) = (l.lookup_row + origin_row, l.lookup_col + origin_col);
                    let bit = match l.orientation {
                        EdgeOrientation::Horizontal => horizontal_edge_bit(mr, mc),
                        EdgeOrientation::Vertical => vertical_edge_bit(mr, mc),
                    };
                    let key = class_key(e);
                    let seen = *expected.entry(key).or_insert(bit);
                    assert_eq!(
                        seen,
                        bit,
                        "class {key:?} disagreed on its master bit under {:?} at origin \
                         ({origin_row}, {origin_col})",
                        t.matrix()
                    );
                }
            }
        }
    }

    #[test]
    fn partition_is_invariant_under_an_origin_shift() {
        let observed = perfect_fragment(8, 40, 17);
        let reference = partition(&observed);
        for (dr, dc) in [(1, 0), (0, 1), (2, 5), (-7, 13), (167, 3)] {
            let moved: Vec<_> = observed
                .iter()
                .map(|e| PuzzleBoardObservedEdge {
                    row: e.row + dr,
                    col: e.col + dc,
                    ..*e
                })
                .collect();
            assert_eq!(
                partition(&moved),
                reference,
                "shift ({dr}, {dc}) repartitioned the observations"
            );
        }
    }

    #[test]
    fn a_clean_fragment_has_zero_dissent_and_unanimous_classes() {
        for origin in [(0, 0), (40, 17), (498, 3), (166, 500)] {
            let observed = perfect_fragment(9, origin.0, origin.1);
            let consensus = EdgeConsensus::build(&observed);
            assert_eq!(
                consensus.dissent_rate(),
                0.0,
                "clean fragment at {origin:?} disagreed with itself"
            );
            for c in consensus.classes() {
                assert!(
                    c.count[0] == 0 || c.count[1] == 0,
                    "class at {origin:?} split despite noise-free dots"
                );
            }
        }
    }

    /// The count identity the gates depend on: an interior readout of a
    /// `w`-square fragment carries `6(w-1)` distinct bits, not `2w(w-1)`.
    #[test]
    fn logical_bits_are_six_per_ring_not_the_dot_count() {
        for w in 4..=12 {
            let observed = perfect_fragment(w, 40, 17);
            let consensus = EdgeConsensus::build(&observed);
            assert_eq!(
                observed.len() as i32,
                2 * w * (w - 1),
                "interior dot count for w={w}"
            );
            assert_eq!(
                consensus.classes().len() as i32,
                6 * (w - 1),
                "logical bit count for w={w}"
            );
        }
    }

    /// The published equivalence, in our own units: a 7-corner span (6 squares)
    /// carries the 30 bits the paper attributes to its 4×4-piece fragment.
    #[test]
    fn a_seven_corner_span_carries_thirty_bits() {
        let consensus = EdgeConsensus::build(&perfect_fragment(6, 40, 17));
        assert_eq!(consensus.classes().len(), 30);
    }

    #[test]
    fn voting_repairs_a_minority_of_corrupted_dots() {
        let clean = perfect_fragment(10, 40, 17);
        // A 10-square fragment reads each bit 3 times, so a single corruption
        // per class is a minority and must be outvoted.
        let mut noisy = clean.clone();
        let mut seen: BTreeSet<ClassKey> = BTreeSet::new();
        let mut corrupted = 0usize;
        for e in noisy.iter_mut() {
            if seen.insert(class_key(e)) {
                e.bit ^= 1;
                corrupted += 1;
            }
        }
        assert!(corrupted > 0);

        let consensus = EdgeConsensus::build(&noisy);
        assert!(consensus.dissent_rate() > 0.0, "corruption went unnoticed");

        let voted = consensus.class_observations();
        let reference = EdgeConsensus::build(&clean).class_observations();
        assert_eq!(voted.len(), reference.len());
        for (got, want) in voted.iter().zip(reference.iter()) {
            assert_eq!(
                (got.row, got.col, got.bit),
                (want.row, want.col, want.bit),
                "vote failed to repair a minority corruption"
            );
        }
    }

    #[test]
    fn a_singleton_class_passes_through_unchanged() {
        let observed = vec![edge(EdgeOrientation::Horizontal, 4, 1, 1, 0.42)];
        let consensus = EdgeConsensus::build(&observed);
        assert_eq!(consensus.classes().len(), 1);
        assert_eq!(consensus.dissent_rate(), 0.0);
        let out = consensus.class_observations();
        assert_eq!(out[0].bit, 1);
        assert!((out[0].confidence - 0.42).abs() < 1e-6);
    }

    #[test]
    fn a_partial_split_is_discounted_relative_to_a_unanimous_one() {
        let confidence_of = |dots: &[PuzzleBoardObservedEdge]| -> Option<f32> {
            let out = EdgeConsensus::build(dots).class_observations();
            assert!(out.len() <= 1, "these fixtures are all one class");
            out.first().map(|e| e.confidence)
        };
        let unanimous = confidence_of(&[
            edge(EdgeOrientation::Vertical, 0, 5, 1, 0.9),
            edge(EdgeOrientation::Vertical, 3, 5, 1, 0.9),
            edge(EdgeOrientation::Vertical, 6, 5, 1, 0.9),
        ])
        .expect("unanimous class survives");
        let outvoted = confidence_of(&[
            edge(EdgeOrientation::Vertical, 0, 5, 1, 0.9),
            edge(EdgeOrientation::Vertical, 3, 5, 1, 0.9),
            edge(EdgeOrientation::Vertical, 6, 5, 0, 0.9),
        ])
        .expect("2-1 split still has a majority");
        assert!((unanimous - 0.9).abs() < 1e-6);
        assert!(
            outvoted < unanimous,
            "a 2-1 split must carry less evidence than a 3-0 one: {outvoted} vs {unanimous}"
        );
    }

    /// A class with no majority is an **erasure**, not a coin flip: it must not
    /// reach the decoder at all. Guessing it fed a random bit into the
    /// count-based gates and produced wrong-origin decodes — see
    /// [`EdgeClass::voted_bit`].
    #[test]
    fn a_tied_class_is_erased_rather_than_guessed() {
        for (a, b) in [(1u8, 0u8), (0, 1)] {
            let tied = EdgeConsensus::build(&[
                edge(EdgeOrientation::Vertical, 0, 5, a, 0.9),
                edge(EdgeOrientation::Vertical, 3, 5, b, 0.9),
            ]);
            assert_eq!(tied.classes().len(), 1, "the two dots share one class");
            assert!(
                tied.class_observations().is_empty(),
                "a 1-1 split must not emit a bit"
            );
            assert!(tied.dissent_rate() > 0.0, "the split must still be visible");
        }
    }

    /// Determinism, stated over what the decoder actually consumes.
    ///
    /// The *representative's* raw `(row, col)` legitimately depends on which
    /// member was seen first — any member is a valid stand-in. What must not
    /// depend on input order is the reduced class, the voted bit, and the
    /// confidence, because those are the whole of the decoder's input.
    #[test]
    fn output_is_a_function_of_the_set_not_the_input_order() {
        let observed = perfect_fragment(7, 40, 17);
        let reversed: Vec<_> = observed.iter().rev().copied().collect();
        let reduce = |v: &[PuzzleBoardObservedEdge]| -> Vec<(ClassKey, u8, u32)> {
            v.iter()
                .map(|e| (class_key(e), e.bit, e.confidence.to_bits()))
                .collect()
        };
        let forward = EdgeConsensus::build(&observed);
        let backward = EdgeConsensus::build(&reversed);
        assert_eq!(
            reduce(&forward.class_observations()),
            reduce(&backward.class_observations())
        );
        assert_eq!(forward.classes().len(), backward.classes().len());
        assert_eq!(
            forward.dissent_rate().to_bits(),
            backward.dissent_rate().to_bits()
        );
    }

    /// Choosing a different representative from the same class cannot move the
    /// residue slot the decode tables are keyed by — the property that makes
    /// "any member will do" safe rather than merely convenient.
    #[test]
    fn every_member_of_a_class_resolves_to_the_same_table_slot() {
        let observed = perfect_fragment(8, 40, 17);
        for t in GRID_TRANSFORMS_D4 {
            let mut slot: BTreeMap<ClassKey, (u8, i32, i32)> = BTreeMap::new();
            for e in &observed {
                let l = transform_edge_lookup(e, &t);
                let (orientation, period_row, period_col) = match l.orientation {
                    EdgeOrientation::Horizontal => (0u8, 167, CODE_PERIOD),
                    EdgeOrientation::Vertical => (1u8, CODE_PERIOD, 167),
                };
                let resolved = (
                    orientation,
                    l.lookup_row.rem_euclid(period_row),
                    l.lookup_col.rem_euclid(period_col),
                );
                let key = class_key(e);
                let seen = *slot.entry(key).or_insert(resolved);
                assert_eq!(
                    seen,
                    resolved,
                    "class {key:?} spans two table slots under {:?}",
                    t.matrix()
                );
            }
        }
    }
}
