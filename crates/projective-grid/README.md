# projective-grid

[![docs.rs](https://docs.rs/projective-grid/badge.svg)](https://docs.rs/projective-grid)
[![crates.io](https://img.shields.io/crates/v/projective-grid.svg)](https://crates.io/crates/projective-grid)

Recover a **projective lattice** — an `(i, j) → point` labelling on a square or
hexagonal grid — from a set of 2D feature points plus optional per-feature
local axis directions.

You bring the points (from whatever corner / feature detector you already
have); `projective-grid` figures out how they tile a regular lattice under
perspective and hands back each point's integer grid coordinate together with
a fitted projective transform.

The crate is deliberately small and **image-free**: there are no image,
pixel-buffer, or camera types anywhere in the public surface, and no
target-specific identifiers (marker IDs, ring IDs, calibration metadata). It
is **target-agnostic** — the same lattice recovery serves a chessboard
detector, a laser-dot cloud, a scanned form, or a photographed board game.
The detection surface is single-precision (`f32`); the standalone projective
geometry kernel stays generic over `f32` / `f64` via the [`Float`] trait.

## When to use it

Reach for `projective-grid` when you already have a cloud of 2D points that
*should* lie on a regular grid and you need to know **which grid cell each one
is** — robustly, under perspective and mild lens distortion.

It handles:

- **Perspective + mild distortion** — the lattice is fitted projectively, and
  the grow / topological paths use local geometry that tolerates curvature a
  single global homography would not.
- **Multi-component grids** — when the lattice is split into islands (e.g. by
  occlusion), [`detect_grid_all`] returns each connected component with its
  own labels; [`detect_grid`] returns just the largest.
- **Component merging** — nearby components that share a consistent lattice are
  reconciled using local geometry only.

## When *not* to use it

This crate does **lattice recovery and projective consistency, not feature
detection**. It will not find corners in an image for you — the caller supplies
the points (and, optionally, local axis directions per point). If you have an
image and need corners first, run a corner detector and convert its output into
[`PointFeature`] / [`OrientedFeature`] values before calling in.

It recovers both **square** and **hexagonal** lattices. Both use the
**topological** assembler — the sole grid builder in this crate. Hex runs
the same Delaunay back-half — its triangles are the unit cells directly, so
there is no diagonal/quad-merge stage. The lattice family is selected by
`LatticeKind` on the request; there is no further algorithm choice to make.

## Three kinds of evidence

How much you know about each point's orientation picks the [`Evidence`] variant.
The square variants share one back-half — the less-oriented kinds synthesize the
missing axes from neighbour geometry and then run the same strategy — so they
produce the same [`GridSolution`] shape. The same evidence ladder applies to
hex (`Positions` synthesizes three axis families;
[`Evidence::Oriented3`] supplies them directly):

- **Unoriented — [`Evidence::Positions`]** (`&[PointFeature]`). Just points: a
  dot grid, a circle grid, or corners with no axis estimate. Both local grid
  directions are recovered per point from neighbour chords (folded modulo π, so
  the estimate is perspective-invariant and never assumes the axes are 90°
  apart). Works when the lattice is the dominant local structure; if your
  point cloud carries dense sub-lattice clutter (e.g. marker-glyph corners
  between the true grid points), neighbour statistics cannot recover the
  axes — supply measured orientations (`Oriented1`/`Oriented2`) instead.
- **Single-axis — [`Evidence::Oriented1`]** (`&[OrientedFeature<1>]`). One
  trusted direction per point (e.g. a detector that recovers a dominant edge
  orientation but not the orthogonal one). The supplied axis is kept; the second
  is synthesized from neighbours.
- **Dual-axis — [`Evidence::Oriented2`]** (`&[OrientedFeature<2>]`). Two local
  grid directions per point — the native shape, e.g. ChESS-style corner axes.
  No synthesis; the strongest input.

- **Triple-axis — [`Evidence::Oriented3`]** (`&[OrientedFeature<3>]`). The
  **hex-native** shape: a hexagonal lattice has three axis families, and a
  detector that recovers all three feeds them here. `(Square, Oriented3)`
  stays `UnsupportedCombination` (square has only two families).

`Evidence::CoordinateHypotheses` (a decode-feedback roadmap slot) returns
`UnsupportedCombination` for detection. Coordinate hypotheses *are* consumable
through the separate [`check_consistency`] entry point, which scores
caller-proposed labels against a projective fit.

### Evidence × lattice support matrix

| Evidence | Square | Hex |
|---|---|---|
| `Positions` | ✅ (synthesize 2 axes) | ✅ (synthesize 3 axes, topological) |
| `Oriented1` | ✅ (synthesize 2nd axis) | ❌ `UnsupportedCombination` |
| `Oriented2` | ✅ (native, topological) | ❌ `UnsupportedCombination` |
| `Oriented3` | ❌ `UnsupportedCombination` | ✅ (native, topological) |

All square and hex paths run the same topological assembler — the sole grid
builder in the crate.

## Quickstart

A fully self-contained, image-free example: synthesize a small `3×3` grid,
wrap the features as evidence, detect, and read the recovered labels. (This is
the body of [`examples/hello_grid.rs`](examples/hello_grid.rs) —
`cargo run -p projective-grid --example hello_grid`.)

```rust
use nalgebra::Point2;
use projective_grid::{
    detect_grid, DetectionParams, DetectionRequest, Evidence, LatticeKind, LocalAxis,
    OrientedFeature, PointFeature,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Build a 3x3 grid of oriented features. The `+ j * 6.0` term adds a
    // mild perspective-style shear so this is a genuine projective grid.
    let mut features: Vec<OrientedFeature<2>> = Vec::new();
    for j in 0..3 {
        for i in 0..3 {
            let x = 60.0 + i as f32 * 40.0 + j as f32 * 6.0;
            let y = 50.0 + j as f32 * 40.0;
            let point = PointFeature::new(features.len(), Point2::new(x, y));
            // Two roughly-orthogonal local axes: horizontal and vertical.
            let axes = [
                LocalAxis::new(0.0, Some(0.02)),
                LocalAxis::new(std::f32::consts::FRAC_PI_2, Some(0.02)),
            ];
            features.push(OrientedFeature::new(point, axes));
        }
    }

    // Wrap as Oriented2 evidence and ask for a square lattice.
    let request = DetectionRequest::new(
        LatticeKind::Square,
        Evidence::Oriented2(&features),
        None, // grid dimensions unknown
        DetectionParams::default(),
    );

    let solution = detect_grid(request)?;
    for entry in &solution.grid.entries {
        // coord.u = i, coord.v = j; source_index maps back to the input slice.
        println!("(i={}, j={}) <- feature {}", entry.coord.u, entry.coord.v, entry.source_index);
    }
    Ok(())
}
```

Running it prints all nine features, labelled `(0,0)` through `(2,2)` with a
sub-pixel fit residual.

## Algorithm (square)

Square lattice detection uses the **topological** assembler — the sole grid
builder in the crate. It consumes [`Evidence::Oriented2`] (or synthesizes axes
from `Positions` / `Oriented1`) and produces a [`GridSolution`]:

- the Shu/Brunton/Fiala axis-driven grid finder (Delaunay triangulation + a
  per-cell axis test). Image-free; recovers dense grids and copes well with
  distortion. May return several components (see [`detect_grid_all`]).

**Hex** also uses the topological assembler. On a hex point lattice the Delaunay
triangles *are* the unit cells, so the diagonal/quad-merge stage is bypassed;
the axial `(q, r)` walk and the projective fit back-half are shared with the
square topological path.

## Inputs & outputs

**Inputs** are wrapped in an [`Evidence`] enum — see *Three kinds of evidence*
above. For square lattices `Positions`, `Oriented1`, and `Oriented2` are
supported; for hex lattices `Positions` and `Oriented3` are supported on the
topological path. The other combinations (`(Square, Oriented3)`,
`(Hex, Oriented1/Oriented2)`, and `CoordinateHypotheses` for detection) return
`UnsupportedCombination`.

**Output** is a [`GridSolution`]:

| Field | Meaning |
|---|---|
| `grid.entries: Vec<GridEntry>` | One labelled feature each. A [`GridEntry`] carries `coord` (the `(i, j)` label, rebased so the labelled bounding box starts at `(0, 0)`), `source_index` (back into your input slice), `image_position`, and `residual_px` (reprojection residual when a fit was computed). |
| `fit: Option<LatticeFit>` | The fitted model-plane-to-image projective transform plus a residual summary (`count`, `mean_px`, `max_px`). |
| `rejected: Vec<RejectedFeature>` | Features the detector could not place, each with a [`RejectionReason`] (`Unlabelled`, `ValidationDropped`, or `ResidualTooHigh`). |

## Learn more

Algorithm deep-dive and conceptual background:
[book chapter](https://vitalyvorobyev.github.io/calib-targets-rs/projective_grid.html).

## License

Licensed under either of MIT or Apache-2.0 at your option.

[`Float`]: https://docs.rs/projective-grid/latest/projective_grid/trait.Float.html
[`PointFeature`]: https://docs.rs/projective-grid/latest/projective_grid/feature/struct.PointFeature.html
[`OrientedFeature`]: https://docs.rs/projective-grid/latest/projective_grid/feature/struct.OrientedFeature.html
[`LocalAxis`]: https://docs.rs/projective-grid/latest/projective_grid/feature/struct.LocalAxis.html
[`Evidence`]: https://docs.rs/projective-grid/latest/projective_grid/detect/enum.Evidence.html
[`Evidence::Positions`]: https://docs.rs/projective-grid/latest/projective_grid/detect/enum.Evidence.html
[`Evidence::Oriented1`]: https://docs.rs/projective-grid/latest/projective_grid/detect/enum.Evidence.html
[`Evidence::Oriented2`]: https://docs.rs/projective-grid/latest/projective_grid/detect/enum.Evidence.html
[`detect_grid`]: https://docs.rs/projective-grid/latest/projective_grid/detect/fn.detect_grid.html
[`detect_grid_all`]: https://docs.rs/projective-grid/latest/projective_grid/detect/fn.detect_grid_all.html
[`check_consistency`]: https://docs.rs/projective-grid/latest/projective_grid/check/fn.check_consistency.html
[`GridSolution`]: https://docs.rs/projective-grid/latest/projective_grid/result/struct.GridSolution.html
[`GridEntry`]: https://docs.rs/projective-grid/latest/projective_grid/result/struct.GridEntry.html
[`RejectionReason`]: https://docs.rs/projective-grid/latest/projective_grid/result/enum.RejectionReason.html
