# PuzzlePole: the cylindrical construction

> Internal design note for the PuzzlePole target — how the pattern closes
> around a cylinder, what that costs the decoder, and which coordinate
> conventions the crate commits to.
>
> Public-facing material lives in the book (`book/src/`); this file is the
> derivation, the evidence and the things a reader has to know before touching
> `crates/calib-targets-puzzleboard/src/pole/`.

Source: Juri Zach & Peer Stelldinger, *PuzzlePoles: Cylindrical Fiducial
Markers Based on the PuzzleBoard Pattern*, arXiv:2511.19448 (2025), on top of
Stelldinger, Schönherr & Biermann, *PuzzleBoard: A New Camera Calibration
Pattern with Position Encoding*, arXiv:2409.20127.

## 1. The problem

Every planar target in this workspace fails the same way: turn it far enough
and it stops being visible. A cylinder does not — but wrapping a coded pattern
around one destroys the code at the seam, because the two ends were never meant
to be adjacent. The PuzzleBoard master pattern *is* periodic, at 501 cells,
which is a fine calibration board and a useless cylinder.

PuzzlePole's answer is to find a **short** period that the code already
contains, and wrap that.

## 2. Which axis wraps, and why there is no choice

In this crate's convention (`src/code_maps.rs`):

```text
horizontal edge bit (row, col) = map_b[row % 167][col % 3]    map_b is 167 x 3
vertical   edge bit (row, col) = map_a[row % 3][col % 167]    map_a is 3 x 167
checkerboard colour (row, col) = (row + col) % 2
```

The long, aperiodic 167-cycle sits in the **row** index of `map_b` and in the
**col** index of `map_a`. A wrap needs an axis along which the pattern can be
made to repeat early, and only an axis carrying a long code can repeat early in
a way that is not already trivial. Both axes carry one — but they carry
*different* ones, and the pole must pick the axis whose long code admits an
early repeat. That is the row axis. Hence:

| pole direction | master axis | crate name |
|---|---|---|
| **circumference** | row | `master_row`, `j`, `Coord::v` |
| **axial** (along the cylinder) | col | `master_col`, `i`, `Coord::u` |

This is the single most important convention in the module, and it is the
opposite of what "rows go down the page" intuition suggests. It agrees with the
paper, whose generator parameter `start y` indexes exactly this axis.

## 3. The seam condition

A circumference of `p` pieces starting at master row `s` is seamless when the
whole row of puzzle pieces recurs `p` rows later — for **two consecutive
rows**, across the full 501-column master width:

```text
for r in {s, s+1}, for every col c in 0..501:
    horizontal_edge_bit(r, c) == horizontal_edge_bit(r + p, c)
    vertical_edge_bit(r, c)   == vertical_edge_bit(r + p, c)
    (r + c) % 2               == (r + p + c) % 2
```

`pole::periods::is_seamless` is literally this predicate, evaluated against the
shipped maps. Two consequences fall out rather than being asserted:

- **`p` must be a multiple of 6.** The `% 3` comes from `map_a[row % 3]`, the
  `% 2` from the checkerboard colour. This is the paper's "stripe periodicity
  of 3 times chessboard periodicity of 2". Pinned by
  `a_seamless_period_is_always_a_multiple_of_six`, which checks that *no*
  non-multiple closes anywhere.
- **Two rows is exactly what a local 3x3 piece code needs.** That is why the
  seam introduces no code the master does not already contain — the paper's
  "only the internal local 3-by-3-patches of the sub-pattern are repeated
  twice, but no additional patch has been generated".

## 4. Verifying the paper's table

The issue asked for the paper's Table 1 to be verified against the shipped
author maps rather than transcribed. It was. `is_seamless` re-derives it, and
`pole::periods::tests` pins the result:

| period | paper `start y` | closes? | every seamless start row over the master |
|---|---|---|---|
| 12 | 73 | yes | 73, 118, 240, 285, 407, 452 |
| 18 | 7 | yes | 7, 174, 341 |
| 24 | 242 | yes | 75, 242, 409 |
| 30 | 176 | yes | 9, 49, 114, 176, 216, 281, 343, 383, 448 |
| 36 | 325 | **no** | 41, 160, 208, 327, 375, ~~494~~ |
| 42 | 410 | yes | 76, 123, 243, 290, 410, 457 |
| 48 | 115 | yes | 115, 282, 449 |

**Six of seven reproduce exactly. Period 36 does not.** Row 325 does not repeat
at period 36 — not even one row, let alone two. Row 327 does, and
it is one step away from the paper's 325. The maps are the
authors' own, pinned byte-for-byte by
`code_maps::tests::shipped_maps_are_the_authors_code_verbatim`, and the other
six rows agree, so a different code revision is not a plausible explanation: it
reads as a transcription slip. `SUPPORTED_PERIODS` therefore ships 160 for
period 36 and `the_papers_uncorrected_period_36_start_row_is_rejected` keeps
325 rejected rather than silently repaired.

Two cross-checks against numbers the paper states elsewhere, both pinned as
tests:

- Its pole is "a 7 x 12 periodic PuzzleBoard pattern with the y corner point
  IDs from 73 to 85 ... edge length of a Puzzle piece is 3 centimeters ...
  diameter of 11.46cm and a height ... of 18cm." Rows 73..=85 is 13 corner rows
  = `p + 1` *labels* with the first and last identical (a statement about
  the wrapped pattern, not the height of the printed sheet — see §6);
  `12 * 30 / pi = 114.59 mm`;
  7 corner columns = 6 cells = 180 mm. All three reproduce.
- "23 unique PuzzlePoles of the same size as given in Figure 3 or 71 unique
  Poles of the size as given in Figure 4" — `floor(501 / 21)` and
  `floor(501 / 7)`, the disjoint axial windows of §7.

### A start row names a pattern modulo 501, not modulo 167

Seamlessness depends on `map_b` only through `start_row % 167`, which makes it
tempting to store the reduction. **That is wrong, and it was wrong here first.**
`map_a` is indexed by `start_row % 3` and the checkerboard colour by
`start_row % 2`, and 167 is neither even nor a multiple of 3 — so `s`, `s + 167`
and `s + 334` are three *different patterns* that all close.

No rotation of the circumference relates them either: undoing the `map_a` shift
needs a rotation of `k ≢ 0 (mod 3)`, while leaving the stripe alone needs
`k ≡ 0 (mod p)`, and `3 | p` makes those incompatible.

So the shipped table stores the paper's **absolute** `start y` — 242, not 75;
176, not 9; 327, not 160; 410, not 76 — and lists all three phases of every
seamless residue. A board printed from the paper's parameters and one printed
from the reduction are *not the same target*, and a decoder told the wrong one
reads the vertical dots off by a row. `reducing_a_start_row_mod_167_names_a_different_pattern`
pins exactly that.

One phase is excluded: period 36 at start row 494 is seamless, but its `p + 2`
piece strip runs off the master's last row and cannot be cut. It is left out
rather than shipped as a spec that fails at render time.

Two incidental facts fell out of the same analysis:

- **The master's row period is 1002, not 501.** Both maps repeat after 501 rows,
  but 501 is odd, so the checkerboard colour inverts. A seam predicate that
  ignores the colour would call 501 a period.
- Several periods admit **more than one** seamless residue, which the paper does
  not mention. Those are genuinely different patterns
  (`distinct_start_rows_are_distinct_patterns`) and are all shipped, because a
  pole's pattern is part of its identity.

## 5. The consequence the decoder cannot ignore

The repetition is **exactly two rows deep**. The third row does not repeat, at
any supported period — `pole_construction_repeats_exactly_two_rows` asserts
both halves of that.

So the seam guarantees no new local **3x3** codes, and nothing beyond. The
detector's window is `min_window` *corners* on a side, currently 7, which spans
six piece rows. A window straddling the seam therefore corresponds to **no
position on the 501x501 master at all**.

**A PuzzlePole cannot be decoded against the master.** Any design that tries
will decode fragments away from the seam and silently fail — or worse, mislabel
— on fragments that cross it, which is precisely the case a 360-degree marker
exists to handle.

What a pole *is* generated by is a **modified code stripe**: `map_b` with its
row period shortened from 167 to `p`,

```text
map_b_pole[r][c] = map_b[(s + r) % 167][c]      r in 0..p
```

tiled cyclically, with `map_a` untouched (its row period 3 divides `p`, so it
is already `p`-periodic). Every pole window — seam-crossing or not — is
consistent with that generator. The decode is then the *existing* algorithm
with one period changed, which is why no second decoder is needed and why the
planar path can stay byte-identical.

## 6. The object frame

`pole::geometry` commits to a right-handed frame with the cylinder axis on
**+Z**:

```text
theta = 2*pi*k / p              k = cyclic corner index, 0 at the seam
r     = p * cell / (2*pi)
x = r cos(theta),  y = r sin(theta),  z = j * cell     j = axial corner index
```

- **Origin** on the axis, in the plane of the pole's first axial corner column.
- **+Z** along increasing axial corner index, i.e. increasing master column.
- **theta = 0** at the seam — the corner row the strip starts at, and the row
  its duplicated end lands on.
- **theta increases with `k`**, counter-clockwise seen from +Z.

The paper puts its cylinder axis on `y`; the choice is arbitrary and what
matters is that ours is fixed, documented and tested.

### Index order, and why it is not free

A pole indexes corners `(axial, cyclic)` — the same order the planar board uses
for `(master_col, master_row)`, because the axial direction *is* the master
column axis and the circumference *is* the master row axis. Keeping that order
is what lets `PuzzlePoleCorner` drop into the generic `LabeledCorner` carrier
without the meaning of `target_position` shifting under it.

### The orientation invariant

This is the part that is easy to get backwards, and backwards means a mirrored
printed sheet — which a decoder searching rotations only can never recover
from. So it is derived, not chosen, and then asserted.

The printable renderer lays the master out the way every target here is laid
out: master **col** along page **x** (rightwards), master **row** along page
**y** (downwards). Page `y` runs *down*, so the normal pointing out of the page
at the reader is `y_page x x_page`, not `x_page x y_page` — the y-down image
convention flips the usual sense, and skipping that step is exactly how one
arrives at the wrong answer.

Wrapping sends page `x` to the cylinder axis `z_hat` and page `y` to the
circumferential tangent `theta_hat`, so the printed side ends up along

```text
theta_hat x z_hat  =  r_hat        (outward)
```

The ink faces **outward**, which is what a camera needs. With `theta`
increasing in `k` and `z` increasing in `j`, both indices run forwards and the
sheet is not mirrored. `geometry::tests::the_printed_side_faces_outward`
asserts the identity numerically at six positions around the pole, using a
*centred* difference for the tangent — a forward difference is a chord and sits
half a step angle off, which is enough to make the test look like it fails.

`geometry::tests` also pins the quarter turn, the half turn, the seam identity,
the axis order, and that every corner lies on the declared cylinder.

### Surface versus object coordinates

Two coordinates are carried per corner and they are not redundant:

- **surface** `(z, arc)` — the corner on the *unrolled* strip, in mm, in the
  planar `(col-like, row-like)` order. A printed square of side `cell` becomes
  an arc of length `cell`, so `arc = k * cell` and the strip is exactly one
  circumference long. This is what the generic
  `LabeledCorner::target_position` carries, so every existing 2-D consumer
  keeps working and no serialized shape changes.
- **object** `(x, y, z)` — the 3-D point a PnP solver needs.

### How much to print, and how to assemble it

The strip is printed `p + 2` pieces tall. Trim it through the **mid-line of the
first and last pieces** — the line through those pieces' vertical-edge dots —
leaving `p + 1` pieces of material for a `p`-piece circumference, i.e. exactly
one piece of overlap. Wrap it and lay the last piece over the first.

Both details are forced, not stylistic:

- **Trim mid-piece, not at a piece boundary.** A circumferential edge dot sits
  *on* the joint, so cutting at a boundary would halve it. Cutting mid-piece
  leaves every dot whole and the two half-pieces recombine into one.
- **Two repeating rows, not one.** The overlapping band covers master rows
  `s + p` and `s + p + 1` and has to reproduce master rows `s` and `s + 1`
  underneath it. That is exactly the seam condition, and one repeating row
  would leave the joint half a piece short. This is also why the paper's
  Table 1 asks its generator for `p + 2`.

`printed_strip_squares()` is `p + 2` and `circumference_corner_rows()` is `p`;
they are deliberately separate accessors, because conflating them is the
classic PuzzleBoard unit error. `calib-targets-print`'s
`the_overlapping_band_reproduces_the_band_it_covers` asserts the whole thing
against rendered pixels, for every supported period — the band that ends up on
top is the band underneath it, to the pixel — and a companion test confirms the
repetition does *not* extend past the overlap.

## 7. Pole identity

The axial start column is what distinguishes one pole from another. Poles cut
from **disjoint corner-column windows** of the 501-wide master share no corner,
so a decoded corner ID identifies its pole as well as its place on it —
the paper's "each corner point can only belong to one PuzzlePole".
`PuzzlePoleSpec::distinct_poles` enumerates that family.

## 8. Routing the decode

The pole's origin space is `p x axial_cells` — at most a few thousand
hypotheses, against the planar master's 501^2 = 251 001. The CRT collapse in
`decode/hard.rs` and `decode/soft.rs` exists solely to make that quarter-million
tractable, and it is built on `gcd(3, 167) = 1`. On a pole the two row moduli
are 3 and `p` with `3 | p`, so they are **not** coprime, `crt_master_row` is
arithmetic nonsense there, and the free product of the two argmax sets contains
pairs no origin realises — which would inflate `best_matched`, deflate the BER,
and can manufacture an accept. Post-filtering the argmax sets to consistent
pairs does *not* repair this: the best consistent origin may sit at a row class
that is not in the H argmax at all.

The resolution is not to fix the collapse but to avoid needing it. **A pole is
decoded through `decode/fixed.rs`**, which already enumerates an arbitrary
origin rectangle at two table lookups per origin and already supports a
restricted `ClassRange`. Direct enumeration over a few thousand origins is
cheap, needs no coprimality, and yields a strictly *stronger* runner-up than the
CRT path — `FixedScan` is an exact top-2 over every `(transform, shift)` pair,
where `assemble_global_runner_up` is an assembly of per-family maxima.

`crt_master_col` is untouched: the column axis is still `lcm(3, 167) = 501`.

Two consequences worth stating, because they are not obvious:

- **The period must be threaded as data, not as a constant.**
  `decode/tables.rs` carries `const _: () = assert!(H_ROWS == LONG && V_COLS == LONG)`
  — a compile-time statement that the two families *share* a long axis. That
  assertion is precisely what a pole breaks. Only the H family's long period
  changes (167 to `p`) and only its packed pattern table is derived; the short
  axis stays 3 in all four positions, and `map_a` is used **verbatim** provided
  the decoder keeps its internal circumference coordinate in master-row units
  taken mod `p`. Converting to a seam-relative index at the output boundary is
  one subtraction; doing it earlier would rotate `map_a` by `s mod 3` for no
  gain.
- **The symmetry group stays C4.** `projective-grid` canonicalises a labelled
  grid against the *image* axes, so all four rotations are physically reachable
  — a pole can be photographed lying on its side. The 90-degree hypotheses are
  not ambiguity but free rejection: the axis asymmetry lets `shift_range`
  eliminate them geometrically when the fragment does not fit the strip, and
  folding the axial extent mod `p` forces mismatches the planar long axis of
  167 never produces.

## 9. How much a decoder must see

Measured exhaustively in `research/puzzleboard-rings` (`pbr pole floor`), with
the full write-up at
[`report/puzzlepole-floor.md`](../../research/puzzleboard-rings/report/puzzlepole-floor.md).
The constants below are outputs of that measurement, not choices.

`min_window = 7` was measured for the *planar* master over its 251 001
positions. A pole's position space is `p × axial extent`, so the floor had to be
re-measured rather than inherited.

**The answer is a 2-D frontier, not a scalar**, because the two axes have
different periods and a pole fragment is naturally wide-and-short. Clean on
every supported pole at any length, in corners (circumference × axial):

| group | rectangle | square |
|---|---|---|
| C4 (what production searches) | **5 × 8** | 7 × 7 |
| D4 (rotations and reflections) | 5 × 10 | 9 × 9 |

Two structural facts came with it:

- **An axial extent of 4 corners never works**, at any circumference span: its
  interior readout gives two columns of `map_a`, and a 3 × 2 block of a
  sub-perfect map is not unique — sub-perfection is a 3 × 3 property.
- **A pole longer than `500 + span_x` corner columns is undecodable** at any
  window size, because the axial pattern's period is `lcm(3, 167) = 501`. The
  spec's existing bound (the strip must fit the master's columns) already keeps
  every constructible pole inside that.

### The shape of the search decides whether p = 12 works

The smallest circumference span anywhere on the frontier is **5, for every
period and both groups**. A fragment of `span_y` corner rows subtends
`(span_y − 1) · 360 / p` degrees, so the arc a decode demands is `1440 / p`:

| p | 12 | 18 | 24 | 30 | 36 | 42 | 48 |
|---|---|---|---|---|---|---|---|
| arc demanded | **120.0°** | 80.0° | 60.0° | 48.0° | 40.0° | 34.3° | 30.0° |

A cylinder shows roughly 120–140° before the limb foreshortens the pattern past
detection — a stated modelling assumption, the one input here not derived from
the maps. So **every supported period is single-view decodable**, refuting the
expectation that p = 12 might not be. But p = 12 needs *exactly* the
conservative 120°, with no margin, **and only at the wide corner of its
frontier**: its square-ish points cost 180° (`7 × 5`) and 150° (`6 × 6`), both
impossible from one view.

**A decoder that searches square windows cannot read a 12-piece pole from a
single view; one that searches `5 × 8` can.** The shape of the search matters
more than the period does, and that is a design constraint on the decode path,
not a tuning knob.

### What replaces `WindowTooThin`

The planar guard requires the corner span to reach `min_window` on *both* axes.
That is a bounding-box proxy for per-family long-axis coverage, and it fails on
a ring twice over: a fragment that wraps overstates its coverage, and the two
long axes are no longer the same length so one number cannot describe both. The
replacement is per-axis and per-period, taken from the frontier above — and it
is measured, so it is not the threshold-tightening the contract forbids.

## 10. Still open

- **Grid assembly on a curved surface** — §6 of this note localises the risk to
  `shared/validate/lines.rs`, which fits *straight* lines to grid rows and
  columns. On a cylinder the axial lines stay straight and the circumferential
  ones project to conics. Needs a measurement on rendered cylinders before any
  change.
- **The colour bit is not modelled** in the floor measurement, matching the
  planar harness: a dot readout carries no parity. Withholding information can
  only raise a floor, so every number above is an upper bound.

## 11. References

- `crates/calib-targets-puzzleboard/src/pole/` — the implementation.
- [`puzzle_detection_spec.md`](puzzle_detection_spec.md) — the planar decoder.
- [`algorithmic_gaps.md`](algorithmic_gaps.md) — open items.
- `research/puzzleboard-rings/` — the exhaustive uniqueness harness, and
  [`report/puzzlepole-floor.md`](../../research/puzzleboard-rings/report/puzzlepole-floor.md)
  for the full floor measurement.
