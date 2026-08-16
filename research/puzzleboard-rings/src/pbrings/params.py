"""The single place where the study's size constants live.

Every other module takes a :class:`Params` and derives what it needs. That is
not decoration: it is what lets the test suite run the *same* code paths at a
toy size where exhaustive brute force is instant. A hard-coded ``167`` anywhere
else would silently create a second implementation that the toy never exercises.

The real PuzzleBoard is ``Params(n_rows=3)``. Everything below is derived from
that one number.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import cached_property


def _is_prime(n: int) -> bool:
    if n < 2:
        return False
    d = 2
    while d * d <= n:
        if n % d == 0:
            return False
        d += 1
    return True


@dataclass(frozen=True)
class Params:
    """Size parameters of one instance of the construction.

    ``n_rows`` (written ``m`` below) is the height of code map A, the order of
    the row-rotation ``σ``, and the side of the code window. It must be prime:
    the whole orbit argument rests on ``σ`` having prime order, so that every
    orbit has size 1 or exactly ``m``.
    """

    n_rows: int = 3

    def __post_init__(self) -> None:
        if not _is_prime(self.n_rows):
            raise ValueError(
                f"n_rows must be prime (σ must have prime order), got {self.n_rows}"
            )

    # -- alphabet ---------------------------------------------------------

    @cached_property
    def alphabet(self) -> int:
        """Number of letters: one per binary column of height ``m``."""
        return 1 << self.n_rows

    @cached_property
    def letter_mask(self) -> int:
        return self.alphabet - 1

    @cached_property
    def n_fixed_letters(self) -> int:
        """Letters fixed by σ: all-zeros and all-ones, for any ``m``."""
        return 2

    # -- the quotient ring graph -----------------------------------------

    def n_tuples(self, k: int) -> int:
        """Number of ``k``-letter tuples."""
        return self.alphabet**k

    def n_fixed_tuples(self, k: int) -> int:
        """Tuples fixed by σ: every letter must itself be σ-fixed."""
        return self.n_fixed_letters**k

    def n_orbits(self, k: int) -> int:
        """σ-orbits of ``k``-tuples, fixed points included."""
        free = self.n_tuples(k) - self.n_fixed_tuples(k)
        return self.n_fixed_tuples(k) + free // self.n_rows

    @cached_property
    def n_vertices(self) -> int:
        """Vertices of the ring graph: σ-orbits of ``(m-1)``-tuples. 24 for m=3."""
        return self.n_orbits(self.n_rows - 1)

    @cached_property
    def n_edges(self) -> int:
        """Usable edges: σ-orbits of ``m``-tuples, *excluding* the σ-fixed ones.

        A σ-fixed window would coincide with its own row shifts, collapsing
        three of the map's windows into one. 168 for m=3.
        """
        free = self.n_tuples(self.n_rows) - self.n_fixed_tuples(self.n_rows)
        return free // self.n_rows

    # -- map and board ----------------------------------------------------

    @cached_property
    def period(self) -> int:
        """Cyclic length of one map: all usable edges but one. 167 for m=3."""
        return self.n_edges - 1

    @cached_property
    def master(self) -> int:
        """Side of the master board. ``gcd(m, period) = 1`` makes it ``m·period``."""
        return self.n_rows * self.period

    @cached_property
    def positions(self) -> int:
        """Distinct board positions — the denominator of every uniqueness figure."""
        return self.master * self.master

    @cached_property
    def min_window(self) -> int:
        """Smallest corner window whose blocks are informative on both maps.

        A ``w``-corner window reads a ``min(w-1, m) × w`` block of map A and a
        ``w × min(w-1, m)`` block of map B, so both reach the full ``m×m``
        code window exactly at ``w = m + 1``. This is the paper's "3×3 pieces"
        for m=3.
        """
        return self.n_rows + 1

    def __str__(self) -> str:
        return (
            f"Params(n_rows={self.n_rows}): alphabet={self.alphabet}, "
            f"vertices={self.n_vertices}, edges={self.n_edges}, "
            f"period={self.period}, master={self.master}×{self.master}"
        )


#: The published PuzzleBoard construction.
REAL = Params(n_rows=3)

#: Toy instance used by the test suite. ``m=2`` gives a 10×10 master with 100
#: positions, so the brute-force reference can enumerate everything instantly
#: while running the identical code paths.
TOY = Params(n_rows=2)
