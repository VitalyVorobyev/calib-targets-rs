# Orientation: an optional cue

`projective-grid` accepts features with two local axes (`Oriented2`), one
(`Oriented1`), or none (`Positions`). Orientation is an **optional cue** that
sharpens seeding and edge classification — it is never required. The
*universal* cue is the grid structure itself: rows are lines, columns are
lines, and local homographies are consistent. That structural cue already
drives the shared `validate` stage and needs zero orientation.

This document records **exactly where** each strategy consumes per-corner
orientation today, and **how each can run orientation-free**, so the planned
dot-grid (`Positions`) path is a fill-in rather than a redesign. Every claim
below is grounded in the current code; line numbers drift, so treat the named
functions as the anchors.

## Why it matters

The next target family is the **dot grid** — a lattice of blobs with **no
per-corner orientation**. The input types already model it
(`Evidence::Positions`, `PointFeature`), but the strategies currently assume
two axes, so `(Square, Positions)` returns `UnsupportedCombination`. Making
orientation optional unblocks dot grids without a second pipeline.

## SeedAndGrow — almost orientation-free already

The BFS grow core (`seed_and_grow::grow::bfs_grow`, currently
`detect/advanced/square/grow.rs`) is **fully axis-agnostic**: it manages the
labelled map, the boundary queue, KD-tree candidate search, prediction
averaging, ambiguity resolution, and the origin rebase — all from positions
alone. Every orientation-dependent decision is delegated to a caller-supplied
**`SquareAttachPolicy`**:

- `accept_candidate` — chessboard checks the candidate's two axes against the
  two global cluster centres (`axes_match_centers`).
- `edge_ok` — chessboard enforces the **axis-slot-swap** parity invariant (the
  edge must align with opposite axis slots at its two endpoints) *plus* an
  axis-free edge-length band.

The trait's defaults are axis-free (`edge_ok` defaults to "accept"), and a
position-only attach policy already works in the crate's own tests.

The **one** place generic SeedAndGrow code reads axes is the **seed finder**'s
chord-pairing (`seed_and_grow::seed`, currently
`detect/advanced/square/seed/finder.rs`): it calls `policy.axes(a)` to split a
seed anchor's neighbours into the `+u` chord set and the `+v` chord set.

### Orientation-free SeedAndGrow

1. **Policy.** Supply an `Unoriented` `SquareAttachPolicy`: `edge_ok` keeps
   only the length band, `accept_candidate` accepts on geometry, and
   `required_label_at` / `label_of` return `None` (no parity — a dot grid has
   no two-colour alternation).
2. **Seed.** Replace the axis chord-pairing with a **geometric** one: take the
   nearest neighbour as the first chord, then the neighbour whose chord is most
   orthogonal to it as the second (the two cell sides are ~90° apart), and
   complete the parallelogram. The rest of the seed finder (edge-ratio match,
   parallelogram closure, the 2×-spacing midpoint-violation check) is already
   axis-free.

That is the whole change — a new policy plus a geometric seed fallback. Validate
and the shared back-half are untouched.

## Topological — needs a geometric edge classifier

Topological reads axes in three places, all in generic code:

1. **Pre-filter** (`build_usable_mask`) — drops corners whose **both** axes are
   uninformative. This is a *quality gate*, not a geometric necessity; with no
   axes it becomes "keep all".
2. **Cluster-prior gate** — opt-in, `None` by default; not load-bearing for the
   standalone crate.
3. **Edge classification** (`topological/classify.rs`) — the load-bearing use,
   and the part that replaced the original Shu/Brunton/Fiala image-colour test:
   - An edge `a→b` is a **Grid** edge iff *both* endpoints see the chord within
     a tolerance (~15°) of one of their own axes.
   - A triangle's third edge is promoted to **Diagonal** when its two grid
     edges meet at a vertex using **different axis slots** — i.e. the two grid
     sides there are orthogonal.

Everything downstream of classification (cell merge, label walk, the
degree/parallelogram/edge-band filters, fit) is already axis-free.

### Orientation-free Topological

The classifier's job is, per Delaunay triangle, to split the three edges into
{two grid sides, one diagonal}. On a regular grid each cell's two triangles
share the cell **diagonal**, which is the **longest** edge (≈ √2 · cell vs
1 · cell), and the two grid sides are the two shorter, near-orthogonal edges.
So a purely **geometric** classifier substitutes for the axis test:

- Per triangle, mark the longest edge `Diagonal` and the other two `Grid`,
  gated by: the two short edges' length ratio ≈ 1, the long/short ratio ≈ √2,
  and the angle between the two short edges ≈ 90° (the cell-corner right angle
  — the geometric statement of the current "different axis slots" test).
- Reuse the existing buddy-consistency (the diagonal's neighbour triangle must
  agree), quad-mesh degree (> 4 illegal), and parallelogram filters unchanged.

This needs only the `positions` already passed to the classifier. It is clean
for dot grids (the cloud *is* the lattice — no within-cell features) but **more
fragile on marker boards**: with no axis signal, small marker-internal
triangles whose geometry happens to look cell-like can no longer be rejected by
orientation. That is precisely why **ChArUco keeps SeedAndGrow** and the
orientation-free topological path is targeted at dot grids, not marker boards.

## Parity clustering lives in the consumer, not here

The orientation histogram + double-angle 2-means that assigns chessboard parity
(the two cluster centres consumed by the policy above) is **chessboard-crate
code** (`calib-targets-chessboard/src/cluster/`), not part of `projective-grid`.
A dot grid has no parity, so this stage is simply skipped — there is nothing to
remove from this crate.

## Summary

| Stage | Reads orientation? | Orientation-free substitute |
|---|---|---|
| SeedAndGrow seed finder | yes (chord-pairing) | geometric nearest + most-orthogonal chord |
| SeedAndGrow grow core | no (policy only) | `Unoriented` policy (length band, no parity) |
| SeedAndGrow validate | no | unchanged |
| Topological pre-filter | yes (quality gate) | keep all |
| Topological classify | yes (load-bearing) | per-triangle longest-edge + √2 + right-angle |
| Topological merge / walk / fit | no | unchanged |
| shared merge / validate / fit | no | unchanged |
| parity clustering | yes — but in the consumer crate | skipped (dot grids have no parity) |
