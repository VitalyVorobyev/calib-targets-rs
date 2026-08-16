"""The slow reference evaluator. Deliberately obvious, deliberately independent.

:mod:`pbrings.evaluate` is fast because of three claims:

1. the vertical dots depend only on ``u = (i mod m, (j-1) mod P)`` and the
   horizontal dots only on ``v = (j mod m, (i-1) mod P)``;
2. ``(i, j) ↦ (u, v)`` is a bijection of ``Z_master²``;
3. under any transform the alias indicator factorises as ``f(v)·g(u)``.

This module uses **none** of them, and imports nothing from
:mod:`pbrings.evaluate`, :mod:`pbrings.transforms` or :mod:`pbrings.window`
that could smuggle one back in. It materialises the board, walks every position,
transforms each window by literally moving its corners, and counts matches in a
dictionary. If the two ever disagree, the fast path is wrong.

It is slow on the real board (minutes) and instantaneous on the toy — which is
the point of :data:`pbrings.params.TOY`.
"""

from __future__ import annotations

from dataclasses import dataclass

from .params import Params
from .ring import Ring

# The eight elements of D4, written out again rather than imported, so that a
# sign error in the fast path cannot hide behind a matching sign error here.
_MATRICES: dict[str, tuple[int, int, int, int]] = {
    "id": (1, 0, 0, 1),
    "rot90": (0, 1, -1, 0),
    "rot180": (-1, 0, 0, -1),
    "rot270": (0, -1, 1, 0),
    "flip_row": (-1, 0, 0, 1),
    "flip_col": (1, 0, 0, -1),
    "transpose": (0, 1, 1, 0),
    "anti_transpose": (0, -1, -1, 0),
}

_GROUPS: dict[str, tuple[str, ...]] = {
    "fixed": ("id",),
    "c4": ("id", "rot90", "rot180", "rot270"),
    "d4": tuple(_MATRICES),
}

Edge = tuple[str, int, int]  # (kind, row, col), kind in {"v", "h"}
Pattern = tuple[tuple[Edge, int], ...]


@dataclass(frozen=True)
class BruteResult:
    positions: int
    n_unique: int
    hypothesis_histogram: dict[int, int]

    @property
    def fraction_unique_positions(self) -> float:
        return self.n_unique / self.positions


def master_planes(
    ring_a: Ring, ring_b: Ring, p: Params
) -> tuple[list[list[int]], list[list[int]]]:
    """The two dot planes of the master board, built cell by cell.

    ``vertical[i][j]`` is the dot on the vertical edge left of cell ``(i, j)``,
    ``horizontal[i][j]`` the dot on the horizontal edge above it — matching
    ``vertical_edge_bit`` / ``horizontal_edge_bit`` in the crate.
    """
    m, per, mas = p.n_rows, p.period, p.master
    a, b = ring_a.bits, ring_b.bits
    vertical = [[0] * mas for _ in range(mas)]
    horizontal = [[0] * mas for _ in range(mas)]
    for i in range(mas):
        for j in range(mas):
            vertical[i][j] = int(a[i % m][j % per])
            horizontal[i][j] = int(b[j % m][i % per])
    return vertical, horizontal


def _slots(span: int, readout: str) -> list[Edge]:
    """Every edge of a ``span``-corner fragment that the readout model exposes."""
    out: list[Edge] = []
    for r in range(span - 1):
        for c in range(span):
            if readout == "interior" and c in (0, span - 1):
                continue
            out.append(("v", r, c))
    for r in range(span):
        for c in range(span - 1):
            if readout == "interior" and r in (0, span - 1):
                continue
            out.append(("h", r, c))
    return sorted(out)


def _read(
    vertical: list[list[int]],
    horizontal: list[list[int]],
    slots: list[Edge],
    i: int,
    j: int,
    mas: int,
) -> Pattern:
    """The dots a fragment anchored at master ``(i, j)`` shows, slot by slot."""
    out: list[tuple[Edge, int]] = []
    for slot in slots:
        kind, r, c = slot
        if kind == "v":
            bit = vertical[(i + r) % mas][(j + c - 1) % mas]
        else:
            bit = horizontal[(i + r - 1) % mas][(j + c) % mas]
        out.append((slot, bit))
    return tuple(out)


def _move(name: str, r: int, c: int, span: int) -> tuple[int, int]:
    a, b, cc, d = _MATRICES[name]
    nr, nc = a * r + b * c, cc * r + d * c
    if a < 0 or b < 0:
        nr += span - 1
    if cc < 0 or d < 0:
        nc += span - 1
    return nr, nc


def transform_pattern(pattern: Pattern, name: str, span: int) -> Pattern:
    """Move every dot with its edge, re-deriving the edge from its two corners."""
    out: list[tuple[Edge, int]] = []
    for (kind, r, c), bit in pattern:
        if kind == "v":
            p1, p2 = _move(name, r, c, span), _move(name, r + 1, c, span)
        else:
            p1, p2 = _move(name, r, c, span), _move(name, r, c + 1, span)
        (r1, c1), (r2, c2) = p1, p2
        if r1 == r2:
            slot: Edge = ("h", r1, min(c1, c2))
        else:
            slot = ("v", min(r1, r2), c1)
        out.append((slot, bit))
    return tuple(sorted(out))


def brute_metrics(
    ring_a: Ring,
    ring_b: Ring,
    span: int,
    readout: str,
    group: str,
    p: Params,
    positions: list[tuple[int, int]] | None = None,
) -> BruteResult:
    """Count consistent (position, orientation) hypotheses the obvious way.

    ``positions`` restricts the *queried* positions; the lookup table is always
    built over the whole master, so a restricted run still checks against every
    possible alias.
    """
    if group not in _GROUPS:
        raise ValueError(f"unknown group {group!r}")
    mas = p.master
    vertical, horizontal = master_planes(ring_a, ring_b, p)
    slots = _slots(span, readout)

    table: dict[Pattern, int] = {}
    for i in range(mas):
        for j in range(mas):
            key = _read(vertical, horizontal, slots, i, j, mas)
            table[key] = table.get(key, 0) + 1

    queries = positions if positions is not None else [
        (i, j) for i in range(mas) for j in range(mas)
    ]
    histogram: dict[int, int] = {}
    n_unique = 0
    for i, j in queries:
        observed = _read(vertical, horizontal, slots, i, j, mas)
        n = 0
        for name in _GROUPS[group]:
            moved = transform_pattern(observed, name, span)
            n += table.get(moved, 0)
        histogram[n] = histogram.get(n, 0) + 1
        if n == 1:
            n_unique += 1
    return BruteResult(
        positions=len(queries), n_unique=n_unique, hypothesis_histogram=histogram
    )
