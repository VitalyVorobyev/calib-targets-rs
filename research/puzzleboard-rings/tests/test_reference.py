"""The reproduction gate: the authors' board must give the authors' numbers.

If anything in this file fails, no other result in the study means anything —
either the board was read wrong or the uniqueness definition is not the paper's.
"""

from __future__ import annotations

import pytest

from pbrings import refboard
from pbrings.evaluate import evaluate_window
from pbrings.graph import RingGraph
from pbrings.params import REAL
from pbrings.ring import Ring
from pbrings.window import ALL, INTERIOR, WindowSpec, paper_window

#: Pins the shipped blobs. A change in the crate should fail loudly here rather
#: than silently invalidate the compatibility claim.
SHA256_MAP_A = "1c5d2339a56cbefe7299bf1c3cba35d4241dd2f42244539dd7508dc4735ef91c"
SHA256_MAP_B = "27b49dee29794fe09707dff12ac62c48320c22779f5a9d0d541f4103750de1b2"


@pytest.fixture(scope="module")
def board():
    return refboard.load()


def test_shipped_blobs_are_the_ones_we_measured(board):
    assert board.sha256_a == SHA256_MAP_A
    assert board.sha256_b == SHA256_MAP_B


def test_blobs_round_trip_through_our_reader(board):
    """Our packing convention is the crate's, byte for byte."""
    d = refboard.data_dir()
    assert board.ring_a.packed() == (d / "map_a.bin").read_bytes()
    # Map B is stored transposed, so re-pack it the way the crate stores it.
    transposed = Ring.from_bits(board.ring_b.bits, REAL).bits.T
    packed_b = bytearray((transposed.size + 7) // 8)
    for idx, bit in enumerate(transposed.reshape(-1)):
        if bit:
            packed_b[idx // 8] |= 1 << (idx % 8)
    assert bytes(packed_b) == (d / "map_b.bin").read_bytes()


def test_both_shipped_maps_are_valid_de_bruijn_rings(board):
    for ring in (board.ring_a, board.ring_b):
        assert ring.is_valid
        assert ring.n_sigma_fixed == 0
        assert ring.n_orbit_duplicates == 0
        assert len(set(ring.orbit_ids)) == REAL.period


def test_shipped_maps_omit_a_self_loop(board):
    """The structural prediction, checked against published data.

    Nothing in the authors' construction mentions self-loops; that the omitted
    σ-orbit is one anyway is a consequence of a closed trail needing a balanced
    subgraph.
    """
    g = RingGraph(REAL)
    for ring in (board.ring_a, board.ring_b):
        omitted = ring.omitted_edges(g)
        assert len(omitted) == 1
        assert omitted[0] in g.loops


def test_master_period_is_501_squared(board):
    assert REAL.master == 501
    assert REAL.positions == 251001


def test_18_bit_windows_are_unique_at_fixed_orientation(board):
    """The paper's "the 18 bits of each 3×3 pattern are unique if the
    orientation is known", and the crate's ``master_4x4_windows_unique``."""
    m = evaluate_window(board.ring_a, board.ring_b, paper_window(3), "fixed", REAL)
    assert m.informative_bits == 24  # all edges of a 3×3 piece block
    assert m.n_unique == REAL.positions

    detector = WindowSpec(span=5, readout=INTERIOR)
    m2 = evaluate_window(board.ring_a, board.ring_b, detector, "fixed", REAL)
    assert m2.informative_bits == 18
    assert m2.n_unique == REAL.positions


def test_code_covers_the_published_share_of_the_18_bit_space():
    """The companion paper's "more than 95.7 % of all (3,3) patterns"."""
    assert REAL.positions / 2**18 == pytest.approx(0.9575, abs=5e-5)


def test_reproduces_the_published_99_33_percent(board):
    """*"99.33 % of all 3×3 such local patterns are unique under orientation"*.

    The denominator is distinct **patterns**, not board positions: 1672 pairs of
    positions share a pattern, so 249343 patterns exist and 247671 of them are
    unique. Counted over positions instead the figure is 98.67 %.
    """
    m = evaluate_window(board.ring_a, board.ring_b, paper_window(3), "c4", REAL)
    assert m.n_edges == 24
    assert 100.0 * m.fraction_unique_patterns == pytest.approx(99.33, abs=0.005)
    assert 100.0 * m.fraction_unique_positions == pytest.approx(98.67, abs=0.005)


def test_reproduces_the_published_100_percent_at_four_by_four(board):
    """*"using all 40 edges of ... 4×4 pieces ... all these patterns are unique"*."""
    m = evaluate_window(board.ring_a, board.ring_b, paper_window(4), "c4", REAL)
    assert m.n_edges == 40
    assert m.n_unique == REAL.positions
    assert m.fraction_unique_patterns == 1.0


def test_reflections_are_what_break_the_guarantee(board):
    """The same fragments under D4 instead of C4 — the cost our detector pays."""
    c4 = evaluate_window(board.ring_a, board.ring_b, paper_window(4), "c4", REAL)
    d4 = evaluate_window(board.ring_a, board.ring_b, paper_window(4), "d4", REAL)
    assert c4.n_unique == REAL.positions
    assert d4.n_unique < REAL.positions


@pytest.mark.parametrize(
    "span,expected_unique",
    [(4, 0), (5, 0), (6, 230805), (7, 250497), (8, 250947), (9, 251001)],
)
def test_detector_readout_reproduces_the_crate_measurement(board, span, expected_unique):
    """Explains ``window_uniqueness_report``: 0/7 at the small windows is not
    sampling luck, it is exactly zero unique positions."""
    spec = WindowSpec(span=span, readout=INTERIOR)
    m = evaluate_window(board.ring_a, board.ring_b, spec, "d4", REAL)
    assert m.n_unique == expected_unique


@pytest.mark.parametrize("group", ["c4", "d4"])
def test_interior_readout_lags_the_paper_readout_by_two_corners(board, group):
    """Our sampler cannot read a fragment's outermost ring of dots.

    That ring is one edge deep on every side, so an interior readout spanning
    ``s`` corners sees exactly what the paper's model sees from ``s-2``. The
    detector therefore needs two more corners of visible board than the
    published window sizes suggest — the same code, read through a stricter
    sampler.
    """
    for span in (6, 7, 8):
        interior = evaluate_window(
            board.ring_a, board.ring_b, WindowSpec(span, INTERIOR), group, REAL
        )
        paper = evaluate_window(
            board.ring_a, board.ring_b, WindowSpec(span - 2, ALL), group, REAL
        )
        assert interior.informative_bits == paper.informative_bits, span
        assert interior.n_unique == paper.n_unique, span
        # ...while reading many more dots to learn the same thing, because the
        # extra rows repeat modulo the map height.
        assert interior.n_edges > paper.n_edges
