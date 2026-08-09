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
`calib-targets-chessboard`. It is the canonical stage map for the generic
grid finder; the chessboard wrapper that drives it (prefilter, recall
boosters, precision check) is documented in
`crates/calib-targets-chessboard/docs/PIPELINE.md`. The topological finder is
the **sole** grid builder for every target family — the earlier `SeedAndGrow`
builder was removed once topological matched or beat it on every path.

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
| 6. Generic component merge | `projective-grid` | Reunites disconnected patches using target-agnostic local geometry |
| 7a. Public validation + projective fit | `projective-grid` | Ordinary facade path: canonical labels, generic geometry checks, mandatory fit |
| 7b. Detector-builder handoff | `projective-grid::expert::square` | Pattern-specific path: merged axis-slot components, before generic validation/fit |
| 8. Public orchestration (sort, diagnostics attribution) | `projective-grid` | Generic multi-component bookkeeping |
| Parity align + boosters + final geometry check | `calib-targets-chessboard` | Needs parity, `CornerAug`, and the chessboard booster stack |

The chessboard wrapper takes branch 7b through
`expert::square::assemble_oriented2_components`. It does not run a generic fit
and does not canonicalize the axis slots before chessboard parity/recovery;
ordinary `detect_grid*` callers take branch 7a. Both branches execute the same
topology and component merge—there is no duplicated chessboard grid builder.

## Steps

### Chessboard input adaptation (chessboard-specific)

`calib-targets-chessboard/src/pipeline/inputs.rs::topological_inputs`

ChESS corners are converted into `projective-grid`'s image-free input:
parallel vectors of pixel positions and `[AxisEstimate; 2]` per corner.
A corner passes the prefilter when its strength clears
`min_corner_strength` **and** neither of its axes is more uncertain than
the alignment window the cell test will judge it against
(`max(σ₀, σ₁) <= topological.axis_align_tol_rad`, via
`pipeline::axis_admission_sigma`). Corners that fail the prefilter keep their
pixel position but have their axes replaced with the no-information
sentinel (`sigma = π`).

*Why:* a corner with an unreliable local lattice direction must not be
allowed to vote on which edges are grid edges, but dropping it entirely
would renumber the corner array and break trace/index stability. Keeping
it as a position with dead axes satisfies both.

*Why that threshold specifically:* the gate is derived from the tolerance it
protects rather than being an independent constant. Stage 3's cell test asks
"is this axis aligned to within `axis_align_tol_rad`?"; an estimate whose own
1σ spread is wider than that window cannot answer the question, so admitting
it injects noise into edge classification instead of evidence. Coupling the
two means loosening the cell test loosens admission by exactly the same
amount, and there is nothing separate to tune. Note the builder's internal
`max_axis_sigma_rad` (0.6 rad) is a distinct, much looser backstop — it is not
an admission gate. Before 0.11.0 this half of the prefilter was a
fit-residual ratio (`fit_rms <= max_fit_rms_ratio * contrast`), which
chess-corners 1.0 made inexpressible by removing both scalars.

Separately,
`calib-targets-chessboard/src/pipeline/recover.rs::clustered_augs` runs
the chessboard's orientation clustering once, up front. The resulting two
global grid-direction centers are handed to the generic core as
`TopologicalParams::axis_cluster_centers`, and the same `(augs, centers)`
pair is reused later for booster recovery so clustering is not repeated.

### Step 0 — Axis cache + usability prefilter (generic)

`projective-grid/src/topological/axis.rs::build_axis_caches`, then
`build_usable_mask` and `axes_pass_cluster_gate` in
`topological/square_detector.rs`

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

`projective-grid/src/topological/delaunay.rs::triangulate`, driven by
`triangulate_usable` in `topological/square_detector.rs`

Only the usable features are triangulated; the resulting triangle vertex
indices are remapped back into the global feature index space so every
downstream stage shares indices with the input. The triangulator runs in
`f64` internally for robustness on near-degenerate inputs.

*Why:* Delaunay gives a cheap, well-conditioned candidate-neighbour graph
without committing to a prior cell size — which matters because
cross-cluster nearest-neighbour distances are unreliable on boards with
markers.

### Step 2 — Edge classification: Grid / Diagonal / Spurious (generic)

`projective-grid/src/topological/classify.rs::classify_all_edges`

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

`projective-grid/src/topological/quads.rs::merge_triangle_pairs`

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

`projective-grid/src/topological/filter.rs::filter_quads`

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

`projective-grid/src/topological/walk.rs::label_components`

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

### Step 6 — Generic component merge

`merge_walk_components` in `topological/square_detector.rs`, calling the
shared `merge_components_local` implementation

Disconnected quad-walk patches are aligned and merged using local position and
cell-scale agreement. The merge deliberately does not use the global
projective fit, so mild radial distortion does not prevent compatible patches
from joining.

### Step 7a — Public validation + projective fit

`build_component_solution` and `run_fit_with_residual_drop` in
`topological/square_detector.rs`, calling the shared validation and fit kernels

For each component the shared post-grow validation runs row/column line
collinearity and per-corner local-homography checks; its optional edge-shape
gate is disabled by default. The labels are canonicalized to image axes before
a projective transform is fitted from grid coordinates to pixels;
corners whose reprojection residual exceeds `max_residual_px` are dropped
and the transform is refit once.

*Why:* an independent geometric gate over the labelled set, catching
gross mislabels that survived the topological merge.

### Step 7b — Detector-builder handoff

`projective_grid::expert::square::assemble_oriented2_components`

Pattern-specific detectors may stop immediately after Step 6. The returned
components retain the walk's axis-slot frame and stable caller source indices;
they deliberately have no generic fit or public-frame promise. The chessboard
uses this seam because parity alignment and its recovery policy interpret those
axis slots before applying their own final geometry gate.

### Step 8 — Orchestration (generic)

`detect_square_oriented2_all` in `topological/square_detector.rs`, reached
through `projective_grid::detect_grid_all` on the ordinary facade branch

Component solutions are sorted by labelled-corner count descending (ties
broken by smallest source index, for determinism). Every feature that no
component admitted is collected into a global rejected/unlabelled set and
attached to the largest diagnostics record. Ordinary `GridDetection` values
contain only accepted labels and their mandatory fit. `detect_grid` returns
only the largest component; `detect_grid_all` returns all of them.

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

The shared topology branch stops at merged labelled components. The chessboard
wrapper then:

1. Reuses the orientation clusters computed during input preparation and
   **parity-aligns** the
   topological labels against the chessboard parity convention
   (`(i + j) % 2`) — the chessboard-specific discipline the generic
   classifier deliberately omits.
2. Marks the component and runs the **recall boosters**
   (`calib_targets_chessboard::boosters::apply_boosters_with_directional_edge_scale`)
   — interior gap fill and line extrapolation — under the same axis /
   parity / edge gates the topological walk uses. Boosters use the larger
   directional median as their edge scale, while the final reported
   `cell_size` stays on the conservative all-edge median.
3. Merges boosted components by shared corner identity, runs a local geometry
   component merge, then **canonicalises** each surviving component:
   a mandatory geometry check
   (`run_geometry_check`), rebase to non-negative labels, axis-orientation
   canonicalisation, and sort. Detections are ordered by labelled count
   and capped at `max_components`.

*Why:* parity, recall boosting, and the final precision-protective
geometry check depend on chessboard-only types and conventions, so they
stay out of the generic crate. The geometry check can only *drop*
labelled corners — it never adds wrong labels — preserving the
chessboard precision contract (wrong `(i, j)` labels are unrecoverable;
missing corners are acceptable).

## Reproducible diagnostics and performance

`scripts/topological_campaign.py` is the supported local evidence driver. A
single TOML file (`scripts/topological_campaign.toml`) selects explicit input
images, the ordinary ChESS/chessboard settings, an optional nested expert
topological block, and warmup/repeat counts. Run:

```text
.venv/bin/python scripts/topological_campaign.py all
```

The overlay trace observes the same Rust execution that continues through
chessboard recovery and the final public output. The renderer does not
triangulate, classify, merge, or fit in Python. `trace.json` beside every image
contains exact stage state; the quality report evaluates the generic and final
checkpoints independently with one-to-one 3 px matching followed by the best
D4 plus integer-translation label alignment. The release timing report records
p50/p95/mean/max together with the CPU, compiler, Git revision, dirty-tree
digest, and every named pipeline span.

## Known limits

- **Three-corner cells are not recovered as quads.** The merge needs a
  complete cell (two triangles sharing a diagonal). One missing corner per
  cell starves the surrounding flood-fill. The recall boosters can later
  refill such a corner from local geometry once enough of its neighbours are
  labelled, but a cell missing a corner up front is not recovered by the
  initial triangle-pair merge.
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
  ChArUco detection mitigates this with a raised `min_corner_strength` floor
  (`CharucoParams::for_board`) that keeps marker-bit saddles out of the grid
  entirely, so the per-cell axis test is never poisoned in the first place.

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
  `crates/projective-grid/src/topological/`.
- Chessboard adapter + recovery source:
  `crates/calib-targets-chessboard/src/pipeline/`.
- `crates/calib-targets-chessboard/docs/PIPELINE.md` — the chessboard wrapper
  pipeline reference (prefilter, axis clustering, the topological adapter,
  recovery boosters, and the precision check).
