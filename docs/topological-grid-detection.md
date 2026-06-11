# Topological grid detection

The topological grid finder recovers a chessboard's `(i, j)` corner
lattice from a cloud of detected corners **without ever sampling the
image again**. It decides which corner-to-corner links are true lattice
edges purely from each corner's local orientation, builds lattice cells
by merging triangles, and floods integer coordinates across the cell
mesh. The lineage is the Shu / Brunton / Fiala grid finder, with the
paper's image-color cell test replaced by an axis-alignment test so the
core stays image-free and tolerant of perspective and radial distortion.

This document explains the pipeline step by step and draws a clean line
between the **generic, image-free** machinery in the `projective-grid`
crate and the **chessboard-specific** wrapper in
`calib-targets-chessboard`. It is the companion reference for the
`SeedAndGrow` path documented in
`crates/calib-targets-chessboard/docs/PIPELINE.md`; the two grid builders
produce the same `(i, j) -> corner` output and downstream consumers stay
agnostic to which ran.

## Vocabulary

A few terms recur throughout; define them once:

- **Corner / feature.** A detected chessboard saddle point. Each carries
  a pixel position and two **local axes** — the two orthogonal grid
  directions visible in its immediate neighbourhood, each with an
  uncertainty `sigma`.
- **Axis slot.** Each corner stores its two axes in fixed slots
  (`axes[0]`, `axes[1]`). Axes are *undirected*: an angle `θ` and `θ + π`
  describe the same axis, so all axis comparisons work modulo π.
- **Grid edge.** A link between two corners that runs along the lattice
  (a true cell side).
- **Diagonal edge.** A link that crosses a cell from one corner to the
  opposite corner. Delaunay triangulation introduces one of these per
  cell; the pipeline must identify and remove it.
- **Spurious edge.** A link that is neither a cell side nor a cell
  diagonal — typically a triangulation artefact spanning unrelated
  corners.
- **Quad.** Four corners forming one lattice cell, i.e. a 2x2 block of
  the recovered grid bounded by four Grid edges.

## Two crates, one pipeline

The pipeline is split across two crates with a deliberate dependency
direction: `projective-grid` has **no image types and no workspace
dependencies**, so it can be reused by any detector that can supply
oriented point features. `calib-targets-chessboard` adapts ChESS corners
into that generic shape on the way in and applies chessboard-specific
parity, recall, and geometry logic on the way out.

| Stage | Crate | Why it lives there |
|---|---|---|
| Corner adaptation + strength/fit prefilter + orientation clustering | `calib-targets-chessboard` | Needs ChESS corner fields and chessboard parity semantics |
| 0. Axis cache + usability prefilter | `projective-grid` | Generic: only needs per-feature axis angles + sigmas |
| 1. Delaunay triangulation | `projective-grid` | Generic candidate-neighbour graph |
| 2. Edge classification (Grid / Diagonal / Spurious) | `projective-grid` | Generic: axis-alignment test, no image |
| 3. Triangle-pair to quad merge | `projective-grid` | Generic cell assembly |
| 4. Quad filtering | `projective-grid` | Generic degeneracy / scale gates |
| 5. Topological walk (flood-fill `(i, j)`) | `projective-grid` | Generic labelling |
| 6. Per-component validation + projective fit | `projective-grid` | Generic geometry check (disabled by the chessboard wrapper — see below) |
| 7. Orchestration (sort, build rejected set) | `projective-grid` | Generic multi-component bookkeeping |
| Component merge + parity align + boosters + final geometry check | `calib-targets-chessboard` | Needs parity, `CornerAug`, and the chessboard booster stack |

The chessboard wrapper **disables** the generic validation and residual
gates (Stage 6) by pushing their tolerances to infinity, because it owns
its own mandatory geometry check downstream. The generic core is asked
solely to produce labelled `(coord -> corner)` components.

## Steps

### Chessboard input adaptation (chessboard-specific)

`calib_targets_chessboard::topological::inputs::topological_inputs`

ChESS corners are converted into `projective-grid`'s image-free input:
parallel vectors of pixel positions and `[AxisEstimate; 2]` per corner.
A corner passes the prefilter when its strength clears
`min_corner_strength` **and** its fit residual clears the fit-RMS gate
(`fit_rms <= max_fit_rms_ratio * contrast`) — the same Stage-1 gate the
`SeedAndGrow` path applies. Corners that fail the prefilter keep their
pixel position but have their axes replaced with the no-information
sentinel (`sigma = π`).

*Why:* a corner with an unreliable local lattice direction must not be
allowed to vote on which edges are grid edges, but dropping it entirely
would renumber the corner array and break trace/index stability. Keeping
it as a position with dead axes satisfies both.

Separately,
`calib_targets_chessboard::topological::recovery::clustered_augs` runs
the chessboard's orientation clustering once, up front. The resulting two
global grid-direction centers are handed to the generic core as
`TopologicalParams::axis_cluster_centers`, and the same `(augs, centers)`
pair is reused later for booster recovery so clustering is not repeated.

### Step 0 — Axis cache + usability prefilter (generic)

`projective_grid::detect::square::topological::axis::build_axis_caches`,
then `build_usable_mask` and `axes_pass_cluster_gate` in
`.../topological/mod.rs`

Each feature's two axis angles and a per-slot **informative** flag are
precomputed once. An axis is informative when its `sigma` is `None`
(no uncertainty info, trust the angle) or finite and below
`max_axis_sigma_rad` (default `0.6 rad ≈ 34°`). A feature is **usable**
when at least one slot is informative, and — if the optional
`axis_cluster_centers` gate is supplied — when at least one informative
axis lies within `cluster_axis_tol_rad` (default `16°`) of one of the two
global grid directions, measured modulo π.

*Why:* only corners with a trustworthy local lattice direction can
meaningfully classify edges. The optional cluster gate additionally
rejects corners whose orientation disagrees with the board's global grid
direction before they ever reach the triangulation.

### Step 1 — Delaunay triangulation (generic)

`projective_grid::detect::square::topological::delaunay::triangulate`,
driven by `triangulate_usable` in `.../topological/mod.rs`

Only the usable features are triangulated; the resulting triangle vertex
indices are remapped back into the global feature index space so every
downstream stage shares indices with the input. The triangulator runs in
`f64` internally for robustness on near-degenerate inputs.

*Why:* Delaunay gives a cheap, well-conditioned candidate-neighbour graph
without committing to a prior cell size — which matters because
cross-cluster nearest-neighbour distances are unreliable on boards with
markers.

### Step 2 — Edge classification: Grid / Diagonal / Spurious (generic)

`projective_grid::detect::square::topological::classify::classify_all_edges`

This is the image-free replacement for the paper's color cell test, and
the heart of the method. For each directed Delaunay half-edge from corner
`a` to corner `b`, the edge direction `θ = atan2(b - a)` is compared
(modulo π) to each endpoint's informative axes. The edge is a **Grid**
edge when *both* endpoints see it within `axis_align_tol_rad`
(default `15°`) of one of their own informative axes. Otherwise it is
provisionally **Spurious**.

Diagonals are *not* found by a fixed `axis ± π/4` angle, because under a
projective warp a projected cell diagonal is not the angle bisector in
image space. Instead, after the Grid/Spurious pass, each triangle is
inspected: if it has exactly two Grid edges and those two edges meet at a
shared vertex using **different axis slots**, the triangle's remaining
edge is promoted to **Diagonal**.

*Why:* axis alignment, not pixel color, decides what counts as a lattice
edge — and the "two Grid edges through different slots" rule is the
local, distortion-tolerant way to recognise that the third edge crosses a
cell rather than bordering it. Note the classifier only checks that an
edge aligns with *some* endpoint axis, not the parity-correct one; the
chessboard wrapper adds the parity discipline later.

### Step 3 — Triangle-pair to quad merge (generic)

`projective_grid::detect::square::topological::quads::merge_triangle_pairs`

Delaunay arbitrarily splits each lattice cell into two triangles along a
diagonal. This step reverses that: a triangle with exactly one Diagonal
edge (and two Grid edges) is fused with the neighbour triangle on the
other side of that diagonal. Removing the shared diagonal yields a
quadrilateral whose four perimeter edges are all Grid edges — one lattice
cell. Triangles with zero or more than one Diagonal edge are skipped (they
cannot be paired unambiguously). The four corners are ordered clockwise
(image y-down) starting from the geometrically top-left vertex.

*Why:* it recovers true cells from Delaunay's arbitrary triangle split,
and does so topologically (which edge is the diagonal) before any further
geometric test — consistent with the paper's topology-first principle.

### Step 4 — Quad filtering (generic)

`projective_grid::detect::square::topological::filter::filter_quads`

Three gates, in order:

1. **Mesh-degree (topological).** Each corner accumulates a degree from
   every incident quad-perimeter edge. A corner well inside a regular grid
   tops out at a bounded degree; a corner with too many incident edges is
   a junction artefact. A quad with two or more over-degree corners is
   dropped.
2. **Opposing-edge ratio (parallelogram).** A quad whose opposing edge
   lengths differ by more than `opposing_edge_ratio_max` (default `1.5`)
   is an extreme parallelogram and is rejected.
3. **Per-component cell-size band.** Connected quad-mesh components are
   formed, a per-component median edge length is computed, and quads with
   any perimeter edge outside
   `[edge_length_min_rel, edge_length_max_rel] x median`
   (defaults `0.4` and `2.5`) are dropped. The band is per-component, so a
   frame with two boards at different scales does not reject one of them.

*Why:* drop degenerate junctions, sheared quads, and quads formed across
a missing corner (too long) or across a spurious within-cell feature
(too short) — failure modes the parallelogram test alone admits when both
opposing pairs scale together.

### Step 5 — Topological walk: flood-fill `(i, j)` (generic)

`projective_grid::detect::square::topological::walk::label_components`

Each connected quad-mesh component is labelled independently. A seed quad
gets the canonical labels `(0,0), (1,0), (1,1), (0,1)` clockwise. Labels
propagate to neighbour quads across shared edges: the two shared corners
keep their labels, and the other two are derived by stepping one cell in
the outward lattice direction. A component is dropped if two quads ever
disagree on a corner's label (it is not single-valued). Finally each
component's `(i, j)` bounding box is rebased so its minimum is `(0, 0)`.

*Why:* because cell topology was already established (Step 3), the labels
are consistent by construction rather than by local geometric guessing.
Rebasing satisfies the workspace's hard "non-negative grid labels"
invariant.

### Step 6 — Per-component validation + projective fit (generic)

`build_component_solution` and `fit_and_residuals` in
`.../topological/mod.rs`, calling the shared
`projective_grid::validate::square::validate`

For each component the shared post-grow validation runs three
pattern-agnostic checks — row/column line collinearity, per-corner local
homography residual, and per-edge length band — and blacklists outliers.
A projective transform is then fitted from grid coordinates to pixels;
corners whose reprojection residual exceeds `max_residual_px` are dropped
and the transform is refit once.

*Why:* an independent geometric gate over the labelled set, catching
gross mislabels that survived the topological merge.

> **Chessboard wrapper note.** When the chessboard path drives this core,
> it sets `line_tol`, `local_h_tol`, `edge_length_band`, and
> `max_residual_px` to infinity
> (`calib_targets_chessboard::topological::detection_params_for_topological`),
> disabling this entire step. The chessboard owns its own mandatory
> geometry check downstream, so the core is asked only for labelled
> components.

### Step 7 — Orchestration (generic)

`detect_square_oriented2_topological_all` in `.../topological/mod.rs`,
reached through `projective_grid::detect_grid_all`

Component solutions are sorted by labelled-corner count descending (ties
broken by smallest source index, for determinism). Every feature that no
component admitted is collected into a global rejected/unlabelled set and
attached to the largest solution, so single-component callers see a
complete picture. `detect_grid` returns only the largest component;
`detect_grid_all` returns all of them.

*Why:* the topological path can legitimately yield several disconnected
grids (e.g. one board split by occlusion); the orchestrator preserves
them with their own coordinate frames while keeping the
single-component contract intact for callers that want just the dominant
grid.

### Chessboard recovery (chessboard-specific)

`calib_targets_chessboard::topological::recovery` and
`pipeline::geometry_check` / `pipeline::output`, driven by
`detect_all_topological` in
`calib_targets_chessboard::topological`

The generic core stops at labelled components. The chessboard wrapper
then:

1. Runs a first **local-geometry component merge**
   (`projective_grid::detect::advanced::square::component_merge::merge_components_local`)
   — local geometry only, no global homography, so it tolerates radial
   distortion that would break a global fit.
2. Re-clusters the labelled corners' axes and **parity-aligns** the
   topological labels against the chessboard parity convention
   (`(i + j) % 2`) — the chessboard-specific discipline the generic
   classifier deliberately omits.
3. Marks the component and runs the **recall boosters**
   (`calib_targets_chessboard::boosters::apply_boosters_with_directional_edge_scale`)
   — interior gap fill and line extrapolation — under the same axis /
   parity / edge gates the `SeedAndGrow` BFS uses. Boosters use the larger
   directional median as their edge scale, while the final reported
   `cell_size` stays on the conservative all-edge median.
4. Merges boosted components by shared corner identity, runs a **second**
   local component merge, then **canonicalises** each surviving component
   through the same path as `SeedAndGrow`: a mandatory geometry check
   (`run_geometry_check`), rebase to non-negative labels, axis-orientation
   canonicalisation, and sort. Detections are ordered by labelled count
   and capped at `max_components`.

*Why:* parity, recall boosting, and the final precision-protective
geometry check depend on chessboard-only types and conventions, so they
stay out of the generic crate. The geometry check can only *drop*
labelled corners — it never adds wrong labels — preserving the
chessboard precision contract (wrong `(i, j)` labels are unrecoverable;
missing corners are acceptable).

## Known limits

- **Three-corner cells are not recovered as quads.** The merge needs a
  complete cell (two triangles sharing a diagonal). One missing corner per
  cell starves the surrounding flood-fill. The seed-and-grow path can
  still predict and validate a single missing corner from local geometry.
- **Delaunay is not projective-invariant.** Severe perspective combined
  with radial distortion can make Delaunay triangles span more than one
  physical cell, leaving cells the diagonal-inference rule cannot resolve.
- **Axis quality is load-bearing.** Every classification decision rests on
  per-corner axis estimates; low-resolution or noisy inputs can fail
  before the topology has enough reliable evidence.
- **Corners detected inside marker bits poison the per-cell axis test.**
  The edge classifier only checks that an edge aligns with *some* endpoint
  axis, not the parity-correct one, so a marker-internal corner whose axes
  happen to match the global grid directions can be admitted into a quad.
  This is why ChArUco detection pins the `SeedAndGrow` builder and the
  topological builder is opt-in for chessboard/puzzleboard use.

## References

- C. Shu, A. Brunton, M. Fiala. *A topological approach to finding grids
  in calibration patterns.* Machine Vision and Applications 21(6), 2010.
  (Cited as "Shu/Brunton/Fiala 2009" in parts of the codebase; same work.)
  The original uses an image-color cell test; this implementation replaces
  it with the axis-alignment test described above so the core stays
  image-free.
- `projective-grid` crate documentation — see the "Topological grid
  finder" section of the book chapter
  [`book/src/projective_grid.md`](../book/src/projective_grid.md).
- Generic core source:
  `crates/projective-grid/src/detect/square/topological/`.
- Chessboard adapter + recovery source:
  `crates/calib-targets-chessboard/src/topological/`.
- `crates/calib-targets-chessboard/docs/PIPELINE.md` — the companion
  `SeedAndGrow` pipeline reference.
