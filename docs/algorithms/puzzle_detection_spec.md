# PuzzleBoard decode — specification

> Specification of what `calib-targets-puzzleboard` **implements**. The
> user-facing narrative lives in the book
> ([decode](../../book/src/algo_puzzleboard_decode.md),
> [code maps and registration](../../book/src/algo_puzzleboard_code_maps.md));
> this file is the internal contract: entry points, invariants, constants and
> their justifications. An appendix records a design that was considered and not
> built.

## Scope

Given a labelled chessboard grid and the image it came from, recover for every
labelled corner its **absolute** position on the 501 × 501 master PuzzleBoard,
or decline.

The contract is asymmetric and non-negotiable: **a miss is acceptable, a wrong
absolute label is not.** A missing corner costs a calibration nothing; a corner
carrying the wrong master ID silently poisons it.

## Stages and entry points

| # | Stage | Entry point |
|---|---|---|
| 0 | Chessboard grid | `ChessboardDetector::detect_all` (multi-component) |
| 1 | Edge sampling | `detector::edge_sampling::sample_edge_bit_with_candidates`, driven by `PuzzleBoardDetector::sample_all_edges` |
| 2 | Confidence filter | `decode_component`, `min_bit_confidence` |
| 3 | Window gate | `required_edges` + `observed_corner_span` |
| 4 | Origin decode | `decode::{hard, soft, fixed}` over `decode::tables` |
| 5 | Uniqueness gate | `decode::passes_uniqueness_gate` |
| 6 | Component selection | `is_better_component_decode`, `origins_conflict` |
| 7 | Registration | `wrap_master`, `master_ij_to_id`, `master_target_position` |

### 1 — Edge sampling

An observation is emitted for an interior edge only when the corners bounding
**both** adjacent squares were detected: a horizontal edge at corner `(c, r)`
requires `(c+1, r)`, `(c, r±1)`, `(c+1, r±1)`; a vertical edge at `(c, r)`
requires `(c, r+1)`, `(c±1, r)`, `(c±1, r+1)`.

**Invariant (load-bearing).** Because the fragment's outermost corners
therefore host no dots, every lookup cell an observation references is
non-negative and lies on the printed board. The fixed-board search relies on
this to restrict its hypothesis space; see stage 4.

Polarity is explicit: **bit 0 = black dot, bit 1 = white dot**. Bright/dark
references come from the two adjacent squares, so the read is local and immune
to global exposure. Confidence is
`|midpoint − ref_mid| / (0.5 · dynamic_range)`, clamped to `[0, 1]`.

### 2–3 — Gates before decoding

- `min_bit_confidence` (**0.15**) — below this a bit is *unknown*, not guessed.
- `min_window` (**7**) — required corner span on **both** axes, not just an edge
  count. A wide-but-short strip can meet a count floor while carrying too little
  code distance on its thin axis. Justification in
  [§ Constants](#constants-and-their-justification).

### 4 — Origin decode

A hypothesis is `(D4 transform, master origin)`. The predicted bit depends on
the origin only through its residues mod 3 and mod 167, giving 501 horizontal
and 501 vertical classes, so:

```text
score(mr, mc) = H[mr mod 167][mc mod 3] + V[mr mod 3][mc mod 167]
```

`decode::tables::ClassTables::build` fills both tables per transform in
`O(501 · N)`; every origin afterwards is two lookups and an add.

| path | origin search | cost |
|---|---|---|
| `Full × Hard` | crossed-CRT argmax separation → `O(501)` | `O(8 · 501 · N)` |
| `Full × Soft` | serial 501² walk | `O(8 · (501 · N + 501²))` |
| `FixedBoard × *` | scan the board's shift rectangle | `O(8 · (reachable · N + L_r · L_c))` |

**Why the hard path may separate and the soft path may not.** With an integer
key, a table entry below the maximum is at least one below it, so it provably
cannot reach the maximum sum; the argmax of the sum is therefore exactly the
pair of per-table argmaxes. `f32` rounding invalidates that step, so the soft
scorer walks the origins.

**Fixed-board restriction.** A declared board is cut from the master, so its bit
at board cell `(r, c)` is the master bit at `(origin + r, origin + c)` — the
same scoring problem restricted to a rectangle of origins. Two consequences:

1. Only the residue classes that rectangle reaches can be read, so the
   precompute is built over fewer classes. Declaring a board is therefore
   *cheaper* than not declaring one, up to the point where the rectangle spans
   the maps' 167-long period.
2. Only shifts keeping **every** observation on the board are scored. By the
   stage-1 invariant every observation does lie on the printed board, so a shift
   placing one outside it cannot describe the physical scene. Excluding those
   keeps each hypothesis at `O(1)` and keeps impossible placements out of the
   uniqueness gate, where they could only suppress a correct decode.

### 5 — Uniqueness gate

```text
accept  iff  margin > k_winner
where   margin   = best_matched − runner_up_matched
        k_winner = edges_observed − best_matched
```

Parameter-free. The runner-up is the highest matched count of any *distinct*
origin **across all eight transforms** — a fragment too small to break D4
symmetry must not be allowed to invent an orientation. Applied to both scorers:
the soft path's `alignment_min_margin` gates the score gap, which does not
enforce origin uniqueness and was measured to false-accept at every window size.

### 6 — Component selection

Components are ranked by `edges_matched`, then bit-error rate, then the scorer's
own tie-break. Two *well-supported* components disagreeing on the master origin
(`origins_conflict`, compared modulo the cyclic master) is an unrecoverable
ambiguity: the frame is refused.

### 7 — Registration

`wrap_master` reduces raw master coordinates into `[0, 501)` **before** ID and
position computation. Four of the eight D4 transforms have negative diagonal
entries and can produce negative raw coordinates; those still yield a correct
`id` (which reduces modulo 501) but a wrong `target_position` (a plain
multiplication). Wrapping first keeps the documented invariant:

```text
target_position.x == (id % 501) · cell_size
target_position.y == (id / 501) · cell_size
```

## Constants and their justification

| constant | value | basis |
|---|---|---|
| `min_window` | 7 | Clean `D4 × position` uniqueness begins at 6 × 6 (measured, `window_uniqueness_report`); a 300k-trial noise sweep puts the zero-false-accept floor at 7 × 7 at both 30 % and 40 % BER. 7 = clean threshold + one square of noise margin. |
| `min_bit_confidence` | 0.15 | Below this the dot read carries no usable evidence; dropping beats guessing because the scorers weight by confidence. |
| `max_bit_error_rate` | 0.30 | The paper allows up to 40 %; 0.30 is the shipped default, with 0.40 available in the sweep preset. |
| `sample_radius_rel` | 1/6 | Dot radius as a fraction of edge length — large enough to average sensor noise, small enough to stay inside the dot under foreshortening. |
| `SEPARATION_PRODUCT_CAP` | 1024 | Guards the CRT separation against degenerate all-tied inputs; above it the transform falls back to a direct table scan, so worst-case cost never exceeds the pre-CRT version. |

### The `min_window` measurement

Clean, noise-free windows at seven planted origins:

| window | edge bits | decoded | rejected as D4-aliased |
|---|---|---|---|
| 3 × 3 | 12 | 0/7 | 7 |
| 4 × 4 | 24 | 0/7 | 7 |
| 5 × 5 | 40 | 5/7 | 2 |
| 6 × 6 | 60 | 7/7 | 0 |
| 7 × 7 | 84 | 7/7 | 0 |

The paper's "4 × 4 is unique" holds across master *positions at a fixed
orientation* — which is what `code_maps::tests::master_4x4_windows_unique`
verifies. The decoder has no fixed orientation, and over `D4 × position` every
4 × 4 window tested had a perfect alias.

## Invariants

1. Every emitted corner carries `id ∈ [0, 501²)` and a `target_position`
   consistent with it (stage 7).
2. `FixedBoard` never returns an origin the declared board does not cover, so
   every `target_position` lies inside the printed board.
3. Any subset of a printed board decodes to the same master IDs a full view
   would produce — so fragments from different frames or cameras join on `id`.
4. A decode that cannot be shown unique is declined, never guessed.

## Harnesses

```text
# decode cost vs observation count and declared board size (no image pipeline)
cargo test --release -p calib-targets-puzzleboard --lib -- \
    decode_scaling_report --ignored --nocapture

# clean-window uniqueness threshold under D4 × position
cargo test --release -p calib-targets-puzzleboard --lib -- \
    window_uniqueness_report --ignored --nocapture

# per-stage timing on a public fixture
cargo run --release -p calib-targets-bench --bin puzzleboard_stage_timing
```

---

## Appendix — considered, not implemented

An earlier design proposed **anchor-based hypothesis generation**: decode small
reliable neighbourhoods first, use them to propose a shortlist of board
positions, then score only that shortlist against the full observed graph.

It was not built, and the reason is that the premise it optimises away turned
out not to be the cost. The cyclic structure of the code already reduces the
full hypothesis space to two 501-entry tables, after which scoring *every*
origin is cheaper than proposing a shortlist would be. An anchor stage would
also weaken the uniqueness gate, which depends on knowing the best competing
origin over the whole space — a shortlist cannot certify that.

The parts of that design that *were* worth having landed differently: soft
per-bit evidence became `SoftLogLikelihood`, and "reject ambiguity rather than
guess" became the parameter-free uniqueness gate.
