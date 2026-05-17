# Regular Grid Detection Example

Reference: `crates/projective-grid/examples/regular_grid.rs` — the
zero-config story for the standalone [`projective-grid`](projective_grid.md)
crate: a bare point cloud goes in, a labelled `(i, j)` grid comes out,
with no validator scaffolding and no image.

---

## Quick run

```bash
cargo run -p projective-grid --example regular_grid
```

The example synthesizes its own input (no image files needed): a 6×5
lattice pushed through a fixed perspective homography, so the cloud
looks like a board photographed at an angle.

---

## Walkthrough

### 1. Synthesize a point cloud

`projective-grid` knows nothing about images — it works on
`&[nalgebra::Point2<f32>]`. The example builds 30 points by warping a
clean integer lattice through a 3×3 homography. In a real application
these points would come from a corner detector, a blob detector, or a
laser-dot extractor.

### 2. One call: `detect_regular_grid`

```rust
use nalgebra::Point2;
use projective_grid::detect_regular_grid;

let mut points = Vec::new();
for j in 0..4 {
    for i in 0..5 {
        points.push(Point2::new(i as f32 * 30.0, j as f32 * 30.0));
    }
}
let grid = detect_regular_grid(&points).expect("clean grid detects");
assert_eq!(grid.points.len(), 20);
```

`detect_regular_grid` is equivalent to
`RegularGridDetector::default().detect(points)`. Internally it
estimates the cell size and the two grid-axis directions from the
cloud's nearest-neighbour offsets, drives the generic seed → grow →
extend → fill → validate pipeline with a built-in permissive
regular-grid policy, and cleans up the output (connectivity prune,
top-left canonicalise, `(j, i)` sort).

### 3. Handle the `Result`

Detection returns `Result<RegularGridDetection, RegularGridError>`.
The example matches every failure variant explicitly:

- `TooFewPoints { found }` — fewer than 4 points (the minimum for a
  2×2 seed quad).
- `DegeneratePointCloud` — coincident points, or no measurable
  spread; the grid-axis estimator found nothing usable.
- `NoGridFound` — a usable axis estimate, but no roughly-square seed
  quad could be assembled.

A subtle point worth knowing: a **collinear but uniformly spaced**
cloud is *not* flagged `DegeneratePointCloud`. It survives axis
estimation and fails later as `NoGridFound`. The enum is
`#[non_exhaustive]`, so callers always need a wildcard arm.

### 4. Read the result

A `RegularGridDetection` carries:

- `points: Vec<DetectedGridPoint>` — each with its rebased `(i, j)`
  label, pixel `position`, and `source_index` back into the input
  slice. Sorted row-major (`(j, i)`).
- `cell_size: f32` — the inferred mean lattice spacing in pixels.
- `axis_i` / `axis_j: Vector2<f32>` — unit vectors for the `+i` /
  `+j` grid directions in pixel space.
- `stats: RegularGridStats` — per-stage counters (`input_points`,
  `components_found`, `labelled_before_prune`, `pruned_disconnected`,
  `dropped_by_validation`, `canonicalized`).

`RegularGridDetection::labelled_map()` rebuilds the
`(i, j) → source_index` lookup when random access is more convenient
than the sorted vector.

---

## Going further

- **Tuning** — `crates/projective-grid/examples/regular_grid_tuning.rs`
  shows `RegularGridParams`: the `prune_disconnected` /
  `canonicalize_top_left` toggles and the `ExtensionStrategy`
  (`Disabled` / `Global` / `Local`) boundary-extension selector.
- **Multiple grids** —
  `crates/projective-grid/examples/multi_component.rs` uses
  `RegularGridDetector::detect_all` to recover several disjoint
  lattices from one cloud, one `RegularGridDetection` per board.

See the [`projective-grid` chapter](projective_grid.md) for the full
pipeline description and the advanced validator-driven entry point
`detect_square_grid`.
