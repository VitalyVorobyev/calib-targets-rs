"""Surveying the space of valid boards.

The point of this module is a reduction. Evaluating a *pair* of rings is
milliseconds, and the pair space is the square of a space with ~10⁹⁶ elements,
so a pair survey learns almost nothing per unit of work. But the quantity that
decides whether a board is perfect turns out to be a property of a *single*
ring, which collapses a product search into two independent linear ones.

The reduction, derived in :mod:`pbrings.evaluate` and verified by the tests:
the number of hypotheses a fragment admits is a sum of rank-one outer products,
one per group element,

    hypotheses(u, v) = Σ_g row_g[u] · col_g[v]

with the identity contributing exactly 1 everywhere. So a position is aliased
iff some non-identity ``g`` has ``row_g[u] > 0`` **and** ``col_g[v] > 0``, and
the aliased set is a union of combinatorial rectangles. A board is perfect iff,
for every non-identity ``g``, one of the two factors vanishes identically.

For the half turn — the only transform that does not exchange the two code maps
— those factors are per-ring. That single integer,
:func:`half_turn_support`, is this study's main statistic: a ring with support
zero at span ``s`` cannot be aliased by the half turn at that span *no matter
what it is paired with*.
"""

from __future__ import annotations

import concurrent.futures as cf
from collections.abc import Callable
from dataclasses import dataclass, field

import numpy as np

from .evaluate import self_alias_support, transform_terms
from .graph import RingGraph
from .params import Params
from .ring import Ring
from .sampling import sample_ring
from .window import WindowSpec


def interior(span: int) -> WindowSpec:
    """The fragment shape our sampler can actually read, at ``span`` corners."""
    return WindowSpec(span=span, readout="interior")


def half_turn_support(ring: Ring, spec: WindowSpec, p: Params) -> int:
    """How many master offsets of ``ring`` collide with their own half turn.

    Zero means the ring single-handedly kills the half-turn term of any board
    it belongs to. The value does not depend on which of the two code-map roles
    the ring plays — the construction is symmetric under transposing the board
    — so this is one number per (ring, span), not two.
    """
    return int((self_alias_support(ring, spec, p, role="a") > 0).sum())


def cross_supports(
    ring_a: Ring, ring_b: Ring, spec: WindowSpec, p: Params
) -> dict[str, tuple[int, int]]:
    """Support sizes of every C4 term's two factors, for one pair.

    The quarter turns exchange the code maps, so unlike the half turn their
    factors mix both rings and cannot be attributed to either one.
    """
    out: dict[str, tuple[int, int]] = {}
    for t in transform_terms(ring_a, ring_b, spec, "c4", p):
        out[t.name] = (int((t.row > 0).sum()), int((t.col > 0).sum()))
    return out


def aliased_positions(ring_a: Ring, ring_b: Ring, spec: WindowSpec, p: Params) -> int:
    """Master positions that are *not* uniquely localisable under C4.

    Computed as the union of the rank-one rectangles rather than by counting
    hypotheses, which is the same number and skips the 501×501 accumulation.
    """
    mask = np.zeros((p.master, p.master), dtype=bool)
    for t in transform_terms(ring_a, ring_b, spec, "c4", p):
        if t.name == "id":
            continue
        mask |= np.outer(t.row > 0, t.col > 0)
    return int(mask.sum())


@dataclass(frozen=True)
class RingProfile:
    """Everything measurable about one ring without reference to a partner."""

    letters: tuple[int, ...]
    #: span → half-turn self-alias support.
    support: dict[int, int] = field(default_factory=dict)
    #: Dots set, out of ``n_rows · period``.
    ones: int = 0
    #: Longest run of equal dots along a map row, maximised over rows.
    max_run: int = 0

    @property
    def clean_from(self) -> int | None:
        """Smallest surveyed span at which the half-turn term vanishes."""
        clean = [s for s, v in sorted(self.support.items()) if v == 0]
        return clean[0] if clean else None


def _max_run(bits: np.ndarray) -> int:
    """Longest run of equal values along the cyclic period axis.

    Concatenating the row with itself makes every cyclic run appear at least
    once as a complete run bounded by two changes; measuring only the gaps
    *between* changes keeps the artificial seam out of the answer.
    """
    best = 0
    period = bits.shape[1]
    for row in bits:
        if bool(np.all(row == row[0])):
            return period
        doubled = np.concatenate([row, row])
        changes = np.flatnonzero(doubled[1:] != doubled[:-1]) + 1
        runs = np.diff(changes)
        if runs.size:
            best = max(best, int(runs.max()))
    return min(best, period)


def profile_ring(ring: Ring, spans: tuple[int, ...], p: Params) -> RingProfile:
    bits = ring.bits
    return RingProfile(
        letters=ring.letters,
        support={s: half_turn_support(ring, interior(s), p) for s in spans},
        ones=int(bits.sum()),
        max_run=_max_run(bits),
    )


def rank_key(prof: RingProfile, spans: tuple[int, ...]) -> tuple[int, ...]:
    """Order rings best-first: smallest support at the smallest span wins.

    An integer tuple, never a float — ties must break the same way on every
    machine and in every process, or a resumed or reordered survey reports a
    different "best".
    """
    return tuple(prof.support[s] for s in spans)


@dataclass
class SurveySummary:
    """A whole survey folded down to histograms plus a few kept specimens.

    Per-ring rows never cross a process boundary: a chunk of 200 000 rings is
    ~300 MB of letter tuples, and shipping them home breaks the pool. Workers
    reduce locally and return one of these.
    """

    n: int = 0
    spans: tuple[int, ...] = ()
    #: span → (half-turn support → how many rings had it).
    support: dict[int, dict[int, int]] = field(default_factory=dict)
    ones: dict[int, int] = field(default_factory=dict)
    max_run: dict[int, int] = field(default_factory=dict)
    #: The best ``keep`` rings seen, best first. The only rows with letters.
    best: list[RingProfile] = field(default_factory=list)
    keep: int = 32

    def add(self, prof: RingProfile) -> None:
        self.n += 1
        for span, value in prof.support.items():
            bucket = self.support.setdefault(span, {})
            bucket[value] = bucket.get(value, 0) + 1
        self.ones[prof.ones] = self.ones.get(prof.ones, 0) + 1
        self.max_run[prof.max_run] = self.max_run.get(prof.max_run, 0) + 1
        self.best.append(prof)
        if len(self.best) > 4 * self.keep:
            self._trim()

    def _trim(self) -> None:
        self.best.sort(key=lambda pr: (rank_key(pr, self.spans), pr.letters))
        del self.best[self.keep :]

    def merge(self, other: SurveySummary) -> SurveySummary:
        """Fold another summary in. Integer addition and a re-sort, so the
        result cannot depend on the order chunks happened to finish in."""
        self.n += other.n
        self.spans = self.spans or other.spans
        for span, bucket in other.support.items():
            mine = self.support.setdefault(span, {})
            for value, count in bucket.items():
                mine[value] = mine.get(value, 0) + count
        for hist, src in ((self.ones, other.ones), (self.max_run, other.max_run)):
            for value, count in src.items():
                hist[value] = hist.get(value, 0) + count
        self.best.extend(other.best)
        self._trim()
        return self

    def support_histogram(self, span: int) -> dict[int, int]:
        return dict(sorted(self.support[span].items()))


def _chunk(args: tuple[int, int, tuple[int, ...], int, int]) -> SurveySummary:
    entropy, index, spans, count, keep = args
    p = Params()
    g = RingGraph(p)
    rng = np.random.default_rng(
        np.random.SeedSequence(entropy=entropy, spawn_key=(index,))
    )
    summary = SurveySummary(spans=spans, keep=keep)
    for _ in range(count):
        summary.add(profile_ring(sample_ring(g, p, rng), spans, p))
    summary._trim()
    return summary


def survey_rings(
    n: int,
    spans: tuple[int, ...],
    p: Params,
    *,
    seed: int = 0,
    workers: int = 1,
    chunk: int = 5000,
    keep: int = 32,
    progress: Callable[[SurveySummary], None] | None = None,
) -> SurveySummary:
    """Profile ``n`` uniformly sampled valid rings, folded to a summary.

    Reproducible from ``(seed, n, chunk)`` alone: chunk ``i`` draws from
    ``SeedSequence(seed, spawn_key=(i,))``, so neither the worker count nor the
    completion order can change the result.

    Work is dispatched one wave of ``workers`` jobs at a time, so a long survey
    can report through ``progress`` instead of going silent for minutes.

    With ``workers > 1`` this spawns processes, and the start method is *spawn*
    on macOS: every worker re-imports the calling module. A caller that runs a
    survey at module scope will therefore run it again in each worker, which
    recurses until the pool collapses. Call it from inside a
    ``if __name__ == "__main__":`` block.
    """
    sizes = [chunk] * (n // chunk) + ([n % chunk] if n % chunk else [])
    jobs = [(seed, i, spans, size, keep) for i, size in enumerate(sizes)]
    total = SurveySummary(spans=spans, keep=keep)
    if workers <= 1:
        for job in jobs:
            total.merge(_chunk(job))
            if progress:
                progress(total)
        return total
    for start in range(0, len(jobs), workers):
        wave = jobs[start : start + workers]
        with cf.ProcessPoolExecutor(max_workers=len(wave)) as pool:
            for batch in pool.map(_chunk, wave):
                total.merge(batch)
        if progress:
            progress(total)
    return total


@dataclass
class PairSummary:
    """How often a uniformly random *pair* of rings makes a perfect board.

    A board is perfect at a span when every non-identity rotation's rank-one
    term vanishes. That splits into two conditions, tallied separately because
    they behave very differently: the half turn is a per-ring property, the
    quarter turn is irreducibly a property of the pair.
    """

    n: int = 0
    spans: tuple[int, ...] = ()
    #: span → count of pairs whose half-turn term vanishes.
    half_clean: dict[int, int] = field(default_factory=dict)
    #: span → count whose quarter-turn term vanishes.
    quarter_clean: dict[int, int] = field(default_factory=dict)
    #: span → count that are perfect: every master position uniquely localisable.
    perfect: dict[int, int] = field(default_factory=dict)

    def merge(self, other: PairSummary) -> PairSummary:
        self.n += other.n
        self.spans = self.spans or other.spans
        for mine, theirs in (
            (self.half_clean, other.half_clean),
            (self.quarter_clean, other.quarter_clean),
            (self.perfect, other.perfect),
        ):
            for span, count in theirs.items():
                mine[span] = mine.get(span, 0) + count
        return self


def _pair_chunk(args: tuple[int, int, tuple[int, ...], int]) -> PairSummary:
    entropy, index, spans, count = args
    p = Params()
    g = RingGraph(p)
    rng = np.random.default_rng(
        np.random.SeedSequence(entropy=entropy, spawn_key=(index,))
    )
    out = PairSummary(spans=spans)
    for _ in range(count):
        a, b = sample_ring(g, p, rng), sample_ring(g, p, rng)
        out.n += 1
        for span in spans:
            cs = cross_supports(a, b, interior(span), p)
            half = cs["rot180"][0] * cs["rot180"][1] == 0
            quarter = cs["rot90"][0] * cs["rot90"][1] == 0
            if half:
                out.half_clean[span] = out.half_clean.get(span, 0) + 1
            if quarter:
                out.quarter_clean[span] = out.quarter_clean.get(span, 0) + 1
            if half and quarter:
                out.perfect[span] = out.perfect.get(span, 0) + 1
    return out


def survey_pairs(
    n: int,
    spans: tuple[int, ...],
    p: Params,
    *,
    seed: int = 0,
    workers: int = 1,
    chunk: int = 500,
    progress: Callable[[PairSummary], None] | None = None,
) -> PairSummary:
    """Tally how many of ``n`` uniformly random ring pairs make a perfect board.

    Same spawn caveat as :func:`survey_rings` — call it from inside a
    ``if __name__ == "__main__":`` block.
    """
    sizes = [chunk] * (n // chunk) + ([n % chunk] if n % chunk else [])
    jobs = [(seed, i, spans, size) for i, size in enumerate(sizes)]
    total = PairSummary(spans=spans)
    if workers <= 1:
        for job in jobs:
            total.merge(_pair_chunk(job))
            if progress:
                progress(total)
        return total
    for start in range(0, len(jobs), workers):
        with cf.ProcessPoolExecutor(max_workers=workers) as pool:
            for batch in pool.map(_pair_chunk, jobs[start : start + workers]):
                total.merge(batch)
        if progress:
            progress(total)
    return total
