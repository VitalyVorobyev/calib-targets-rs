"""How big the search space actually is — counted exactly, not estimated.

The brief says not to attempt exhaustive enumeration because the number of
Eulerian cycles is astronomical. It is worth knowing *how* astronomical, and the
closed-trail characterisation makes that a finite computation:

    valid rings = Σ over the 6 self-loops ℓ of
                    cyclic Eulerian circuits of (G − ℓ)
                  × period          (which column the ring starts at)
                  × m               (the global σ-shift)
                  × closing lifts   (the σ-choice at each branch point)

The circuit count comes from the BEST theorem — arborescences towards a root,
times a factorial per vertex — and the arborescence count from the Matrix-Tree
theorem. Both are done in exact Python integers: the Laplacian minor is 23×23
with a determinant far outside float64, so ``numpy.linalg.det`` would return a
confident and meaningless number here.
"""

from __future__ import annotations

from math import factorial

from .graph import RingGraph
from .params import Params


def integer_determinant(matrix: list[list[int]]) -> int:
    """Exact determinant by Bareiss fraction-free elimination.

    Every intermediate is an integer, so nothing is ever rounded — which is the
    whole point, given the result has ~70 digits.
    """
    n = len(matrix)
    if n == 0:
        return 1
    m = [row[:] for row in matrix]
    sign = 1
    previous = 1
    for k in range(n - 1):
        if m[k][k] == 0:
            for i in range(k + 1, n):
                if m[i][k] != 0:
                    m[k], m[i] = m[i], m[k]
                    sign = -sign
                    break
            else:
                return 0
        for i in range(k + 1, n):
            for j in range(k + 1, n):
                m[i][j] = (m[i][j] * m[k][k] - m[i][k] * m[k][j]) // previous
        previous = m[k][k]
    return sign * m[n - 1][n - 1]


def arborescence_count(g: RingGraph, root: int, deleted: int | None = None) -> int:
    """Spanning arborescences oriented towards ``root`` (the Matrix-Tree count).

    Self-loops never appear in an arborescence, so they cancel out of the
    Laplacian; deleting one changes the count only through the out-degree it
    removes, which is why it must be passed in rather than ignored.
    """
    n = g.n_vertices
    laplacian = [[0] * n for _ in range(n)]
    for e in range(g.n_edges):
        if e == deleted:
            continue
        tail, head = g.tails[e], g.heads[e]
        laplacian[tail][tail] += 1
        laplacian[tail][head] -= 1
    minor = [
        [laplacian[i][j] for j in range(n) if j != root]
        for i in range(n)
        if i != root
    ]
    return integer_determinant(minor)


def eulerian_circuits_from_root(g: RingGraph, deleted: int, root: int = 0) -> int:
    """BEST: Eulerian circuits of ``G − deleted`` as edge sequences from ``root``."""
    out_degree = [
        sum(1 for e in g.out_edges[v] if e != deleted) for v in range(g.n_vertices)
    ]
    product = 1
    for v in range(g.n_vertices):
        product *= factorial(out_degree[v] - 1)
    return arborescence_count(g, root, deleted) * product


def cyclic_circuits(g: RingGraph, deleted: int, root: int = 0) -> int:
    """Circuits counted once each, rather than once per visit to the root.

    An edge sequence from ``root`` picks a starting point, and a circuit passes
    through ``root`` exactly ``outdeg(root)`` times.
    """
    visits = sum(1 for e in g.out_edges[root] if e != deleted)
    total = eulerian_circuits_from_root(g, deleted, root)
    if total % visits:  # pragma: no cover - would mean the count is inconsistent
        raise AssertionError("circuit count is not divisible by the root's degree")
    return total // visits


def lifts_per_circuit(g: RingGraph, deleted: int, p: Params) -> int:
    """Closing lifts of one circuit, counted exhaustively on an actual circuit.

    This is measured rather than predicted: the natural guess ``m^(B-1)`` is
    wrong, because on these graphs the σ-twist turns out not to depend on the
    branch choices at all, so every one of the ``m^B`` tuples closes. The test
    suite pins that the count is the same for every circuit, which is what
    uniform sampling needs.
    """
    import numpy as np

    from .sampling import closing_lifts, random_circuit

    circuit = random_circuit(g, deleted, np.random.default_rng(0))
    return len(closing_lifts(g, circuit, p))


def count_valid_rings(g: RingGraph, p: Params, lifts: int | None = None) -> int:
    """Every valid ring, exactly.

    ``lifts`` may be supplied to avoid recomputing the (expensive but constant)
    closing-lift count.
    """
    if lifts is None:
        lifts = lifts_per_circuit(g, g.loops[0], p)
    total = 0
    for loop in g.loops:
        total += cyclic_circuits(g, loop) * p.period * p.n_rows * lifts
    return total


def count_summary(g: RingGraph, p: Params, lifts: int | None = None) -> dict[str, object]:
    if lifts is None:
        lifts = lifts_per_circuit(g, g.loops[0], p)
    per_loop = {
        int(loop): cyclic_circuits(g, loop) for loop in g.loops
    }
    rings = count_valid_rings(g, p, lifts)
    return {
        "arborescences_to_root_0": arborescence_count(g, 0),
        "cyclic_circuits_per_loop": per_loop,
        "closing_lifts_per_circuit": lifts,
        "shift_images_per_circuit_lift": p.period * p.n_rows,
        "valid_rings": rings,
        "valid_rings_digits": len(str(rings)),
        "candidate_pairs": rings * rings,
    }
