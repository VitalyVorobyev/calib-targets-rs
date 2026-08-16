# PuzzleBoard code maps and registration

> Code: `calib-targets-puzzleboard/src/code_maps.rs` (the maps),
> `tools/import_author_maps.rs` and `tools/generate_code_maps.rs` (where they
> come from), `detector/pipeline.rs` (registration).

The [decode chapter](algo_puzzleboard_decode.md) takes the master pattern as
given. This chapter answers the two questions underneath it: **how is a pattern
with that uniqueness property constructed**, and once decoded, **how does an
origin become an absolute corner ID and an object-space coordinate**.

## The construction

### The requirement

We need a binary array whose every window is distinct — a *perfect map*, the
two-dimensional analogue of a De Bruijn sequence. The PuzzleBoard needs two of
them, one per edge orientation, and it needs them **cyclic**, so a board can be
cut from anywhere on the master.

The paper's choice is a `(3, 167; 3, 3)₂` **sub-perfect** map: shape 3 × 167,
window 3 × 3, binary. "Sub-perfect" because a true perfect map of window 3 × 3
would hold all 2⁹ = 512 windows; this one holds the 501 that fit its domain,
and asks only that they be pairwise *distinct*.

### The letter trick

Reading a 3 × 167 array by columns, each column is 3 bits — call it a **letter**
`l ∈ {0..7}` via `l = b₀ | b₁≪1 | b₂≪2`. The array becomes a length-167 string
over an 8-letter alphabet, and a 3 × 3 window becomes a 3-letter substring.

Now the cyclic requirement bites. The map has only 3 rows, so shifting the
window down by one row wraps — and on letters, a row shift is the permutation

```text
σ : (b₀, b₁, b₂) → (b₁, b₂, b₀)
```

σ has order 3, with fixed points `0` (all zero) and `7` (all one). So the
windows starting at the three row offsets of a given column position are
`t`, `σ(t)`, `σ²(t)` — the **σ-orbit** of that triple.

For all 501 windows (167 column positions × 3 row shifts) to be distinct, two
conditions suffice:

1. **No triple is a σ-fixed point** — otherwise its three row shifts coincide,
   and three windows collapse into one.
2. **No two column positions share a σ-orbit** — otherwise a window at one
   position equals a row-shifted window at the other.

The 512 possible triples split into 8 singleton orbits (the σ-fixed ones) and
**168 orbits of size 3**. We need 167 of them, one per column position. That
budget — 167 needed out of 168 available — is what makes the search easy.

### Finding a valid sequence

`tools/generate_code_maps.rs` does stochastic hill-climbing: start from a random
length-167 letter sequence; define the energy as the number of invalid column
triples (σ-fixed ones plus orbit duplicates); propose single-letter mutations
and accept those that lower it; restart on a local minimum. With one orbit of
slack it converges in milliseconds.

The companion paper (arXiv:2405.03309) gives a closed-form construction, but no
reference implementation — local search is simply the cheaper route to the same
contract.

Verification is independent of construction:
[`verify_cyclic_window_unique`] enumerates all 501 cyclic windows of a map and
asserts pairwise distinctness, and `master_4x4_windows_unique` does the
end-to-end check over all 501 × 501 master positions. Both run as ordinary unit
tests.

### Provenance of the shipped maps — and why it matters

**The maps in `src/data/` are not generated. They are imported.**

`tools/import_author_maps.rs` takes the reference implementation's arrays from
[PStelldinger/PuzzleBoard][upstream] (CC0 1.0): `map_a` is the author's `code1`
verbatim, and `map_b` is `rot90(code2[::-1, ::-1])`, which re-expresses `code2`
so its fundamental period is stored as 167 × 3. `src/data/map_metadata.json`
records the import, the derivation, and the packing.

This is a deliberate interoperability decision. A board printed from the
reference tooling decodes here, and a board printed from this crate decodes with
the reference Python decoder — because both sides agree on the same code.

A regenerated pair satisfies exactly the same uniqueness property and is, in
every mathematical sense, just as good. It is also a **different code**: boards
printed with it will not decode against the reference implementation. The
generator is a research and validation tool, not the shipping path. If you run
it, you have forked the pattern.

## Registration: from origin to absolute IDs

Decode returns a `GridAlignment` — a D4 transform plus a translation — that maps
the fragment's local corner coordinates onto the master. Turning that into
usable output takes three steps.

### 1. Apply the alignment

For each labelled corner at local `(u, v)`:

```text
(raw_i, raw_j) = alignment.apply(u, v)
```

### 2. Wrap into the master

`wrap_master` reduces both coordinates into `[0, 501)`. This is not
housekeeping — it is load-bearing. Four of the eight D4 transforms have negative
entries on the diagonal, so a corner far from the origin can map to a *negative*
raw coordinate. A negative value still yields the correct `id` (the ID
computation reduces modulo 501 anyway) but the wrong `target_position`, since
that one is a plain multiplication by the cell size. Wrapping first keeps both
consistent:

```rust,ignore
let (master_i, master_j) = wrap_master(raw_i, raw_j);
```

### 3. Emit the ID and the object-space point

```text
id              = master_j · 501 + master_i
target_position = (master_i · cell_size,  master_j · cell_size)
```

which preserves the invariant a consumer can rely on:

```text
target_position.x == (id % 501) · cell_size
target_position.y == (id / 501) · cell_size
```

`id` is what makes multi-view calibration work without correspondence search:
two cameras that see overlapping parts of the board report the *same* `id` for
the same physical corner, so their observations join on it directly.

### Origin conflicts

A view can produce several disconnected grid components, each decoding
independently. If two well-supported components disagree on the master origin —
compared modulo the cyclic master by `origins_conflict` — then at least one
labelling is wrong and the image contains no evidence of which. The detector
refuses the frame rather than emitting a plausible-looking mixture.

### Per-view origin drift is expected

The master origin reported for a view depends on which print-corner the
chessboard stage happened to call local `(0, 0)`, which depends on what the
camera framed. Two views of the same board routinely report different origins.
That is not an inconsistency: after registration both produce the *same*
absolute IDs for the same physical corners, which is the only thing downstream
consumers should compare.

## Cross-references

- [PuzzleBoard edge-code decode](algo_puzzleboard_decode.md) — how the origin is
  recovered in the first place.
- [PuzzleBoard pipeline](pipeline_puzzleboard.md) — the end-to-end detector.
- [calib-targets-puzzleboard crate](puzzleboard.md) — API and printable targets.

[upstream]: https://github.com/PStelldinger/PuzzleBoard
[`verify_cyclic_window_unique`]: https://docs.rs/calib-targets-puzzleboard/latest/calib_targets_puzzleboard/code_maps/fn.verify_cyclic_window_unique.html
