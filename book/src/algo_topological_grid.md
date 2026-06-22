# Topological grid finder

> Code: `projective_grid::topological`
> (`detect_grid_all` with `SquareAlgorithm::Topological`). In-repo
> deep-dive: `docs/topological-grid-detection.md`.

The topological grid finder is the **sole grid builder** in the
workspace. Given a cloud of oriented features (positions + two undirected
local axes each) and the two global grid directions from
[axis clustering](algo_axis_clustering.md), it recovers an integer
`(i, j)` corner lattice **without ever sampling the image again**. Every
target type — chessboard, ChArUco, PuzzleBoard, marker board — routes
through this one path.

It is the Shu / Brunton / Fiala (2010) topological grid finder, with the
paper's image-color cell test replaced by an **axis-alignment test** so
the core stays image-free and tolerant of perspective and radial
distortion.

> **Historical note.** An earlier `SeedAndGrow` builder once coexisted
> with this one behind a `GraphBuildAlgorithm` selector. It has been
> **removed**; `GraphBuildAlgorithm` is now a single-variant,
> `#[non_exhaustive]` enum (`Topological`) retained only so the config
> schema stays stable if a future alternative builder is added. There is
> no algorithm choice to make.

## Vocabulary

- **Grid edge** — a link between two corners that runs along the lattice
  (a true cell side).
- **Diagonal edge** — a link that crosses a cell corner-to-corner.
  Delaunay introduces one per cell; the pipeline identifies and removes
  it.
- **Spurious edge** — a link that is neither a cell side nor a cell
  diagonal (a triangulation artefact).
- **Quad** — four corners forming one lattice cell, bounded by four grid
  edges.
- **Axis slot** — each corner stores its two axes in fixed slots
  (`axes[0]`, `axes[1]`); axes are undirected, compared modulo π.

## Stages

The generic, image-free core runs these stages (full source under
`crates/projective-grid/src/detect/square/topological/`):

1. **Axis cache + usability prefilter.** Precompute each feature's two
   axis angles and an *informative* flag per slot (an axis is informative
   when its `sigma` is below `max_axis_sigma_rad`). A feature is usable
   when at least one slot is informative — and, if the optional cluster
   centres are supplied, when at least one informative axis lies within
   `cluster_axis_tol_rad` of one global grid direction (modulo π).
2. **Delaunay triangulation.** Triangulate only the usable features to
   get a cheap, well-conditioned candidate-neighbour graph without
   committing to a prior cell size — important because cross-cluster
   nearest-neighbour distances are unreliable on boards with markers.
3. **Edge classification (Grid / Diagonal / Spurious).** For each
   Delaunay half-edge `a → b`, compare the edge direction `atan2(b − a)`
   (modulo π) to each endpoint's informative axes. The edge is a **Grid**
   edge when *both* endpoints see it within `axis_align_tol_rad` of one of
   their own axes; otherwise it is provisionally **Spurious**. Diagonals
   are then promoted topologically: a triangle with exactly two Grid edges
   meeting at a shared vertex through **different axis slots** has its
   third edge promoted to **Diagonal**. Crucially, diagonals are *not*
   found by a fixed `axis ± π/4` rule — under a projective warp a
   projected diagonal is not the angle bisector in image space.
4. **Triangle-pair → quad merge.** A triangle with exactly one Diagonal
   edge is fused with its neighbour across that diagonal; removing the
   shared diagonal yields a quadrilateral whose four edges are all Grid
   edges — one lattice cell, ordered clockwise (image y-down) from its
   top-left vertex.
5. **Quad filtering.** Three gates: a topological **mesh-degree** gate
   (drop junction artefacts with too many incident edges), an
   **opposing-edge ratio** gate (reject extreme parallelograms), and a
   per-component **cell-size band** (drop quads with any edge outside
   `[min, max] × component-median`, computed per connected component so
   two boards at different scales coexist).
6. **Topological walk (flood-fill).** Each connected quad-mesh component
   is labelled independently: a seed quad gets `(0,0),(1,0),(1,1),(0,1)`
   clockwise, and labels propagate across shared edges. A component is
   dropped if two quads ever disagree on a corner's label. Each
   component's `(i, j)` bbox is rebased so its minimum is `(0, 0)`.
7. **Per-component validation + projective fit (generic).** A
   pattern-agnostic geometry gate (line collinearity, local-homography
   residual, edge-length band) plus a projective fit with a residual gate.
   **The chessboard wrapper disables this stage** (pushes its tolerances
   to `+∞`) because it owns its own mandatory geometry check downstream;
   the core is asked only for labelled components.
8. **Orchestration.** Component solutions are sorted by labelled-corner
   count (ties broken by smallest source index, for determinism). Every
   unplaced feature is collected into a global rejected/unlabelled set.
   `detect_grid` returns the largest component; `detect_grid_all` returns
   all of them.

## Hex lattices

The same algorithm serves a hexagonal point lattice: on a hex lattice the
Delaunay triangles *are* the unit cells, so the diagonal/quad-merge stage
is bypassed and the axial `(q, r)` walk runs directly. The projective-fit
back-half is shared with the square path.

## Why axis alignment, not pixel colour

The paper decides what counts as a lattice edge by sampling the image
between two corners and checking the light/dark cell pattern. Replacing
that with an axis-alignment test makes the core **image-free** and
**distortion-tolerant**: a grid edge is exactly the link both endpoints
agree runs along one of their own local axes, and a cell diagonal is
recognised by the local "two grid edges through different slots" rule
rather than by any global angle. The classifier only checks that an edge
aligns with *some* endpoint axis, not the parity-correct one — the
chessboard wrapper adds parity discipline in
[recovery & validation](algo_recovery_validation.md).

## Known limits

- **Three-corner cells are not recovered as quads.** The merge needs a
  complete cell (two triangles sharing a diagonal); one missing corner
  per cell starves the surrounding flood-fill. The downstream booster
  recovery fills single interior holes from local geometry.
- **Delaunay is not projective-invariant.** Severe perspective combined
  with radial distortion can make a Delaunay triangle span more than one
  physical cell, leaving cells the diagonal-inference rule cannot resolve.
- **Axis quality is load-bearing.** Every classification decision rests
  on per-corner axis estimates; low-resolution or noisy inputs can fail
  before the topology has enough reliable evidence.
- **Marker-internal corners can poison the per-cell axis test.** Because
  the classifier checks alignment with *some* endpoint axis, a corner
  detected inside a marker bit whose axes happen to match the grid
  directions can be admitted. The marker-bearing targets defend against
  this with a strength floor that cuts marker-internal saddles *before*
  the grid grows — see the [ChArUco pipeline](pipeline_charuco.md).

## Cross-references

- `docs/topological-grid-detection.md` — the generic core in full, stage
  by stage, with the clean line between `projective-grid` and the
  chessboard adapter.
- [Axis clustering](algo_axis_clustering.md) — supplies the two global
  grid directions used by the usability prefilter.
- [Recovery & validation](algo_recovery_validation.md) — the
  chessboard-specific component merge, parity alignment, recall boosters,
  and mandatory precision pass that run *after* this core.
- [The Grid Model](projective_grid.md) — the public detection surface
  (`Evidence`, `detect_grid` / `detect_grid_all`, `GridSolution`).
