# Orientation: an optional cue

`projective-grid` accepts features with three local axes (`Oriented3`,
hex-native), two (`Oriented2`), one (`Oriented1`), or none (`Positions`).
Orientation is an **optional cue** that sharpens seeding and edge
classification — it is never required. The
*universal* cue is the grid structure itself: rows are lines, columns are
lines, and local homographies are consistent. That structural cue already
drives the shared `validate` stage and needs zero orientation.

This document records **exactly where** the topological strategy and its shared
recovery engine consume per-corner orientation, and **how the orientation-free /
single-axis inputs are supported**. Every claim below is grounded in the current
code; line numbers drift, so treat the named functions as the anchors.

## How it is implemented today: synthesize up front

All three square evidence kinds are supported, and the implementation took the
**simplest** route — rather than rewriting the strategy to run without axes, the
facade **synthesizes the missing axes from neighbour geometry up front** and then
runs the existing two-axis topological strategy unchanged:

- `Evidence::Positions` → `orient::synthesize_oriented2` recovers **both** local
  grid directions per point (square); `orient::synthesize_oriented3` recovers
  **all three** hex axis families.
- `Evidence::Oriented1` → `orient::synthesize_oriented2_from_oriented1` keeps the
  supplied axis (anchored) and recovers the **second** (square only).
- `Evidence::Oriented2` → used directly (square).
- `Evidence::Oriented3` → used directly (hex). Three axis families, consumed by
  the hex topological path.

The synthesis is perspective-invariant: it folds neighbour-chord angles modulo
π (collinear `±u` neighbours are antipodal, so they collapse to one direction
*exactly* under any homography) and runs a per-corner undirected
`(cos 2θ, sin 2θ)` `k`-means seeded from a global `k`-mode prior (`k = 2` for
square via 4 nearest neighbours, `k = 3` for hex via 6 nearest neighbours). It
assumes **no** fixed inter-axis angle, so the recovered directions track the
local projected grid. A corner whose synthesized axes are wrong is rejected by
the downstream geometry gates — it becomes a *missing* corner, never a
*mislabelled* one.

> Recall: zero wrong labels holds for all three kinds, and the synthesized
> square paths reach **recall parity** with the two-axis path. The gap that a
> hard axis-voucher would leave under strong perspective is closed by the
> geometry-first attach policy used inside the recovery engine
> (`shared::positions_policy`) plus the post-convergence recovery schedule
> (`shared::recovery`); the topological strategy is a first-class
> orientation-free path. Parity is measured per-image and gated — see
> `docs/development/detection-pipeline.md`.

The up-front synthesis above is the **entry seam** (it lets the topological
strategy run unchanged on synthesized axes). On top of it, the recovery engine
realizes the "run truly without axes" design below as the shipped
`PositionsAttachPolicy`, where the synthesized axes are only a *soft* cue and
the geometry is the gate.

## The shared grid-growth engine — almost orientation-free already

The pattern-agnostic grid-growth primitives — the boundary-extension, interior-
fill, and recovery engine under `shared/` — are what the topological strategy's
synthesized-axis path runs (and what the chessboard crate composes for its own
recovery). The BFS grow core (`shared::grow`) is **fully axis-agnostic**: it
manages the labelled map, the boundary queue, KD-tree candidate search,
prediction averaging, ambiguity resolution, and the origin rebase — all from
positions alone. Every orientation-dependent decision is delegated to a
caller-supplied **`SquareAttachPolicy`** (`shared::grow::SquareAttachPolicy`):

- `accept_candidate` — the chessboard policy checks the candidate's two axes
  against the two global cluster centres (`axes_match_centers`).
- `edge_ok` — the chessboard policy enforces the **axis-slot-swap** parity
  invariant (the edge must align with opposite axis slots at its two endpoints)
  *plus* an axis-free edge-length band.

The trait's defaults are axis-free (`edge_ok` defaults to "accept"), and a
position-only attach policy already works in the crate's own tests.

### Orientation-free growth (shipped: `PositionsAttachPolicy`)

The shipped orientation-free attach policy is the crate-private
`shared::positions_policy::PositionsAttachPolicy`:

1. **Policy.** Eligibility is all corners; `required_label_at` / `label_of`
   return `None` (no parity — a dot grid has no two-colour alternation);
   `edge_ok` keeps only the local-pitch length band; `accept_candidate` accepts
   on geometry, using the *synthesized* axes only as a **soft** cue with a wide
   tolerance (`soft_axis_tol_rad`) — a noisy synthesized axis can never block a
   geometrically-coherent attach, so a wrong axis costs a *missing* corner, not
   a mislabel.
2. **Recovery.** The geometry-only recovery schedule (`shared::recovery`:
   extension → fill → revalidate → drop filters) wraps the policy and pushes
   recall to parity with the oriented path.

Validate and the shared back-half are untouched. A fully axis-free chord
fallback (geometric nearest + most-orthogonal chord) remains a possible future
option for inputs with *no* reliable synthesized axes, but is not the shipped
path.

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
orientation. That is precisely why the orientation-free topological path is
targeted at **dot grids**, not marker boards. On marker boards the **oriented**
topological path runs instead, and ChArUco additionally cuts the weak marker-bit
corners with a `min_corner_strength` floor before the grid grows (see
`crates/calib-targets-charuco/docs/PIPELINE.md`), so the marker-internal
triangles never reach the classifier.

## Axis clustering is shared math; parity semantics live in the consumer

The orientation histogram + double-angle 2-means that recovers the two global
grid-direction centres is **shared math** in this crate
(`projective_grid::cluster`, re-exported as `cluster_axes` with
`AxisClusterCenters` / `AxisAssignment`). What stays **chessboard-crate code**
is the *parity semantics* on top of those centres — mapping the
canonical/swapped axis assignment onto the two-colour `(i, j)` parity and the
slot-coherence repair (`calib-targets-chessboard/src/cluster/slot_coherence.rs`).
A dot grid has no parity, so the consumer simply skips the parity mapping and
uses the cluster centres (if any) as a soft prior — there is nothing
parity-specific in this crate to remove.

## Summary

| Stage | Reads orientation? | Orientation-free substitute |
|---|---|---|
| Topological pre-filter | yes (quality gate) | keep all |
| Topological classify | yes (load-bearing) | per-triangle longest-edge + √2 + right-angle |
| Topological merge / walk / fit | no | unchanged |
| shared grow core | no (policy only) | `PositionsAttachPolicy` (length band, soft axis cue, no parity) |
| shared recovery (extension / fill) | no | unchanged (geometry-only) |
| shared merge / validate / fit | no | unchanged |
| axis clustering (`cluster_axes`) | yes (shared math, this crate) | soft prior; skip the consumer-side parity mapping |
