"""What a fragment of the board actually lets you read.

Everything in this study is indexed by a **span**: the number of chessboard
corners along one side of the visible fragment. A fragment spanning ``rows`` ×
``cols`` corners encloses ``(rows-1) × (cols-1)`` pieces. Most of the planar
study uses square fragments; a :class:`WindowSpec` is rectangular because a
PuzzlePole fragment is not square — its two axes have different periods (see
:mod:`pbrings.pole`), so its uniqueness floor is a 2-D frontier rather than a
single span.

Two readout models matter, and conflating them is the single easiest way to get
a wrong answer here — both are described in the literature as "24 edges" at the
smallest useful size, yet they are *different* sets of 24 edges carrying
different amounts of information:

``ALL``
    Every edge bounding the visible pieces, the outer ring included. This is
    what the PuzzleBoard paper counts: "all 24 edges of a 3×3 PuzzleBoard
    pieces" is a span-4 fragment, and "all 40 edges of 4×4 pieces" is span 5.

``INTERIOR``
    Only edges whose dot our detector can actually sample. A dot sits at an edge
    midpoint and is read against the two squares flanking it, so it is readable
    only when the corners of *both* squares were detected — which excludes the
    fragment's outermost ring of edges. This is the model
    ``calib-targets-puzzleboard`` implements, and it costs two columns of each
    code block relative to ``ALL``.

At span 4 that difference is 24 informative bits versus 18, and since the master
already fills 95.75 % of the 18-bit space, it is the difference between mostly
unique and hopelessly aliased. The gap is a property of the *sampler*, not of
the code.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import cached_property

ALL = "all"
INTERIOR = "interior"

Slot = tuple[int, int]


@dataclass(frozen=True)
class WindowSpec:
    """The edge slots visible in a ``rows × cols`` corner fragment.

Use :meth:`square` for the square fragments the planar
    study talks about; both extents are spelled out here so that a rectangle can
    never be created by forgetting an argument.

    Slots are named by their anchor corner. A vertical segment ``(r, c)`` runs
    from corner ``(r, c)`` to ``(r+1, c)``; a horizontal one runs from ``(r, c)``
    to ``(r, c+1)``. The crate's lookup convention — horizontal edge ``(r, c)``
    reads master cell ``(r-1, c)``, vertical edge ``(r, c)`` reads ``(r, c-1)``
    — is applied in :mod:`pbrings.evaluate` and :mod:`pbrings.pole`.
    """

    rows: int
    cols: int
    readout: str = ALL

    def __post_init__(self) -> None:
        if self.rows < 2 or self.cols < 2:
            raise ValueError("a fragment must span at least 2 corners on each axis")
        if self.readout not in (ALL, INTERIOR):
            raise ValueError(f"unknown readout {self.readout!r}")

    @classmethod
    def square(cls, span: int, readout: str = ALL) -> WindowSpec:
        """A ``span × span`` fragment — the shape the planar study measures."""
        return cls(rows=span, cols=span, readout=readout)

    # -- shape ------------------------------------------------------------

    @property
    def is_square(self) -> bool:
        return self.rows == self.cols

    @property
    def span(self) -> int:
        """Side length in corners. Only defined for a square fragment."""
        if not self.is_square:
            raise ValueError(f"{self} is rectangular; use .rows / .cols")
        return self.rows

    @property
    def pieces(self) -> int:
        """Side length in chessboard squares — the paper's window label."""
        return self.span - 1

    @property
    def piece_rows(self) -> int:
        return self.rows - 1

    @property
    def piece_cols(self) -> int:
        return self.cols - 1

    def transposed(self) -> WindowSpec:
        """The same fragment with its two axes exchanged — the shape a quarter
        turn or a transposition lands in."""
        return WindowSpec(rows=self.cols, cols=self.rows, readout=self.readout)

    # -- slot ranges ------------------------------------------------------
    #
    # ALL      : vertical  r ∈ [0, rows-1), c ∈ [0, cols)
    #            horizontal r ∈ [0, rows),  c ∈ [0, cols-1)
    # INTERIOR : drop the outer ring — vertical loses its first and last
    #            column, horizontal its first and last row.

    @cached_property
    def v_rows(self) -> range:
        return range(0, self.rows - 1)

    @cached_property
    def v_cols(self) -> range:
        return range(0, self.cols) if self.readout == ALL else range(1, self.cols - 1)

    @cached_property
    def h_rows(self) -> range:
        return range(0, self.rows) if self.readout == ALL else range(1, self.rows - 1)

    @cached_property
    def h_cols(self) -> range:
        return range(0, self.cols - 1)

    # -- slots ------------------------------------------------------------

    @cached_property
    def v_slots(self) -> tuple[Slot, ...]:
        return tuple((r, c) for r in self.v_rows for c in self.v_cols)

    @cached_property
    def h_slots(self) -> tuple[Slot, ...]:
        return tuple((r, c) for r in self.h_rows for c in self.h_cols)

    @cached_property
    def v_index(self) -> dict[Slot, int]:
        return {s: i for i, s in enumerate(self.v_slots)}

    @cached_property
    def h_index(self) -> dict[Slot, int]:
        return {s: i for i, s in enumerate(self.h_slots)}

    @cached_property
    def n_edges(self) -> int:
        """Observable dots. For a square span ``s``: ``2·s·(s-1)`` for ALL,
        ``2·(s-1)·(s-2)`` for INTERIOR."""
        return len(self.v_slots) + len(self.h_slots)

    def v_block_shape(self, n_rows: int) -> tuple[int, int]:
        """Shape of the informative map-A block this window reads.

        Map A is ``m × period``; its *rows* wrap modulo ``m``, so anything past
        ``m`` rows of the fragment is a repeat and carries nothing new.
        """
        return (min(len(self.v_rows), n_rows), len(self.v_cols))

    def h_block_shape(self, n_rows: int) -> tuple[int, int]:
        """Shape of the informative map-B block. Map B is ``period × m``, so it
        is its *columns* that wrap modulo ``m``."""
        return (len(self.h_rows), min(len(self.h_cols), n_rows))

    def informative_bits(self, n_rows: int) -> int:
        vr, vc = self.v_block_shape(n_rows)
        hr, hc = self.h_block_shape(n_rows)
        return vr * vc + hr * hc

    def __str__(self) -> str:
        shape = f"span{self.rows}" if self.is_square else f"{self.rows}x{self.cols}"
        return f"{shape}/{self.readout}"


def paper_window(pieces: int) -> WindowSpec:
    """The paper's ``k × k`` pieces with every bounding edge readable."""
    return WindowSpec.square(pieces + 1, readout=ALL)


def detector_window(span: int) -> WindowSpec:
    """A fragment spanning ``span`` corners, read the way our sampler reads it."""
    return WindowSpec.square(span, readout=INTERIOR)
