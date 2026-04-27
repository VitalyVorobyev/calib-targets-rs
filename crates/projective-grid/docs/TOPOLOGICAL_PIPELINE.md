# Topological grid pipeline — atomic stages

Concise stage-by-stage map of `projective_grid::topological::build_grid_topological`,
an opt-in alternative to chessboard-v2's seed-and-grow grid builder.
Based on Shu / Brunton / Fiala 2009: Delaunay triangulation → edge
classification (Grid / Diagonal / Spurious) → triangle pair-merge into
quads → topological + geometric quad filters → BFS labelling on the
quad mesh.

The chessboard-v2 pipeline (separately documented in
`crates/calib-targets-chessboard/docs/PIPELINE.md`) wraps this builder
when `DetectorParams::graph_build_algorithm = Topological`. ChArUco
unconditionally pins `ChessboardV2` instead — the per-cell axis test
in Stage 2 below cannot survive marker-internal X-corners.

## Stage table

| # | Name | In | Out | Decision | Failure modes | Knobs |
|---|---|---|---|---|---|---|
| 0 | axis-validity pre-filter | per-corner `axes: [AxisHint; 2]` | `usable_mask` over corners | drop corners where **both** `axes[k].sigma ≥ max_axis_sigma_rad`. Sigma `= π` is the no-info sentinel from `chess-corners`. | high angular noise globally → too few usable corners; opposite failure: too tight → loses real corners that happen to have one wide axis | `max_axis_sigma_rad` (default `0.6 rad ≈ 34°`) |
| 1 | Delaunay triangulation | `&[Point2]` (all positions) | `Triangulation { triangles, halfedges }` | `delaunator` crate; f32 → f64 input, half-edge structure | < 3 usable corners; degenerate point clouds | — |
| 2 | edge classification | triangles + positions + axes + `usable_mask` | per-half-edge `EdgeKind ∈ {Grid, Diagonal, Spurious}` | At each endpoint compute edge angle `θ = atan2(b − a)`; find min undirected (mod π) distance to that endpoint's usable axes; classify per-endpoint as **Grid** if `d < axis_align_tol_rad`, **Diagonal** if `\|d − π/4\| < diagonal_angle_tol_rad`, else **Spurious**. Whole-edge type is the **conjunction** at both endpoints. | axis uncertainty blurs Grid/Diagonal distinction; unusable corner forces Spurious at one end → loses real grid edges | `axis_align_tol_rad` (default `15°`), `diagonal_angle_tol_rad` (default `15°`) |
| 3 | triangle composition | edge kinds | per-triangle bucket: `mergeable` (1 D, 2 G) / `all_grid` (3 G) / `multi_diag` (≥ 2 D) / `has_spurious` | counts per triangle; `mergeable` are merge-eligible; the other buckets are diagnostic only (failure-mode telemetry) | uneven foreshortening → `all_grid` spike (cell diagonal not at 45°); dense clutter → `multi_diag` | — |
| 4 | triangle-pair merge → quads | mergeable triangles + half-edges | `Vec<Quad>` (4 vertices in CW order) | for each `mergeable` triangle, look up the buddy of its unique Diagonal half-edge; if buddy's triangle is also `mergeable` with the same Diagonal, fuse — quad's four perimeter edges are all Grid | triangles without a unique Diagonal (boundary or ambiguous); known regression: triangles whose two long edges classify as Diagonal (severe perspective + radial distortion, see overview Gap 8) | — |
| 5 | topological filter | quads + per-corner incidence | quads with `< 2` illegal corners | per-corner incidence count over quad-perimeter edges; corner is illegal if its incidence count > 8 (degree > 4 in the quad mesh, impossible in a regular grid) | dense corners at boundaries / occlusions inflate incidence | — |
| 6 | geometric filter | filtered quads + positions | quads passing `max(l_01/l_23, l_12/l_30) ≤ edge_ratio_max` | opposing edges of each quad must not differ by more than `edge_ratio_max`; rejects extreme parallelograms | severe perspective leaves legitimate quads with high opposing-edge ratio (loosen knob) | `edge_ratio_max` (default `10.0`) |
| 7 | topological walking | filtered quads + positions | `Vec<TopologicalComponent>` with labelled `(i, j)` per corner | BFS on quad-adjacency graph (shared edges); seed quad gets `(0,0), (1,0), (1,1), (0,1)` CW; labels propagate through shared-edge cell-step rules; rebase each component to `(0, 0)` | disconnected components left independent (handled by `crate::component_merge`); rare label conflicts at boundary if filters too loose | — |
| 8 | output + diagnostics | labelled components | `TopologicalGrid { components, diagnostics: TopologicalStats }` | per-component bbox rebase; emit per-stage counters | — | — |

## Component merge

After this builder finishes, `projective_grid::component_merge::merge_components_local`
runs as a shared post-stage (the same one chessboard-v2 uses) — image-
free, local-geometry-only, distortion-tolerant. See the
`projective_grid::component_merge` module docs for the merge predicate.

Documented gap (see `docs/projective_grid_overview.md`): merge requires
overlapping `(i, j)` labels after a candidate alignment; spatially-
disjoint components separated by a missing row never reach the overlap
test and stay split.

## Diagnostics (`TopologicalStats`)

```text
corners_in, corners_used,
triangles,
grid_edges, diagonal_edges, spurious_edges,
triangles_mergeable, triangles_all_grid,
triangles_multi_diag, triangles_has_spurious,
quads_merged, quads_kept,
components,
```

Plus per-quadrant labelled / unlabelled counts and unlabelled-corner
axis sigmas printed by `bench diagnose --algorithm topological` —
canonical starting point for triangle-merge / classification gap
investigations.

## Known limitations vs chessboard-v2

- **ChArUco-style targets** (markers inside white squares) — corners
  detected inside marker bits have axes that lock to the marker's
  local orientation, not the chessboard grid. The Stage 2 axis test
  classifies their incident edges as Spurious or Diagonal, breaking
  triangle merging across the marker. Use chessboard-v2 instead;
  CharucoDetector pins this.
- **Heavy perspective + radial distortion** — `triangles_all_grid`
  counter spikes in the most foreshortened region. The merge step
  refuses pairs whose two long edges classify as Diagonal because
  the cell's diagonal is no longer at 45° to its sides. Documented as
  Gap 8 in `docs/projective_grid_overview.md` — fix options:
  permissive pairing, spurious salvage, or hybrid extension via
  chessboard-v2's local-H Stage 6.

## Cross-references

- `crates/calib-targets-chessboard/docs/PIPELINE.md` — the seed-and-
  grow alternative + the chessboard-specific axis-slot-swap edge
  invariant.
- `CLAUDE.md` "Graph-build algorithm selection" — when to use which.
- `docs/projective_grid_overview.md` — original integrated reference;
  this stage map supersedes the overview's prose for the topological
  pipeline.
