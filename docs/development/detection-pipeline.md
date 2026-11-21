# Detection pipeline internals

The per-corner invariants the whole stack relies on. Read this before touching
graph-build, clustering, or orientation code.

## Where each topic lives

- Grid builder (Delaunay, edge classes, cell test, flood-fill, component merge):
  [`docs/algorithms/topological-grid-detection.md`](../algorithms/topological-grid-detection.md)
- Chessboard stages, recovery boosters, wrong-label geometry check:
  [`crates/calib-targets-chessboard/docs/PIPELINE.md`](../../crates/calib-targets-chessboard/docs/PIPELINE.md)
- Orientation handling and clustering in `projective-grid`:
  [`crates/projective-grid/docs/ORIENTATION.md`](../../crates/projective-grid/docs/ORIENTATION.md)
- ChArUco stages and marker-internal corners:
  [`crates/calib-targets-charuco/docs/PIPELINE.md`](../../crates/calib-targets-charuco/docs/PIPELINE.md)
- Bench harness (`run` / `check` / `diagnose` / `preview`, `--engine`):
  [`commands.md`](commands.md) and [`debugging.md`](debugging.md)

## Corner orientation contract (axes-only)

The only per-corner orientation signal is `ChessCorner.axes: [AxisEstimate; 2]`,
populated by the `chess-corners` adapter. There is no `orientation` field and
none may be added; no single-axis "legacy" angle is derived.

Convention (enforced across the workspace):

- `axes[0].angle ∈ [0, π)`, `axes[1].angle ∈ (axes[0].angle, axes[0].angle + π)`.
- `axes[1] − axes[0] ≈ π/2` (the two axes are orthogonal grid directions, NOT
  diagonals of unit squares).
- The CCW sweep from `axes[0]` to `axes[1]` crosses a **dark** sector. This is
  what encodes parity: at parity-0 corners `axes[0] ≈ Θ_horizontal`
  (dark-entering), at parity-1 corners `axes[0] ≈ Θ_vertical`. Adjacent
  chessboard corners therefore have opposite axis-slot assignments.
- Default-constructed axes carry `sigma = π` (no information).
- Edges in the grid graph align with one of the corner's own axes (no ±π/4
  offset); clustering and edge validation read `axes` directly.

**Undirected circular mean.** Any circular mean of axis angles (2-means
refinement, histogram peak centroid) MUST accumulate `(cos 2θ, sin 2θ)` and
halve the atan2 result. Accumulating raw `(cos θ, sin θ)` breaks at the 0°/180°
seam and returns wrong centres when a peak sits near 0°. Reference
implementation: `crates/projective-grid/src/cluster/circular.rs::refine_2means_double_angle`.

## Cell-size rule

Never pass a pre-computed global cell size into a graph-build step.
Cross-cluster nearest-neighbour distances are bimodal on boards with ArUco
markers (marker-internal pairs vs true board pairs), so any global mode finder
can pick the wrong mode. The topological builder commits to no global cell size:
every length judgement comes from **local** triangle geometry (per-cell
edge-length bands and the √2 diagonal ratio inside each Delaunay triangle),
which tolerates smooth pitch variation. A detector that must commit to a cell
size up front validates it against local geometry — close a small consistent
patch and trust the estimate only if the patch agrees.
