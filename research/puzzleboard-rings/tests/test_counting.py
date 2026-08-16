"""The exact size of the search space, checked against a literal count."""

from __future__ import annotations

import pytest

from pbrings.counting import (
    arborescence_count,
    count_valid_rings,
    cyclic_circuits,
    eulerian_circuits_from_root,
    integer_determinant,
)
from pbrings.params import REAL, TOY
from pbrings.sampling import enumerate_valid_rings


def test_integer_determinant_on_known_matrices():
    assert integer_determinant([[2]]) == 2
    assert integer_determinant([[1, 2], [3, 4]]) == -2
    assert integer_determinant([[2, 0, 0], [0, 3, 0], [0, 0, 5]]) == 30
    assert integer_determinant([[1, 2], [2, 4]]) == 0


def test_determinant_stays_exact_far_beyond_float64():
    """A float determinant would silently lose this; the real Laplacian minor is
    worse still, so the integer path is load-bearing rather than fastidious."""
    n = 24
    matrix = [[0] * n for _ in range(n)]
    for i in range(n):
        matrix[i][i] = 10**5
    value = integer_determinant(matrix)
    assert isinstance(value, int)
    assert value == 10 ** (5 * n)


def test_arborescence_count_is_root_independent(real_graph):
    """True for any balanced strongly connected digraph — a good self-check."""
    counts = {arborescence_count(real_graph, v) for v in range(real_graph.n_vertices)}
    assert len(counts) == 1


def test_toy_circuit_count_matches_brute_force_enumeration(toy_graph, all_circuits):
    for loop in toy_graph.loops:
        assert eulerian_circuits_from_root(toy_graph, loop, 0) == len(
            all_circuits(toy_graph, loop, 0)
        )


def test_toy_ring_count_matches_exhaustive_enumeration(toy_graph, toy):
    """The whole counting chain — Matrix-Tree, BEST, lifts, shifts — validated
    against literally listing every valid ring."""
    assert count_valid_rings(toy_graph, toy) == len(enumerate_valid_rings(toy))


def test_real_space_is_astronomical(real_graph):
    """The brief's premise, made precise: exhaustive enumeration is hopeless."""
    rings = count_valid_rings(real_graph, REAL, lifts=3**7)
    assert isinstance(rings, int)
    assert 10**96 < rings < 10**97


def test_every_self_loop_yields_the_same_number_of_circuits(real_graph):
    counts = {cyclic_circuits(real_graph, loop) for loop in real_graph.loops}
    assert len(counts) == 1


@pytest.mark.parametrize("p", [TOY, REAL])
def test_counts_are_python_ints_not_floats(p):
    from pbrings.graph import RingGraph

    g = RingGraph(p)
    assert type(arborescence_count(g, 0)) is int
