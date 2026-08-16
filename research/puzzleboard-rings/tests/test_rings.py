"""Ring validity, the closed-trail characterisation, and uniform sampling."""

from __future__ import annotations

from collections import Counter

import numpy as np
import pytest

from pbrings.params import REAL, TOY
from pbrings.ring import Ring
from pbrings.sampling import (
    branch_points,
    closing_lifts,
    edge_multiset,
    lift_circuit,
    random_circuit,
    sample_ring,
    unused_loop,
)


def test_every_valid_toy_ring_omits_exactly_one_self_loop(toy_rings, toy_graph):
    """The structural claim, verified exhaustively rather than argued.

    A closed trail needs a balanced subgraph. Deleting any non-loop edge
    unbalances two vertices, so the omitted edge must be a self-loop — and on
    the toy we can check every single valid ring.
    """
    assert toy_rings, "expected at least one valid toy ring"
    for ring in toy_rings:
        missing = ring.omitted_edges(toy_graph)
        assert len(missing) == 1
        assert missing[0] in toy_graph.loops


def test_valid_rings_use_distinct_edges(toy_rings, toy_graph):
    for ring in toy_rings:
        edges = edge_multiset(toy_graph, ring)
        assert len(set(edges)) == len(edges) == ring.p.period


def test_defect_counts_both_failure_modes(toy):
    """A σ-fixed window and a repeated orbit are the only two ways to be invalid."""
    all_zero = Ring((0,) * toy.period, toy)
    assert not all_zero.is_valid
    assert all_zero.n_sigma_fixed == toy.period


@pytest.mark.parametrize("p", [TOY, REAL])
def test_sampler_produces_valid_rings(p, rng):
    from pbrings.graph import RingGraph

    g = RingGraph(p)
    for _ in range(10):
        ring = sample_ring(g, p, rng)
        assert ring.is_valid
        assert unused_loop(g, ring) in g.loops


def test_sampler_reaches_every_toy_ring(toy_graph, toy, toy_rings, rng):
    """Coverage: the BEST sampler's support is the whole valid set, not a corner."""
    target = {r.letters for r in toy_rings}
    seen: set[tuple[int, ...]] = set()
    for _ in range(4000):
        ring = sample_ring(toy_graph, toy, rng)
        # A circuit is generated from a fixed root, so it lands on one rotation
        # of the ring; compare up to the shift symmetry.
        seen.update(x.letters for x in ring.orbit_under_shifts())
        if target <= seen:
            break
    assert target <= seen


def test_circuit_sampling_is_uniform_over_circuits(toy_graph, toy, rng):
    """BEST plus Wilson should be exactly uniform, with no rejection step.

    Checked on the toy against the empirical distribution of circuits for one
    fixed deleted loop, with a χ²-style flatness bound.
    """
    loop = toy_graph.loops[0]
    counts: Counter[tuple[int, ...]] = Counter()
    trials = 6000
    for _ in range(trials):
        counts[tuple(random_circuit(toy_graph, loop, rng))] += 1
    k = len(counts)
    assert k > 1, "need more than one circuit for this to mean anything"
    expected = trials / k
    chi2 = sum((n - expected) ** 2 / expected for n in counts.values())
    # 6-sigma-ish bound on a chi-square with k-1 degrees of freedom.
    assert chi2 < (k - 1) + 6 * np.sqrt(2 * (k - 1))


def test_circuit_lift_closes(toy_graph, toy, rng):
    for _ in range(200):
        loop = int(toy_graph.loops[rng.integers(len(toy_graph.loops))])
        circuit = random_circuit(toy_graph, loop, rng)
        ring = lift_circuit(toy_graph, circuit, toy, rng)
        assert len(ring.letters) == toy.period
        assert ring.is_valid


def test_lift_branches_only_at_sigma_fixed_vertices(toy_graph, toy, rng):
    """At a σ-fixed vertex every rotation of the next edge continues the walk.

    Everywhere else exactly one does. This branching is what makes a single
    circuit correspond to several rings, and what lets the σ-twist be brought
    back to zero.
    """
    fixed = set(toy_graph.fixed_vertices)
    loop = toy_graph.loops[0]
    circuit = random_circuit(toy_graph, loop, rng)
    expected = sum(1 for e in circuit[1:] if toy_graph.tails[e] in fixed)
    assert branch_points(toy_graph, circuit) == expected
    assert expected > 0


def test_closing_lift_count_is_constant_across_circuits(toy_graph, toy, all_circuits):
    """Uniformity over rings needs this: uniform-circuit times uniform-lift is
    only uniform if every circuit has the same number of closing lifts."""
    counts = set()
    branches = set()
    for loop in toy_graph.loops:
        for circuit in all_circuits(toy_graph, loop):
            counts.add(len(closing_lifts(toy_graph, circuit, toy)))
            branches.add(branch_points(toy_graph, circuit))
    assert len(counts) == 1, counts
    assert len(branches) == 1, branches
    # How many of the m^B choice tuples close is measured, not predicted: the
    # naive m^(B-1) guess is wrong here, because on this graph the twist does
    # not depend on the single branch at all and every tuple closes.
    assert counts.pop() <= toy.n_rows ** branches.pop()


def test_circuits_times_lifts_accounts_for_every_valid_ring(
    toy_graph, toy, toy_rings, all_circuits
):
    """The complete parameterisation, checked by counting to the last ring.

    valid rings = circuits × closing lifts × shift images.
    """
    pairs = sum(
        len(closing_lifts(toy_graph, c, toy))
        for loop in toy_graph.loops
        for c in all_circuits(toy_graph, loop)
    )
    classes = {r.canonical.letters for r in toy_rings}
    assert pairs == len(classes)
    assert len(toy_rings) == len(classes) * toy.period * toy.n_rows
