"""A ring: one cyclic letter sequence, and the validity condition it must meet.

A *ring* is a cyclic sequence of ``period`` letters. Read as a binary array it
is a code map of shape ``m × period``; read as a walk it is a closed trail in
the :mod:`~pbrings.graph` ring graph. Both views are needed — the array view
builds the board, the trail view drives generation and mutation.

Validity, stated three equivalent ways:

* every cyclic ``m×m`` window of the map is distinct (the paper's sub-perfect
  condition);
* no column tuple is σ-fixed, and no two column tuples share a σ-orbit;
* the trail uses ``period`` *distinct* edges of the ring graph.
"""

from __future__ import annotations

from dataclasses import dataclass
from functools import cached_property

import numpy as np

from .graph import RingGraph
from .params import Params
from .sigma import Tuple_, is_sigma_fixed, orbit_id


@dataclass(frozen=True)
class Ring:
    """A cyclic letter sequence of length ``p.period``."""

    letters: tuple[int, ...]
    p: Params

    def __post_init__(self) -> None:
        if len(self.letters) != self.p.period:
            raise ValueError(
                f"ring must have {self.p.period} letters, got {len(self.letters)}"
            )
        if any(x < 0 or x >= self.p.alphabet for x in self.letters):
            raise ValueError("letter outside the alphabet")

    # -- windows ----------------------------------------------------------

    @cached_property
    def windows(self) -> tuple[Tuple_, ...]:
        """The ``period`` cyclic column tuples, one per starting column."""
        n, m = self.p.period, self.p.n_rows
        s = self.letters
        return tuple(tuple(s[(c + j) % n] for j in range(m)) for c in range(n))

    @cached_property
    def orbit_ids(self) -> tuple[int, ...]:
        return tuple(orbit_id(t, self.p) for t in self.windows)

    # -- validity ---------------------------------------------------------

    @cached_property
    def n_sigma_fixed(self) -> int:
        return sum(1 for t in self.windows if is_sigma_fixed(t, self.p))

    @cached_property
    def n_orbit_duplicates(self) -> int:
        return len(self.orbit_ids) - len(set(self.orbit_ids))

    @cached_property
    def defect(self) -> int:
        """Zero exactly when the ring is valid; otherwise how far off it is."""
        return self.n_sigma_fixed + self.n_orbit_duplicates

    @property
    def is_valid(self) -> bool:
        return self.defect == 0

    def omitted_edges(self, g: RingGraph) -> tuple[int, ...]:
        """Ring-graph edges this ring does *not* use. Exactly one, when valid."""
        used = {g.edge_of(t) for t in self.windows}
        return tuple(e for e in range(g.n_edges) if e not in used)

    # -- array view -------------------------------------------------------

    @cached_property
    def bits(self) -> np.ndarray:
        """The code map as an ``m × period`` uint8 array. ``bits[r][c]`` is bit
        ``r`` of letter ``c``, matching the crate's ``letters_to_bit_rows``."""
        m, n = self.p.n_rows, self.p.period
        arr = np.empty((m, n), dtype=np.uint8)
        for r in range(m):
            for c, letter in enumerate(self.letters):
                arr[r, c] = (letter >> r) & 1
        return arr

    @classmethod
    def from_bits(cls, arr: np.ndarray, p: Params) -> "Ring":
        """Inverse of :attr:`bits`, for an ``m × period`` array."""
        if arr.shape != (p.n_rows, p.period):
            raise ValueError(f"expected shape {(p.n_rows, p.period)}, got {arr.shape}")
        letters = tuple(
            int(sum(int(arr[r, c]) << r for r in range(p.n_rows)))
            for c in range(p.period)
        )
        return cls(letters, p)

    def packed(self) -> bytes:
        """Row-major, LSB-first packing — the crate's ``map_*.bin`` format."""
        m, n = self.p.n_rows, self.p.period
        out = bytearray((m * n + 7) // 8)
        flat = self.bits.reshape(-1)
        for idx, bit in enumerate(flat):
            if bit:
                out[idx // 8] |= 1 << (idx % 8)
        return bytes(out)

    @classmethod
    def from_packed(cls, blob: bytes, p: Params, transposed: bool = False) -> "Ring":
        """Read a crate ``map_*.bin``.

        ``transposed`` reads a ``period × m`` blob (map B's storage shape),
        where the letter of row ``r`` is built from that row's ``m`` bits.
        """
        rows, cols = (p.period, p.n_rows) if transposed else (p.n_rows, p.period)
        total = rows * cols
        if len(blob) != (total + 7) // 8:
            raise ValueError(f"expected {(total + 7) // 8} bytes, got {len(blob)}")
        flat = np.array(
            [(blob[i // 8] >> (i % 8)) & 1 for i in range(total)], dtype=np.uint8
        )
        arr = flat.reshape(rows, cols)
        return cls.from_bits(arr.T if transposed else arr, p)

    # -- symmetry ---------------------------------------------------------

    def shifted(self, k: int) -> "Ring":
        """Cyclic shift by ``k`` columns — a translation of the board."""
        n = self.p.period
        k %= n
        return Ring(self.letters[k:] + self.letters[:k], self.p)

    def sigma_shifted(self, k: int) -> "Ring":
        """Apply σ^k to every letter — a row rotation of the map."""
        out = list(self.letters)
        for _ in range(k % self.p.n_rows):
            out = [(x >> 1) | ((x & 1) << (self.p.n_rows - 1)) for x in out]
        return Ring(tuple(out), self.p)

    def reversed_(self) -> "Ring":
        return Ring(tuple(reversed(self.letters)), self.p)

    def complemented(self) -> "Ring":
        mask = self.p.letter_mask
        return Ring(tuple(x ^ mask for x in self.letters), self.p)

    def orbit_under_shifts(self) -> list["Ring"]:
        """The ``period · m`` images under cyclic and σ shifts.

        These are the transformations that provably leave every board metric
        alone — they relabel positions without changing the code. Reversal and
        complementation are *not* included: whether they preserve the metrics is
        a question the test suite answers rather than an assumption made here.
        """
        return [
            self.shifted(k).sigma_shifted(j)
            for k in range(self.p.period)
            for j in range(self.p.n_rows)
        ]

    @cached_property
    def canonical(self) -> "Ring":
        """Lexicographically smallest image under :meth:`orbit_under_shifts`."""
        return min(self.orbit_under_shifts(), key=lambda r: r.letters)

    @cached_property
    def stabiliser_size(self) -> int:
        """How many shift symmetries fix this ring — 1 for a generic ring."""
        return sum(1 for r in self.orbit_under_shifts() if r.letters == self.letters)

    def __str__(self) -> str:
        return "".join(
            format(x, "x") if self.p.alphabet <= 16 else f"{x}," for x in self.letters
        )
