# Detection pipeline internals

How the grid builder works, where it is wired, and the per-corner invariants the
whole stack relies on. Read this before touching graph-build, clustering, or
orientation code.

## The grid builder: topological only

`projective_grid::topological::build_grid_topological` is the **sole** grid
builder, used by every target type — chessboard, ChArUco, puzzleboard, and the
marker board. It is the Shu/Brunton/Fiala 2009 grid finder with an axis-driven
cell test that replaces the paper's image-color sampling so `projective-grid`
stays image-free and standalone: Delaunay triangulation → classify edges
(Grid / Diagonal / Spurious) → merge triangle pairs into cells → flood-fill
integer labels → shared validate → shared fit. It carries no global cell-size
dependency, and the same `(i, j) → corner_idx` map flows to every downstream
consumer.

**Topological is the sole grid builder, so the request carries no algorithm
choice.** The historical `projective_grid::SquareAlgorithm` and
`calib_targets_chessboard::DetectorParams::graph_build_algorithm` (typed
`GraphBuildAlgorithm`) selector enums — first collapsed to single-variant
`#[non_exhaustive]` reserved seams when seed-and-grow was retired — have since
been **removed** entirely; what to detect is selected by the `LatticeKind` +
`Evidence` shape on the `DetectionRequest`, not by an algorithm enum. The
historical `SeedAndGrow` variant (a self-consistent 4-corner seed plus BFS grow
with axis-coupled boosters) was retired once the topological builder matched or
beat it on every shipping path, including ChArUco. The wire string
`"seed_and_grow"` no longer deserializes; config loaders that previously
accepted a `graph_build_algorithm` value now ignore it.

### Marker-internal corners (formerly the ChArUco concern)

On ChArUco-style scenes ChESS fires spurious corners *inside* marker bits whose
axes lock to the marker rather than the grid; historically those poisoned the
topological per-cell axis test, which is why ChArUco used to pin the
seed-and-grow builder. That pin is gone. ChArUco now runs the topological
builder with two `CharucoParams::for_board` settings that handle the marker-bit
corners directly:

- a `min_corner_strength` floor (set to `33.0`) cuts the weak ChESS responses on
  marker-bit saddles **before** the grid grows, so the grid never extends into
  the marker interior; and
- `enable_final_edge_shape_check` is disabled (the marker-ID and
  board-alignment validation downstream of grid recovery is the precision gate
  for ChArUco, so the chessboard component stays recall-oriented).

See `crates/calib-targets-charuco/docs/PIPELINE.md` for the full ChArUco stage
map and `docs/algorithms/algorithmic_gaps.md` for the remaining open items.

### Component merge

`projective_grid::component_merge::merge_components_local` runs as a post-stage
on the topological builder's per-component walk output and uses local geometry
only — no global homography, so it tolerates heavy radial distortion that would
break a global fit. The chessboard crate's historical `enable_component_merge`
flag is now backed by this shared implementation via
`DetectorParams::component_merge: LocalMergeParams`.

### Orientation source

The chessboard detector consumes the per-corner ChESS axis estimates carried by
each `ChessCorner` directly: clustering, Delaunay admission, and the recovery
boosters all read `ChessCorner.axes`. (The experimental chessboard-level
`OrientationSource::NeighbourEdges` knob — which synthesized the two grid
directions from neighbour geometry — was removed; the orientation-free path now
lives only in `projective-grid` for external callers, see below.)

The `projective-grid` facade still runs a **geometry-only recovery schedule**
(`RecoverySchedule`) on the synthesized-axis paths (`Evidence::Positions` /
`Evidence::Oriented1`) under `RecoverySchedule::Auto`: boundary extension
(local-H + cardinal-BFS) → interior fill → revalidate → drop filters
(topological wrong-label + largest-component), iterated to a fixed point. (This
schedule originated with the retired seed-and-grow builder and survived the
retirement; it now powers the topological synthesized-axis path.) It is
precision-safe by construction — every attachment passes the same geometric
gates, and a geometrically-incoherent attach is *dropped*, never mislabelled. On
a clean synthetic perspective grid it reaches full recall at zero wrong labels
(see `projective-grid/tests/detect_square_positions.rs`).

The chessboard topological adapter pins `RecoverySchedule::Off` and keeps its
own ChESS-axis-coupled recovery, so production output is byte-identical.

### Bench harness selector

```bash
cargo run -p calib-targets-bench -- {run,preview,diagnose} \
    [--engine {pipeline,grid}]
```

Runs the chessboard pipeline on the topological builder; the `grid` engine
drives `detect_grid_all` directly (bypassing chessboard recovery) over the
ChESS-axis evidence. Output JSON / overlay filenames carry the engine +
orientation-method slugs so cells coexist in the same directory. `bench
diagnose` reports the per-triangle composition counters (mergeable /
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

Do **not** pass a pre-computed global cell-size into a graph-build step.
Cross-cluster nearest-neighbor distance distributions are bimodal on boards with
ArUco markers (marker-internal pairs vs true board pairs), and all mode finders —
multimodal mean-shift included — can pick the wrong mode. The topological builder
sidesteps this entirely: it never commits to a global cell size, deriving every
length judgement from **local** triangle geometry (per-cell edge-length bands and
the √2 diagonal ratio inside each Delaunay triangle), so it tolerates the smooth
pitch variation a global scalar would mis-model. If a future detector must commit
to a cell size up front, validate it against local geometry — try to close a
small consistent patch and only trust the estimate if that patch agrees — rather
than trusting a single global mode.
