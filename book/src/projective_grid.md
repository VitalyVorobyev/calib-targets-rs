# The Grid Model

> Code: [`projective-grid`](https://github.com/VitalyVorobyev/calib-targets-rs/tree/main/crates/projective-grid).

This is the **foundational model every detector in the workspace shares**.
Before you tune a chessboard, ChArUco, or PuzzleBoard detector it pays to
understand the layer beneath them all — the `projective-grid` crate. Given a
cloud of 2D feature points — optionally carrying one, two, or three local axis
directions per point — it recovers an `(i, j) → point` labelling: which integer
grid cell each feature occupies under perspective, together with a fitted
projective transform from model-plane coordinates to image pixels.

The **input-feature kinds** introduced here (plain points versus oriented
features) and the **recovery algorithm** below are exactly the vocabulary
the [Tuning the Detector](tuning.md) chapter's parameters act on — read this
first, and the tuning knobs stop being a flat list of names.

The crate is deliberately small and **image-free**. There are no
image, pixel-buffer, or camera types anywhere in its public surface,
and no target-specific identifiers (marker IDs, ring IDs, calibration
metadata). It is **target-agnostic**: the same lattice recovery serves
a chessboard detector, a laser-dot cloud, a scanned form, or a
photographed board game. The detection surface is single-precision
(`f32`); the standalone projective geometry kernel stays generic over
`f32` / `f64` via the `Float` trait. The other workspace detectors sit *above* this
crate — they run a corner detector, convert its output into generic
point or oriented features, and call in here for the labelling.

The crate ships independently on crates.io and is used directly for
non-calibration tasks: rectifying a photograph of a board game,
fitting a locally-planar lattice to a laser-dot cloud, extracting a
grid from a scanned document, or building a new detector for a pattern
the workspace doesn't yet ship.

---

## The model

Three small pieces define the public surface.

**Two lattice families** (`LatticeKind`). `Square` is the orthogonal
`(i, j)` grid and is detected by the topological algorithm. `Hex` (axial `(q, r)`)
is also detected on the **topological path**: its triangles are the unit
cells directly, so there is no diagonal/quad-merge stage.

**Two tasks.**

- *Detection* — `detect_grid` / `detect_grid_all`: recover labels from
  raw evidence when you do **not** know which feature is which cell.
- *Consistency* — `check_consistency`: you already have a proposed
  `(i, j)` label per feature (e.g. from a marker decode) and want to
  know whether those labels are geometrically consistent under a single
  projective fit. This is a separate entry point with its own request
  and report types; it does not go through the `Evidence` enum.

**Explicit evidence shapes** (`Evidence`). Detection input is wrapped
in an enum that names exactly how much orientation the caller can
supply. The less-oriented square kinds synthesize the missing axes from
neighbour geometry up front and then run the same strategy:

| Variant | Payload | Square | Hex |
|---|---|---|---|
| `Positions` | `&[PointFeature]` | ✅ synthesize 2 axes | ✅ synthesize 3 axes (topological) |
| `Oriented1` | `&[OrientedFeature<1>]` | ✅ synthesize 2nd axis | ❌ `UnsupportedCombination` |
| `Oriented2` | `&[OrientedFeature<2>]` | ✅ native (topological) | ❌ `UnsupportedCombination` |
| `Oriented3` | `&[OrientedFeature<3>]` | ❌ `UnsupportedCombination` | ✅ native (topological) |
| `CoordinateHypotheses` | features + hypotheses | use `check_consistency` instead | — |

Each feature carries a `PointFeature` (position + caller-owned
`source_index`) plus `N` undirected `LocalAxis` directions (`N = 0` for
`Positions`). Any unsupported `(lattice, evidence)` combination — for
example `(Square, Oriented3)`, `(Hex, Oriented1/Oriented2)`, or
`CoordinateHypotheses` for detection — returns a typed
`GridError::UnsupportedCombination { task, lattice, evidence }`, never a
guessed answer.

---

## Worked example

A fully self-contained, image-free example: synthesize a small `3×3`
grid with a mild perspective shear, wrap the features as evidence,
detect, and read the recovered labels. This is the body of
[`examples/hello_grid.rs`][hello-grid] — run it with
`cargo run -p projective-grid --example hello_grid`.

```rust,ignore
use nalgebra::Point2;
use projective_grid::{
    detect_grid, DetectionParams, DetectionRequest, Evidence, LatticeKind, LocalAxis,
    OrientedFeature, PointFeature, SquareAlgorithm,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build a 3x3 grid of oriented features. The `+ j * 6.0` term adds a
    // mild perspective-style shear, so this is a genuine projective grid,
    // not a perfectly axis-aligned one.
    let mut features: Vec<OrientedFeature<2>> = Vec::new();
    for j in 0..3 {
        for i in 0..3 {
            // Image-frame position: origin top-left, x right, y down.
            let x = 60.0 + i as f32 * 40.0 + j as f32 * 6.0;
            let y = 50.0 + j as f32 * 40.0;

            // `source_index` is a stable caller-owned handle; the solution
            // reports it back so you can map a label to the input feature.
            let point = PointFeature::new(features.len(), Point2::new(x, y));

            // Two roughly-orthogonal local axes: horizontal (0 rad) and
            // vertical (pi/2 rad), each with a small angular sigma.
            let axes = [
                LocalAxis::new(0.0, Some(0.02)),
                LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(0.02)),
            ];
            features.push(OrientedFeature::new(point, axes));
        }
    }

    // Wrap as Oriented2 evidence and ask for a square lattice. Grid
    // dimensions are unknown (`None`); the detector infers the extent.
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None,
        DetectionParams::default(),
    );

    // `detect_grid` returns the largest recovered component.
    let solution = detect_grid(request)?;
    for entry in &solution.grid.entries {
        // coord.u = i, coord.v = j; source_index maps back to the input.
        println!(
            "(i={}, j={}) <- feature {}",
            entry.coord.u, entry.coord.v, entry.source_index
        );
    }
    Ok(())
}
```

Running it labels all nine features `(0,0)` through `(2,2)` with a
sub-pixel fit residual. Two sibling examples under
`crates/projective-grid/examples/` round out the surface:
`detect_square_oriented2` (a larger detection run) and
`check_square_consistency` (the consistency task on pre-labelled
features).

---

## The square algorithm: Topological

Detection of `(Square, Oriented2)` uses the **`SquareAlgorithm::Topological`**
algorithm — the sole grid builder for all square targets. It is the Shu /
Brunton / Fiala 2009 axis-driven grid finder: a Delaunay triangulation over
the corner cloud whose edges are classified by per-corner axis match, with
triangle pairs merged into cells and integer coordinates flooded across the
mesh. Image-free; recovers dense grids and copes well with distortion. May
produce several components — see `detect_grid_all` below.

The `GraphBuildAlgorithm` type still exists in the public API (for
`DetectorParams`), but the `SeedAndGrow` variant was removed; `Topological`
is the only variant.

The algorithm shares the post-detection validation and projective fit across
all target types, recovering the full pattern with zero wrong labels. The
deep-dive — the axis-classification test, the triangle-to-cell merge, and the
line between the generic machinery here and the chessboard-specific wrapper —
lives in `docs/topological-grid-detection.md` in the workspace repository.

**Hex** also uses the topological algorithm. On a hex point lattice the
Delaunay triangles *are* the unit cells, so the diagonal/quad-merge stage is
bypassed; the axial `(q, r)` walk and the projective fit back-half are
otherwise shared with the square topological path. The fit residual is the
precision gate.

### Single vs. multi-component results

`detect_grid` returns the **largest** recovered component as one
`GridSolution`. When the lattice is split into islands (for example by
occlusion) and the secondary components matter, call `detect_grid_all`:
it returns a `DetectionReport` whose `solutions` vector holds one
`GridSolution` per qualifying component, ordered by labelled-count
descending. The topological path may yield several components.

---

## Inputs

Detection input is the `Evidence` enum (see *The model* above). For the
native `Oriented2` shape each element is an `OrientedFeature<2>`:

- `point: PointFeature` — `position` (image-frame pixel center) and a
  stable, caller-owned `source_index`. The solution reports the
  `source_index` back so a recovered label maps to the exact input.
- `axes: [LocalAxis; 2]` — two undirected local lattice directions,
  each an `angle_rad` plus an optional `sigma_rad` (angular
  uncertainty). Axes are *undirected*: `θ` and `θ + π` denote the same
  direction.

`DetectionRequest::new(lattice, evidence, dimensions, params)` bundles
the lattice family, the evidence, optional known `GridDimensions`, and
a `DetectionParams`. `DetectionParams` carries `max_residual_px` (the
fit residual gate) and the algorithm selector (always `Topological`),
with a `topological` sub-config and a shared `validate` sub-config;
`Default` covers all the tuning knobs and the builder-style `with_*`
methods override individual fields.

---

## Outputs

A successful detection is a `GridSolution`:

| Field | Meaning |
|---|---|
| `grid: LabelledGrid` | The labelled component: `entries` (one per placed feature), the `lattice` family, an inclusive coordinate `bbox`, and the optional caller-supplied `dimensions`. |
| `fit: Option<LatticeFit>` | The fitted model-plane-to-image projective transform plus a `residuals: ResidualSummary` (`count`, `mean_px`, `max_px`). |
| `rejected: Vec<RejectedFeature>` | Features this component could not place. |

Each `GridEntry` carries:

- `coord: Coord` — the `(i, j)` label as `coord.u` / `coord.v`, rebased
  so the labelled bounding box starts at `(0, 0)`.
- `source_index: usize` — back into the caller's input slice.
- `image_position: Point2<f32>` — the feature's image-frame pixel-center
  position.
- `residual_px: Option<f32>` — reprojection residual in pixels, present
  when a fit was computed.

Each `RejectedFeature` carries the `source_index`, an optional
`coord`, an optional `residual_px`, and a `RejectionReason`:
`Unlabelled` (never placed — e.g. noise outside the recovered lattice),
`ValidationDropped` (placed by the topological pass but dropped by
post-detection validation: line collinearity, local-homography residual,
or edge-length band), or `ResidualTooHigh` (reprojection residual
exceeded `max_residual_px`).

For multi-component runs, `detect_grid_all` returns a `DetectionReport`
with the per-component `solutions` vector plus a top-level `rejected`
slot.

---

## Checking caller-supplied labels

When labels already exist — for instance after decoding marker IDs into
`(i, j)` coordinates — `check_consistency` scores them against a single
projective fit instead of recovering them from scratch. Build a
`ConsistencyRequest::new(lattice, features, hypotheses, dimensions,
params)` from position-only `PointFeature`s and a parallel slice of
`CoordinateHypothesis` (each pairing a `source_index` with a proposed
`Coord`), with a `ConsistencyParams` whose `max_residual_px` sets the
acceptance threshold. The returned `ConsistencyReport` has `passed`
(true when every residual clears the threshold), the full `solution`
(labels, fit, and any over-residual `rejected` entries), and a
`max_residual_px()` convenience accessor. `check_square_consistency` in
the examples directory is the runnable version.

This is also the one entry point that consumes coordinate hypotheses;
`Evidence::CoordinateHypotheses` exists for symmetry in the detection
enum but `detect_grid` does not yet act on it.

---

## Conventions

- **Coordinates.** Image pixels: origin top-left, x right, y down. Grid
  `i` (`coord.u`) increases right, `j` (`coord.v`) increases down.
- **Undirected axes.** A `LocalAxis` angle is undirected — `θ` and
  `θ + π` are the same direction. Any circular mean over axis angles
  must therefore accumulate `(cos 2θ, sin 2θ)` and halve the resulting
  `atan2`; naive `(cos θ, sin θ)` averaging breaks at the 0°/180° seam.
- **Non-negative, top-left-origin labels.** Output `(i, j)` is rebased
  so the labelled bounding-box minimum is `(0, 0)`.
- **Single precision.** The detection surface is pinned to `f32`. Only
  the standalone projective geometry kernel stays generic over
  `F: Float`, for a future `f64` calibration consumer.

---

## Out of scope

- **3D grids.** Coordinates are 2D (`nalgebra::Point2`); there is no 3D
  support.
- **Non-planar surfaces.** The fit assumes a single planar homography
  maps the labelled set; severely curved surfaces are not modelled here.
- **Feature detection.** This crate does lattice recovery and projective
  consistency, not corner finding. Bring your own points; if you have an
  image, run a corner detector first and convert its output into
  `PointFeature` / `OrientedFeature` values before calling in.
- **Dense, unstructured point clouds.** Pure noise without any local
  axis structure does not yield a usable Delaunay classification.

---

## Learn more

API reference: [`projective-grid` on docs.rs](https://docs.rs/projective-grid).
The topological grid finder has an in-repo deep-dive at
`docs/topological-grid-detection.md`.

[hello-grid]: https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/crates/projective-grid/examples/hello_grid.rs
