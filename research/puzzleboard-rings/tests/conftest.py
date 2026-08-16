from __future__ import annotations

import numpy as np
import pytest

from pbrings.graph import RingGraph
from pbrings.params import REAL, TOY
from pbrings.sampling import enumerate_valid_rings


@pytest.fixture(scope="session")
def toy():
    return TOY


@pytest.fixture(scope="session")
def real():
    return REAL


@pytest.fixture(scope="session")
def toy_graph():
    return RingGraph(TOY)


@pytest.fixture(scope="session")
def real_graph():
    return RingGraph(REAL)


@pytest.fixture(scope="session")
def toy_rings():
    """Every valid toy ring — 80 of them, cheap enough to enumerate."""
    return enumerate_valid_rings(TOY)


@pytest.fixture
def rng():
    return np.random.default_rng(20260816)


def _all_circuits(g, deleted: int, start: int = 0) -> list[list[int]]:
    """Every Eulerian circuit of ``G - deleted`` from ``start``, by brute force.

    The ground truth the BEST sampler and the Matrix-Tree count are both
    measured against, so it shares no machinery with either.
    """
    edges = [e for e in range(g.n_edges) if e != deleted]
    out: dict[int, list[int]] = {}
    for e in edges:
        out.setdefault(g.tails[e], []).append(e)
    found: list[list[int]] = []

    def walk(v: int, used: frozenset[int], seq: list[int]) -> None:
        if len(seq) == len(edges):
            if v == start:
                found.append(list(seq))
            return
        for e in out.get(v, []):
            if e not in used:
                walk(g.heads[e], used | {e}, seq + [e])

    walk(start, frozenset(), [])
    return found


@pytest.fixture(scope="session")
def all_circuits():
    return _all_circuits
