"""The per-ring reduction, and that the survey reproduces itself.

Everything here defends one claim: that a board's half-turn aliasing can be
attributed to its two rings *separately*. If that is wrong the survey measures
nothing, so it is checked against the pair evaluator on both the toy and the
real parameters, and on rings that are not the shipped ones.
"""

from __future__ import annotations

import numpy as np
import pytest

from pbrings import refboard
from pbrings.evaluate import evaluate_window, self_alias_support, transform_terms
from pbrings.params import Params
from pbrings.sampling import sample_ring
from pbrings.survey import (
    _max_run,
    aliased_positions,
    half_turn_support,
    interior,
    profile_ring,
    rank_key,
    survey_pairs,
    survey_rings,
)


def _spans(p: Params) -> tuple[int, ...]:
    """A few window sizes either side of the interesting region."""
    return tuple(range(p.n_rows + 1, p.n_rows + 5))


# ---------------------------------------------------------------------------
# The reduction
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("params_name", ["toy", "real"])
def test_self_alias_support_reproduces_the_pair_evaluator(
    params_name, toy, real, toy_graph, real_graph, rng
):
    """The per-ring factor IS the half-turn term's factor, elementwise."""
    p = toy if params_name == "toy" else real
    g = toy_graph if params_name == "toy" else real_graph
    a, b = sample_ring(g, p, rng), sample_ring(g, p, rng)
    for span in _spans(p):
        spec = interior(span)
        terms = {t.name: t for t in transform_terms(a, b, spec, "c4", p)}
        half = terms["rot180"]
        assert not half.swaps, "the half turn must not exchange the code maps"
        assert np.array_equal(self_alias_support(a, spec, p, role="a"), half.row)
        assert np.array_equal(self_alias_support(b, spec, p, role="b"), half.col)


def test_quarter_turn_does_not_split_per_ring(real, real_graph, rng):
    """The reduction is claimed only for the half turn; asking for it on a
    quarter turn must fail loudly rather than return a wrong number."""
    ring = sample_ring(real_graph, real, rng)
    with pytest.raises(ValueError, match="exchange"):
        self_alias_support(ring, interior(real.n_rows + 4), real, role="a", name="rot90")


def test_half_turn_support_is_role_independent(real, real_graph, rng):
    """A ring's support is intrinsic — the board is symmetric under transposing
    the two code maps, so it cannot matter which role the ring is given."""
    for _ in range(4):
        ring = sample_ring(real_graph, real, rng)
        for span in _spans(real):
            spec = interior(span)
            as_a = int((self_alias_support(ring, spec, real, role="a") > 0).sum())
            as_b = int((self_alias_support(ring, spec, real, role="b") > 0).sum())
            assert as_a == as_b


def test_aliased_positions_matches_the_hypothesis_count(real, real_graph, rng):
    """The union-of-rectangles shortcut equals counting hypotheses per position."""
    a, b = sample_ring(real_graph, real, rng), sample_ring(real_graph, real, rng)
    for span in _spans(real):
        spec = interior(span)
        metrics = evaluate_window(a, b, spec, "c4", real)
        assert aliased_positions(a, b, spec, real) == real.positions - metrics.n_unique


def test_a_clean_ring_kills_the_half_turn_for_every_partner(real, real_graph, rng):
    """The whole point of the reduction: support zero is a guarantee that holds
    against an arbitrary partner, not a property of a particular pairing."""
    board = refboard.load(real)
    span = real.n_rows + 4
    spec = interior(span)
    assert half_turn_support(board.ring_a, spec, real) == 0
    for _ in range(3):
        partner = sample_ring(real_graph, real, rng)
        terms = {t.name: t for t in transform_terms(board.ring_a, partner, spec, "c4", real)}
        assert not terms["rot180"].row.any()


# ---------------------------------------------------------------------------
# Bookkeeping
# ---------------------------------------------------------------------------


def test_max_run_counts_cyclically():
    # A run that wraps the end of the period must be counted as one run.
    assert _max_run(np.array([[1, 0, 0, 1, 1]], dtype=np.uint8)) == 3
    assert _max_run(np.array([[1, 1, 1, 1]], dtype=np.uint8)) == 4
    assert _max_run(np.array([[1, 0, 1, 0]], dtype=np.uint8)) == 1


def test_profile_ring_records_the_shipped_board(real):
    board = refboard.load(real)
    spans = _spans(real)
    prof = profile_ring(board.ring_a, spans, real)
    assert prof.letters == board.ring_a.letters
    assert prof.ones == int(board.ring_a.bits.sum())
    assert prof.clean_from is not None
    assert prof.support[prof.clean_from] == 0


def test_survey_is_reproducible_and_worker_count_independent(real):
    """Chunks are seeded by index, and every reduction is order-independent, so
    the worker count must not be visible in the result."""
    spans = (real.n_rows + 3, real.n_rows + 4)
    one = survey_rings(12, spans, real, seed=7, workers=1, chunk=5)
    again = survey_rings(12, spans, real, seed=7, workers=1, chunk=5)
    parallel = survey_rings(12, spans, real, seed=7, workers=3, chunk=5)
    assert one.n == again.n == parallel.n == 12
    assert one.support == again.support == parallel.support
    assert [r.letters for r in one.best] == [r.letters for r in parallel.best]


def test_survey_seeds_differ(real):
    spans = (real.n_rows + 4,)
    a = survey_rings(6, spans, real, seed=1, chunk=6)
    b = survey_rings(6, spans, real, seed=2, chunk=6)
    assert [r.letters for r in a.best] != [r.letters for r in b.best]


def test_summary_histograms_total_the_sample(real):
    spans = (real.n_rows + 3, real.n_rows + 4)
    summary = survey_rings(20, spans, real, seed=3, chunk=10)
    for span in spans:
        assert sum(summary.support_histogram(span).values()) == summary.n
    assert sum(summary.ones.values()) == summary.n
    assert sum(summary.max_run.values()) == summary.n


def test_best_is_sorted_and_bounded(real):
    spans = (real.n_rows + 3, real.n_rows + 4)
    summary = survey_rings(60, spans, real, seed=5, chunk=10, keep=4)
    assert len(summary.best) == 4
    keys = [rank_key(pr, spans) for pr in summary.best]
    assert keys == sorted(keys)
    # and the kept minimum really is the sample minimum
    assert keys[0][0] == min(summary.support_histogram(spans[0]))


def test_pair_survey_perfect_agrees_with_the_evaluator(real, real_graph, rng):
    """``perfect`` must mean what it says: zero aliased positions."""
    from pbrings.survey import _pair_chunk

    spans = (real.n_rows + 4, real.n_rows + 5)
    summary = _pair_chunk((11, 0, spans, 12))
    assert summary.n == 12
    # Re-derive the same tally the slow way, from the exact aliased count.
    import numpy as np

    from pbrings.sampling import sample_ring as draw

    p = real
    g = real_graph
    local = np.random.default_rng(np.random.SeedSequence(entropy=11, spawn_key=(0,)))
    expected = dict.fromkeys(spans, 0)
    for _ in range(12):
        a, b = draw(g, p, local), draw(g, p, local)
        for span in spans:
            if aliased_positions(a, b, interior(span), p) == 0:
                expected[span] += 1
    for span in spans:
        assert summary.perfect.get(span, 0) == expected[span]


def test_pair_survey_is_worker_count_independent(real):
    spans = (real.n_rows + 4,)
    one = survey_pairs(20, spans, real, seed=9, workers=1, chunk=5)
    many = survey_pairs(20, spans, real, seed=9, workers=4, chunk=5)
    assert one.n == many.n == 20
    assert one.perfect == many.perfect
    assert one.half_clean == many.half_clean
    assert one.quarter_clean == many.quarter_clean
