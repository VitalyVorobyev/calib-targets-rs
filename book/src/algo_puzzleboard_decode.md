# PuzzleBoard edge-code decode

> Code: `calib-targets-puzzleboard` — `detector/edge_sampling.rs` (reading the
> dots) and `detector/decode/` (recovering the position). Based on Stelldinger
> 2024, [arXiv:2409.20127](https://arxiv.org/abs/2409.20127).

A PuzzleBoard is a self-identifying chessboard: every **interior edge** carries a
dot at its midpoint, and the pattern of dots is designed so that a fragment of
the board identifies *where on the board it is*. Decode turns the dots a camera
happened to see into an absolute position on a fixed **501 × 501 master
pattern**, so even a partial view yields absolute corner IDs and object-space
coordinates — no need to see a corner of the board, no need for the whole target
to be in frame.

This chapter is about the decoder specifically. The grid that feeds it comes from
the [chessboard pipeline](pipeline_chessboard.md); the end-to-end flow is in
[PuzzleBoard pipeline](pipeline_puzzleboard.md).

## What is printed

Two cyclic binary maps are superposed on the board:

| map | shape | governs | lookup |
|---|---|---|---|
| **A** | `3 × 167` | **vertical** edges | `map_a[mr mod 3][mc mod 167]` |
| **B** | `167 × 3` | **horizontal** edges | `map_b[mr mod 167][mc mod 3]` |

A *horizontal* edge at master cell `(mr, mc)` is the edge between square
`(mr, mc)` and the square below it; a *vertical* edge is the edge between
`(mr, mc)` and the square to its right. Both maps tile cyclically, which is what
makes the 501 × 501 master storable in 126 bytes.

The dot colour is the bit, and the polarity is worth stating precisely because
it is easy to get backwards:

> **bit 0 = black dot, bit 1 = white dot.**

The dot sits on the boundary between a light and a dark square, so its visible
half-moon always falls on the square of the opposite colour, and the intensity
at the exact edge midpoint reads the dot directly. The sampler compares that
intensity against bright/dark references taken from the two adjacent squares,
which is what makes the read robust to vignetting and local exposure.

### Why 3 × 167

Each map is a *sub-perfect map*: over its cyclic domain, all 501 of its 3 × 3
windows are pairwise distinct. Superposing the two makes every 4 × 4 window of
squares — 3 × 4 horizontal bits plus 4 × 3 vertical ones, 24 bits in all —
distinct across every one of the 501 × 501 master positions.

`501 = 3 · 167` is not incidental. Because `gcd(3, 167) = 1`, a master row `mr`
is uniquely determined by the pair `(mr mod 3, mr mod 167)`, and likewise for
columns. That is the Chinese Remainder Theorem, and the decoder leans on it
twice: once to store the pattern compactly, and once to avoid searching it. See
[Code map construction](algo_puzzleboard_code_maps.md) for how the maps are
built and where the shipped ones come from.

## What the decoder is given

Edge sampling emits one observation per interior edge it could read:

```rust,ignore
struct PuzzleBoardObservedEdge {
    row: i32,            // anchor corner, in the fragment's own frame
    col: i32,
    orientation: EdgeOrientation,  // Horizontal | Vertical
    bit: u8,             // 0 = black, 1 = white
    confidence: f32,     // 0 = ambiguous, 1 = crisp
}
```

An observation is produced only when the corners bounding *both* adjacent
squares were detected. That rule matters more than it looks: it means the
fragment's outermost corners host no dots, and therefore every lookup cell an
observation refers to lies on the printed board. The [fixed-board
search](#restricting-the-search-to-a-declared-board) depends on that.

Confidence is not decoration — it weights every score below, and bits under
`min_bit_confidence` (default `0.15`) are dropped entirely rather than guessed.

## The hypothesis space

A fragment knows neither where it sits on the master nor **which way up it is**:
the camera may have seen the board rotated or mirrored. So a hypothesis is a
pair — one of the 8 D4 transforms, and a master origin:

```text
8 transforms  ×  501 rows  ×  501 columns  =  2,008,008 hypotheses
```

Scoring one means predicting the bit at every observed edge and counting
agreement. Done naively that is `8 · 501² · N` bit comparisons — for a
1200-edge fragment, 2.4 *billion*. The decoder does not do that.

## Collapsing the search

### Step 1: the score depends only on residues

Because the maps are cyclic, the predicted bit at an origin depends on that
origin only through `mr mod 167`, `mr mod 3`, `mc mod 3`, `mc mod 167`. There
are only 501 distinct horizontal classes and 501 vertical ones, and an origin's
score splits into one term from each:

```text
score(mr, mc) = H[mr mod 167][mc mod 3] + V[mr mod 3][mc mod 167]
```

So instead of scoring each origin against each observation, the decoder builds
the two tables once per transform — `O(501 · N)` — after which **every origin
costs two lookups and an add**. That is the single most important step:
`O(8 · 501² · N)` becomes `O(8 · (501 · N + 501²))`.

### Step 2: CRT removes the `501²` as well

The remaining origin walk is 2 M table reads. For the hard scorer it disappears
entirely. Since `501 = 3 · 167` with `gcd(3, 167) = 1`, the four residues are
mutually independent and each ranges over its full domain, so

```text
argmax over origins of ( H[·] + V[·] )  =  ( argmax over H ,  argmax over V )
```

and the winner is recovered from the two per-table argmaxes by CRT inversion:
`mr = (334·va + 168·ha) mod 501`. The scan drops to `O(501)`.

The separation needs an **integer** key, and that is a real constraint rather
than an implementation detail. With integers, a table entry below the maximum is
at least one below it, so it provably cannot reach the maximum sum. With `f32`,
rounding breaks that implication: two origins built from different table values
can land on the same sum. The hard scorer ranks on an integer bit-match count
and separates safely; the soft scorer ranks on an `f32` log-likelihood sum and
therefore keeps the `501²` walk, stripped down to two reads and a compare per
origin.

A pathological all-tied input (an empty or degenerate observation set) can make
the per-table argmax sets large; the hard path falls back to a direct table scan
for the affected transform, so worst-case cost never exceeds the pre-CRT
version.

## Scoring: hard and soft

Both scorers consume the same tables.

**`HardWeighted`** ranks by `(bits matched, summed confidence of matched bits)`,
lexicographically, and rejects anything whose bit-error rate exceeds
`max_bit_error_rate` (default `0.30`). Integer-keyed, so it gets the CRT
collapse.

**`SoftLogLikelihood`** — the default — scores each bit as
`log σ(±κ · confidence)`, clipped below by a per-bit floor so one
catastrophically wrong bit cannot dominate, and sums. A crisp bit contributes
strongly; a marginal one barely moves the score. This is materially better on
noisy or small fragments, and it is the same transfer function the ChArUco
board matcher uses. It additionally requires the winner to clear
`alignment_min_margin` over the runner-up.

## The uniqueness gate

Winning is not enough. A decode is accepted only if

```text
margin > k_winner
```

where `margin = best_matched − runner_up_matched` and
`k_winner = edges_observed − best_matched` is the winner's own mismatch count.
Equivalently: the winner's *net* score must strictly beat the runner-up's
matched count.

This is parameter-free — it compares two counts — and it separates two failure
modes that any single magnitude threshold conflates:

- A **clean exact** read has `k_winner = 0` and passes at any margin ≥ 1, so the
  code's exact-uniqueness design is honoured at any fragment size.
- A **noisy ambiguous** read fails: if a wrong origin matches nearly as many
  bits (small margin) *while* the winner itself mismatches many (large
  `k_winner`), the winner is not meaningfully closer to a perfect read than its
  competitor, and the decode declines.

Declining is the right answer. A missing label costs a calibration nothing; a
*wrong* absolute label is unrecoverable.

The runner-up is taken across all eight transforms, not just within the winning
one, precisely because a fragment too small to break the board's D4 symmetry
must not be allowed to invent an orientation.

## How big a fragment do you need?

The paper's headline is that a 4 × 4 fragment is unique. That is true **across
positions at a fixed orientation** — and the decoder does not have a fixed
orientation. Over `D4 × position`, measured on clean, noise-free windows at
seven planted origins:

| window | edge bits | decoded | rejected as D4-aliased |
|---|---|---|---|
| 3 × 3 | 12 | 0 / 7 | 7 |
| 4 × 4 | 24 | 0 / 7 | 7 |
| 5 × 5 | 40 | 5 / 7 | 2 |
| 6 × 6 | 60 | 7 / 7 | 0 |
| 7 × 7 | 84 | 7 / 7 | 0 |

Every clean 4 × 4 window tested had a *perfect* alias under some other
transform. Clean uniqueness begins at 6 × 6.

Noise pushes the floor up again: the code's minimum Hamming distance is 1 at
4 × 4, so a single flipped bit can turn a fragment into a perfect read of a
different location. A 300k-trial sweep over random origins and error patterns
puts the smallest window with zero false accepts at 7 × 7 (84 interior edges),
at both 30 % and 40 % bit-error rates.

Hence `min_window = 7` by default: one square above the clean-uniqueness
threshold, as the noise budget. The gate is applied to the corner *span* on both
axes, because a wide-but-short strip can meet an edge-count floor while carrying
too little code distance on its thin axis.

Reproduce the table with:

```text
cargo test --release -p calib-targets-puzzleboard --lib -- \
    window_uniqueness_report --ignored --nocapture
```

## Restricting the search to a declared board

When the caller knows which board they printed, `PuzzleBoardSearchMode::FixedBoard`
restricts the origin to that board's rectangle. Because a printed board is a
sub-rectangle *cut from* the master, its bit at board cell `(r, c)` is exactly
the master bit at `(origin + r, origin + c)` — so this is the same scoring
problem, restricted, and it reuses the same class tables.

Restricting the origins also restricts the residue classes they can reach, so
the precompute itself does less work. That is why declaring the board is
*cheaper* than not declaring it rather than merely bounded — until the board
grows past the maps' 167-long period, where there is nothing left to restrict.

The scan considers only shifts under which **every** observation lands on the
board. Because a dot is only sampled where the surrounding corners were
detected, every observation does lie on the printed board, so a shift placing
one outside it cannot describe the physical scene. Excluding those keeps each
hypothesis at two table lookups, and keeps impossible placements out of the
uniqueness gate where they could only ever suppress a correct decode.

Two guarantees follow: the decode cannot return a position the board does not
cover, and any subset of the board decodes to the same master IDs a full view
would — so fragments from different frames or different cameras stitch without
further work.

## Complexity

With `N` observed edges, `w` the observed window in squares, and `L_r × L_c` the
shift rectangle a declared board admits:

| stage | cost |
|---|---|
| Edge sampling | `O(N · r²)`, `r` = dot sample radius |
| Class precompute, per transform | `O(min(501, reachable classes) · N)` |
| Origin scan — hard, full master | `O(501)` via CRT |
| Origin scan — soft, full master | `O(501²)` |
| Origin scan — fixed board | `O(L_r · L_c)` |
| **Full master, hard** | `O(8 · 501 · N)` |
| **Full master, soft** | `O(8 · (501 · N + 501²))` |
| **Fixed board** | `O(8 · (reachable · N + L_r · L_c))` |

Measured decode-only cost (synthetic observations, no image pipeline), in
milliseconds:

| window | edges | full/hard | full/soft | fixed 25×25 | fixed 130×130 | fixed 501×501 |
|---|---|---|---|---|---|---|
| 7 × 7 | 84 | 0.33 | 2.19 | 0.08 | 0.73 | 3.94 |
| 13 × 13 | 312 | 2.09 | 3.90 | 0.20 | 1.88 | 5.23 |
| 25 × 25 | 1200 | 7.97 | 10.29 | 0.07 | 5.80 | 10.83 |

Reproduce with:

```text
cargo test --release -p calib-targets-puzzleboard --lib -- \
    decode_scaling_report --ignored --nocapture
```

## Multiple components, and the origin conflict

A view can yield several disconnected grid components. Each decodes
independently, and they are ranked by edges matched, then bit-error rate, then
the scorer's own tie-breaks.

If two *well-supported* components disagree on the master origin, that is an
unrecoverable ambiguity — some part of the labelling must be wrong, and nothing
in the image says which. The detector refuses the frame rather than picking one.

## Cross-references

- [Code map construction](algo_puzzleboard_code_maps.md) — how the two maps are
  built, why they work, and where the shipped ones come from.
- [PuzzleBoard pipeline](pipeline_puzzleboard.md) — the end-to-end detector.
- [calib-targets-puzzleboard crate](puzzleboard.md) — API, search modes,
  printable targets.
- [Topological grid finder](algo_topological_grid.md) — the grid whose interior
  edges this decoder samples.
