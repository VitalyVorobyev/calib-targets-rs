"""The quotient ring graph — 24 vertices and 168 edges for the real board.

A cyclic letter sequence traces a closed walk in the de Bruijn graph on letter
tuples. Quotienting that graph by σ turns "all windows distinct" into "all edges
distinct", which is what makes the construction a *trail* problem:

    vertices  = σ-orbits of (m-1)-tuples          →  4 fixed + 20 free = 24
    edges     = σ-orbits of m-tuples, non-fixed   →  4·2 + 20·8       = 168

An edge is a window; its tail is the orbit of the window's first ``m-1`` letters
and its head the orbit of the last ``m-1``. A valid map of length ``period`` is
a closed trail of ``period`` distinct edges. Since a closed trail needs every
vertex balanced and the full graph already is, the single omitted edge must be a
**self-loop** — see :func:`loops`.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import cached_property

from .params import Params
from .sigma import Tuple_, free_orbit_reps, is_sigma_fixed, orbit_id, orbit_reps, pack


@dataclass(frozen=True)
class RingGraph:
    """The σ-quotient de Bruijn graph for one :class:`Params`."""

    p: Params

    # -- vertices ---------------------------------------------------------

    @cached_property
    def vertex_reps(self) -> tuple[Tuple_, ...]:
        return orbit_reps(self.p.n_rows - 1, self.p)

    @cached_property
    def _vertex_index(self) -> dict[int, int]:
        return {pack(t, self.p): i for i, t in enumerate(self.vertex_reps)}

    def vertex_of(self, t: Tuple_) -> int:
        """Index of the vertex whose orbit contains the ``(m-1)``-tuple ``t``."""
        return self._vertex_index[orbit_id(t, self.p)]

    @cached_property
    def fixed_vertices(self) -> tuple[int, ...]:
        return tuple(
            i for i, t in enumerate(self.vertex_reps) if is_sigma_fixed(t, self.p)
        )

    # -- edges ------------------------------------------------------------

    @cached_property
    def edge_reps(self) -> tuple[Tuple_, ...]:
        """One ``m``-tuple per usable edge, the orbit's canonical representative."""
        return free_orbit_reps(self.p.n_rows, self.p)

    @cached_property
    def _edge_index(self) -> dict[int, int]:
        return {pack(t, self.p): i for i, t in enumerate(self.edge_reps)}

    def edge_of(self, t: Tuple_) -> int:
        """Index of the edge whose orbit contains the ``m``-tuple ``t``."""
        return self._edge_index[orbit_id(t, self.p)]

    @cached_property
    def tails(self) -> tuple[int, ...]:
        return tuple(self.vertex_of(t[:-1]) for t in self.edge_reps)

    @cached_property
    def heads(self) -> tuple[int, ...]:
        return tuple(self.vertex_of(t[1:]) for t in self.edge_reps)

    @cached_property
    def loops(self) -> tuple[int, ...]:
        """Edges whose tail and head coincide.

        Exactly these may be omitted from a valid map: deleting any non-loop
        edge unbalances two vertices, and an unbalanced digraph has no closed
        trail covering all its edges.
        """
        return tuple(
            e for e in range(self.n_edges) if self.tails[e] == self.heads[e]
        )

    # -- shape ------------------------------------------------------------

    @cached_property
    def n_vertices(self) -> int:
        return len(self.vertex_reps)

    @cached_property
    def n_edges(self) -> int:
        return len(self.edge_reps)

    @cached_property
    def out_edges(self) -> tuple[tuple[int, ...], ...]:
        buckets: list[list[int]] = [[] for _ in range(self.n_vertices)]
        for e, v in enumerate(self.tails):
            buckets[v].append(e)
        return tuple(tuple(b) for b in buckets)

    @cached_property
    def in_edges(self) -> tuple[tuple[int, ...], ...]:
        buckets: list[list[int]] = [[] for _ in range(self.n_vertices)]
        for e, v in enumerate(self.heads):
            buckets[v].append(e)
        return tuple(tuple(b) for b in buckets)

    def out_degree(self, v: int) -> int:
        return len(self.out_edges[v])

    def in_degree(self, v: int) -> int:
        return len(self.in_edges[v])

    # -- invariants -------------------------------------------------------

    @cached_property
    def is_balanced(self) -> bool:
        return all(
            self.out_degree(v) == self.in_degree(v) for v in range(self.n_vertices)
        )

    @cached_property
    def is_strongly_connected(self) -> bool:
        return self._reaches(self.out_edges, self.heads) and self._reaches(
            self.in_edges, self.tails
        )

    def _reaches(
        self, adj: tuple[tuple[int, ...], ...], other_end: tuple[int, ...]
    ) -> bool:
        seen = {0}
        stack = [0]
        while stack:
            v = stack.pop()
            for e in adj[v]:
                w = other_end[e]
                if w not in seen:
                    seen.add(w)
                    stack.append(w)
        return len(seen) == self.n_vertices

    @cached_property
    def is_eulerian(self) -> bool:
        return self.is_balanced and self.is_strongly_connected

    def summary(self) -> dict[str, object]:
        """Everything ``pbr graph info`` prints, as plain data."""
        degrees: dict[int, int] = {}
        for v in range(self.n_vertices):
            degrees[self.out_degree(v)] = degrees.get(self.out_degree(v), 0) + 1
        return {
            "n_rows": self.p.n_rows,
            "alphabet": self.p.alphabet,
            "n_vertices": self.n_vertices,
            "n_fixed_vertices": len(self.fixed_vertices),
            "n_edges": self.n_edges,
            "out_degree_histogram": dict(sorted(degrees.items())),
            "n_loops": len(self.loops),
            "loops": list(self.loops),
            "balanced": self.is_balanced,
            "strongly_connected": self.is_strongly_connected,
            "eulerian": self.is_eulerian,
            "period": self.p.period,
            "master": self.p.master,
            "positions": self.p.positions,
        }
