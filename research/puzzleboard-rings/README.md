# PuzzleBoard de Bruijn ring study

A standalone numerical study of the PuzzleBoard position code: reproduce the
published construction, measure what makes a board good, and explore the space
of valid ring pairs.

**This is not part of the calib-targets library.** No crate depends on it, no CI
gate runs it, and nothing it produces ships without a separate decision. It
reads the shipped code maps read-only, for comparison.

Sources: Stelldinger, Schönherr & Biermann, *PuzzleBoard: A New Camera
Calibration Pattern with Position Encoding* (arXiv:2409.20127), and Stelldinger,
*On de Bruijn Rings and Families of Almost Perfect Maps* (arXiv:2405.03309).

## Quick start

```bash
uv sync
uv run pbr graph info          # the ring graph: 24 vertices, 168 edges, 6 self-loops
uv run pbr graph count         # exact size of the search space
uv run pbr ring check --authors
uv run pbr eval reference      # the reproduction table
uv run pbr eval verify --toy   # fast evaluator vs the brute-force reference
uv run pytest -m "not slow"
```

## The representation

A **ring** is a cyclic sequence of 167 letters over Σ = {0..7}, each letter a
3-bit column of a code map. `σ: (b₀,b₁,b₂) → (b₁,b₂,b₀)` is the row rotation; it
has order 3 and fixes only the all-zero and all-one letters. A ring is *valid*
when all 167 of its column triples avoid σ-fixed points and lie in distinct
σ-orbits — equivalently, when all 501 cyclic 3×3 windows of the map are
distinct.

A **board** superposes two rings: map A (3 × 167) on the vertical edges, map B
(167 × 3) on the horizontal ones. Since `gcd(3, 167) = 1` the combined pattern
has period 501 × 501.

### The ring graph

Quotienting the letter de Bruijn graph by σ turns "all windows distinct" into
"all edges distinct":

| | count | note |
|---|---|---|
| vertices | **24** | σ-orbits of letter pairs: 4 σ-fixed + 20 free |
| edges | **168** | σ-orbits of letter triples, excluding the 8 σ-fixed ones |
| out-degrees | 2 (×4), 8 (×20) | 4·2 + 20·8 = 168 |
| self-loops | **6** | the only edges that may be omitted |

A valid ring is a **closed trail of 167 distinct edges**. A closed trail needs a
balanced subgraph and the full graph already is one, so the single omitted edge
must be a self-loop. Both of the authors' shipped maps do omit a self-loop
(edges 54 and 111) — a prediction of the model, confirmed against published
data.

A circuit does not determine a ring on its own. At a σ-fixed vertex the incoming
prefix is its own σ-image, so *every* rotation of the next edge continues the
walk and the lift branches three ways. There are 7 such branch points per
circuit, all of them closing, so each circuit lifts to 3⁷ = 2187 rings.

## Window notation — read this before comparing any two numbers

Everything is indexed by **span**: corners along one side of the visible
fragment. A span of `s` encloses `(s-1)²` pieces. Two readout models matter, and
both get described as "24 edges" at the smallest useful size while being
*different* sets of 24 edges:

- **`all`** — every edge bounding the visible pieces. The paper's model: "all 24
  edges of a 3×3 PuzzleBoard pieces" is span 4.
- **`interior`** — only edges our detector can sample. A dot is read against the
  two squares flanking it, so it needs both squares' corners, which excludes the
  fragment's outermost ring of edges.

An interior readout at span `s` sees exactly what the paper's model sees at
`s-2`. The detector therefore needs **two more corners of visible board** than
the published window sizes suggest — a property of the sampler, not of the code.

## Reproduction

`pbr eval reference`, over all 251001 master positions, exactly:

| readout | span | pieces | edges | bits | fixed | C4 | D4 |
|---|---|---|---|---|---|---|---|
| all | 4 | 3×3 | 24 | 24 | 100% | **98.6733%** | 91.9538% |
| all | 5 | 4×4 | 40 | 30 | 100% | **100%** | 99.7992% |
| interior | 5 | 4×4 | 24 | 18 | 100% | 0% | 0% |
| interior | 6 | 5×5 | 40 | 24 | 100% | 98.6733% | 91.9538% |
| interior | 7 | 6×6 | 60 | 30 | 100% | **100%** | 99.7992% |
| interior | 8 | 7×7 | 84 | 36 | 100% | 100% | 99.9785% |
| interior | 9 | 8×8 | 112 | 42 | 100% | 100% | **100%** |

The paper's *"99.33 % of all 3×3 such local patterns are unique under
orientation"* comes out as **99.3294 %** once the denominator is read the
paper's way — distinct **patterns**, not board positions. 1672 pairs of
positions share a pattern, so there are 249343 patterns of which 247671 are
unique. Counted over positions the same board gives 98.6733 %.

The paper's 4×4 claim reproduces as an exact 100 %.

## Metrics

Uniqueness is always relative to a group, because a decode hypothesis is a
(position, orientation) pair:

- **fixed** — orientation known.
- **C4** — the four rotations. A camera imaging an opaque planar board can
  produce any of them; this is the group the published figures assume.
- **D4** — rotations plus reflections. Our detector currently searches all
  eight, which is what its 7×7 minimum window pays for.

Counts are always reported as integers with their denominator. A bare percentage
cannot answer this study's central question: 99.9996 % rounds to 100.0 %.

## Why this is cheap

The code factorises. The vertical dots depend only on `u = (i mod 3, (j-1) mod
167)` and the horizontal dots only on `v = (j mod 3, (i-1) mod 167)`, and
`(i,j) ↦ (u,v)` is a bijection of `Z₅₀₁²`. Under any transform the alias
indicator separates into a function of `u` times a function of `v`, so counting
hypotheses over all 251001 positions is a sum of rank-one outer products — exact,
no sampling, a few milliseconds. `pbrings.brute` recomputes the same numbers by
materialising the board and moving every dot by its corners, using none of that,
and the test suite asserts exact agreement.

## Size of the search space

`pbr graph count`:

| | |
|---|---|
| arborescences to root 0 | 144 115 188 075 855 872 = 2⁵⁷ |
| cyclic circuits per self-loop | ≈ 1.15 × 10⁹⁰ (identical for all 6) |
| closing lifts per circuit | 2187 = 3⁷ |
| shift images | 501 |
| **valid rings** | **≈ 7.57 × 10⁹⁶** |
| candidate pairs | ≈ 5.7 × 10¹⁹³ |

Computed with the Matrix-Tree and BEST theorems in exact Python integers — the
Laplacian minor is 23 × 23 and its determinant has no float64 representation.
The whole chain is validated against literally enumerating every valid ring at
the toy size, where it gives 80 = 80.

## Layout

```
src/pbrings/
  params.py     the only module allowed to contain 3, 167, 501, 168 or 24
  sigma.py      σ and the orbit structure
  graph.py      the 24/168 quotient ring graph
  counting.py   Matrix-Tree + BEST, in exact integers
  ring.py       validity, the array view, the symmetry group
  sampling.py   Wilson → BEST → Hierholzer → lift; uniform valid rings
  window.py     span and readout model — the edge slots a fragment exposes
  transforms.py the D4 action, resolved to edge-slot permutations
  evaluate.py   the fast factorised evaluator
  brute.py      the slow independent reference
  refboard.py   the authors' shipped maps
  cli.py        pbr
```

Every module takes a `Params`, so the test suite runs the *same* code at
`Params(n_rows=2)` — a 10×10 master with 100 positions, where brute force is
instantaneous and every valid ring can be listed.

## Status

Phases 0–2 of the plan are in place: construction, validation, the exact space
count, the metric core, and the reproduction gate. Still to come: the practical
tracks on the shipped board (best print origin, non-square window admissibility,
the bit-error operating surface), the unbiased distribution over sampled ring
pairs, the search, and the write-up.
