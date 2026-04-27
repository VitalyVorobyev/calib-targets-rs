# projective-grid (Standalone)

> Code: [`projective-grid`](https://github.com/VitalyVorobyev/calib-targets-rs/tree/main/crates/projective-grid).

`projective-grid` is the pattern-agnostic core of the workspace's
grid detectors. It provides the algorithmic pieces every grid-
detection pipeline needs — seed-and-grow BFS, boundary extension via
fitted homography, per-cell rectification, circular-statistics peak
picking, line / local-homography validation — with no dependency on
calibration-specific types.

You can consume it directly for non-calibration use cases: rectifying
a photograph of a board game, fitting a locally-planar lattice to a
laser-dot cloud, extracting a grid from a scanned document, or
building a new detector for a pattern the workspace doesn't yet ship.

---

## Pipeline

The crate centres on a five-stage pipeline. Pattern-specific gates
(parity, axis-cluster, marker rules, …) plug in via the
`square::grow::GrowValidator` trait; the geometric machinery is
generic.

| Stage | Entry points | What it does |
|---|---|---|
| **Cell-size estimate** | `estimate_global_cell_size`, `estimate_local_steps` | Infer approximate lattice spacing from a raw point cloud. |
| **Seed-and-grow** | `square::grow::bfs_grow` + `GrowValidator` | BFS from a 2×2 seed quad, predicting each next cell with adaptive per-neighbour local-step. |
| **Boundary extension** | `square::grow_extension::extend_via_global_homography` | Fit a global H over the BFS-validated set; extend outward into perspective-foreshortened territory. Residual gate disables the pass under heavy lens distortion. |
| **Validation** | `square::validate` | Line collinearity + local-homography residuals → blacklist of outlier corners; iterate Stages 5–7 until convergence. |
| **Rectification** | `square::rectify::SquareGridHomography`, `square::mesh::SquareGridHomographyMesh`, hex equivalents | Single global homography or per-cell mesh, depending on distortion regime. |

Reusable utilities:

- **Circular statistics** (`circular_stats`) — plateau-aware peak
  detection and double-angle 2-means for axis-angle histograms.
- **Homography** (`homography`) — 4-point + DLT solver with Hartley
  normalisation and a reprojection-quality diagnostic.

---

## Extension point: `GrowValidator`

```rust,ignore
use projective_grid::square::grow::{Admit, GrowValidator, LabelledNeighbour};
use nalgebra::Point2;

impl GrowValidator for MyValidator {
    fn is_eligible(&self, idx: usize) -> bool { /* … */ }
    fn required_label_at(&self, i: i32, j: i32) -> Option<u8> { /* … */ }
    fn label_of(&self, idx: usize) -> Option<u8> { /* … */ }

    fn accept_candidate(
        &self,
        idx: usize,
        at: (i32, i32),
        prediction: Point2<f32>,
        neighbours: &[LabelledNeighbour],
    ) -> Admit {
        // Accept / Reject per candidate in order of increasing
        // distance to `prediction`.
    }

    fn edge_ok(
        &self,
        candidate_idx: usize,
        neighbour_idx: usize,
        at_candidate: (i32, i32),
        at_neighbour: (i32, i32),
    ) -> bool { /* soft per-edge check */ true }
}
```

The same validator is used by `bfs_grow` (Stage 5) and
`extend_via_global_homography` (Stage 6) — so parity, axis-matching,
and edge invariants are enforced identically across both paths.

The chessboard detector's plug-in
(`crates/calib-targets-chessboard/src/grow.rs`) is the reference
implementation: chess-specific axis-slot logic on top of the generic
BFS / boundary-extension machinery.

---

## Module layout

```
projective-grid/src/
├── lib.rs
├── float_helpers.rs          (private)
├── global_step.rs            cell-size estimation from a raw cloud
├── local_step.rs             per-region local-step estimation
├── homography.rs             Homography, HomographyQuality, 4pt + DLT
├── circular_stats.rs         wrap_pi, smooth_circular_5, pick_two_peaks,
│                             refine_2means_double_angle
├── affine.rs                 AffineTransform2D (generic 2D)
├── component_merge.rs        merge_components_local
├── square/                   4-connected square-grid support
│   ├── alignment.rs          D4 transforms
│   ├── grow.rs               GrowValidator, bfs_grow, GrowResult
│   ├── grow_extend.rs        extend_from_labelled (post-cluster boost)
│   ├── extension/            Stage 6 — global / local homography
│   │   ├── common.rs         try_attach_at_cell (shared per-cell ladder)
│   │   ├── global.rs         extend_via_global_homography
│   │   └── local.rs          extend_via_local_homography
│   ├── index.rs              GridCoords (i, j)
│   ├── mesh.rs               SquareGridHomographyMesh (per-cell)
│   ├── rectify.rs            SquareGridHomography (global)
│   ├── seed/                 2×2 seed primitives + finder
│   │   ├── mod.rs            Seed, SeedOutput, midpoint check
│   │   └── finder.rs         find_quad, SeedQuadValidator
│   ├── smoothness.rs         square_predict_grid_position,
│   │                         square_find_inconsistent_corners
│   └── validate/             post-grow validation
│       ├── mod.rs            validate(), LabelledEntry, ValidationParams
│       ├── lines.rs          line collinearity flags
│       ├── local_h.rs        local-H residual
│       └── step.rs           per-corner step + step-deviation flags
├── topological/              Shu/Brunton/Fiala 2009 grid finder
│   ├── mod.rs                build_grid_topological, AxisHint
│   ├── classify.rs           edge classification
│   ├── delaunay.rs           triangulation wrapper
│   ├── quads.rs              triangle-pair → quad merge
│   ├── topo_filter.rs        topological + geometric filter
│   └── walk.rs               flood-fill (i, j) labelling
└── hex/                      6-connected hex-grid (geometry only,
    ├── alignment.rs           no seed-and-grow path yet)
    ├── mesh.rs
    ├── rectify.rs
    └── smoothness.rs
```

---

## Invariants worth keeping in mind

### Undirected-angle circular means

When averaging axis directions (orientations, not headings), accumulate
`(cos 2θ, sin 2θ)` and halve the resulting atan2. `circular_stats::
refine_2means_double_angle` does this correctly; naive `(cos θ, sin θ)`
averaging silently breaks at the 0°/180° seam.

### Plateau-aware peak detection

When a physical direction's mass straddles a histogram bin boundary,
the smoothed peak is flat-topped across two adjacent bins. Naive
strict local-maximum detection misses it entirely. `circular_stats::
pick_two_peaks` handles this by looking for maximal runs of equal-
valued bins bordered on both sides by strictly lower values, and
returning the plateau's midpoint.

### Non-negative grid labels with visual top-left origin

All `(i, j)` output from `bfs_grow` is rebased so the bounding-box
minimum is `(0, 0)`. Downstream consumers that canonicalise axis
direction (the chessboard detector does this in
`calib_targets_chessboard::Detector::detect`) additionally swap /
flip axes so `(0, 0)` sits at the **visual top-left** of the detected
grid — `+i` points right (+x), `+j` points down (+y). This is not
enforced by `bfs_grow` itself — it's a pattern-side contract.

### Stage 6 is precision-safe

Boundary extension via global H goes through *every* gate the BFS uses
— `is_eligible`, `label_of` against `required_label_at`,
`accept_candidate`, and `edge_ok` — plus a tighter ambiguity gate
(2.5× vs BFS's 1.5×) and a single-claim guarantee (one corner index
can only be claimed by one cell per pass). Plus the H-residual gate:
the median / max reprojection residual on the BFS-validated set must
be small (in pixel units) for the H to be trusted; under heavy lens
distortion the gate fires and Stage 6 becomes a no-op.

---

## When not to use this

- **3D grids.** Coordinates are `nalgebra::Point2<f32>`. There's no
  3D support.
- **Non-planar surfaces.** Stage 6 assumes a single planar
  homography fits the labelled set. Severely curved surfaces need the
  mesh variant for rectification, and Stage 6 will refuse to extend.
- **Dense point clouds without structure.** The seed finder assumes
  the lattice spacing is recoverable from the seed's own edge
  lengths; pure noise won't yield a stable seed.
