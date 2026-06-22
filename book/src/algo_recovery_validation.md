# Recovery & validation

> Code: `projective_grid::shared` (`merge_components_local`, the
> `fill`/`grow`/`extension` boosters, the `validate` precision pass, and
> `LabelledGrid::normalize`).

The [topological grid finder](algo_topological_grid.md) stops at labelled
`(i, j)` components. Recovery & validation is the **back half** that turns
those raw components into a precision-safe, normalized grid. It does three
jobs, in this order:

1. **Recovery boosters** — recover corners the topological walk missed.
2. **The precision pass (`drop_set`)** — prove the labelled set, dropping
   anything that slipped through.
3. **Normalization** — rebase, canonicalize, and sort the final grid.

These live in `projective-grid` so any lattice consumer can reuse them;
the chessboard wrapper sequences them and supplies the parity discipline.

## Recovery boosters (recall, can only add)

The boosters extend a component to recover missed corners. Each addition
re-runs the *same* axis / parity / edge-slot-swap invariants the
topological walk uses, so a booster can only add a corner that would have
been admitted had the walk reached it:

- **Local component merge** (`merge_components_local`) — reunite
  disconnected components in label space using **local geometry only** (no
  global homography), so it tolerates radial distortion that would break a
  global fit. Run before *and* after the fill/extension boosters.
- **Interior gap fill + line extrapolation** (`fill_grid_holes`) — fill
  interior holes with ≥ 3 labelled neighbours, and extend each labelled
  row / column one corner at a time. A per-axis **directional edge scale**
  is used because a partially-grown component can be anisotropic before
  its boundaries fill in.
- **Extension** (the `extension` submodule) — homography-based outward
  extension, global or per-candidate local-H.
- **Weak-cluster rescue** (caller-driven) — re-admit `NoCluster` corners
  within a loosened tolerance, with the full invariant stack still
  enforced.

Boosters are capped by an iteration limit to prevent unbounded growth.

## The precision pass: `drop_set`

The precision pass is **mandatory and subtractive** — it can only *drop*
or *refuse*, never add or relabel. A corner that survives it has been
*proven* to sit at a real intersection. It composes these checks:

- **Line collinearity.** For every row (`j = const`) and column
  (`i = const`) with enough members, fit a line in pixel space (a
  projective-line fit when there are enough members, to absorb mild lens
  distortion) and flag members whose perpendicular residual exceeds the
  tolerance.
- **Local-H residual.** For every labelled corner with ≥ 4 non-collinear
  labelled neighbours, fit a 4-point [local homography](algo_homography.md)
  from the grid-closest neighbours, predict the corner's pixel position,
  and flag a residual over tolerance. Tolerances can be **step-aware**:
  per-corner local step from finite differences, so foreshortened cells
  get a tighter pixel tolerance and radially-distorted cells a looser one.
- **Topological wrong-label checks.** Direct structural checks that catch
  mislabels the line/H residuals can miss: interior **skipped-corner
  edges**, **duplicate-pixel** labels, and a **frontier line-spacing
  smoothness** test. The frontier test is second-order and
  distortion-model-agnostic: under any smooth (C²) lens distortion the
  edge-length sequence along a grid line is a smooth function, so a
  *kink* in that sequence at the frontier is a false attachment, not a
  legitimately foreshortened corner — it is flagged regardless of the
  absolute edge length.
- **Largest-component filter.** Keep only the largest cardinally-connected
  component, dropping isolated leaks outside the main grid.

> **Why not a global smooth-warp residual gate.** A natural-seeming
> addition would be to fit a single low-order `(i, j) → pixel` warp over
> the whole labelled set and drop high-residual corners. This was
> investigated and **falsified**: a global low-order fit *extrapolates*
> almost exactly through a false leaf one cell past the true board edge
> (giving it a tiny residual), while it fits the interior so tightly that
> legitimately barrel-distorted periphery corners get large residuals — the
> gate is simultaneously too loose and too tight. The discriminating signal
> for that false-positive class is the *local* second-order spacing kink,
> which a global fit averages away — hence the frontier line-spacing
> smoothness check above rather than a global-warp gate.

The attribution logic decides *which* flagged corner is the outlier
(e.g. a corner flagged in ≥ 2 lines is the outlier; an isolated local-H
flag with no supporting line evidence is deferred rather than dropped),
so the pass blames the genuine intruder rather than its innocent
neighbours. After updating the drop set the caller re-runs its
seed/grow/validate loop, capped to prevent infinite cycling.

> **Why "can only subtract" is the whole contract.** Wrong `(i, j)`
> labels are unrecoverable for downstream calibration; missing corners
> are acceptable. A precision pass that could *add* a label could add a
> wrong one. By construction this pass never does — it is the last gate
> that makes a false positive impossible.

## Normalization: `LabelledGrid::normalize`

Grid-result normalization is owned by `projective_grid::LabelledGrid::normalize`
— a single source of truth that target detectors call instead of
re-implementing it at their output stage. Three steps, in order:

1. **Rebase** the coordinate bbox minimum to `(0, 0)` so every label is
   non-negative (the hard non-negative-label invariant for overlay /
   calibration consumers).
2. **Canonicalize orientation** so the first lattice axis (`u`) points
   roughly `+x` (right) and the second (`v`) roughly `+y` (down) in image
   pixels. The grid finder assigns `(u, v)` from its internal axis-slot
   convention, which has no relation to image orientation; this step
   decides the permutation / sign-flip from the averaged step vectors over
   adjacent labelled pairs. Positions are never modified — only labels are
   permuted.
3. **Sort** entries by `(v, u)` for a stable output order, and recompute
   the bbox.

Because normalize permutes labels, any `LatticeFit` computed against the
*pre*-normalization labels is invalid afterwards — normalize before
fitting, or refit.

## Cross-references

- [Topological grid finder](algo_topological_grid.md) — produces the raw
  components this stage repairs and proves.
- [Homography & lattice fit](algo_homography.md) — the projective fit used
  inside the local-H residual check and the final fit.
- [Axis clustering](algo_axis_clustering.md) — the `(features, centres)`
  pair reused by the boosters.
- [Chessboard pipeline](pipeline_chessboard.md) — how the chessboard
  detector sequences merge → parity-align → boost → precision pass →
  normalize.
