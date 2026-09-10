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

The second half does the same job for :mod:`pbrings.pole`. That module has no
factorisation to distrust — a pole's two position coordinates are not
independent, so there is no fast path to check — but it does have a vectorised
readout, a rectangular D4 action and a placement enumeration that is cyclic on
one axis and clamped on the other, all three of which are easy to get subtly
wrong. The oracle here rebuilds the pole cell by cell, walks every placement,
moves every dot by its corners and *derives* each destination shape from where
the corners land rather than being told. It shares nothing with
:mod:`pbrings.pole` but the ring pair.
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


# ---------------------------------------------------------------------------
# PuzzlePole
# ---------------------------------------------------------------------------


def pole_planes(
    ring_a: Ring,
    ring_b: Ring,
    squares: int,
    start_row: int,
    corner_cols: int,
    p: Params,
) -> tuple[list[list[int]], list[list[int]]]:
    """The two dot planes of a pole: ``squares`` cell rows by ``corner_cols-1``
    cell columns.

    Pole row ``R`` is master row ``start_row + R``, wrapped. Map B is read
    through the shortened ``squares × m`` stripe; map A unmodified. Written out
    element by element so that a mistake in the vectorised version cannot be
    mirrored here.
    """
    m, per = p.n_rows, p.period
    a, b = ring_a.bits, ring_b.bits
    stripe = [
        [int(b[k][(start_row + r) % per]) for k in range(m)] for r in range(squares)
    ]
    vertical = [
        [int(a[(start_row + r) % m][c % per]) for c in range(corner_cols - 1)]
        for r in range(squares)
    ]
    horizontal = [
        [stripe[r][c % m] for c in range(corner_cols - 1)] for r in range(squares)
    ]
    return vertical, horizontal


def _rect_slots(rows: int, cols: int) -> list[Edge]:
    """The interior edges of a ``rows × cols`` corner fragment.

    Interior only: a dot needs the corners of both flanking squares, so the
    fragment's outermost ring is unreadable — and on a pole's clamped axis the
    square outside the strip does not exist either.
    """
    out: list[Edge] = []
    for r in range(rows - 1):
        for c in range(1, cols - 1):
            out.append(("v", r, c))
    for r in range(1, rows - 1):
        for c in range(cols - 1):
            out.append(("h", r, c))
    return sorted(out)


def _pole_read(
    vertical: list[list[int]],
    horizontal: list[list[int]],
    slots: list[Edge],
    t: int,
    x: int,
    squares: int,
) -> Pattern:
    """The dots a fragment anchored at pole ``(t, x)`` shows.

    ``t`` wraps modulo the circumference; ``x`` does not wrap at all.
    """
    out: list[tuple[Edge, int]] = []
    for slot in slots:
        kind, r, c = slot
        if kind == "v":
            bit = vertical[(t + r) % squares][x + c - 1]
        else:
            bit = horizontal[(t + r - 1) % squares][x + c]
        out.append((slot, bit))
    return tuple(out)


def _rect_move(name: str, r: int, c: int, rows: int, cols: int) -> tuple[int, int]:
    """Move one corner of a ``rows × cols`` grid. Each output coordinate is a
    signed multiple of exactly one input coordinate, so a negative coefficient
    is offset by the extent of the axis that fed it."""
    a, b, cc, d = _MATRICES[name]
    nr, nc = a * r + b * c, cc * r + d * c
    if a < 0:
        nr += rows - 1
    if b < 0:
        nr += cols - 1
    if cc < 0:
        nc += rows - 1
    if d < 0:
        nc += cols - 1
    return nr, nc


def rect_image_shape(name: str, rows: int, cols: int) -> tuple[int, int]:
    """The corner grid a transformed ``rows × cols`` fragment occupies.

    Measured from where the corners actually land, not looked up — so a wrong
    shape shows up as a shape mismatch instead of a silent miscount.
    """
    corners = [
        _rect_move(name, r, c, rows, cols) for r in range(rows) for c in range(cols)
    ]
    return max(q[0] for q in corners) + 1, max(q[1] for q in corners) + 1


def transform_rect_pattern(
    pattern: Pattern, name: str, rows: int, cols: int
) -> Pattern:
    """Move every dot with its edge, re-deriving the edge from its two corners."""
    out: list[tuple[Edge, int]] = []
    for (kind, r, c), bit in pattern:
        if kind == "v":
            p1 = _rect_move(name, r, c, rows, cols)
            p2 = _rect_move(name, r + 1, c, rows, cols)
        else:
            p1 = _rect_move(name, r, c, rows, cols)
            p2 = _rect_move(name, r, c + 1, rows, cols)
        (r1, c1), (r2, c2) = p1, p2
        if r1 == r2:
            slot: Edge = ("h", r1, min(c1, c2))
        else:
            slot = ("v", min(r1, r2), c1)
        out.append((slot, bit))
    return tuple(sorted(out))


@dataclass(frozen=True)
class PoleBruteResult:
    n_placements: int
    n_ambiguous: int
    hypothesis_histogram: dict[int, int]


def pole_brute_metrics(
    ring_a: Ring,
    ring_b: Ring,
    squares: int,
    start_row: int,
    corner_cols: int,
    span_y: int,
    span_x: int,
    group: str,
    p: Params,
) -> PoleBruteResult:
    """Count consistent ``(placement, orientation)`` hypotheses the obvious way.

    ``span_y`` is the circumference extent in corners and ``span_x`` the axial
    one. Lookup tables are built for every shape the group can land in, which
    for a rectangle means two of them.
    """
    if group not in _GROUPS:
        raise ValueError(f"unknown group {group!r}")
    vertical, horizontal = pole_planes(
        ring_a, ring_b, squares, start_row, corner_cols, p
    )

    def fits(rows: int, cols: int) -> bool:
        return rows <= squares + 1 and cols <= corner_cols

    tables: dict[tuple[int, int], dict[Pattern, int]] = {}
    for shape in {(span_y, span_x), (span_x, span_y)}:
        rows, cols = shape
        if not fits(rows, cols):
            continue
        slots = _rect_slots(rows, cols)
        table: dict[Pattern, int] = {}
        for t in range(squares):
            for x in range(corner_cols - cols + 1):
                key = _pole_read(vertical, horizontal, slots, t, x, squares)
                table[key] = table.get(key, 0) + 1
        tables[shape] = table

    src_slots = _rect_slots(span_y, span_x)
    histogram: dict[int, int] = {}
    n_placements = 0
    n_ambiguous = 0
    for t in range(squares):
        for x in range(corner_cols - span_x + 1):
            observed = _pole_read(vertical, horizontal, src_slots, t, x, squares)
            n = 0
            for name in _GROUPS[group]:
                shape = rect_image_shape(name, span_y, span_x)
                table = tables.get(shape)
                if table is None:
                    continue
                moved = transform_rect_pattern(observed, name, span_y, span_x)
                n += table.get(moved, 0)
            histogram[n] = histogram.get(n, 0) + 1
            n_placements += 1
            if n > 1:
                n_ambiguous += 1
    return PoleBruteResult(
        n_placements=n_placements,
        n_ambiguous=n_ambiguous,
        hypothesis_histogram=histogram,
    )
