"""PuzzlePole: the same code wrapped round a cylinder, and what it costs.

A **PuzzlePole** is a strip of the master PuzzleBoard wrapped around a cylinder.
In this crate's convention a horizontal-edge bit is ``map_b[mr % P][mc % m]``
and a vertical-edge bit is ``map_a[mr % m][mc % P]`` (``m = 3``, ``P = 167``),
so the long aperiodic code lives in the **row** index of map B and in the
**column** index of map A. Only the row axis carries a long code that can be
made to repeat early, so:

* the **circumference** axis is the master **row**, and
* the **axial** axis (along the cylinder) is the master **column**.

The pole is **not the master.** Its pattern comes from a *modified* code stripe
— map B with its row period shortened from ``P`` to the circumference ``p``::

    stripe[r][k] = map_b[(s + r) % P][k]        r ∈ [0, p)

tiled cyclically, with map A used **verbatim** (its row period ``m`` divides
``p``). Every readout in this module is built through :attr:`PolePattern.stripe`
so the study and the crate cannot drift apart. The seam only guarantees that
*two* consecutive piece rows recur ``p`` rows later — the third does not — which
is exactly why a pole cannot be decoded against the 501×501 master and needs its
own measurement.

Why the fast planar evaluator does not apply
--------------------------------------------

:mod:`pbrings.evaluate` is fast because ``(i, j) ↦ (u, v)`` with
``u = (i mod m, (j-1) mod P)`` and ``v = (j mod m, (i-1) mod P)`` is a
**bijection** of ``Z_master²``: the row index contributes its mod-``m`` residue
to one coordinate and its mod-``P`` residue to the other, and ``gcd(m, P) = 1``
makes those independent by CRT. Then the alias indicator separates as
``f(u)·g(v)`` and the whole count is a sum of rank-one outer products.

On a pole that argument collapses. The circumference coordinate ``t`` enters the
vertical dots as ``t mod m`` and the horizontal dots as ``t mod p`` — and since
``m | p``, the first is a *function* of the second. The two indices are no
longer independent, the position set is no longer a product, the indicator does
not factorise, and the outer-product sum is invalid. This is the same
non-coprimality that stops the Rust decoder collapsing a pole's origin scan by
CRT, wearing a different hat. So this module takes the brute path — which is at
most a few hundred thousand keys, and instantaneous.

Placements: one axis wraps, the other does not
----------------------------------------------

The planar harness enumerates ``P² · m² = 501²`` positions because *both* axes
wrap. A pole is asymmetric and getting it wrong silently changes every number:

* **circumference** — cyclic. All ``p`` origins ``t`` are placements, wrapping
  ones included.
* **axial** — clamped. A strip ``W`` corner columns wide admits only
  ``W - span_x + 1`` origins ``x``, and a fragment may not hang off the end.

Units — check them before comparing anything
--------------------------------------------

Three unit systems are in play and the number 40 appears in two of them meaning
different things:

* the paper counts **pieces** with every bounding edge — ``2w(w+1)`` edges;
* this harness counts **pieces** interior-only — ``2w(w-1)`` edges;
* the Rust ``min_window`` counts **corners** per side.

``interior(w) = 2w(w-1) = total(w-1)``, so an interior readout at ``w`` pieces
sees the paper's edge count at ``w-1``. Everything below is in **corners**, and
says so.

What is deliberately *not* modelled: the checkerboard colour. A dot readout
carries no parity bit, matching :mod:`pbrings.evaluate` and :mod:`pbrings.brute`.
Withholding information can only raise a uniqueness floor, never lower it, so
every floor here is an upper bound on the floor a colour-aware decoder needs.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import cached_property
from math import floor

import numpy as np

from .params import Params
from .ring import Ring
from .transforms import group_names, slot_action
from .window import INTERIOR, WindowSpec

# ---------------------------------------------------------------------------
# The seam condition
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class PolePeriod:
    """A circumference that closes seamlessly, and where its stripe starts.

    ``squares`` is the number of puzzle pieces around the cylinder; ``start_row``
    the master row the wrapped strip begins at. Mirrors ``PuzzlePolePeriod`` in
    ``crates/calib-targets-puzzleboard/src/pole/periods.rs``.
    """

    squares: int
    start_row: int

    @property
    def strip_corner_rows(self) -> int:
        """Corner rows a printable wrap strip spans: ``squares + 1``.

        The first and last land on each other when the strip is wrapped.
        """
        return self.squares + 1

    def __str__(self) -> str:
        return f"p={self.squares}@s={self.start_row}"


def piece_rows_agree(ring_a: Ring, ring_b: Ring, r1: int, r2: int, params: Params) -> bool:
    """Are the puzzle-piece rows at master rows ``r1`` and ``r2`` identical?

    Compares all three families — horizontal-edge bit, vertical-edge bit and
    checkerboard colour — across a full master width. Both maps tile cyclically,
    so agreement over ``params.master`` columns is agreement everywhere.
    """
    m, per, mas = params.n_rows, params.period, params.master
    cols = np.arange(mas)
    horizontal_ok = bool(
        np.array_equal(ring_b.bits[cols % m, r1 % per], ring_b.bits[cols % m, r2 % per])
    )
    vertical_ok = bool(
        np.array_equal(ring_a.bits[r1 % m, cols % per], ring_a.bits[r2 % m, cols % per])
    )
    colour_ok = (r1 - r2) % 2 == 0
    return horizontal_ok and vertical_ok and colour_ok


def is_seamless(ring_a: Ring, ring_b: Ring, squares: int, start_row: int, params: Params) -> bool:
    """Does the pattern repeat with period ``squares`` starting at ``start_row``?

    Two consecutive piece rows must recur ``squares`` rows later. Two is what a
    local ``m×m`` piece code needs, so a seam satisfying this creates no new
    local codes; it is *not* a claim that the pole decodes uniquely, which is
    what the rest of this module measures.
    """
    if squares <= 0:
        return False
    return all(
        piece_rows_agree(ring_a, ring_b, start_row + k, start_row + k + squares, params)
        for k in (0, 1)
    )


def seamless_start_rows(
    ring_a: Ring, ring_b: Ring, squares: int, params: Params, *, full_domain: bool = False
) -> list[int]:
    """Seamless start rows for ``squares``.

    By default this enumerates ``0 .. P`` — the **map-B phases**, the choice that
    picks a different code stripe and so a different code. A start row actually
    names a pole only modulo ``m·P``: adding ``P`` keeps the stripe and advances
    map A's row phase, which is a genuinely different pattern (see
    :attr:`PolePattern.vplane`). ``full_domain=True`` enumerates all of those, and
    gives exactly ``m`` times as many rows, because the seam predicate's
    map-A and colour terms depend on ``squares`` alone.

    The two are kept apart because they answer different questions: the map-B
    phase changes the code, and the map-A phase was *measured* to leave the
    decode floor alone. ``pbr pole floor --phases`` sweeps the latter.
    """
    limit = params.master if full_domain else params.period
    return [
        s for s in range(limit) if is_seamless(ring_a, ring_b, squares, s, params)
    ]


def seam_depth(ring_a: Ring, ring_b: Ring, period: PolePeriod, params: Params, limit: int = 8) -> int:
    """How many consecutive piece rows actually recur ``squares`` rows later.

    The construction is designed to give exactly ``2``. A deeper repeat would
    mean the pole *is* locally the master over a taller window; a shallower one
    would mean the seam is not closed at all.
    """
    depth = 0
    while depth < limit and piece_rows_agree(
        ring_a, ring_b, period.start_row + depth, period.start_row + depth + period.squares, params
    ):
        depth += 1
    return depth


# ---------------------------------------------------------------------------
# The derived pattern
# ---------------------------------------------------------------------------


class PolePattern:
    """The two dot planes of one pole, on a ``p × (W-1)`` cell lattice.

    Cell rows are cyclic modulo ``p`` (the circumference); cell columns run
    ``0 .. W-2`` and do not wrap (the axial strip has ends).
    """

    def __init__(
        self,
        period: PolePeriod,
        corner_cols: int,
        ring_a: Ring,
        ring_b: Ring,
        params: Params,
    ) -> None:
        if corner_cols < 2:
            raise ValueError("a pole must be at least 2 corner columns wide")
        self.period = period
        self.corner_cols = corner_cols
        self.ring_a = ring_a
        self.ring_b = ring_b
        self.params = params

    @property
    def squares(self) -> int:
        return self.period.squares

    @cached_property
    def stripe(self) -> np.ndarray:
        """The derived ``p × m`` code stripe: map B with its row period cut to ``p``.

        ``stripe[r][k] = map_b[(s + r) % P][k]``. Every horizontal dot on the
        pole is read out of this table, so the pole cannot silently be measured
        as if it were the master.
        """
        per = self.params.period
        rows = (self.period.start_row + np.arange(self.squares)) % per
        # ring_b.bits is m × P with bits[k][r] = map_b[r][k].
        return np.ascontiguousarray(self.ring_b.bits[:, rows].T)

    @cached_property
    def vplane(self) -> np.ndarray:
        """``vplane[R][C]`` — the dot on the vertical edge left of cell ``(R, C)``.

        Pole row ``R`` *is* master row ``s + R``, so map A is indexed through
        ``s`` too even though the map itself is used unmodified.

        That makes ``s mod m`` a real degree of freedom. ``s`` and ``s + P``
        give the same stripe but different map-A row phases, and no
        circumference rotation relates them: a rotation by ``k`` would need
        ``k ≡ 0 (mod p)`` to leave the stripe alone and ``k ≢ 0 (mod m)`` to
        move map A, which is impossible while ``m | p``. So a start row is only
        a pole once it is read modulo ``m·P``, and reducing it modulo ``P``
        — as the crate's table does with the paper's start rows — picks one of
        three distinct patterns. Whether that matters is measured, not assumed.
        """
        m, per = self.params.n_rows, self.params.period
        rows = (self.period.start_row + np.arange(self.squares))[:, None] % m
        cols = np.arange(self.corner_cols - 1)[None, :] % per
        return self.ring_a.bits[rows, cols]

    @cached_property
    def hplane(self) -> np.ndarray:
        """``hplane[R][C]`` — the dot on the horizontal edge above cell ``(R, C)``."""
        m = self.params.n_rows
        rows = np.arange(self.squares)[:, None]
        cols = np.arange(self.corner_cols - 1)[None, :] % m
        return self.stripe[rows, cols]

    # -- placements -------------------------------------------------------

    def admits(self, spec: WindowSpec) -> bool:
        """Can a fragment of this shape sit on this pole at all?

        It may span at most ``p + 1`` corner rows — beyond that it would wrap
        past itself — and at most ``W`` corner columns.
        """
        return spec.rows <= self.squares + 1 and spec.cols <= self.corner_cols

    def axial_origins(self, spec: WindowSpec) -> int:
        """``W - span_x + 1``. The clamped axis, and the whole asymmetry."""
        return self.corner_cols - spec.cols + 1

    def n_placements(self, spec: WindowSpec) -> int:
        if not self.admits(spec):
            return 0
        return self.squares * self.axial_origins(spec)

    def placement_grid(self, spec: WindowSpec) -> tuple[np.ndarray, np.ndarray]:
        """``(t, x)`` for every placement, circumference-major."""
        n_x = self.axial_origins(spec)
        t = np.repeat(np.arange(self.squares), n_x)
        x = np.tile(np.arange(n_x), self.squares)
        return t, x

    def parts(self, spec: WindowSpec) -> tuple[np.ndarray, np.ndarray]:
        """The vertical and horizontal dots every placement of ``spec`` shows.

        Two ``(n_placements, n_slots)`` bit arrays, in the spec's slot order.
        """
        if spec.readout != INTERIOR:
            raise ValueError(
                "a pole is measured through the interior readout only: the outer "
                "ring of a fragment has no flanking square inside the fragment, "
                "and at the strip's ends no square outside it either"
            )
        if not self.admits(spec):
            raise ValueError(f"{spec} does not fit on {self.period} at W={self.corner_cols}")
        t, x = self.placement_grid(spec)
        p = self.squares
        v = np.empty((len(t), len(spec.v_slots)), dtype=np.uint8)
        for k, (r, c) in enumerate(spec.v_slots):
            v[:, k] = self.vplane[(t + r) % p, x + c - 1]
        h = np.empty((len(t), len(spec.h_slots)), dtype=np.uint8)
        for k, (r, c) in enumerate(spec.h_slots):
            h[:, k] = self.hplane[(t + r - 1) % p, x + c]
        return v, h

    # -- the code's own sub-perfection ------------------------------------

    def stripe_windows(self) -> list[tuple[int, ...]]:
        """The ``m·p`` cyclic ``m × m`` windows of the derived stripe.

        Cyclic in both axes: ``p`` row shifts and ``m`` column shifts.
        """
        m, p = self.params.n_rows, self.squares
        s = self.stripe
        return [
            tuple(int(s[(r + i) % p, (c + j) % m]) for i in range(m) for j in range(m))
            for r in range(p)
            for c in range(m)
        ]

    def stripe_is_sub_perfect(self) -> bool:
        """Are all ``m·p`` of them distinct?

        Provable from the seam — a window at ``t ≤ p-m`` is a master window at
        ``s+t``, and the ``m-1`` that straddle the seam are master windows at
        ``s+p-m+1 …`` by the two-row repeat — so a ``False`` here is a bug in
        the seam table, not a property of the code.
        """
        w = self.stripe_windows()
        return len(set(w)) == len(w)


# ---------------------------------------------------------------------------
# Uniqueness
# ---------------------------------------------------------------------------


def _match_counts(query: np.ndarray, table: np.ndarray) -> np.ndarray:
    """For each row of ``query``, how many rows of ``table`` equal it.

    Packing to bytes first keeps the key size independent of the window's bit
    count, and one ``np.unique`` over the concatenation avoids materialising an
    ``n_query × n_table`` comparison.
    """
    if len(table) == 0 or query.shape[1] == 0:
        return np.zeros(len(query), dtype=np.int64)
    both = np.concatenate(
        [np.packbits(table, axis=1), np.packbits(query, axis=1)], axis=0
    )
    _, inverse = np.unique(both, axis=0, return_inverse=True)
    inverse = np.asarray(inverse).ravel()
    n_table = len(table)
    counts = np.bincount(inverse[:n_table], minlength=int(inverse.max()) + 1)
    return counts[inverse[n_table:]].astype(np.int64)


@dataclass(frozen=True)
class PoleWindow:
    """Uniqueness of one fragment shape on one pole, under one group."""

    period: PolePeriod
    corner_cols: int
    span_y: int
    span_x: int
    group: str
    n_placements: int
    n_ambiguous: int
    n_edges: int
    hypothesis_histogram: dict[int, int]

    @property
    def is_clean(self) -> bool:
        return self.n_ambiguous == 0

    def format_ambiguous(self) -> str:
        """Integers with their denominator. Never a bare percentage — the whole
        question is whether a count is zero, and 99.9996 % rounds to 100 %."""
        return f"{self.n_ambiguous} / {self.n_placements}"


def window_uniqueness(pole: PolePattern, spec: WindowSpec, group: str) -> PoleWindow:
    """Count consistent ``(placement, orientation)`` hypotheses, exhaustively.

    A placement ``q`` is uniquely localisable iff exactly one ``(q', g)`` in
    ``placements × group`` reproduces ``readout(q, identity)`` — namely
    ``(q, identity)``, which always matches, so the count is at least 1.

    A quarter turn of a ``span_y × span_x`` fragment is a ``span_x × span_y``
    one, so each group element is matched against the placements of *its own*
    destination shape. When that shape does not fit on the pole — a fragment
    wider than the circumference cannot be a rotated copy of anything — the
    element contributes nothing, which is a real effect and not a special case.
    """
    src_v, src_h = pole.parts(spec)
    total = np.zeros(len(src_v), dtype=np.int64)
    tables: dict[WindowSpec, np.ndarray] = {}
    for name in group_names(group):
        act = slot_action(name, spec)
        if not pole.admits(act.dst):
            continue
        if act.dst not in tables:
            dv, dh = pole.parts(act.dst)
            tables[act.dst] = np.concatenate([dv, dh], axis=1)
        if act.swaps:
            moved = np.concatenate(
                [src_h[:, list(act.v_from)], src_v[:, list(act.h_from)]], axis=1
            )
        else:
            moved = np.concatenate(
                [src_v[:, list(act.v_from)], src_h[:, list(act.h_from)]], axis=1
            )
        total += _match_counts(moved, tables[act.dst])
    if total.size and int(total.min()) < 1:
        raise AssertionError("a placement failed to match itself; the readout is wrong")
    counts = np.bincount(total)
    return PoleWindow(
        period=pole.period,
        corner_cols=pole.corner_cols,
        span_y=spec.rows,
        span_x=spec.cols,  # type: ignore[arg-type]
        group=group,
        n_placements=int(total.size),
        n_ambiguous=int((total > 1).sum()),
        n_edges=spec.n_edges,
        hypothesis_histogram={int(k): int(v) for k, v in enumerate(counts) if v},
    )


def sweep_shapes(pole: PolePattern, max_span: int) -> tuple[range, range]:
    """The ``(span_y, span_x)`` ranges worth sweeping, in corners.

    ``span_y`` stops at ``p + 1`` — a fragment cannot wrap past itself — and
    both stop at ``max_span``, past which no single view could supply the
    corners anyway.
    """
    rows = range(4, min(pole.squares + 1, max_span) + 1)
    cols = range(4, min(pole.corner_cols, max_span) + 1)
    return rows, cols


def min_span_y(pole: PolePattern, span_x: int, group: str, max_span: int) -> int | None:
    """Smallest ``span_y`` at which the ambiguous count is exactly 0, or None.

    The count is non-increasing in both spans — a taller or wider fragment's
    readout contains a shorter one's, on the same placement set or a subset —
    so a binary search is exact, and :func:`floor_grid` checks the monotonicity
    it rests on rather than assuming it.
    """
    rows, _ = sweep_shapes(pole, max_span)
    if not rows:
        return None

    def clean(span_y: int) -> bool:
        return window_uniqueness(
            pole, WindowSpec(span_y, span_x, INTERIOR), group
        ).is_clean

    hi = rows.stop - 1
    if not clean(hi):
        return None
    lo = rows.start
    if clean(lo):
        return lo
    while hi - lo > 1:  # invariant: lo is dirty, hi is clean
        mid = (lo + hi) // 2
        if clean(mid):
            hi = mid
        else:
            lo = mid
    return hi


def pareto_floor(pole: PolePattern, group: str, max_span: int = 16) -> list[tuple[int, int]]:
    """The Pareto-minimal ``(span_y, span_x)`` shapes with 0 ambiguous placements.

    Not a scalar: the two axes have different periods and a pole fragment is
    naturally wide-and-short, so the floor is a frontier. Empty means no shape
    within ``max_span`` decodes this pole cleanly.
    """
    _, cols = sweep_shapes(pole, max_span)
    frontier: list[tuple[int, int]] = []
    best = None
    for span_x in cols:
        span_y = min_span_y(pole, span_x, group, max_span)
        if span_y is None:
            continue
        if best is None or span_y < best:
            frontier.append((span_y, span_x))
            best = span_y
    return frontier


def floor_grid(
    pole: PolePattern, group: str, max_span: int = 16
) -> dict[tuple[int, int], PoleWindow]:
    """Every ``(span_y, span_x)`` in the sweep, measured. Slower than the
    frontier search, and the only way to check its monotonicity assumption."""
    rows, cols = sweep_shapes(pole, max_span)
    return {
        (r, c): window_uniqueness(pole, WindowSpec(r, c, INTERIOR), group)
        for r in rows
        for c in cols
    }


# ---------------------------------------------------------------------------
# What a single view can supply
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ViewBudget:
    """How much circumference one camera view can actually deliver.

    A cylinder self-occludes: past roughly 120–140° of arc the surface turns
    away from the camera fast enough that corner detection fails, so a view
    supplies a *fraction* of the circumference and no more, however good the
    lens. This is a stated modelling assumption, not a measurement — it is the
    one input to this study that is not derived from the code maps, and the
    verdict it produces should be read as "under a ``arc_degrees`` view".
    """

    arc_degrees: float

    def visible_cells(self, squares: int) -> int:
        return floor(squares * self.arc_degrees / 360.0)

    def visible_corner_rows(self, squares: int) -> int:
        """Corner rows a view supplies: ``cells + 1``, and never more than
        ``p + 1``."""
        return min(self.visible_cells(squares) + 1, squares + 1)


def required_arc_degrees(squares: int, span_y: int) -> float:
    """Circumference arc a fragment of ``span_y`` corner rows subtends.

    ``span_y`` corners bound ``span_y - 1`` cells, and each cell is
    ``360/p`` degrees — so the demand is ``(span_y - 1)·360/p``. Stating the
    floor this way is what makes it comparable across circumferences: the same
    ``span_y`` is a third of a 12-piece pole and a twelfth of a 48-piece one.
    """
    return (span_y - 1) * 360.0 / squares


#: The band the verdict is reported over: an optimistic and a conservative view.
DEFAULT_ARCS = (140.0, 120.0)


# ---------------------------------------------------------------------------
# The shipped table
# ---------------------------------------------------------------------------

#: Circumferences the PuzzlePoles paper builds (Table 1 of arXiv:2511.19448).
#: Only this list is transcribed — which sizes someone chose to print is not a
#: property of the code. Every start row below is *derived* by :func:`is_seamless`.
PAPER_CIRCUMFERENCES = (12, 18, 24, 30, 36, 42, 48)


def supported_periods(
    ring_a: Ring,
    ring_b: Ring,
    params: Params,
    circumferences: tuple[int, ...] = PAPER_CIRCUMFERENCES,
) -> list[PolePeriod]:
    """Every seamless ``(squares, start_row)`` for the given circumferences.

    The mirror of ``SUPPORTED_PERIODS`` in the crate; the test suite parses that
    table out of ``periods.rs`` and asserts the two agree, so neither side can
    drift.
    """
    return [
        PolePeriod(squares, s)
        for squares in circumferences
        for s in seamless_start_rows(ring_a, ring_b, squares, params)
    ]
