"""The load-bearing test: the fast evaluator must equal the brute-force one.

:mod:`pbrings.evaluate` is fast because the code factorises into two 501-entry
tables whose alias indicators separate. :mod:`pbrings.brute` assumes none of
that — it materialises the board, walks every position, and moves each dot by
its corners. Exhaustive agreement on the toy is what licenses trusting the fast
path on the real board.
"""

from __future__ import annotations

import itertools

import pytest

from pbrings.brute import brute_metrics, transform_pattern
from pbrings.evaluate import evaluate_window
from pbrings.params import REAL, TOY
from pbrings.sampling import sample_ring
from pbrings.transforms import C4_NAMES, D4_NAMES, apply_corner, slot_action
from pbrings.window import ALL, INTERIOR, WindowSpec

GROUPS = ["fixed", "c4", "d4"]
READOUTS = [ALL, INTERIOR]


# ---------------------------------------------------------------------------
# The D4 action itself
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("span", [3, 4, 5, 7])
def test_corner_maps_are_permutations(span):
    corners = [(r, c) for r in range(span) for c in range(span)]
    for name in D4_NAMES:
        images = [apply_corner(name, rc, span) for rc in corners]
        assert sorted(images) == sorted(corners), name


@pytest.mark.parametrize("span", [4, 6])
@pytest.mark.parametrize("readout", READOUTS)
def test_slot_actions_are_permutations(span, readout):
    spec = WindowSpec(span=span, readout=readout)
    for name in D4_NAMES:
        act = slot_action(name, spec)
        assert sorted(act.v_from) == list(range(len(spec.v_slots))), name
        assert sorted(act.h_from) == list(range(len(spec.h_slots))), name


def test_rotations_swap_the_two_code_maps_but_reflections_along_an_axis_do_not():
    """A 90° turn makes vertical dots horizontal — the source of every rotation alias."""
    spec = WindowSpec(span=5, readout=ALL)
    expected = {
        "id": False,
        "rot90": True,
        "rot180": False,
        "rot270": True,
        "flip_row": False,
        "flip_col": False,
        "transpose": True,
        "anti_transpose": True,
    }
    for name, swaps in expected.items():
        assert slot_action(name, spec).swaps is swaps, name


def test_c4_is_a_subgroup_of_d4():
    assert set(C4_NAMES) <= set(D4_NAMES)
    assert len(D4_NAMES) == 8


def _square_slots(span):
    for r in range(span - 1):
        for c in range(span):
            yield ("v", r, c)
    for r in range(span):
        for c in range(span - 1):
            yield ("h", r, c)


@pytest.mark.parametrize("span", [4, 5])
def test_transform_pattern_is_an_involution_where_it_should_be(span):
    pattern = tuple(
        sorted(((kind, r, c), (r + c) % 2) for kind, r, c in _square_slots(span))
    )
    assert transform_pattern(pattern, "id", span) == pattern
    for name in ("rot180", "flip_row", "flip_col", "transpose", "anti_transpose"):
        once = transform_pattern(pattern, name, span)
        assert transform_pattern(once, name, span) == pattern, name


# ---------------------------------------------------------------------------
# Fast vs brute
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("span,readout,group", list(itertools.product([3, 4, 5], READOUTS, GROUPS)))
def test_fast_matches_brute_on_the_toy(span, readout, group, toy, toy_rings):
    """Exhaustive over all 100 toy positions, for several valid ring pairs."""
    spec = WindowSpec(span=span, readout=readout)
    if not spec.v_slots or not spec.h_slots:
        pytest.skip("readout model leaves no dots at this span")
    pairs = [(toy_rings[0], toy_rings[0]), (toy_rings[0], toy_rings[13]), (toy_rings[7], toy_rings[41])]
    for ring_a, ring_b in pairs:
        fast = evaluate_window(ring_a, ring_b, spec, group, toy)
        slow = brute_metrics(ring_a, ring_b, span, readout, group, toy)
        assert fast.n_unique == slow.n_unique, (span, readout, group)
        assert fast.hypothesis_histogram == slow.hypothesis_histogram


def test_fast_matches_brute_for_random_toy_pairs(toy, toy_graph, rng):
    spec = WindowSpec(span=4, readout=ALL)
    for _ in range(15):
        ring_a = sample_ring(toy_graph, toy, rng)
        ring_b = sample_ring(toy_graph, toy, rng)
        for group in GROUPS:
            fast = evaluate_window(ring_a, ring_b, spec, group, toy)
            slow = brute_metrics(ring_a, ring_b, span=4, readout=ALL, group=group, p=toy)
            assert fast.hypothesis_histogram == slow.hypothesis_histogram


def test_class_count_is_consistent_with_the_histogram(toy, toy_rings):
    """``n_classes`` is the report's denominator; it must reconcile with the
    hypothesis histogram rather than drift from it."""
    spec = WindowSpec(span=4, readout=ALL)
    for group in GROUPS:
        m = evaluate_window(toy_rings[0], toy_rings[13], spec, group, toy)
        merged_positions = sum(s * n for s, n in m.class_size_histogram.items())
        assert merged_positions <= m.positions
        assert m.n_classes <= m.positions
        assert m.n_unique <= m.n_classes


# ---------------------------------------------------------------------------
# Real board — slow, exact
# ---------------------------------------------------------------------------


@pytest.mark.slow
@pytest.mark.parametrize("group", ["c4", "d4"])
def test_fast_matches_brute_on_the_real_board(group):
    """Full brute force over all 251001 master positions. Minutes, not seconds."""
    from pbrings import refboard

    b = refboard.load()
    spec = WindowSpec(span=4, readout=ALL)
    fast = evaluate_window(b.ring_a, b.ring_b, spec, group, REAL)
    slow = brute_metrics(b.ring_a, b.ring_b, span=4, readout=ALL, group=group, p=REAL)
    assert fast.hypothesis_histogram == slow.hypothesis_histogram
    assert fast.n_unique == slow.n_unique
