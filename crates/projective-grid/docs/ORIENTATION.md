# Orientation: an optional cue

`projective-grid` accepts features with three local axes (`Oriented3`,
hex-native), two (`Oriented2`), one (`Oriented1`), or none (`Positions`).
Orientation is an **optional input cue**: missing families can be estimated
from neighbour chords before classification. This synthesis is itself an
assumption about local point-cloud structure, not equivalent evidence. The
*universal* cue is the grid structure itself: rows are lines, columns are
lines, and local homographies are consistent. That structural cue drives the
shared `validate` stage and needs zero orientation.

This document records **where** the topological strategy and its shared
recovery engine consume per-corner orientation, and **how the orientation-free /
single-axis inputs run**. Named functions are the anchors; line numbers drift.

Only `(Square, Oriented2)` has real-image evidence; every other evidence kind
is experimental (Gap 21 in
[`docs/algorithms/algorithmic_gaps.md`](../../../docs/algorithms/algorithmic_gaps.md)).

## Synthesize up front

The facade (`detect.rs`) does not run the strategy without axes. It
**synthesizes the missing axes from neighbour geometry** (`orient.rs`, public as
`expert::orientation::synthesize_*`) and then runs the same topological
strategy it runs on native axes:

- `Evidence::Positions` → `synthesize_oriented2` recovers **both** local grid
  directions per point (square); `synthesize_oriented3` recovers **all three**
  hex axis families.
- `Evidence::Oriented1` → square uses `synthesize_oriented2_from_oriented1`;
  hex uses the internal three-family counterpart. Both preserve the supplied
  angle and sigma as one anchored physical family and estimate only the
  missing family/families.
- `Evidence::Oriented2` → used directly (square).
- `Evidence::Oriented3` → used directly (hex).

The synthesis folds neighbour-chord angles modulo π (collinear `±u` neighbours
are antipodal, so they collapse to one direction) and runs a per-corner undirected
`(cos 2θ, sin 2θ)` `k`-means seeded from a global `k`-mode prior (`k = 2` for
square via 4 nearest neighbours, `k = 3` for hex via 6 nearest neighbours). It
assumes **no** fixed inter-axis angle, so the recovered directions track the
local projected grid when the selected neighbours actually belong to those
families. Nearest-neighbour selection is not projectively invariant, and
clutter or severe perspective can corrupt the modes. Downstream gates reduce
that risk but do not prove that every surviving label is correct.

On synthesized square evidence the facade then runs the geometry-only recovery
schedule (below), in which the synthesized axes are only a *soft* cue and the
geometry is the gate.

## The shared grid-growth engine

The pattern-agnostic grid-growth primitives — boundary extension, interior
fill, and the recovery schedule under `shared/` — are what the topological
synthesized-axis path runs (and the chessboard crate composes the grow / fill
primitives for its own recovery). The BFS grow core (`shared/grow/`) is
**axis-agnostic**: it manages the labelled map, the boundary queue, kd-tree
candidate search, prediction averaging, ambiguity resolution, and the origin
rebase — all from positions alone. Every orientation-dependent decision is
delegated to a caller-supplied **`SquareAttachPolicy`**
(`expert::attachment::SquareAttachPolicy`). The chessboard policy
(`calib-targets-chessboard/src/pipeline/boosters.rs`) uses it like this:

- `accept_candidate` — checks the candidate's two axes against the two global
  cluster centres (`axes_match_centers`).
- `edge_ok` — enforces the **axis-slot-swap** parity invariant (the edge must
  align with opposite axis slots at its two endpoints) *plus* an axis-free
  edge-length band.

The trait's defaults are axis-free (`edge_ok` defaults to "accept").

### Orientation-free growth: `PositionsAttachPolicy`

The orientation-free attach policy is the crate-private
`shared/positions_policy.rs::PositionsAttachPolicy`:

1. **Policy.** Eligibility is all corners; `required_label_at` / `label_of`
   return `None` (no parity — a dot grid has no two-colour alternation);
   `edge_ok` keeps only the local-pitch length band; `accept_candidate` accepts
   on geometry, using the *synthesized* axes only as a **soft** cue with a wide
   tolerance (`soft_axis_tol_rad`) — a noisy synthesized axis can never block a
   geometrically-coherent attach, so a wrong axis costs a *missing* corner, not
   a mislabel.
2. **Recovery.** The geometry-only recovery schedule
   (`shared/recovery_schedule.rs`: local-H + cardinal-BFS boundary extension →
   interior fill → revalidate → wrong-label and largest-component drop filters,
   repeated to a fixed point) wraps the policy and recovers labels missed by
   the synthesized-axis front-half. `RecoverySchedule::Auto` (the default)
   enables it for `Positions` / `Oriented1` square evidence and leaves it off
   for native `Oriented2`; hex has no recovery schedule.

Validate and the shared back-half are untouched.

## Topological — where axes are read

Topological reads axes in three places (`topological/square_detector.rs`,
`topological/classify.rs`):

1. **Pre-filter** (`build_usable_mask`) — drops corners whose **both** axes are
   uninformative. This is a *quality gate*, not a geometric necessity.
2. **Cluster-prior gate** — opt-in (`TopologicalParams::axis_cluster_centers`,
   `None` by default); not load-bearing for the standalone crate.
3. **Edge classification** (`topological/classify.rs`) — the load-bearing use,
   in place of the image-colour test of Shu/Brunton/Fiala:
   - An edge `a→b` is a **Grid** edge iff *both* endpoints see the chord within
     `axis_align_tol_rad` (default 15°) of one of their own axes.
   - A triangle's third edge is promoted to **Diagonal** when its two grid
     edges meet at a vertex using **different axis slots** — i.e. the two grid
     sides there are orthogonal.

Everything downstream of classification (cell merge, label walk, the
degree/parallelogram/edge-band filters, fit) is axis-free. Without native
orientation the classifier runs on synthesized axes; a classifier that needs no
axes at all is Gap 1 in
[`docs/algorithms/algorithmic_gaps.md`](../../../docs/algorithms/algorithmic_gaps.md).

## Axis clustering is shared math; parity semantics live in the consumer

The double-angle 2-means that recovers the two global grid-direction centres is
**shared math** in this crate (`cluster/`, public as
`expert::orientation::{cluster_axes, AxisClusterCenters, AxisAssignment}`).
What stays **chessboard-crate code** is the *parity semantics* on top of those
centres — mapping the canonical/swapped axis assignment onto the two-colour
`(i, j)` parity. A dot grid has no parity, so the consumer skips the parity
mapping and uses the cluster centres (if any) as a soft prior — there is
nothing parity-specific in this crate.

## Summary

| Stage | Reads orientation? | Without native orientation |
|---|---|---|
| Topological pre-filter | yes (quality gate) | runs on synthesized axes |
| Topological classify | yes (load-bearing) | runs on synthesized axes |
| Topological merge / walk / fit | no | unchanged |
| shared grow core | no (policy only) | `PositionsAttachPolicy` (length band, soft axis cue, no parity) |
| shared recovery (extension / fill) | no | unchanged (geometry-only) |
| shared merge / validate / fit | no | unchanged |
| axis clustering (`cluster_axes`) | yes (shared math, this crate) | soft prior; skip the consumer-side parity mapping |
