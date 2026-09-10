# The PuzzlePole decode-window uniqueness floor

An exhaustive measurement of how much of a cylindrical PuzzlePole a decoder must
see before its position is determined. Reproduce every number with:

```bash
uv run pbr pole periods                 # the seam table and the p-slice
uv run pbr pole floor -W 501            # the Pareto frontiers
uv run pbr pole floor --grid --squares 42 --canonical -W 501 --groups c4 --max-span 12
uv run pbr pole view -W 501             # the single-view verdict
uv run pbr pole verify                  # fast path vs the brute-force oracle
```

## What was measured

A PuzzlePole is a strip of the master PuzzleBoard wrapped around a cylinder. The
**circumference** axis is the master row and the **axial** axis the master
column, because the long aperiodic code lives in map B's row index and map A's
column index. The pole's pattern comes from a *modified* code stripe — map B with
its row period cut from 167 to the circumference `p`,

```
stripe[r][k] = map_b[(s + r) % 167][k]        r ∈ [0, p)
```

tiled cyclically, with map A unmodified (its row period 3 divides `p`). Every
readout below is built through that stripe, so the study and the crate cannot
drift.

**Placements.** `(t, x)`. The circumference is **cyclic** — all `p` origins `t`
are placements, wrapping ones included — and the axial axis is **clamped**: a
strip `W` corner columns wide admits only `W - span_x + 1` origins `x`. The
planar harness enumerates 501² positions because both of its axes wrap; here
only one does, and the denominators below reflect that.

**Windows.** Rectangular `(span_y, span_x)` in **corners**, interior readout: a
dot is read against the two squares flanking it, so a fragment's outermost ring
cannot be sampled at all. `span_y` is the circumference extent, `span_x` the
axial one. The two axes have different periods and a pole fragment is naturally
wide-and-short, so the floor is a 2-D Pareto frontier, not a scalar.

**Pass criterion.** A placement `q` is uniquely localisable iff exactly one
`(q', g)` in `placements × group` reproduces `readout(q, identity)` — namely
`(q, identity)`. A quarter turn of a `span_y × span_x` fragment is a
`span_x × span_y` one, so each group element is matched against the placements of
its own destination shape; when that shape does not fit on the pole, the element
contributes nothing. The floor is the Pareto-minimal set of `(span_y, span_x)`
with an ambiguous count of **exactly 0**.

**Units.** Everything here is in **corners**. Three unit systems are in play and
the number 40 appears in two of them meaning different things: the paper counts
pieces with all bounding edges (`2w(w+1)` edges), this harness counts pieces
interior-only (`2w(w-1)`), and the Rust `min_window` counts corners per side.
`interior(w) = total(w-1)`, so an interior readout at `w` pieces sees the paper's
edge count at `w-1`.

**Not modelled:** the checkerboard colour. A dot readout carries no parity bit,
matching the planar harness. Withholding information can only raise a floor, so
every number here is an upper bound on what a colour-aware decoder needs.

**Why the fast planar evaluator was not used.** `pbrings.evaluate` is fast
because `(i, j) ↦ (u, v)` is a *bijection* of `Z_501²`: the row index contributes
its mod-3 residue to one coordinate and its mod-167 residue to the other, and
`gcd(3, 167) = 1` makes those independent. On a pole the circumference
coordinate `t` enters the vertical dots as `t mod 3` and the horizontal dots as
`t mod p`, and since `3 | p` the first is a *function* of the second. The
positions are no longer a product set, the alias indicator does not factorise,
and the rank-one outer-product sum is invalid — the same non-coprimality that
stops the Rust decoder collapsing a pole's origin scan by CRT. `pbrings.pole`
therefore takes the brute path, which is at most a few hundred thousand keys.
`pbrings.brute` carries an independent oracle for it, and `tests/test_pole.py`
asserts exact agreement (192/192 in `pbr pole verify`, plus the reported floor
points themselves at full pole length).

## The seam table, re-derived

`pbr pole periods`. Derived from the shipped maps, not transcribed. The test
suite parses `SUPPORTED_PERIODS` out of the crate's `pole/periods.rs` and
asserts two things against the derivation: that every period the crate ships is
seamless, and that the two tables list the same `(circumference, map-B phase)`
pairs. Start rows here are the map-B phases, i.e. reduced modulo 167; see the
note below on the third degree of freedom.

|   p | start rows | seam depth | stripe 3×3 windows distinct |
|----:|-----------:|-----------:|----------------------------:|
|  12 |    73, 118 |          2 |                     36 / 36 |
|  18 |          7 |          2 |                     54 / 54 |
|  24 |         75 |          2 |                     72 / 72 |
|  30 | 9, 49, 114 |          2 |                     90 / 90 |
|  36 |    41, 160 |          2 |                   108 / 108 |
|  42 |    76, 123 |          2 |                   126 / 126 |
|  48 |        115 |          2 |                   144 / 144 |

**Answer to question 2 — the p-slice preserves the code's sub-perfection.** All
`3p` cyclic 3×3 windows of the derived `p × 3` stripe are distinct, for every
supported `(p, s)`: no exceptions, so no bug in the seam table. This is provable
— a window at `t ≤ p-3` is a master window at `s+t`, and the two that straddle
the seam are master windows at `s+p-2` and `s+p-1` by the two-row repeat — and
it is verified computationally rather than assumed.

The seam is **exactly two** piece rows deep at every supported period. Two is
what a local 3×3 piece code needs, so the seam creates no new local codes; but
the third row does *not* repeat, which is why a pole cannot be decoded against
the 501×501 master and needs this measurement at all.

### Two incidental facts the measurement turned up

* **The master's row period is 1002, not 501.** Both code maps repeat after 501
  rows, but 501 is odd, so the checkerboard colour inverts. A seam predicate that
  forgets the colour would call 501 a period.
* **A start row names a pole only modulo 501.** `s` and `s + 167` share a stripe
  but index map A one row on, and no circumference rotation relates them: a
  rotation by `k` would need `k ≡ 0 (mod p)` to leave the stripe alone and
  `k ≢ 0 (mod 3)` to move map A, which `3 | p` forbids. So each table entry names
  one of *three* distinct patterns, and the crate reducing the paper's start rows
  mod 167 (242→75, 176→9, 327→160, 410→76) prints a different pattern than the
  paper's own `start y`. Measured over all three phases of every entry, the
  smallest `span_y` on the frontier is identical in every case; only one
  intermediate frontier point moves (p=42, span_x=6: 8 at phase 1, 10 at phases
  0 and 2). **The choice of phase is free.**

  A consequence worth stating as arithmetic: the seam predicate's map-A and
  colour terms depend on `p` alone, so enumerating start rows over the whole
  501-row master gives *exactly* three times as many as enumerating over 167 —
  6, 3, 3, 9, 6, 6, 3 for p = 12, 18, 24, 30, 36, 42, 48, thirty-six in all. A
  table written in the 501-row domain with any other count is incomplete.
  `pbr pole periods` compares the study's derivation with the crate's
  `SUPPORTED_PERIODS` on the map-B phase, which is the part that is
  domain-independent and the part that actually chooses the code.

## The floor

### C4 — the group production searches

Pareto-minimal `(span_y × span_x)` in corners, at `W = 501` corner columns (the
longest axially distinguishable pole, so these are safe for any pole length).
Every listed point has an ambiguous count of exactly 0; the denominator is the
placement count, `p · (W - span_x + 1)`.

|   p |   s | frontier (span_y × span_x) | ambiguous / placements at the widest point |
|----:|----:|:---------------------------|:-------------------------------------------|
|  12 |  73 | 7×5, 6×6, **5×7**          | 0 / 5940                                   |
|  12 | 118 | 7×5, 6×6, **5×7**          | 0 / 5940                                   |
|  18 |   7 | 7×5, **5×7**               | 0 / 8910                                   |
|  24 |  75 | 7×5, **5×7**               | 0 / 11880                                  |
|  30 |   9 | 7×5, **5×7**               | 0 / 14850                                  |
|  30 |  49 | 8×5, 7×6, **5×7**          | 0 / 14850                                  |
|  30 | 114 | 7×5, **5×7**               | 0 / 14850                                  |
|  36 |  41 | 8×5, **5×7**               | 0 / 17820                                  |
|  36 | 160 | 7×5, **5×7**               | 0 / 17820                                  |
|  42 |  76 | 10×5, 8×6, 7×7, **5×8**    | 0 / 20748                                  |
|  42 | 123 | 7×5, **5×7**               | 0 / 20790                                  |
|  48 | 115 | 7×5, **5×7**               | 0 / 23760                                  |

Every frontier point is tight — one corner row less always leaves a non-zero
count. The margins are why this report quotes integers: at p=42 s=76, `7×6`
leaves **3 / 20832** ambiguous placements and `6×7` leaves **3 / 20790**, both of
which round to 100.0000 % unique.

### D4 — the opt-in group, rotations plus reflections

|   p |   s | frontier (span_y × span_x)     |
|----:|----:|:-------------------------------|
|  12 |  73 | 7×5, 6×7, **5×8**              |
|  12 | 118 | 8×5, 6×7, **5×9**              |
|  18 |   7 | 9×5, 6×9, **5×10**             |
|  24 |  75 | 9×5, 8×7, 7×8, 6×9, **5×10**   |
|  30 |   9 | 7×5, 6×8, **5×10**             |
|  30 |  49 | 8×5, 7×6, 6×9, **5×10**        |
|  30 | 114 | 9×5, 7×7, 6×8, **5×9**         |
|  36 |  41 | 9×5, 8×7, 7×8, 6×9, **5×10**   |
|  36 | 160 | 8×5, 7×8, **5×10**             |
|  42 |  76 | 10×5, 8×6, 7×8, 6×9, **5×10**  |
|  42 | 123 | 9×5, 8×7, 6×8, **5×10**        |
|  48 | 115 | 9×5, 8×7, 6×8, **5×10**        |

Tightest D4 witnesses: p=24 s=75 at `7×7` leaves **1 / 11880**; p=48 s=115 at
`7×7` leaves **1 / 23760**; p=18 s=7 at `8×8` leaves **1 / 8892**.

### The smallest window that is clean on *every* supported pole

The number a crate constant would pin, as a function of the pole's axial extent
`W` in corner columns:

| W (corner cols) | C4 rectangle | C4 square | D4 rectangle | D4 square |
|----------------:|:-------------|:----------|:-------------|:----------|
|              25 | 5 × 7        | 7 × 7     | 5 × 8        | 7 × 7     |
|              51 | 5 × 8        | 7 × 7     | 5 × 9        | 9 × 9     |
|             101 | 5 × 8        | 7 × 7     | 5 × 9        | 9 × 9     |
|             201 | 5 × 8        | 7 × 7     | 5 × 10       | 9 × 9     |
|             501 | **5 × 8**    | **7 × 7** | **5 × 10**   | **9 × 9** |

`7 × 7` corners is exactly the planar interior C4 floor on the master board, so
under C4 **a pole never costs a decoder more square window than a flat board
does** — and a rectangle as short as 5 corner rows suffices if it is 8 corner
columns wide.

### Two structural facts about the axes

**`span_x = 4` never works, at any `span_y`.** The interior readout of a 4-corner
axial extent gives only two columns of map A, and a 3×2 block of map A is not
unique — sub-perfection is a 3×3 property. So the axial position is never pinned.
At p=42 s=76 the whole `span_x = 4` column of the grid sits at 19614 / 20916
ambiguous even at `span_y = 12`.

**The axial extent has a hard cap at `W = 500 + span_x`.** The axial pattern has
period `lcm(3, 167) = 501` columns, so two placements 501 apart are identical. At
p=12 s=73 with a `9×7` window: `W = 507` gives 0 / 6012, `W = 508` gives
24 / 6024, `W = 509` gives 48 / 6036. A longer pole is not decodable at any
window size.

### Worked grids

Ambiguous placements at `W = 501`, C4, interior readout. The denominator depends
on `span_x` because the axial axis is clamped. Both grids are truncated at
`span_x = 10`; every column beyond it is zero from the same `span_y` on.

**p = 12, s = 73** — frontier 7×5, 6×6, 5×7:

```
 span_y \ span_x       4       5       6       7       8       9      10
     placements     5976    5964    5952    5940    5928    5916    5904
              4     5955    3288    1618    1473    1468    1463    1458
              5     5788    1060      96       0       0       0       0
              6     5607      12       0       0       0       0       0
              7     5604       0       0       0       0       0       0
              8+    5604       0       0       0       0       0       0
```

**p = 42, s = 76** — the outlier, frontier 10×5, 8×6, 7×7, 5×8:

```
 span_y \ span_x       4       5       6       7       8       9      10
     placements    20916   20874   20832   20790   20748   20706   20664
              4    20916   17456   10118    9394    9295    9263    9231
              5    20764    6392     524      42       0       0       0
              6    19890    1199      36       3       0       0       0
              7    19722     507       3       0       0       0       0
              8    19676     310       0       0       0       0       0
              9    19645     155       0       0       0       0       0
             10    19614       0       0       0       0       0       0
```

Both grids are monotone in both axes, as they must be: a taller or wider
fragment's readout contains a smaller one's over the same placement set or a
subset. That monotonicity is what the frontier's binary search rests on, and
`test_ambiguity_is_monotone_in_both_spans` checks it rather than assuming it.

## Answer to question 1 — is the floor reachable from a single view?

A cylinder self-occludes; past roughly 120–140° of arc the surface turns away
fast enough that corner detection fails. That band is a **stated modelling
assumption**, the one input here not derived from the code maps.

The demand is best stated as an *arc*, not a corner count: `span_y` corners bound
`span_y - 1` cells, each `360/p` degrees, so a fragment of `span_y` corner rows
subtends `(span_y - 1) · 360 / p` degrees.

At `W = 501` the smallest `span_y` anywhere on the frontier is **5 for every
supported period, under both C4 and D4** — the frontiers differ only in how much
`span_x` that costs. So:

|   p | needs span_y | at span_x (C4 / D4) | arc demanded | supply at 140° | supply at 120° | verdict |
|----:|-------------:|:--------------------|-------------:|---------------:|---------------:|:--------|
|  12 |            5 | 7 / 8–9             |   **120.0°** |         5 rows |         5 rows | **feasible, zero margin** |
|  18 |            5 | 7 / 10              |        80.0° |         8 rows |         7 rows | feasible |
|  24 |            5 | 7 / 10              |        60.0° |        10 rows |         9 rows | feasible |
|  30 |            5 | 7 / 9–10            |        48.0° |        12 rows |        11 rows | feasible |
|  36 |            5 | 7 / 10              |        40.0° |        15 rows |        13 rows | feasible |
|  42 |            5 | 7–8 / 10            |        34.3° |        17 rows |        15 rows | feasible |
|  48 |            5 | 7 / 10              |        30.0° |        19 rows |        17 rows | feasible |

**Every supported period is single-view decodable, including p = 12** — which
refutes the prior expectation that it might not be. But p = 12 is the marginal
case and the qualification matters:

* it demands **exactly 120.0°** of usable arc, the conservative end of the
  assumed band, with zero margin: all five corner rows the arc supplies must be
  detected, including the two sitting on the limb;
* it only works at the *wide* corner of the frontier. The square-ish points cost
  far more arc: `7×5` needs 180° (impossible from one view) and `6×6` needs 150°
  (outside the band). A decoder that searches square windows will not decode a
  12-piece pole from one view, and one that searches `5 × 8` will.

p = 18 demands 80° and has real margin; everything from 24 up is comfortable.
The relationship is simply `1440 / p` degrees, so the arc cost falls off as the
pole gets fatter.

`span_y = 4` is not an option for a full-length pole: it decodes only short poles
(`W ≤ 101`) and only for `p ≤ 24`.

## What this suggests for the crate

Stated as measurements, not as a recommendation — the constant is an output of
this table, not a tuned value:

* a pole decoder searching **C4** is clean on every supported period with a
  `5 × 8` corner window (circumference × axial), or `7 × 7` if it insists on
  square windows;
* searching **D4** costs `5 × 10`, or `9 × 9` square;
* `span_x ≥ 5` is a hard structural requirement — 4 axial corners can never pin
  the axial position;
* a pole longer than `500 + span_x` corner columns is undecodable at any window
  size and should be refused at construction;
* nothing here warns off p = 12, but a window search restricted to square
  fragments does make it single-view-undecodable, so the shape of the search
  matters more than the period does.
