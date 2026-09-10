"""The authors' shipped code maps — the reference board every number is measured against.

The maps live in the calib-targets crate, imported verbatim from
PStelldinger/PuzzleBoard (CC0 1.0). We read them **in place** rather than
vendoring a copy: a stale duplicate that silently diverges from what the library
actually ships would quietly invalidate the compatibility claim. The sha256 of
each blob is pinned in the test suite, so a change on the crate side surfaces as
a failing test instead of a wrong report.

Storage conventions, from ``crates/calib-targets-puzzleboard/src/code_maps.rs``:

* ``map_a`` is ``m × period`` (3 × 167), carrying the **vertical**-edge dots;
  its letters are its columns.
* ``map_b`` is ``period × m`` (167 × 3), carrying the **horizontal**-edge dots;
  its letters are its rows. It is the same object as map A transposed, which is
  why both are :class:`~pbrings.ring.Ring` values here.
* Both are packed row-major, LSB-first: bit ``i`` of the blob is at
  ``row·cols + col``.
"""

from __future__ import annotations

import hashlib
import json
import re
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path

from .params import REAL, Params
from .ring import Ring

#: Location of the crate's source directory, relative to this file.
_CRATE_SRC = (
    Path(__file__).resolve().parents[4] / "crates" / "calib-targets-puzzleboard" / "src"
)

#: Location of the crate's data directory.
_CRATE_DATA = _CRATE_SRC / "data"


@dataclass(frozen=True)
class ReferenceBoard:
    """The authors' ring pair, plus provenance."""

    ring_a: Ring
    ring_b: Ring
    sha256_a: str
    sha256_b: str
    metadata: dict[str, object]
    source_dir: Path


def data_dir() -> Path:
    """The crate data directory. Raises if the layout moved."""
    if not _CRATE_DATA.is_dir():
        raise FileNotFoundError(
            f"crate data directory not found at {_CRATE_DATA}; this research "
            "project expects to live at <repo>/research/puzzleboard-rings"
        )
    return _CRATE_DATA


#: `PuzzlePolePeriod { squares: N, start_row: M }`, however rustfmt breaks it.
_PERIOD_RE = re.compile(
    r"PuzzlePolePeriod\s*\{\s*squares:\s*(\d+)\s*,\s*start_row:\s*(\d+)\s*,?\s*\}"
)


@lru_cache(maxsize=None)
def crate_pole_periods() -> tuple[tuple[int, int], ...]:
    """``SUPPORTED_PERIODS`` as the crate ships it, read out of ``periods.rs``.

    Read in place and parsed rather than transcribed, for the same reason the
    maps are: a transcribed table is a second source of truth that drifts
    silently. This is the *claim* the study checks — :mod:`pbrings.pole`
    re-derives the same pairs from the maps, and the test suite asserts the two
    agree.
    """
    source = _CRATE_SRC / "pole" / "periods.rs"
    if not source.is_file():
        raise FileNotFoundError(f"crate pole periods not found at {source}")
    text = source.read_text()
    marker = "pub const SUPPORTED_PERIODS"
    start = text.index(marker)
    end = text.index("];", start)
    found = tuple(
        (int(a), int(b)) for a, b in _PERIOD_RE.findall(text[start:end])
    )
    if not found:
        raise ValueError(f"no PuzzlePolePeriod entries parsed out of {source}")
    return found


@lru_cache(maxsize=None)
def load(p: Params = REAL) -> ReferenceBoard:
    """Load the shipped maps as rings.

    Only defined for the real construction — the authors published one board.
    """
    if p != REAL:
        raise ValueError("the reference board exists only for the real parameters")
    d = data_dir()
    blob_a = (d / "map_a.bin").read_bytes()
    blob_b = (d / "map_b.bin").read_bytes()
    meta = json.loads((d / "map_metadata.json").read_text())
    return ReferenceBoard(
        ring_a=Ring.from_packed(blob_a, p, transposed=False),
        ring_b=Ring.from_packed(blob_b, p, transposed=True),
        sha256_a=hashlib.sha256(blob_a).hexdigest(),
        sha256_b=hashlib.sha256(blob_b).hexdigest(),
        metadata=meta,
        source_dir=d,
    )
