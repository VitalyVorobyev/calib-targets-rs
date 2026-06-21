# Detection pipeline internals

How the grid builders work, which one is wired where, and the per-corner
invariants the whole stack relies on. Read this before touching graph-build,
clustering, or orientation code.

## Graph-build algorithm selection

`calib_targets_chessboard::DetectorParams::graph_build_algorithm` selects
between two grid builders, both producing the same `(i, j) → corner_idx` map
so downstream consumers stay agnostic:

- `GraphBuildAlgorithm::Topological` (**current default**) — Shu/Brunton/Fiala
  2009 grid finder (`projective_grid::topological::build_grid_topological`)
  with an axis-driven cell test that replaces the paper's image-color sampling
  so `projective-grid` stays standalone. Image-free; faster, and higher recall
  than seed-and-grow on the clean-chessboard regression set with precision held
  (default flipped 2026-06-01). **NOT precision-safe on ChArUco-style images** —
  ChESS fires corners inside marker bits whose axes poison the per-cell axis
  test, so the topological builder can label marker-internal corners. Marker
  scenes therefore go through the ChArUco detector, which pins seed-and-grow
  (see below). Topological is not gated against ChArUco; see
  `docs/algorithmic_gaps.md` Gap 8 + 10.
- `GraphBuildAlgorithm::SeedAndGrow` — the invariant-rich seed-and-grow
  pipeline (`crate::seed`/`grow`/`validate` + cluster + boosters,
  ChESS-axis-driven). Pinned for ChArUco and the supported choice whenever
  marker-internal corners are present; battle-tested across all four target
  families.

### ChArUco pinning

`CharucoDetector::new` (`crates/calib-targets-charuco/src/detector/pipeline.rs`)
unconditionally overrides `chessboard.graph_build_algorithm = SeedAndGrow`
regardless of caller choice — marker-cell features defeat the topological cell
test, so the override is a precision guarantee, not a configuration choice.
PuzzleBoard and marker board inherit the caller's choice via their nested
`DetectorParams`.

### Component merge

`projective_grid::component_merge::merge_components_local` runs as a post-stage
for **both** pipelines and uses local geometry only — no global homography, so
it tolerates heavy radial distortion that would break a global fit. The
chessboard crate's historical `enable_component_merge` flag is now backed by
this shared implementation via `DetectorParams::component_merge:
LocalMergeParams`.

### Orientation source

The chessboard detector consumes the per-corner ChESS axis estimates carried by
each `ChessCorner` directly: clustering, Delaunay admission, and the recovery
boosters all read `ChessCorner.axes`. (The experimental chessboard-level
`OrientationSource::NeighbourEdges` knob — which synthesized the two grid
directions from neighbour geometry — was removed; the orientation-free path now
lives only in `projective-grid` for external callers, see below.)

The `projective-grid` facade still runs a **geometry-only recovery schedule**
(`seed_and_grow::recovery`) on the synthesized-axis paths (`Evidence::Positions`
/ `Evidence::Oriented1`) under `RecoverySchedule::Auto`: boundary extension
(local-H + cardinal-BFS) → interior fill → revalidate → drop filters
(topological wrong-label + largest-component), iterated to a fixed point. It is
precision-safe by construction — every attachment passes the same gates as BFS
grow, and a geometrically-incoherent attach is *dropped*, never mislabelled. On
a clean synthetic perspective grid it reaches full recall at zero wrong labels
(see `projective-grid/tests/detect_square_positions.rs`).

The chessboard topological adapter pins `RecoverySchedule::Off` and keeps its
own ChESS-axis-coupled recovery, so production output is byte-identical.

### Bench harness selector

```bash
cargo run -p calib-targets-bench -- {run,preview,diagnose} \
    --algorithm topological \
    [--engine {pipeline,grid}]
```

Runs the chessboard pipeline; the `grid` engine drives `detect_grid_all`
directly (bypassing chessboard recovery) over the ChESS-axis evidence. Output
JSON / overlay filenames carry the engine + algorithm + orientation-method
slugs so cells coexist in the same directory. `bench diagnose --algorithm
topological` reports the per-triangle composition counters (mergeable /
multi-diagonal / has-spurious / all-grid) plus per-quadrant labelled/unlabelled
counts and the unlabelled corners' axis sigmas — the right starting point when
investigating recall holes.

## Corner orientation contract (axes-only)

`Corner::orientation` has been **removed** workspace-wide. The only per-corner
orientation signal is `Corner.axes: [AxisEstimate; 2]`, populated by the
`chess-corners` adapter.

Convention (matches chess-corners 0.6 and enforced across the workspace):

- `axes[0].angle ∈ [0, π)`, `axes[1].angle ∈ (axes[0].angle, axes[0].angle + π)`.
- `axes[1] − axes[0] ≈ π/2` (the two axes are orthogonal grid directions, NOT
  diagonals of unit squares).
- The CCW sweep from `axes[0]` to `axes[1]` crosses a **dark** sector. This is
  what encodes parity: at parity-0 corners `axes[0] ≈ Θ_horizontal`
  (dark-entering), at parity-1 corners `axes[0] ≈ Θ_vertical`. Adjacent
  chessboard corners therefore have opposite axis-slot assignments.
- Default-constructed axes carry `sigma = π` (no information).

**Do not reintroduce `Corner::orientation`** or derive a "legacy" single-axis
angle. All clustering and edge-validation logic now uses `axes` directly. In
particular, edges in the grid graph align with one of the corner's own axes (no
±π/4 offset).

**Undirected circular mean.** Any function computing a circular mean of axis
angles (e.g. 2-means refinement, histogram peak centroid) MUST accumulate
`(cos 2θ, sin 2θ)` and halve the atan2 result. Accumulating raw `(cos θ, sin θ)`
breaks at the 0°/180° seam and silently returns garbage centers when a peak
sits near 0°. This was the root cause of the v1 Phase-4 regression; the fix is
in `calib-targets-core/src/orientation_clustering.rs` and
`crates/calib-targets-chessboard/src/cluster.rs`.

## Cell-size estimation gotcha

Do **not** pass a pre-computed global cell-size into a seed or graph-build step.
Cross-cluster nearest-neighbor distance distributions are bimodal on boards with
ArUco markers (marker-internal pairs vs true board pairs), and all mode finders —
multimodal mean-shift included — can pick the wrong mode. The seed-and-grow
detector solves this by **deriving cell size from a self-consistent 4-corner
seed** (edges match each other within a ratio tolerance, not against a prior
scalar); see `crates/calib-targets-chessboard/src/seed.rs`. If a future detector
must commit to a cell size up front, validate it by trying a seed and only trust
the estimate if the seed closes; otherwise fall back to the seed's own
edge-length mean.
