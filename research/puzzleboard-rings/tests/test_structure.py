"""Invariants of the alphabet, the σ action, and the quotient ring graph."""

from __future__ import annotations

import pytest

from pbrings.params import REAL, TOY, Params
from pbrings.sigma import (
    all_tuples,
    is_sigma_fixed,
    orbit,
    orbit_id,
    orbit_reps,
    pack,
    sigma_letter,
    sigma_tuple,
    unpack,
)

ALL_PARAMS = [TOY, REAL]


def test_n_rows_must_be_prime():
    with pytest.raises(ValueError):
        Params(n_rows=4)


@pytest.mark.parametrize("p", ALL_PARAMS)
def test_sigma_has_order_n_rows(p: Params):
    for letter in range(p.alphabet):
        x = letter
        for _ in range(p.n_rows):
            x = sigma_letter(x, p)
        assert x == letter


@pytest.mark.parametrize("p", ALL_PARAMS)
def test_sigma_fixed_letters_are_all_zero_and_all_one(p: Params):
    fixed = {x for x in range(p.alphabet) if sigma_letter(x, p) == x}
    assert fixed == {0, p.alphabet - 1}
    assert len(fixed) == p.n_fixed_letters


@pytest.mark.parametrize("p", ALL_PARAMS)
@pytest.mark.parametrize("k", [1, 2, 3])
def test_orbit_sizes_partition_the_tuples(p: Params, k: int):
    """With σ of prime order every orbit has size 1 or exactly ``m``."""
    sizes = [len(orbit(t, p)) for t in all_tuples(k, p)]
    assert set(sizes) <= {1, p.n_rows}
    n_fixed = sizes.count(1)
    assert n_fixed == p.n_fixed_tuples(k)
    assert n_fixed + (len(sizes) - n_fixed) // p.n_rows == p.n_orbits(k)
    assert len(orbit_reps(k, p)) == p.n_orbits(k)


@pytest.mark.parametrize("p", ALL_PARAMS)
def test_pack_unpack_round_trip(p: Params):
    for k in (1, 2, 3):
        for t in all_tuples(k, p):
            assert unpack(pack(t, p), k, p) == t


@pytest.mark.parametrize("p", ALL_PARAMS)
def test_orbit_id_is_constant_on_an_orbit(p: Params):
    for t in all_tuples(p.n_rows, p):
        ids = {orbit_id(x, p) for x in orbit(t, p)}
        assert len(ids) == 1


def test_real_graph_matches_the_published_shape(real_graph):
    """The 24 vertices and 168 edges the brief and the paper both quote."""
    g = real_graph
    assert g.n_vertices == 24
    assert g.n_edges == 168
    assert len(g.fixed_vertices) == 4
    degrees = sorted(g.out_degree(v) for v in range(g.n_vertices))
    assert degrees == [2] * 4 + [8] * 20
    assert sum(degrees) == 168
    assert g.p.period == 167
    assert g.p.master == 501
    assert g.p.positions == 251001


def test_toy_graph_shape(toy_graph):
    g = toy_graph
    assert (g.n_vertices, g.n_edges) == (3, 6)
    assert sorted(g.out_degree(v) for v in range(g.n_vertices)) == [1, 1, 4]
    assert g.p.master == 10


@pytest.mark.parametrize("p", ALL_PARAMS)
def test_graph_is_eulerian(p: Params):
    from pbrings.graph import RingGraph

    g = RingGraph(p)
    assert g.is_balanced
    assert g.is_strongly_connected
    for v in range(g.n_vertices):
        assert g.in_degree(v) == g.out_degree(v)


def test_real_graph_has_exactly_six_self_loops(real_graph):
    """The count that makes the space enumerable.

    A valid ring omits exactly one edge, and only a self-loop can be omitted
    without unbalancing the graph — so this is the number of families the whole
    search space splits into.
    """
    assert len(real_graph.loops) == 6


def test_loops_are_the_only_balance_preserving_deletions(real_graph):
    g = real_graph
    for e in range(g.n_edges):
        balanced = g.tails[e] == g.heads[e]
        assert balanced == (e in g.loops)


@pytest.mark.parametrize("p", ALL_PARAMS)
def test_edges_are_exactly_the_non_fixed_windows(p: Params):
    from pbrings.graph import RingGraph

    g = RingGraph(p)
    assert all(not is_sigma_fixed(t, p) for t in g.edge_reps)
    assert len(g.edge_reps) == p.n_edges


@pytest.mark.parametrize("p", ALL_PARAMS)
def test_edge_endpoints_come_from_the_window(p: Params):
    from pbrings.graph import RingGraph

    g = RingGraph(p)
    for e, t in enumerate(g.edge_reps):
        assert g.tails[e] == g.vertex_of(t[:-1])
        assert g.heads[e] == g.vertex_of(t[1:])
        assert sigma_tuple(t, p) != t
