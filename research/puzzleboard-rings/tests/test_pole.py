"""PuzzlePole: the seam, the derived stripe, and the uniqueness floor.

Three claims are load-bearing here and each is checked against something that
does not share machinery with it:

* the seam table is *derived* from the shipped maps, and agrees with the table
  the crate ships — parsed out of ``periods.rs``, not transcribed;
* a pole really is the master's rows ``[s, s+p)`` wrapped, checked against
  :func:`pbrings.brute.master_planes`;
* the vectorised readout, the rectangular D4 action and the cyclic/clamped
  placement enumeration agree exactly with an independent brute-force oracle.

If the third fails, every floor in ``report/puzzlepole-floor.md`` is fiction.
"""

from __future__ import annotations

import itertools

import numpy as np
import pytest

from pbrings import refboard
from pbrings.brute import (
    master_planes,
    pole_brute_metrics,
    rect_image_shape,
    transform_rect_pattern,
)
from pbrings.params import REAL, TOY
from pbrings.pole import (
    PAPER_CIRCUMFERENCES,
    PolePattern,
    PolePeriod,
    ViewBudget,
    floor_grid,
    is_seamless,
    min_span_y,
    pareto_floor,
    piece_rows_agree,
    seam_depth,
    seamless_start_rows,
    supported_periods,
    sweep_shapes,
    window_uniqueness,
)
from pbrings.sampling import enumerate_valid_rings
from pbrings.transforms import D4_NAMES
from pbrings.window import ALL, INTERIOR, WindowSpec

GROUPS = ["c4", "d4"]


@pytest.fixture(scope="module")
def board():
    return refboard.load()


@pytest.fixture(scope="module")
def periods(board):
    return supported_periods(board.ring_a, board.ring_b, REAL)


# ---------------------------------------------------------------------------
# The seam
# ---------------------------------------------------------------------------


def test_every_period_the_crate_ships_is_seamless(board):
    """Soundness. The whole study rests on measuring the poles the crate ships,
    so a table entry the maps do not support would invalidate everything."""
    for squares, start_row in refboard.crate_pole_periods():
        assert is_seamless(board.ring_a, board.ring_b, squares, start_row, REAL), (
            squares,
            start_row,
        )


def test_the_crate_table_is_the_derived_table(board, periods):
    """Completeness, compared where the two tables are comparable.

    A start row names a pole only modulo ``m·P``, and the crate is free to list
    its entries in either fundamental domain. What is domain-independent — and
    what actually decides the code — is the set of ``(circumference, map-B
    phase)`` pairs, so that is what must agree exactly.
    """
    crate = {(p, s % REAL.period) for p, s in refboard.crate_pole_periods()}
    derived = {(q.squares, q.start_row % REAL.period) for q in periods}
    assert crate == derived


def test_the_full_domain_holds_exactly_three_phases_of_each_start_row(board):
    """The study's own claim about the other degree of freedom, checked against
    the maps rather than against the crate: enumerating start rows over the whole
    501-row master gives exactly ``m`` times as many, because the seam
    predicate's map-A and colour terms do not depend on the start row at all."""
    for squares in PAPER_CIRCUMFERENCES:
        short = seamless_start_rows(board.ring_a, board.ring_b, squares, REAL)
        full = seamless_start_rows(
            board.ring_a, board.ring_b, squares, REAL, full_domain=True
        )
        assert len(full) == REAL.n_rows * len(short), squares
        assert sorted({s % REAL.period for s in full}) == short, squares


def test_every_seamless_period_is_a_multiple_of_six(board):
    """``% 3`` from ``map_a[row % 3]`` and ``% 2`` from the checkerboard colour.

    Checked over every circumference up to the largest shipped one, not only the
    ones that turn out to work — a period that closed without being a multiple
    of 6 would mean the colour or map-A term is missing from the predicate.
    """
    for squares in range(1, max(PAPER_CIRCUMFERENCES) + 1):
        rows = seamless_start_rows(board.ring_a, board.ring_b, squares, REAL)
        if rows:
            assert squares % 6 == 0, squares


def test_the_seam_is_exactly_two_piece_rows_deep(board, periods):
    """Two rows is what a local 3×3 piece code needs, so the seam creates no new
    local codes. The *third* row must not repeat — if it did, a pole would be
    locally the master over a taller window and would not need its own measurement
    at all."""
    for period in periods:
        assert seam_depth(board.ring_a, board.ring_b, period, REAL) == 2, period


def test_a_non_seamless_period_is_rejected(board):
    """The predicate has to be able to say no, or it says nothing."""
    assert not is_seamless(board.ring_a, board.ring_b, 12, 74, REAL)
    assert not is_seamless(board.ring_a, board.ring_b, 36, 158, REAL)  # the paper's 325
    assert is_seamless(board.ring_a, board.ring_b, 36, 160, REAL)  # ...and 327


def test_piece_rows_agree_is_reflexive_and_colour_aware(board):
    """The master's *row* period is 1002, not 501.

    Both code maps repeat after 501 rows, but 501 is odd, so the checkerboard
    colour inverts. Anything that compares piece rows and forgets the colour
    would call 501 a period and admit poles whose seam flips black and white.
    """
    assert piece_rows_agree(board.ring_a, board.ring_b, 40, 40, REAL)
    assert not piece_rows_agree(board.ring_a, board.ring_b, 40, 41, REAL)
    assert not piece_rows_agree(board.ring_a, board.ring_b, 40, 40 + REAL.period, REAL)
    assert not piece_rows_agree(board.ring_a, board.ring_b, 40, 40 + REAL.master, REAL)
    assert piece_rows_agree(board.ring_a, board.ring_b, 40, 40 + 2 * REAL.master, REAL)


# ---------------------------------------------------------------------------
# The derived pattern
# ---------------------------------------------------------------------------


def test_a_pole_is_the_master_strip_wrapped(board, periods):
    """Pole row ``R`` is master row ``s + R`` — checked against the planar
    reference's own board construction, which knows nothing about poles."""
    width = 60
    vertical, horizontal = master_planes(board.ring_a, board.ring_b, REAL)
    mv = np.array(vertical)
    mh = np.array(horizontal)
    for period in periods:
        pole = PolePattern(period, width, board.ring_a, board.ring_b, REAL)
        s, p = period.start_row, period.squares
        assert np.array_equal(pole.vplane, mv[s : s + p, : width - 1]), period
        assert np.array_equal(pole.hplane, mh[s : s + p, : width - 1]), period


def test_the_derived_stripe_keeps_the_codes_sub_perfection(board, periods):
    """All ``3p`` cyclic 3×3 windows of the ``p × 3`` slice stay distinct.

    Provable — a window at ``t ≤ p-3`` is a master window at ``s+t``, and the two
    that straddle the seam are master windows at ``s+p-2`` and ``s+p-1`` by the
    two-row repeat — so a failure here is a bug in the seam table, not a
    property of the code.
    """
    for period in periods:
        pole = PolePattern(period, 8, board.ring_a, board.ring_b, REAL)
        windows = pole.stripe_windows()
        assert len(windows) == REAL.n_rows * period.squares, period
        assert len(set(windows)) == len(windows), period


def test_the_map_a_row_phase_is_a_third_degree_of_freedom(board):
    """``s`` and ``s + 167`` are different poles, and not rotations of each other.

    They share a stripe — map B has period 167 — but map A is indexed one row
    on. No circumference rotation repairs that: a rotation by ``k`` would have
    to satisfy ``k ≡ 0 (mod p)`` to leave the stripe alone and ``k ≢ 0 (mod 3)``
    to move map A, and ``3 | p`` forbids both at once. So a start row names a
    pole only modulo 501, and the crate's mod-167 table picks one of three
    patterns per entry.
    """
    period = PolePeriod(12, 73)
    shifted = PolePeriod(12, 73 + REAL.period)
    a = PolePattern(period, 30, board.ring_a, board.ring_b, REAL)
    b = PolePattern(shifted, 30, board.ring_a, board.ring_b, REAL)
    assert is_seamless(board.ring_a, board.ring_b, 12, 73 + REAL.period, REAL)
    assert np.array_equal(a.hplane, b.hplane)
    assert not np.array_equal(a.vplane, b.vplane)
    for k in range(period.squares):
        assert not (
            np.array_equal(np.roll(a.vplane, k, axis=0), b.vplane)
            and np.array_equal(np.roll(a.hplane, k, axis=0), b.hplane)
        ), k


def test_the_row_phase_does_not_move_the_floor(board):
    """...and measuring it says the choice is free.

    All three phases of a start row give the same smallest ``span_y`` anywhere
    on the frontier, so the crate reducing the paper's start rows mod 167
    changes the printed pattern but not what a decoder needs to see.
    """
    for squares, start_row in ((12, 73), (24, 75)):
        floors = [
            pareto_floor(
                PolePattern(
                    PolePeriod(squares, start_row + REAL.period * j),
                    101,
                    board.ring_a,
                    board.ring_b,
                    REAL,
                ),
                "c4",
                12,
            )
            for j in range(REAL.n_rows)
        ]
        assert len({min(q[0] for q in f) for f in floors}) == 1, (squares, start_row)


def test_the_readout_model_is_interior_only(board):
    pole = PolePattern(PolePeriod(12, 73), 20, board.ring_a, board.ring_b, REAL)
    with pytest.raises(ValueError):
        pole.parts(WindowSpec(5, 5, ALL))


# ---------------------------------------------------------------------------
# Placements: one axis wraps, the other does not
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("span_x", [4, 7, 11])
def test_placement_count_is_cyclic_by_clamped(board, span_x):
    """The asymmetry, stated as an equation. Getting it symmetric would silently
    change every denominator in the report."""
    width = 20
    pole = PolePattern(PolePeriod(18, 7), width, board.ring_a, board.ring_b, REAL)
    spec = WindowSpec(6, span_x, INTERIOR)
    assert pole.axial_origins(spec) == width - span_x + 1
    assert pole.n_placements(spec) == 18 * (width - span_x + 1)
    t, x = pole.placement_grid(spec)
    assert sorted(set(t.tolist())) == list(range(18))  # every origin, wraps included
    assert sorted(set(x.tolist())) == list(range(width - span_x + 1))


def test_a_fragment_may_not_wrap_past_itself(board):
    pole = PolePattern(PolePeriod(12, 73), 20, board.ring_a, board.ring_b, REAL)
    assert pole.admits(WindowSpec(13, 6, INTERIOR))
    assert not pole.admits(WindowSpec(14, 6, INTERIOR))
    assert not pole.admits(WindowSpec(6, 21, INTERIOR))


def test_sweep_stops_at_the_circumference(board):
    pole = PolePattern(PolePeriod(12, 73), 40, board.ring_a, board.ring_b, REAL)
    rows, cols = sweep_shapes(pole, 16)
    assert rows == range(4, 14)  # p + 1 = 13
    assert cols == range(4, 17)


# ---------------------------------------------------------------------------
# The rectangular D4 action
# ---------------------------------------------------------------------------


@pytest.mark.parametrize("shape", [(5, 8), (6, 6), (4, 11)])
@pytest.mark.parametrize("readout", [ALL, INTERIOR])
def test_rectangular_slot_actions_are_bijections_onto_the_image_shape(shape, readout):
    """A quarter turn of a ``r × c`` fragment is a ``c × r`` one, and the
    interior readout is covariant under all eight elements — if it were not, a
    rotated pole hypothesis could not be matched at all."""
    from pbrings.transforms import slot_action

    spec = WindowSpec(shape[0], shape[1], readout)
    for name in D4_NAMES:
        act = slot_action(name, spec)
        assert act.dst == (spec.transposed() if act.swaps else spec), name
        assert sorted(act.v_from) == list(range(len(act.dst.v_slots))), name
        assert sorted(act.h_from) == list(range(len(act.dst.h_slots))), name
        assert act.dst.n_edges == spec.n_edges, name


@pytest.mark.parametrize("shape", [(5, 8), (4, 6)])
def test_the_oracle_derives_the_same_image_shape(shape):
    """The oracle measures the image shape from where corners land; the fast path
    reads it off the matrix. They must not disagree."""
    from pbrings.transforms import slot_action

    rows, cols = shape
    spec = WindowSpec(rows, cols, INTERIOR)
    for name in D4_NAMES:
        act = slot_action(name, spec)
        assert rect_image_shape(name, rows, cols) == (act.dst.rows, act.dst.cols), name


def test_transform_rect_pattern_is_an_involution_where_it_should_be():
    rows, cols = 5, 7
    pattern = tuple(
        sorted(
            ((kind, r, c), (r + 2 * c) % 2)
            for kind, r, c in (
                [("v", r, c) for r in range(rows - 1) for c in range(1, cols - 1)]
                + [("h", r, c) for r in range(1, rows - 1) for c in range(cols - 1)]
            )
        )
    )
    assert transform_rect_pattern(pattern, "id", rows, cols) == pattern
    for name in ("rot180", "flip_row", "flip_col"):
        once = transform_rect_pattern(pattern, name, rows, cols)
        assert transform_rect_pattern(once, name, rows, cols) == pattern, name


# ---------------------------------------------------------------------------
# Fast vs oracle — the load-bearing check
# ---------------------------------------------------------------------------


def _agrees(board, params, squares, start_row, width, span_y, span_x, group):
    pole = PolePattern(
        PolePeriod(squares, start_row), width, board.ring_a, board.ring_b, params
    )
    spec = WindowSpec(span_y, span_x, INTERIOR)
    fast = window_uniqueness(pole, spec, group)
    slow = pole_brute_metrics(
        board.ring_a,
        board.ring_b,
        squares,
        start_row,
        width,
        span_y,
        span_x,
        group,
        params,
    )
    assert fast.n_placements == slow.n_placements
    assert fast.n_ambiguous == slow.n_ambiguous
    assert fast.hypothesis_histogram == slow.hypothesis_histogram


@pytest.mark.parametrize(
    "squares,start_row,width", [(12, 73, 9), (18, 7, 11), (30, 49, 8)]
)
@pytest.mark.parametrize("group", GROUPS)
def test_fast_matches_the_oracle_on_real_poles(board, squares, start_row, width, group):
    """Square and rectangular, wrapping and not, both groups."""
    for span_y, span_x in itertools.product(range(4, 8), range(4, 8)):
        if span_x > width:
            continue
        _agrees(board, REAL, squares, start_row, width, span_y, span_x, group)


@pytest.mark.parametrize("group", GROUPS)
def test_fast_matches_the_oracle_at_toy_size(group):
    """The same code paths at ``Params(n_rows=2)``, where the pole is a 10-piece
    wrap of a 10×10 master and nothing about the real sizes can be hiding a bug."""
    rings = enumerate_valid_rings(TOY)
    ring_a, ring_b = rings[0], rings[13]

    class _Pair:
        pass

    pair = _Pair()
    pair.ring_a, pair.ring_b = ring_a, ring_b
    for squares in (10,):
        assert is_seamless(ring_a, ring_b, squares, 0, TOY)
        for span_y, span_x in itertools.product(range(4, 7), range(4, 7)):
            _agrees(pair, TOY, squares, 0, 8, span_y, span_x, group)


@pytest.mark.slow
def test_fast_matches_the_oracle_on_a_full_length_pole(board):
    """The widths the report actually quotes, where the oracle takes minutes."""
    for span_y, span_x in ((5, 7), (7, 5), (4, 7)):
        _agrees(board, REAL, 12, 73, 201, span_y, span_x, "c4")


# ---------------------------------------------------------------------------
# The floor
# ---------------------------------------------------------------------------


def test_every_placement_matches_itself(board):
    """The identity hypothesis always holds, so no histogram may have a 0 bin —
    a placement that failed to match itself would mean the readout tables and
    the query were built differently."""
    pole = PolePattern(PolePeriod(24, 75), 30, board.ring_a, board.ring_b, REAL)
    for group in GROUPS:
        m = window_uniqueness(pole, WindowSpec(5, 6, INTERIOR), group)
        assert 0 not in m.hypothesis_histogram
        assert sum(m.hypothesis_histogram.values()) == m.n_placements


@pytest.mark.parametrize("group", GROUPS)
def test_ambiguity_is_monotone_in_both_spans(board, group):
    """What the binary search in :func:`min_span_y` rests on, checked rather
    than assumed: a taller or wider fragment's readout contains a smaller one's,
    over the same placement set or a subset."""
    pole = PolePattern(PolePeriod(12, 73), 24, board.ring_a, board.ring_b, REAL)
    grid = floor_grid(pole, group, max_span=9)
    for (y, x), here in grid.items():
        for step in ((y + 1, x), (y, x + 1)):
            other = grid.get(step)
            if other is not None:
                assert other.n_ambiguous <= here.n_ambiguous, (y, x, step)


@pytest.mark.parametrize("group", GROUPS)
def test_the_frontier_agrees_with_the_exhaustive_grid(board, group):
    """The binary search must find exactly what the full sweep shows."""
    pole = PolePattern(PolePeriod(18, 7), 20, board.ring_a, board.ring_b, REAL)
    grid = floor_grid(pole, group, max_span=10)
    rows, cols = sweep_shapes(pole, 10)
    for span_x in cols:
        clean = [y for y in rows if grid[(y, span_x)].n_ambiguous == 0]
        expected = min(clean) if clean else None
        assert min_span_y(pole, span_x, group, 10) == expected, span_x
    frontier = pareto_floor(pole, group, 10)
    for span_y, span_x in frontier:
        assert grid[(span_y, span_x)].n_ambiguous == 0
    # Pareto-minimal: no frontier point dominates another.
    for a, b in itertools.permutations(frontier, 2):
        assert not (a[0] <= b[0] and a[1] <= b[1])


def test_d4_is_never_easier_than_c4(board):
    """D4 searches strictly more hypotheses, so it can only ever ambiguate more."""
    pole = PolePattern(PolePeriod(30, 9), 40, board.ring_a, board.ring_b, REAL)
    for span_y, span_x in ((5, 6), (6, 6), (7, 5)):
        spec = WindowSpec(span_y, span_x, INTERIOR)
        c4 = window_uniqueness(pole, spec, "c4")
        d4 = window_uniqueness(pole, spec, "d4")
        assert d4.n_ambiguous >= c4.n_ambiguous, spec


def test_a_longer_pole_is_never_easier(board):
    """More axial placements can only add aliases, never remove one."""
    period = PolePeriod(24, 75)
    spec = WindowSpec(5, 6, INTERIOR)
    previous = -1
    for width in (12, 24, 48, 96):
        pole = PolePattern(period, width, board.ring_a, board.ring_b, REAL)
        here = window_uniqueness(pole, spec, "c4")
        assert here.n_ambiguous >= previous
        previous = here.n_ambiguous


def test_counts_are_reported_with_their_denominator(board):
    """Never a bare percentage: the question is whether a count is zero."""
    pole = PolePattern(PolePeriod(12, 73), 30, board.ring_a, board.ring_b, REAL)
    m = window_uniqueness(pole, WindowSpec(7, 7, INTERIOR), "c4")
    assert m.format_ambiguous() == f"0 / {m.n_placements}"
    assert "%" not in m.format_ambiguous()


# ---------------------------------------------------------------------------
# The single-view model
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    "arc,squares,expected",
    [(120.0, 12, 5), (140.0, 12, 5), (120.0, 48, 17), (140.0, 48, 19)],
)
def test_view_budget_counts_corner_rows_not_cells(arc, squares, expected):
    """``cells + 1`` corners bound ``cells`` cells. Off by one here is off by a
    whole corner row in the verdict."""
    budget = ViewBudget(arc)
    assert budget.visible_corner_rows(squares) == expected
    assert budget.visible_cells(squares) == expected - 1


def test_a_view_never_supplies_more_than_the_whole_circumference():
    assert ViewBudget(360.0).visible_corner_rows(12) == 13
    assert ViewBudget(720.0).visible_corner_rows(12) == 13
