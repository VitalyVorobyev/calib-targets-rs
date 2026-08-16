"""Generating valid rings — uniformly, via the BEST theorem.

A valid ring is a closed trail covering every edge of the ring graph except one
self-loop (see :mod:`pbrings.graph`), so generating one means generating an
Eulerian circuit of ``G − loop``. Doing that *uniformly* matters: the question
"is the authors' board typical or was it selected?" has no meaning against a
biased sampler, and the hill-climbing generator in the crate's
``tools/generate_code_maps.rs`` is biased in an unknown way.

The BEST theorem gives both the count and the sampler. Every Eulerian circuit
from a fixed root corresponds to a pair of

* a spanning arborescence oriented *towards* the root, and
* an arbitrary ordering of each vertex's remaining out-edges,

so sampling one of each uniformly samples circuits uniformly. Arborescences come
from Wilson's cycle-popping algorithm; because each non-root vertex contributes
exactly one out-edge, every arborescence has the same probability
``∏ 1/outdeg(v)`` and the sampler is exactly uniform with no rejection.
"""

from __future__ import annotations

import numpy as np

from .graph import RingGraph
from .params import Params
from .ring import Ring
from .sigma import Tuple_, orbit, pack


def random_arborescence(
    g: RingGraph, root: int, rng: np.random.Generator, skip_edge: int | None = None
) -> dict[int, int]:
    """A uniformly random spanning arborescence oriented towards ``root``.

    Returns ``vertex → out-edge index``; following those edges from any vertex
    reaches the root. Wilson's algorithm: random-walk until the tree is hit,
    then keep only the loop-erased path.
    """
    out = [
        tuple(e for e in g.out_edges[v] if e != skip_edge)
        for v in range(g.n_vertices)
    ]
    if any(not es for es in out):
        raise ValueError("removing that edge left a vertex with no way out")

    in_tree = {root}
    next_edge: dict[int, int] = {}
    for start in range(g.n_vertices):
        if start in in_tree:
            continue
        walker = start
        while walker not in in_tree:
            choices = out[walker]
            next_edge[walker] = int(choices[rng.integers(len(choices))])
            walker = g.heads[next_edge[walker]]
        walker = start
        while walker not in in_tree:
            in_tree.add(walker)
            walker = g.heads[next_edge[walker]]
    return next_edge


def random_circuit(
    g: RingGraph, deleted_loop: int, rng: np.random.Generator, root: int = 0
) -> list[int]:
    """A uniformly random Eulerian circuit of ``G`` minus one self-loop.

    ``deleted_loop`` must be a self-loop: deleting anything else unbalances two
    vertices and no closed trail can cover the rest.
    """
    if g.tails[deleted_loop] != g.heads[deleted_loop]:
        raise ValueError(
            f"edge {deleted_loop} is not a self-loop; deleting it unbalances the graph"
        )
    arb = random_arborescence(g, root, rng, skip_edge=deleted_loop)

    order: list[list[int]] = []
    for v in range(g.n_vertices):
        rest = [e for e in g.out_edges[v] if e != deleted_loop and e != arb.get(v)]
        rng.shuffle(rest)
        if v in arb:
            rest.append(arb[v])  # the tree edge leaves last — this is what BEST needs
        order.append(rest)

    cursor = [0] * g.n_vertices
    circuit: list[int] = []
    v = root
    total = g.n_edges - 1
    while len(circuit) < total:
        if cursor[v] >= len(order[v]):
            raise AssertionError("stuck before covering every edge")
        e = order[v][cursor[v]]
        cursor[v] += 1
        circuit.append(e)
        v = g.heads[e]
    if v != root:
        raise AssertionError("circuit did not close")
    return circuit


def branch_points(g: RingGraph, circuit: list[int]) -> int:
    """How many times the lift of ``circuit`` has a free choice.

    An edge is a σ-*orbit* of windows, and the lift must pick the rotation whose
    leading letters continue the previous window. Normally exactly one rotation
    does. But at a **σ-fixed vertex** the incoming prefix is its own σ-image, so
    *every* rotation of the next edge continues the walk and the lift branches
    ``m`` ways.

    That is not a curiosity: it is why a circuit corresponds to many rings, and
    why the σ-twist can always be brought back to zero. Every Eulerian circuit
    here uses all of the σ-fixed vertices' edges, so this count is the same for
    every circuit — which is what keeps uniform-circuit × uniform-choice
    uniform over rings.
    """
    fixed = set(g.fixed_vertices)
    return sum(1 for e in circuit[1:] if g.tails[e] in fixed)


def circuit_to_letters(
    g: RingGraph, circuit: list[int], p: Params, choices: tuple[int, ...] = ()
) -> tuple[int, ...] | None:
    """Lift an edge sequence to letters, or ``None`` if the σ-twist does not close.

    ``choices`` supplies one σ-power per branch point, in walk order; missing
    entries default to 0.
    """
    m = p.n_rows
    first = g.edge_reps[circuit[0]]
    letters = list(first)
    prefix: Tuple_ = first[1:]
    taken = 0
    for e in circuit[1:]:
        cands = [c for c in orbit(g.edge_reps[e], p) if c[:-1] == prefix]
        if not cands:  # pragma: no cover - would mean the trail is not a walk
            raise AssertionError(f"edge {e} does not continue the walk")
        if len(cands) == 1:
            pick = cands[0]
        else:
            k = choices[taken] if taken < len(choices) else 0
            pick = cands[k % len(cands)]
            taken += 1
        letters.append(pick[-1])
        prefix = pick[1:]
    if len(letters) != p.period + m - 1:  # pragma: no cover
        raise AssertionError(f"expected {p.period + m - 1} letters, got {len(letters)}")
    if tuple(letters[p.period :]) != tuple(letters[: m - 1]):
        return None
    return tuple(letters[: p.period])


def closing_lifts(g: RingGraph, circuit: list[int], p: Params) -> list[Ring]:
    """Every lift of ``circuit`` whose σ-twist closes. Exhaustive — toys only.

    The branching is ``m^B`` with ``B`` branch points, so this is for
    establishing ground truth, not for production sampling.
    """
    n = branch_points(g, circuit)
    out: list[Ring] = []
    for code in range(p.n_rows**n):
        rest, choices = code, []
        for _ in range(n):
            choices.append(rest % p.n_rows)
            rest //= p.n_rows
        letters = circuit_to_letters(g, circuit, p, tuple(choices))
        if letters is not None:
            out.append(Ring(letters, p))
    return out


def lift_circuit(
    g: RingGraph,
    circuit: list[int],
    p: Params,
    rng: np.random.Generator,
    max_tries: int = 256,
) -> Ring:
    """A uniformly random closing lift of one circuit.

    Which branch controls the residual twist is not fixed — for some circuits
    several choices of the last branch close and for others only one — so
    rather than solving for it, the choice tuple is drawn uniformly and
    rejected when it fails to close. Rejection over a uniform draw is uniform
    over the closing tuples by construction, and the acceptance rate is ``1/m``,
    so this costs about ``m`` lifts.
    """
    n = branch_points(g, circuit)
    for _ in range(max_tries):
        choices = tuple(int(x) for x in rng.integers(p.n_rows, size=n))
        letters = circuit_to_letters(g, circuit, p, choices)
        if letters is not None:
            return Ring(letters, p)
    raise AssertionError(  # pragma: no cover
        f"no closing lift found for this circuit in {max_tries} tries"
    )


def sample_ring(g: RingGraph, p: Params, rng: np.random.Generator) -> Ring:
    """One uniformly random valid ring."""
    loop = int(g.loops[rng.integers(len(g.loops))])
    circuit = random_circuit(g, loop, rng)
    ring = lift_circuit(g, circuit, p, rng)
    if not ring.is_valid:  # pragma: no cover - construction guarantees it
        raise AssertionError("BEST sampler produced an invalid ring")
    return ring


def enumerate_valid_rings(p: Params, limit: int = 1 << 20) -> list[Ring]:
    """Every valid ring, by exhaustive search. Only sane at toy sizes.

    This is the ground truth the sampler's coverage and uniformity are checked
    against, so it must not share any machinery with :func:`sample_ring`.
    """
    total = p.alphabet**p.period
    if total > limit:
        raise ValueError(
            f"{total} sequences exceeds the {limit} guard; enumeration is for toys"
        )
    out: list[Ring] = []
    letters = [0] * p.period
    for code in range(total):
        rest = code
        for i in range(p.period):
            letters[i] = rest % p.alphabet
            rest //= p.alphabet
        ring = Ring(tuple(letters), p)
        if ring.is_valid:
            out.append(ring)
    return out


def edge_multiset(g: RingGraph, ring: Ring) -> tuple[int, ...]:
    """The ring-graph edges this ring's windows traverse, in order."""
    return tuple(g.edge_of(t) for t in ring.windows)


def unused_loop(g: RingGraph, ring: Ring) -> int:
    """The single self-loop a valid ring omits."""
    missing = ring.omitted_edges(g)
    if len(missing) != 1:
        raise ValueError(f"expected exactly one omitted edge, got {missing}")
    if missing[0] not in g.loops:
        raise ValueError(f"omitted edge {missing[0]} is not a self-loop")
    return missing[0]
