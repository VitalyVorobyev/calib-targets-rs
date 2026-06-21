# Regular Grid Detection Example

Reference: `crates/projective-grid/examples/hello_grid.rs` — the
minimal, image-free story for the standalone
[`projective-grid`](projective_grid.md) crate: a handful of oriented
feature points go in, a labelled `(i, j)` grid comes out, with no
image and no other workspace crate.

---

## Quick run

```bash
cargo run -p projective-grid --example hello_grid
```

The example synthesizes its own input (no image files needed): a small
square lattice with a mild perspective shear, so the cloud looks like a
board photographed at a slight angle.

---

## Walkthrough

### 1. Build oriented features

`projective-grid` knows nothing about images — for the square detector
it works on `OrientedFeature<F, 2>` values: a position plus two
roughly-orthogonal local axis directions. The example builds a `3×3`
grid by hand. In a real application these positions and axes would come
from a corner detector, a blob detector with a local-orientation
estimate, or a laser-dot extractor.

Each feature pairs a `PointFeature` (an image-frame position plus a
stable, caller-owned `source_index`) with two `LocalAxis` directions
(an angle in radians plus an optional angular uncertainty `sigma`):

```rust
use nalgebra::Point2;
use projective_grid::{LocalAxis, OrientedFeature, PointFeature};

let mut features: Vec<OrientedFeature<f32, 2>> = Vec::new();
for j in 0..3 {
    for i in 0..3 {
        // Image-frame position (origin top-left, x right, y down).
        // The `+ j * 6.0` term shears each successive row, so this is a
        // genuine projective grid, not a perfectly axis-aligned one.
        let x = 60.0 + i as f32 * 40.0 + j as f32 * 6.0;
        let y = 50.0 + j as f32 * 40.0;

        // `source_index` is a stable handle; the solution reports it back
        // so a recovered `(i, j)` label maps to the exact input feature.
        let point = PointFeature::new(features.len(), Point2::new(x, y));

        // Two undirected, roughly-orthogonal axes: horizontal (0 rad)
        // and vertical (pi/2 rad), each with a small angular sigma.
        let axes = [
            LocalAxis::new(0.0, Some(0.02)),
            LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(0.02)),
        ];
        features.push(OrientedFeature::new(point, axes));
    }
}
```

### 2. One call: `detect_grid`

Wrap the features as `Evidence::Oriented2`, bundle them into a
`DetectionRequest` for a `Square` lattice, and call `detect_grid`. Grid
dimensions are unknown (`None`); the detector infers the extent.

```rust
use projective_grid::{
    detect_grid, DetectionParams, DetectionRequest, Evidence, LatticeKind,
};

let request = DetectionRequest::new(
    LatticeKind::Square,
    Evidence::Oriented2(&features),
    None, // grid dimensions unknown; the detector infers the extent
    DetectionParams::default(),
);
let solution = detect_grid(request)?;
assert_eq!(solution.grid.entries.len(), 9);
```

`DetectionParams::default()` carries a `max_residual_px` fit gate and
selects `SquareAlgorithm::Topological` — the sole grid builder (the
`SeedAndGrow` variant was removed). It runs a Delaunay triangulation
over the corner cloud, classifies edges by axis match, merges triangle
pairs into cells, and floods integer coordinates across the mesh, then
fits a projective transform.

### 3. Handle the `Result`

Detection returns `Result<GridSolution, GridError>`. `GridError` is
`#[non_exhaustive]`, so callers always need a wildcard arm. The
variants worth matching:

- `UnsupportedCombination { task, lattice, evidence }` — the requested
  `(lattice, evidence)` pair has no algorithm yet. Today only
  `(Square, Oriented2)` is solved; everything else (a `Hex` lattice, or
  `Positions` / `Oriented1` / `Oriented3` evidence) returns this rather
  than a guessed answer.
- `InsufficientEvidence` — too few features to assemble a `2×2` seed.
- `DegenerateGeometry` — coincident or collinear points; no usable
  lattice spread.
- `InconsistentInput(String)` — input slices disagree or carry
  duplicate `source_index` handles.

### 4. Read the result

A successful detection is a `GridSolution`:

- `grid: LabelledGrid` — the recovered component. `grid.entries` is one
  `GridEntry` per placed feature; `grid.bbox` is the inclusive
  coordinate bounding box; `grid.dimensions` echoes any caller-supplied
  `GridDimensions`.
- `fit: Option<LatticeFit>` — the fitted model-plane-to-image
  projective transform (`model_to_image`) plus a residual summary
  (`residuals.count`, `residuals.mean_px`, `residuals.max_px`).
- `rejected: Vec<RejectedFeature>` — features this component could not
  place, each with a `RejectionReason` (`Unlabelled`,
  `ValidationDropped`, or `ResidualTooHigh`).

Each `GridEntry` carries:

- `coord: Coord` — the `(i, j)` label as `coord.u` / `coord.v`, rebased
  so the labelled bounding box starts at `(0, 0)`.
- `source_index: usize` — back into the input slice.
- `image_position: Point2<F>` — the feature's image-frame pixel-center
  position.
- `residual_px: Option<F>` — reprojection residual in pixels, present
  when a fit was computed.

```rust
for entry in &solution.grid.entries {
    // coord.u = i, coord.v = j; source_index maps back to the input.
    println!(
        "(i={}, j={}) <- feature {} at ({:.1}, {:.1})",
        entry.coord.u,
        entry.coord.v,
        entry.source_index,
        entry.image_position.x,
        entry.image_position.y,
    );
}
```

Running it labels all nine features `(0,0)` through `(2,2)` with a
sub-pixel fit residual.

---

## Going further

- **Multiple components** — `detect_grid_all` returns a
  `DetectionReport` whose `solutions` vector holds one `GridSolution`
  per recovered component, ordered by labelled count descending. Use it
  when the lattice fragments into islands (for example by occlusion)
  and the secondary components matter. The topological algorithm may
  yield several components.
- **Checking caller-supplied labels** — when `(i, j)` labels already
  exist (for instance from a marker decode), `check_consistency` scores
  them against a single projective fit instead of recovering them from
  scratch. The runnable version is
  `crates/projective-grid/examples/check_square_consistency.rs`.
- **A larger detection run** —
  `crates/projective-grid/examples/detect_square_oriented2.rs` exercises
  the same `Evidence::Oriented2` path on a bigger synthetic grid.

See the [`projective-grid` chapter](projective_grid.md) for the full
model — the two lattice families, the `Evidence` shapes, and the
topological algorithm.
