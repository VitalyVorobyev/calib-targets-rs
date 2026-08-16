"""The fast evaluator — exact over every master position, in a few milliseconds.

The board never has to be materialised, because the code **factorises**. Write
``m`` for the map height and ``P`` for the period. The vertical dots a fragment
at master ``(i, j)`` can see depend only on

    u = (i mod m, (j-1) mod P)      — an index into map A, ``m·P = master`` values

and the horizontal dots only on

    v = (j mod m, (i-1) mod P)      — likewise into map B

and ``(i, j) ↦ (u, v)`` is a bijection of ``Z_master²`` onto itself. So the whole
501×501 code table is an outer combination of two 501-entry tables.

The payoff is in the aliasing. Under a transform ``g`` the transformed
fragment's vertical part is a rearrangement of one of the two tables and its
horizontal part of the other, so the number of master positions matching it is a
*product* of two independent counts, one a function of ``u`` alone and the other
of ``v`` alone:

    hypotheses(u, v) = Σ_g  nA_g(·) · nB_g(·)

Summing rank-one outer products over the group gives, exactly and without
sampling, how many (position, orientation) hypotheses each of the 251001
positions admits. A position is uniquely localisable when that count is 1.

:mod:`pbrings.brute` recomputes the same numbers the obvious slow way, using
none of the above, and the test suite asserts they agree.
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass

import numpy as np

from .params import Params
from .ring import Ring
from .transforms import group_names, slot_action
from .window import WindowSpec

#: Sentinel for "this transformed pattern matches no master position".
NO_MATCH = -1


# ---------------------------------------------------------------------------
# Part tables
# ---------------------------------------------------------------------------


def _residues(p: Params) -> tuple[np.ndarray, np.ndarray]:
    """Split the flat index ``u = a·P + b`` into its two residue components."""
    flat = np.arange(p.master, dtype=np.int64)
    return flat // p.period, flat % p.period


def vertical_parts(ring_a: Ring, spec: WindowSpec, p: Params) -> np.ndarray:
    """``(master, n_v_slots)`` bit array: the vertical dots seen at each ``u``.

    Slot ``(r, c)`` reads ``A[(a+r) mod m][(b+c) mod P]``, from the crate's
    convention that a vertical edge anchored at ``(r, c)`` looks up cell
    ``(r, c-1)``.
    """
    a, b = _residues(p)
    bits = ring_a.bits
    out = np.empty((p.master, len(spec.v_slots)), dtype=np.uint8)
    for k, (r, c) in enumerate(spec.v_slots):
        out[:, k] = bits[(a + r) % p.n_rows, (b + c) % p.period]
    return out


def horizontal_parts(ring_b: Ring, spec: WindowSpec, p: Params) -> np.ndarray:
    """``(master, n_h_slots)`` bit array: the horizontal dots seen at each ``v``.

    Slot ``(r, c)`` reads ``B[(a+c) mod m][(b+r) mod P]``, from the convention
    that a horizontal edge anchored at ``(r, c)`` looks up cell ``(r-1, c)``.
    """
    a, b = _residues(p)
    bits = ring_b.bits
    out = np.empty((p.master, len(spec.h_slots)), dtype=np.uint8)
    for k, (r, c) in enumerate(spec.h_slots):
        out[:, k] = bits[(a + c) % p.n_rows, (b + r) % p.period]
    return out


def _keys(parts: np.ndarray) -> list[bytes]:
    """Hashable exact keys — packbits keeps this independent of window size."""
    packed = np.packbits(parts, axis=1)
    return [row.tobytes() for row in packed]


def _lookup(parts: np.ndarray) -> dict[bytes, list[int]]:
    table: dict[bytes, list[int]] = {}
    for i, key in enumerate(_keys(parts)):
        table.setdefault(key, []).append(i)
    return table


def _match(
    query: np.ndarray, table: dict[bytes, list[int]]
) -> tuple[np.ndarray, np.ndarray]:
    """Per query row: how many table entries equal it, and the first such index."""
    counts = np.zeros(len(query), dtype=np.int64)
    where = np.full(len(query), NO_MATCH, dtype=np.int64)
    for i, key in enumerate(_keys(query)):
        hit = table.get(key)
        if hit:
            counts[i] = len(hit)
            where[i] = hit[0]
    return counts, where


# ---------------------------------------------------------------------------
# Hypothesis counting
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class TransformTerm:
    """One group element's contribution, as a rank-one outer product.

    ``row`` is indexed by ``u`` and ``col`` by ``v``, whichever way round the
    transform happened to route the two code maps.
    """

    name: str
    swaps: bool
    row: np.ndarray
    col: np.ndarray
    #: Matched index of the aliased partner, or ``NO_MATCH``.
    row_partner: np.ndarray
    col_partner: np.ndarray


def transform_terms(
    ring_a: Ring, ring_b: Ring, spec: WindowSpec, group: str, p: Params
) -> list[TransformTerm]:
    vparts = vertical_parts(ring_a, spec, p)
    hparts = horizontal_parts(ring_b, spec, p)
    vtable = _lookup(vparts)
    htable = _lookup(hparts)

    terms: list[TransformTerm] = []
    for name in group_names(group):
        act = slot_action(name, spec)
        if act.swaps:
            # Transformed vertical dots come from the horizontal table (a
            # function of v), and the transformed horizontal dots from the
            # vertical table (a function of u).
            new_v = hparts[:, list(act.v_from)]
            new_h = vparts[:, list(act.h_from)]
            col, col_partner = _match(new_v, vtable)  # indexed by v
            row, row_partner = _match(new_h, htable)  # indexed by u
        else:
            new_v = vparts[:, list(act.v_from)]
            new_h = hparts[:, list(act.h_from)]
            row, row_partner = _match(new_v, vtable)  # indexed by u
            col, col_partner = _match(new_h, htable)  # indexed by v
        terms.append(
            TransformTerm(name, act.swaps, row, col, row_partner, col_partner)
        )
    return terms


def hypothesis_counts(terms: list[TransformTerm], p: Params) -> np.ndarray:
    """``[u, v]`` → number of consistent (position, orientation) hypotheses.

    Always at least 1: the true hypothesis always matches.
    """
    total = np.zeros((p.master, p.master), dtype=np.int64)
    for t in terms:
        total += np.outer(t.row, t.col)
    return total


# ---------------------------------------------------------------------------
# Collision structure
# ---------------------------------------------------------------------------


class _UnionFind:
    def __init__(self) -> None:
        self.parent: dict[int, int] = {}

    def find(self, x: int) -> int:
        self.parent.setdefault(x, x)
        while self.parent[x] != x:
            self.parent[x] = self.parent[self.parent[x]]
            x = self.parent[x]
        return x

    def union(self, x: int, y: int) -> None:
        rx, ry = self.find(x), self.find(y)
        if rx != ry:
            self.parent[rx] = ry


def collision_structure(
    terms: list[TransformTerm], total: np.ndarray, p: Params
) -> tuple[dict[int, int], int]:
    """Class-size histogram over positions, plus the count of positions that are
    ambiguous only in *orientation* (aliased with themselves under some ``g``).

    Only positions with more than one hypothesis can be in a non-trivial class,
    and there are few of them, so this walks that set rather than all 251001.
    """
    uu, vv = np.nonzero(total > 1)
    uf = _UnionFind()
    self_only = 0
    for u, v in zip(uu.tolist(), vv.tolist()):
        here = u * p.master + v
        uf.find(here)
        distinct_partner = False
        for t in terms:
            if t.name == "id":
                continue
            # A term's `row`/`col` are indexed by u/v when the transform keeps
            # the two maps in their lanes, and the other way round when it
            # swaps them. Either way the partner's u comes from whichever
            # lookup landed in the vertical table.
            if t.swaps:
                match_u, match_v = t.col_partner[v], t.row_partner[u]
            else:
                match_u, match_v = t.row_partner[u], t.col_partner[v]
            if match_u == NO_MATCH or match_v == NO_MATCH:
                continue
            other = int(match_u) * p.master + int(match_v)
            if other != here:
                distinct_partner = True
                uf.union(here, other)
        if not distinct_partner:
            self_only += 1

    sizes: Counter[int] = Counter()
    for node in uf.parent:
        sizes[uf.find(node)] += 1
    hist: dict[int, int] = {}
    for size in sizes.values():
        hist[size] = hist.get(size, 0) + 1
    return hist, self_only


# ---------------------------------------------------------------------------
# Hamming geometry
# ---------------------------------------------------------------------------


def _row_min_distance(parts: np.ndarray) -> np.ndarray:
    """For each row, the Hamming distance to its nearest distinct-index peer."""
    n = len(parts)
    dist = np.empty((n, n), dtype=np.int32)
    for i in range(n):
        dist[i] = (parts[i] != parts).sum(axis=1)
    np.fill_diagonal(dist, np.iinfo(np.int32).max)
    return dist.min(axis=1).astype(np.int64)


def nn_histogram(vparts: np.ndarray, hparts: np.ndarray) -> dict[int, int]:
    """Histogram of the nearest-neighbour Hamming distance over all positions.

    The distance between two fragment codes is the sum of the distances of their
    two halves, so the nearest neighbour of a position is reached by perturbing
    exactly one half — whichever has the closer neighbour. Hence
    ``NN(u, v) = min(rowmin_A[u], rowmin_B[v])``, and the histogram of a
    ``min`` over an outer product needs no outer product:

        #{min = d} = #{a = d}·#{b ≥ d} + #{a > d}·#{b = d}
    """
    a = np.bincount(_row_min_distance(vparts))
    b = np.bincount(_row_min_distance(hparts))
    n = max(len(a), len(b))
    a = np.pad(a, (0, n - len(a)))
    b = np.pad(b, (0, n - len(b)))
    b_ge = np.cumsum(b[::-1])[::-1]
    a_gt = np.cumsum(a[::-1])[::-1] - a
    out = a * b_ge + a_gt * b
    return {int(d): int(v) for d, v in enumerate(out) if v}


# ---------------------------------------------------------------------------
# Public entry point
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class WindowMetrics:
    """Everything measured for one fragment shape and one symmetry group."""

    span: int
    readout: str
    group: str
    positions: int
    n_edges: int
    informative_bits: int
    n_unique: int
    hypothesis_histogram: dict[int, int]
    class_size_histogram: dict[int, int]
    n_orientation_only: int
    nn_hist: dict[int, int]

    @property
    def n_classes(self) -> int:
        """Distinct patterns: singleton classes plus the merged ones."""
        merged_positions = sum(s * n for s, n in self.class_size_histogram.items())
        merged_classes = sum(self.class_size_histogram.values())
        return self.positions - merged_positions + merged_classes

    @property
    def fraction_unique_positions(self) -> float:
        """Share of board *positions* that decode unambiguously."""
        return self.n_unique / self.positions

    @property
    def fraction_unique_patterns(self) -> float:
        """Share of distinct *patterns* that are unique — the paper's denominator."""
        return self.n_unique / self.n_classes

    @property
    def d_min(self) -> int:
        return min(self.nn_hist) if self.nn_hist else -1

    def format_positions(self) -> str:
        # Never a bare percentage: 99.9996 % rounds to 100.0 % and would answer
        # the study's central question wrong.
        return (
            f"{100.0 * self.fraction_unique_positions:.4f}% "
            f"({self.n_unique}/{self.positions})"
        )

    def format_patterns(self) -> str:
        return (
            f"{100.0 * self.fraction_unique_patterns:.4f}% "
            f"({self.n_unique}/{self.n_classes})"
        )


def evaluate_window(
    ring_a: Ring,
    ring_b: Ring,
    spec: WindowSpec,
    group: str,
    p: Params,
    *,
    with_nn: bool = False,
) -> WindowMetrics:
    """Exact metrics for one fragment shape and symmetry group."""
    terms = transform_terms(ring_a, ring_b, spec, group, p)
    total = hypothesis_counts(terms, p)
    counts = np.bincount(total.ravel())
    hyp_hist = {int(k): int(v) for k, v in enumerate(counts) if v}
    class_hist, self_only = collision_structure(terms, total, p)
    nn: dict[int, int] = {}
    if with_nn:
        nn = nn_histogram(
            vertical_parts(ring_a, spec, p), horizontal_parts(ring_b, spec, p)
        )
    return WindowMetrics(
        span=spec.span,
        readout=spec.readout,
        group=group,
        positions=p.positions,
        n_edges=spec.n_edges,
        informative_bits=spec.informative_bits(p.n_rows),
        n_unique=hyp_hist.get(1, 0),
        hypothesis_histogram=hyp_hist,
        class_size_histogram=class_hist,
        n_orientation_only=self_only,
        nn_hist=nn,
    )
