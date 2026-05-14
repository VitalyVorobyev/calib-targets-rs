---
title: "Grid detection I: topology"
date: 2026-05-15
summary: "How Delaunay triangulation and mesh filtering can turn detected chessboard corners into an ordered grid."
tags: ["feature-detection", "calibration-targets"]
author: "Vitaly Vorobyev"
repoLinks: ["https://github.com/VitalyVorobyev/calib-targets-rs"]
relatedAlgorithms: []
draft: true
relatedDemos: ["delaunay-voronoi"]
difficulty: intermediate
---

# Introduction

Regular grid detection is a backbone of many computer vision tasks: camera
calibration, pose estimation, metrology, industrial inspection, and board-game
analysis. This is a large topic, so this post starts with one specific problem.

Assume that chessboard-like X-corners are already detected in the image. Some
corners may be missing, and some detections may be false positives. The task is
to recover the grid structure: connect neighboring corners, reject inconsistent
detections, and assign integer grid coordinates to the connected points.

The core idea in this post is topological. Build a local candidate graph from
the corner cloud, recover quadrilateral cells, filter the cell mesh, and only
then assign `(i, j)` coordinates. The method follows Shu, Brunton, and Fiala's
topological checkerboard paper, with one important implementation difference:
`projective-grid` is image-free at this stage. It uses the two local ChESS axes
stored at each corner instead of sampling cell color from the image.

That distinction matters. The topological builder in `projective-grid` consumes
only this:

- corner positions in image pixels,
- two undirected local axis estimates per corner,
- a small `TopologicalParams` configuration.

It does not read image intensities, and it does not start from one global
homography.

# Why ordering matters

A detected grid is useful only after its points are ordered. For calibration or
pose estimation, we need correspondences between image points and known points
on the physical target.

For a planar target, these correspondences are especially useful. If lens
distortion is ignored, points on the target plane and points in the image are
related by a homography. Zhang-style calibration uses homographies from several
target views to initialize camera intrinsics. If the camera is already
calibrated, the same correspondences can estimate target pose.

But a homography is the result of grid detection, not a safe starting point.
With occlusion, false corner detections, local blur, and visible lens
distortion, one global model can become fragile. The topological approach first
tries to recover local connectivity, then lets later stages validate or merge
the result.

# The grid as a graph

Assume we have a set of candidate X-junctions from a ChESS-style detector.
Now the problem becomes combinatorial:

- detected corners are nodes,
- neighboring chessboard corners are grid edges,
- a cell is a quadrilateral,
- a recovered board is a connected component with regular grid topology.

Different detectors make different first assumptions. Classical OpenCV-style
detectors often start from black-square segmentation. Other chessboard
detectors use local corner responses and then recover the whole board. Shu,
Brunton, and Fiala take the route discussed here: Delaunay triangulation,
triangle merging, quad-mesh filtering, and topological walking.

In `projective-grid`, this route is exposed as
`projective_grid::build_grid_topological`. It is also available through the
chessboard detector by setting
`DetectorParams::graph_build_algorithm = GraphBuildAlgorithm::Topological`.
The current default in `calib-targets-chessboard` is still `ChessboardV2`, the
seed-and-grow pipeline. ChArUco pins that default internally because marker
interiors produce extra ChESS corners whose axes describe the marker bits, not
the chessboard grid.

# Stage 0: usable corners

The topological builder does not trust every corner equally. Each corner has two
axis estimates, and each axis has an angular uncertainty `sigma`. A corner is
usable for edge classification if at least one of its axes has
`sigma < max_axis_sigma_rad`. The default threshold is `0.6 rad`, about 34
degrees.

The blog overlays show this first because it explains many failures. A corner
can be visually present, but if both local axes are too uncertain, the
topological classifier treats every incident edge as spurious.

Example overlay:

![usable corners](img/02-topo-grid/GeminiChess1/02-usable-corners.png)

# Stage 1: Delaunay as a candidate graph

Shu, Brunton, and Fiala start from Delaunay triangulation of the detected
corners.

:::definition[Delaunay triangulation]
Delaunay triangulation connects a set of points into triangles such that no
point lies inside the circumcircle of any triangle.
:::

For a regular grid, Delaunay usually contains the true neighbor connections
plus one diagonal inside each cell. This makes it a useful candidate graph:
wide enough to contain the right local edges, but not dense enough to connect
everything to everything.

There is an important limitation. Delaunay triangulation is not projective
invariant. Under heavy perspective and radial distortion, a Delaunay triangle
can span more than one physical cell. In `projective-grid`, this shows up as a
rise in the `triangles_all_grid` diagnostic: all three triangle edges look like
grid edges, so there is no unique cell diagonal to merge across.

Example overlay:

![delaunay edge kinds](img/02-topo-grid/GeminiChess1/03-delaunay-edge-kinds.png)

# Stage 2: classify edges

The original paper uses image intensity to decide which neighboring triangles
belong to the same black or white square. `projective-grid` uses corner axes
instead.

For every Delaunay half-edge `(a -> b)`, compute the edge angle
`theta = atan2(y_b - y_a, x_b - x_a)`. At each endpoint, compare that angle to
the two local axes modulo `pi`, because grid axes are undirected.

The edge is classified as:

- `Grid` if both endpoints see it as close to one local axis,
- `Diagonal` if both endpoints see it as close to 45 degrees from the nearest
  local axis,
- `Spurious` otherwise.

The current default tolerances are 22 degrees for grid alignment and 18 degrees
for diagonal alignment. The whole-edge classification is a conjunction. If one
endpoint does not agree, the edge is spurious.

This is the main practical difference from the paper. It keeps the crate
standalone and makes the stage independent of image sampling, but it also means
the method depends on good local axis estimates.

# Stage 3: from triangles to cells

Delaunay gives triangles, but a chessboard grid is made of quadrilateral cells.
For a normal cell, the triangulation gives two triangles separated by one
diagonal. Therefore a triangle is mergeable only when it has exactly one
`Diagonal` edge and two `Grid` edges.

For each mergeable triangle, the implementation looks up the neighboring
triangle across the diagonal half-edge. If the neighbor is also mergeable
through the same shared diagonal, the two triangles are merged into one quad.
The quad vertices are stored in image order: top-left, top-right,
bottom-right, bottom-left.

Example overlay:

![mergeable triangles](img/02-topo-grid/GeminiChess1/04-mergeable-triangles.png)

The trace API reports the triangle composition counters:

- `triangles_mergeable`,
- `triangles_all_grid`,
- `triangles_multi_diag`,
- `triangles_has_spurious`.

These counters are useful because they describe the failure mode. If
`has_spurious` dominates, the corner cloud is noisy or axes are unreliable. If
`all_grid` rises in one region, the view is probably too distorted for the
strict one-diagonal-per-cell assumption.

# Stage 4: filter the quad mesh

After triangle merging, we have candidate quadrilateral cells. Some are correct,
some are artifacts. The implementation applies two filters.

First comes the topological filter from the paper. A regular grid corner can
have at most four distinct grid edges incident to it. In the quad mesh this is
counted as perimeter-edge incidence, so an interior corner has eight half-edge
incidences. Corners above that are illegal. A quad is removed if it has two or
more illegal corners.

Example overlay:

![topology filter](img/02-topo-grid/GeminiChess1/06-topology-filter.png)

Second comes the geometric filter. This is the loose paper-style opposing-edge
ratio check, not a neighboring-cell consistency check. For each quad, compute
the two ratios between opposite side lengths. If either ratio is greater than
`edge_ratio_max`, the quad is rejected. The default is `10.0`, deliberately
large. It removes pathological parallelograms, not merely perspective-skewed
cells.

Example overlay:

![geometry filter](img/02-topo-grid/GeminiChess1/07-geometry-filter.png)

# Stage 5: walk the mesh

Filtered quads are still not enough. We need labels.

The implementation walks each connected component of the quad mesh. It chooses
one seed quad and assigns:

```text
TL = (0, 0)
TR = (1, 0)
BR = (1, 1)
BL = (0, 1)
```

Then it traverses neighboring quads through shared perimeter edges. The shared
corners keep their labels; the two new corners receive the integer cell step
perpendicular to the shared edge. This is pure mesh topology. Pixel positions
are not used to fit lines during this stage.

After walking, each component is rebased so `min(i) = 0` and `min(j) = 0`.
The chessboard wrapper then runs `merge_components_local`, the same
local-geometry component merger used by the seed-and-grow pipeline.

Example overlays:

![walk labels](img/02-topo-grid/GeminiChess1/08-walk-labels.png)

![final grid](img/02-topo-grid/GeminiChess1/09-final-grid.png)

# Stage 6: chessboard recovery and ordering

The image-free `projective-grid` core stops at walked components. The
chessboard detector adds one more layer before returning a public
`Detection`.

First, it builds the same `CornerAug` records used by `ChessboardV2` and runs
orientation clustering. When cluster labels are available, the topological
labels are parity-aligned to them. This is necessary because the recall
boosters assume the chessboard parity convention used by the seed-and-grow
pipeline.

Then each topological component is passed through the existing booster stack:
line extrapolation, interior gap fill, component merge support, and weak
cluster rescue. For recovery, the code uses the larger directional median cell
size under perspective; the final reported `cell_size` still uses the
conservative median over all labelled neighbor distances.

Boosted components may overlap even when the raw topological components did
not. For this reason, the wrapper merges by shared corner identity, then runs
`merge_components_local` again. Only after this step does it build final
detections, canonicalise their labels, sort by labelled count, and apply
`max_components`.

The low-resolution recovery example is visible here:

![GeminiChess4 low-res recovery](img/02-topo-grid/GeminiChess4-low-res-chessboard-v2/09-final-grid.png)

# Using the library

The low-level Rust API is a corner-cloud API. It expects the caller to provide
positions and axes:

```rust
use nalgebra::Point2;
use projective_grid::{build_grid_topological, AxisHint, TopologicalParams};

fn label_grid(
    positions: &[Point2<f32>],
    axes: &[[AxisHint; 2]],
) -> Result<(), Box<dyn std::error::Error>> {
    let grid = build_grid_topological(positions, axes, &TopologicalParams::default())?;

    for component in &grid.components {
        for ((i, j), corner_idx) in &component.labelled {
            println!("corner {corner_idx} -> ({i}, {j})");
        }
    }

    Ok(())
}
```

For image-to-grid detection from Python, use the high-level package and opt in
to the topological graph builder:

```python
import numpy as np
from PIL import Image
import calib_targets as ct

image = np.asarray(Image.open("board.png").convert("L"), dtype=np.uint8)
params = ct.ChessboardParams(graph_build_algorithm="topological")

result = ct.detect_chessboard(image, params=params)
if result is not None:
    print(len(result.detection.corners), "labelled corners")
```

The blog figures were generated by:

```bash
uv run --python .venv/bin/python scripts/render_topological_blog_overlays.py \
  --image-dir testdata/02-topo-grid \
  --out-dir docs/img/02-topo-grid
```

The script writes ten overlays per image and a manifest at
`docs/img/02-topo-grid/manifest.json`.

The low-resolution `GeminiChess4` recovery plot was generated with the
explicit blur and ChessboardV2 final workflow:

```bash
uv run --python .venv/bin/python scripts/render_topological_blog_overlays.py \
  --image-dir testdata/02-topo-grid \
  --out-dir docs/img/02-topo-grid \
  --only GeminiChess4 \
  --variant-name low-res-chessboard-v2 \
  --manifest-name manifest-geminichess4-low-res.json \
  --final-algorithm chessboard_v2 \
  --pre-blur-sigma 2.0
```

# Performance

These measurements were taken on an Apple M4 Pro, commit `70ab0cb`,
`rustc 1.93.0`, release build. Stage timings come from `tracing` spans compiled
with the workspace tracing features, not from a separate timed detector path.

```bash
cargo run --release -p calib-targets-bench --bin topo_stage_timing -- \
  --image-dir testdata/02-topo-grid \
  --out tools/out/topo-grid-performance/stage-breakdown.json \
  --repeats 20 \
  --warmup 3
```

This table measures full image detection with the topological graph builder:
grayscale image already decoded, ChESS corner detection, input adaptation,
topological graph build, recovery, and final detection assembly.

| image | resolution | ChESS corners | labelled | ChESS ms | grid ms | full ms |
|---|---:|---:|---:|---:|---:|---:|
| GeminiChess1 | 800x436 | 112 | 53 | 1.14 | 0.13 | 1.27 |
| GeminiChess2 | 1392x754 | 28 | 26 | 2.60 | 0.13 | 2.73 |
| GeminiChess3 | 1402x761 | 68 | 42 | 2.41 | 0.09 | 2.50 |
| GeminiChess4 | 1024x820 | 42 | 0 | 1.88 | 0.01 | 1.89 |
| gptchess1 | 692x551 | 70 | 60 | 1.05 | 0.11 | 1.16 |
| gptchess2 | 694x556 | 47 | 0 | 1.00 | 0.01 | 1.01 |

For successful cases, the topological chessboard dispatch is around
0.09-0.13 ms. ChESS corner extraction dominates the full image time.

The detailed stage report is in
`tools/out/topo-grid-performance/stage-breakdown.md`.

The grid builder itself is much smaller than the full image detector. The
following Criterion run measures only `build_grid_topological` on synthetic
corner clouds:

```bash
cargo bench -p projective-grid --bench topological -- \
  --sample-size 20 --warm-up-time 1 --measurement-time 2
```

| workload | input corners | mean time |
|---|---:|---:|
| clean 10x10 | 100 | 66.97 us |
| clean 20x20 | 400 | 296.53 us |
| clean 40x40 | 1600 | 1.31 ms |
| clean 60x60 | 3600 | 3.40 ms |
| noisy 20x20 + 50% | 600 | 290.18 us |

The two tables answer different questions. The full detector scales with image
resolution because ChESS corner extraction scans pixels. The grid-only
benchmark scales mainly with the number of corner candidates and the Delaunay
triangulation / mesh stages.

# Practical limits

The topological path works best when false detections are sparse or
unstructured. That is often true for plain chessboards and PuzzleBoard-like
targets: wrong corners may appear, but they rarely form a consistent quad mesh.

The harder case is structured clutter inside the target. ChArUco markers are a
good example. Marker interiors contain many extra X-like corners, and their
local axes are valid for the marker pattern rather than the chessboard grid.
For this reason, ChArUco uses the seed-and-grow path today.

The second hard case is heavy perspective combined with radial distortion.
Delaunay can create triangles that span multiple cells, and the strict
one-diagonal-per-cell merge rule then starves. This is a known recall gap in
the topological path. The current trace API makes that failure visible through
`triangles_all_grid`, `triangles_has_spurious`, and the stage overlays.

There is also a more basic structural limit: the topological core walks a quad
mesh, so one physical cell with only three detected corners cannot create a
quad by itself. A homography-based or cross-ratio recovery step could address
that class of recall problem, but it is not part of the current
`projective-grid` core.

Finally, low-resolution images can fail before topology starts because the
local ChESS axes are noisy. This is why the low-resolution regression variants
use explicit `ChessConfig.pre_blur_sigma_px`; it is an opt-in preprocessing
choice, not a hidden default.

# Summary

The topological grid finder starts from corner positions and local axes, builds
a Delaunay candidate graph, classifies edges, merges triangle pairs into cell
quads, filters the quad mesh, and walks the surviving mesh to assign integer
grid labels.

The strength of the method is that it does not start from one global
homography or straight-line model. It recovers local grid structure first,
which makes it useful for partial targets and images where a single global
model would be premature.

The cost is that the method depends on the Delaunay graph and reliable local
axes. When those assumptions break, the trace now shows where: unusable axes,
spurious edge classifications, missing mergeable triangles, rejected quads, or
empty walked components. That is the practical reason for adding the trace API:
the algorithm is easier to explain when every intermediate decision can be
seen.
