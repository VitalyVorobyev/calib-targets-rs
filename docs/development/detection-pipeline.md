# Detection pipeline internals

How the grid builder works, where it is wired, and the per-corner invariants the
whole stack relies on. Read this before touching graph-build, clustering, or
orientation code.

## The grid builder: topological only

The `projective-grid` topological finder is the **sole** grid builder, used by
every target type — chessboard, ChArUco, puzzleboard, and the marker board. It
is the Shu/Brunton/Fiala grid finder with an axis-driven cell test that replaces
the paper's image-color sampling so `projective-grid` stays image-free:
Delaunay triangulation → classify edges (Grid / Diagonal / Spurious) → merge
triangle pairs into cells → flood-fill integer labels → component merge →
shared validate → shared fit. It carries no global cell-size dependency.

Entry points:

- `projective_grid::detect_grid_all` (facade) → for `(Square, Oriented2)`,
  `topological/square_detector.rs::detect_square_oriented2_all`;
- the chessboard detector calls
  `projective_grid::expert::square::assemble_oriented2_components`, which stops
  after the component merge (no generic validation / fit / recovery) and hands
  the components to the chessboard's own recovery and geometry check
  (`crates/calib-targets-chessboard/src/pipeline/mod.rs`).

There is no algorithm selector: what to detect is chosen by the `LatticeKind` +
`Evidence` shape on the `DetectionRequest`. (`SeedAndGrow` and its
`graph_build_algorithm` key were removed; `ChessboardParams` is
`deny_unknown_fields`, so a config still carrying the key fails to parse.)

### Marker-internal corners

On ChArUco-style scenes ChESS fires spurious corners *inside* marker bits whose
axes lock to the marker rather than the grid; unchecked, they poison the
topological per-cell axis test. `CharucoParams::for_board` handles them with:

- a `min_corner_strength` floor (`33.0`) that cuts the weak ChESS responses on
  marker-bit saddles **before** the grid is built, so it never extends into the
  marker interior; and
- **the chessboard's final wrong-label geometry check stays enabled**, exactly
  as for the standalone chessboard detector. Marker decoding is not a
  substitute: the board alignment searches `D4 × integer translation`, a
  *rigid* relabelling of whatever lattice the chessboard produced, so it
  cannot see or repair a labelling that is wrong *within* a component.

See `crates/calib-targets-charuco/docs/PIPELINE.md` for the full ChArUco stage
map and `docs/algorithms/algorithmic_gaps.md` for the remaining open items.

### Component merge

`projective_grid::shared::merge::merge_components_local` runs as a post-stage
on the topological builder's per-component walk output and uses local geometry
only — no global homography, so it tolerates heavy radial distortion that would
break a global fit. The chessboard tunes it via
`ChessboardAdvancedTuning::component_merge: LocalMergeParams`.

### Orientation source

The chessboard detector consumes the per-corner ChESS axis estimates carried by
each `ChessCorner` directly: clustering, Delaunay admission, and the recovery
boosters all read `ChessCorner.axes`. The orientation-free path lives only in
`projective-grid`, for external callers (see below).

The `projective-grid` facade still runs a **geometry-only recovery schedule**
(`RecoverySchedule`) on the synthesized-axis paths (`Evidence::Positions` /
`Evidence::Oriented1`) under `RecoverySchedule::Auto`: boundary extension
(local-H + cardinal-BFS) → interior fill → revalidate → drop filters
(topological wrong-label + largest-component), iterated to a fixed point. It is
precision-safe by construction — every attachment passes the same geometric
gates, and a geometrically-incoherent attach is *dropped*, never mislabelled. On
a clean synthetic perspective grid it reaches full recall at zero wrong labels
(see `projective-grid/tests/detect_square_positions.rs`).

The chessboard never enters this schedule — `assemble_oriented2_components`
returns before it — and runs its own ChESS-axis-coupled recovery instead.

### Bench harness selector

```bash
cargo run -p calib-targets-bench -- {run,preview,diagnose} \
    [--engine {pipeline,grid}]
```

Runs the chessboard pipeline on the topological builder; the `grid` engine
drives `detect_grid_all` directly (bypassing chessboard recovery) over the
ChESS-axis evidence. Output JSON / overlay filenames carry the engine +
orientation-method slugs so cells coexist in the same directory. `bench
diagnose` reports prefilter survivors, per-component bounding boxes,
per-quadrant labelled/unlabelled counts and the unlabelled corners' axis
sigmas — the right starting point when investigating recall holes.

## Corner orientation contract (axes-only)

`Corner::orientation` has been **removed** workspace-wide. The only per-corner
orientation signal is `Corner.axes: [AxisEstimate; 2]`, populated by the
`chess-corners` adapter.

Convention (enforced across the workspace):

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
sits near 0°. The reference implementation is
`crates/projective-grid/src/cluster/circular.rs::refine_2means_double_angle`.

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
