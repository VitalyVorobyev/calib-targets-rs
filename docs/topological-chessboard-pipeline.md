# Topological Chessboard Detection Pipeline

This note describes the current topological chessboard workflow in this
workspace. It is deliberately implementation-facing: it names the crates and
functions that own each stage.

## Scope

There are two related pipelines:

- `projective-grid`: image-free grid construction from corner positions plus
  per-corner `AxisHint`s.
- `calib-targets-chessboard`: chessboard-specific detection from ChESS corners,
  including input filtering, recovery boosters, canonicalisation, and final
  `Detection` construction.

The topological chessboard path is opt-in through:

```rust
DetectorParams {
    graph_build_algorithm: GraphBuildAlgorithm::Topological,
    ..DetectorParams::default()
}
```

`ChessboardV2` remains the default graph builder. ChArUco keeps using the
seed-and-grow path because marker-internal ChESS corners create structured
clutter whose axes do not describe the chessboard grid.

## 1. Image Preprocessing And ChESS Corners

Entry point:

- Rust: `calib_targets::detect::detect_corners`
- Python: `calib_targets.detect_chessboard(..., chess_cfg=...)`

Input is a grayscale image. `ChessConfig.pre_blur_sigma_px` can apply an
explicit same-size Gaussian blur before ChESS corner extraction. The default is
`0.0`, so existing behavior is unchanged. This option is useful for low
resolution synthetic images where local axes are noisy.

`ChessConfig` is workspace-owned rather than a direct alias of
`chess_corners::ChessConfig` because the workspace default is intentionally
different: relative threshold `0.2` instead of upstream absolute threshold
`0.0`. It is also the serde/Python/FFI contract for this workspace. The other
low-level ChESS config types are re-exported from `chess-corners`.

## 2. Chessboard Input Adapter

Owner:

- `crates/calib-targets-chessboard/src/topological/inputs.rs`

The adapter converts `calib_targets_core::Corner` into the image-free
`projective-grid` input format:

- `positions: Vec<Point2<f32>>`
- `axes: Vec<[AxisHint; 2]>`

It applies the same strength and fit-quality prefilter used by ChessboardV2.
Corners that fail this gate keep their original position, but their axes are
replaced with `[AxisHint::default(); 2]`. This preserves corner indices for
traces while preventing weak corners from classifying Delaunay edges.

## 3. Projective-Grid Core

Owner:

- `projective_grid::topological`

Public entry points:

- `build_grid_topological`
- `build_grid_topological_trace`

The core is image-free. It does not sample intensities and does not fit a
global homography.

The stages are:

1. **Axis-sigma usable filter**
   A corner is usable if at least one axis has
   `sigma < TopologicalParams::max_axis_sigma_rad`.

2. **Delaunay triangulation**
   `delaunator` builds a sparse candidate graph from all input positions.
   Unusable corners remain in the point set, but their incident edges classify
   as spurious.

3. **Half-edge classification**
   Each directed Delaunay edge is classified at both endpoints as `Grid`,
   `Diagonal`, or `Spurious`.

   Current defaults:

   - `axis_align_tol_rad = 22 deg`
   - `diagonal_angle_tol_rad = 18 deg`

   The edge kind is accepted only when both endpoints agree.

4. **Triangle diagnostics**
   Each triangle is bucketed as `Mergeable`, `AllGrid`, `MultiDiagonal`, or
   `HasSpurious`. These counters are often the fastest way to identify why a
   case failed.

5. **Triangle-pair merge**
   Two triangles merge into one quad only when both triangles have exactly one
   diagonal edge and the diagonal half-edges are buddies. This is why one
   physical cell with only three detected corners cannot seed a topological
   component.

6. **Quad filters**
   The topological filter rejects quads with two or more illegal vertices.
   A vertex is illegal when its quad-mesh incidence exceeds the regular-grid
   degree.

   The geometry filter is the paper-style opposing-edge ratio gate
   (`edge_ratio_max`, default `10.0`). It is not a neighboring-cell local
   consistency check.

7. **Topological walk**
   The filtered quad mesh is walked component by component. One seed quad gets
   labels `(0,0)`, `(1,0)`, `(1,1)`, `(0,1)`, and neighboring quads propagate
   labels through shared edges. Each component is rebased so its minimum label
   is `(0,0)`.

## 4. Chessboard-Specific Recovery

Owner:

- `crates/calib-targets-chessboard/src/topological/recovery.rs`

The projective-grid core stops at labelled components. The chessboard wrapper
then reuses existing chessboard machinery:

1. Run `merge_components_local` on the raw topological components.
2. Build `CornerAug` records and run orientation clustering.
3. Parity-align topological labels against cluster labels when clustering is
   available.
4. Mark the current labelled component and run `apply_boosters`.
5. Use the larger directional median cell size for booster recovery. The final
   reported `cell_size` still uses the conservative all-edge median.
6. Merge boosted components by shared corner identity when enough overlap
   exists.
7. Run `merge_components_local` again after boosters.
8. Build final `Detection` objects through the same canonicalisation path used
   by the seed-and-grow detector.
9. Sort detections by labelled count and cap by `max_components`.

This recovery layer is chessboard-specific because it depends on parity,
orientation clusters, `CornerAug`, `GrowResult`, and the chessboard booster
stack. It is not currently promoted into `projective-grid`.

## 5. Tracing And Performance Measurement

The timing surface is `tracing`, not a second public timed API. Enable the
workspace `tracing` features and run:

```bash
cargo run --release -p calib-targets-bench --bin topo_stage_timing -- \
  --image-dir testdata/02-topo-grid \
  --out tools/out/topo-grid-performance/stage-breakdown.json \
  --repeats 20 \
  --warmup 3
```

The benchmark records spans for:

- ChESS corner detection
- input adaptation
- axis-sigma filter
- triangulation
- edge classification
- triangle merge
- topological quad filter
- geometry quad filter
- topological walk
- initial component merge
- orientation clustering
- recovery
- final ordering/canonicalisation

## 6. Known Limits

The current topological core requires complete quad cells. It cannot create a
cell from only three detected corners. That is a real recall limitation under
occlusion.

Delaunay triangulation is not projective invariant. Heavy perspective combined
with radial distortion can make Delaunay triangles span multiple physical
cells, which starves the one-diagonal-per-cell merge stage.

The method also depends on usable local axes. Low-resolution images with noisy
axes may fail before topology has enough reliable evidence. Explicit blur can
improve ChESS axis estimates in these cases; it remains opt-in.
