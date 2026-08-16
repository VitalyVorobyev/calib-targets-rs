"""The row-rotation σ and the orbit structure it induces on letter tuples.

Reading a code map by columns turns it into a string over an alphabet of
``2^m`` letters, one letter per binary column of height ``m``. Because the map
has only ``m`` rows, shifting a window down by one row *wraps*, and on letters
that wrap is the permutation

    σ : (b₀, b₁, …, b_{m-1}) → (b₁, …, b_{m-1}, b₀)

i.e. a right rotation of the letter's bits. σ has order ``m`` and fixes exactly
the all-zero and all-one letters. The ``m`` row shifts of a window are therefore
the σ-orbit of its letter tuple, which is why the map's uniqueness condition is
a statement about *orbits* rather than about tuples.
"""

from __future__ import annotations

from functools import lru_cache

from .params import Params

Tuple_ = tuple[int, ...]


def sigma_letter(letter: int, p: Params) -> int:
    """Apply σ to a single letter."""
    return (letter >> 1) | ((letter & 1) << (p.n_rows - 1))


def sigma_tuple(t: Tuple_, p: Params) -> Tuple_:
    """Apply σ to every letter of a tuple (the diagonal action)."""
    return tuple(sigma_letter(x, p) for x in t)


def is_sigma_fixed(t: Tuple_, p: Params) -> bool:
    """True when all ``m`` row shifts of this window coincide."""
    return sigma_tuple(t, p) == t


def pack(t: Tuple_, p: Params) -> int:
    """Pack a tuple into an integer, letter 0 in the least significant field."""
    code = 0
    for i, x in enumerate(t):
        code |= x << (i * p.n_rows)
    return code


def unpack(code: int, k: int, p: Params) -> Tuple_:
    """Inverse of :func:`pack` for a known tuple length."""
    return tuple((code >> (i * p.n_rows)) & p.letter_mask for i in range(k))


def orbit(t: Tuple_, p: Params) -> list[Tuple_]:
    """The σ-orbit of ``t``, starting at ``t`` — length ``m``, or 1 if fixed."""
    out = [t]
    cur = sigma_tuple(t, p)
    while cur != t:
        out.append(cur)
        cur = sigma_tuple(cur, p)
    return out


def orbit_id(t: Tuple_, p: Params) -> int:
    """Canonical representative of the σ-orbit of ``t``, as a packed integer.

    Two windows are the same window-up-to-row-shift exactly when their tuples
    share an ``orbit_id``.
    """
    return min(pack(x, p) for x in orbit(t, p))


def all_tuples(k: int, p: Params) -> list[Tuple_]:
    """Every ``k``-letter tuple, in packed-code order."""
    out: list[Tuple_] = []
    for code in range(p.alphabet**k):
        out.append(unpack(code, k, p))
    return out


@lru_cache(maxsize=None)
def orbit_reps(k: int, p: Params) -> tuple[Tuple_, ...]:
    """One representative per σ-orbit of ``k``-tuples, sorted by packed code."""
    seen: set[int] = set()
    reps: list[Tuple_] = []
    for t in all_tuples(k, p):
        oid = orbit_id(t, p)
        if oid not in seen:
            seen.add(oid)
            reps.append(unpack(oid, k, p))
    reps.sort(key=lambda t: pack(t, p))
    return tuple(reps)


@lru_cache(maxsize=None)
def free_orbit_reps(k: int, p: Params) -> tuple[Tuple_, ...]:
    """Orbit representatives excluding the σ-fixed ones (orbit size ``m``)."""
    return tuple(t for t in orbit_reps(k, p) if not is_sigma_fixed(t, p))
