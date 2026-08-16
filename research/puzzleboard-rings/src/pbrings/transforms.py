"""The D4 action on a fragment, resolved down to edge-slot permutations.

A decoded hypothesis is a (position, orientation) pair, so uniqueness is always
a statement about a *group*. Which group applies is one of the questions this
study settles:

* **C4** — the four rotations. A camera looking at an opaque planar board can
  produce any of them, and this is the group the published 99.33 % figure is
  stated under.
* **D4** — the rotations plus the four reflections. Our detector searches all
  eight, which costs it both speed and minimum window size.

Rather than hand-deriving how each transform rearranges the dots, this module
maps *corner coordinates* and re-derives every edge from its two endpoints. A
segment that ran between horizontally adjacent corners may come out running
between vertically adjacent ones, and that flip — horizontal dots becoming
vertical dots — is exactly why a rotated fragment has to be matched against the
*other* code map. It is also the mechanism behind every rotation alias.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import lru_cache

from .window import WindowSpec

Coord = tuple[int, int]
Matrix = tuple[int, int, int, int]  # (a, b, c, d) acting on (row, col)

_D4: dict[str, Matrix] = {
    "id": (1, 0, 0, 1),
    "rot90": (0, 1, -1, 0),
    "rot180": (-1, 0, 0, -1),
    "rot270": (0, -1, 1, 0),
    "flip_row": (-1, 0, 0, 1),
    "flip_col": (1, 0, 0, -1),
    "transpose": (0, 1, 1, 0),
    "anti_transpose": (0, -1, -1, 0),
}

C4_NAMES = ("id", "rot90", "rot180", "rot270")
D4_NAMES = tuple(_D4)
FIXED_NAMES = ("id",)


def apply_corner(name: str, rc: Coord, span: int) -> Coord:
    """Map a corner of a ``span × span`` grid, shifted back into range."""
    a, b, c, d = _D4[name]
    r, col = rc
    nr = a * r + b * col
    nc = c * r + d * col
    if a < 0 or b < 0:
        nr += span - 1
    if c < 0 or d < 0:
        nc += span - 1
    return nr, nc


@dataclass(frozen=True)
class SlotAction:
    """How one transform rearranges a fragment's dots.

    ``v_from[j]`` is the source slot feeding target vertical slot ``j``, and
    likewise for ``h_from``. ``swaps`` records whether vertical dots become
    horizontal ones — when they do, the transformed fragment must be matched
    against the opposite code map.
    """

    name: str
    spec: WindowSpec
    swaps: bool
    v_from: tuple[int, ...]
    h_from: tuple[int, ...]


@lru_cache(maxsize=None)
def slot_action(name: str, spec: WindowSpec) -> SlotAction:
    """Resolve one D4 element into edge-slot permutations for ``spec``.

    Raises if the transform does not map the slot set onto itself — which is a
    genuine check, not a formality: an asymmetric readout model would fail here
    rather than silently produce nonsense.
    """
    span = spec.span
    v_from: list[int | None] = [None] * len(spec.v_slots)
    h_from: list[int | None] = [None] * len(spec.h_slots)
    swaps: bool | None = None

    def place(p1: Coord, p2: Coord, src: int) -> None:
        (r1, c1), (r2, c2) = p1, p2
        if r1 == r2:  # lands as a horizontal segment
            table, index, key = h_from, spec.h_index, (r1, min(c1, c2))
        else:  # lands as a vertical segment
            table, index, key = v_from, spec.v_index, (min(r1, r2), c1)
        if key not in index:
            raise AssertionError(f"{name} on {spec}: {key} left the slot set")
        slot = index[key]
        if table[slot] is not None:
            raise AssertionError(f"{name} on {spec}: slot {slot} claimed twice")
        table[slot] = src

    for src, (r, c) in enumerate(spec.v_slots):
        p1 = apply_corner(name, (r, c), span)
        p2 = apply_corner(name, (r + 1, c), span)
        here = p1[0] == p2[0]
        if swaps is None:
            swaps = here
        elif here != swaps:
            raise AssertionError(f"{name}: inconsistent segment mapping")
        place(p1, p2, src)

    for src, (r, c) in enumerate(spec.h_slots):
        p1 = apply_corner(name, (r, c), span)
        p2 = apply_corner(name, (r, c + 1), span)
        place(p1, p2, src)

    if any(x is None for x in v_from) or any(x is None for x in h_from):
        raise AssertionError(f"{name} on {spec}: not every target slot was filled")
    assert swaps is not None
    return SlotAction(
        name=name,
        spec=spec,
        swaps=swaps,
        v_from=tuple(int(x) for x in v_from),  # type: ignore[arg-type]
        h_from=tuple(int(x) for x in h_from),  # type: ignore[arg-type]
    )


def group_names(group: str) -> tuple[str, ...]:
    """``"c4"`` → the four rotations, ``"d4"`` → all eight, ``"fixed"`` → identity."""
    key = group.lower()
    if key == "c4":
        return C4_NAMES
    if key == "d4":
        return D4_NAMES
    if key == "fixed":
        return FIXED_NAMES
    raise ValueError(f"unknown group {group!r}; expected 'c4', 'd4' or 'fixed'")
