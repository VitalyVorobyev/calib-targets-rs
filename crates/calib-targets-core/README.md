# calib-targets-core

Shared types and geometric utilities for the [calib-targets] workspace.
Small, dependency-light, and purely geometric — it does not depend on any
concrete corner detector or image crate. Every detector in the workspace
(chessboard, ChArUco, marker board, PuzzleBoard) emits its result in terms
of these types.

Most users install [`calib-targets`][calib-targets] (the facade) rather
than this crate directly. Install `calib-targets-core` only if you are
writing a new detector or consuming detection results without the facade.

## Install

```toml
[dependencies]
calib-targets-core = "0.8"
nalgebra = "0.34"
```

## Types you will see

| Type | Role |
|---|---|
| [`AxisEstimate`] | One local grid-axis direction: an angle in `[0, π)` plus a 1σ uncertainty. Corners carry two. |
| [`LabeledCorner`] | A detected corner with grid label: `position`, `grid: Option<(i, j)>`, `id`, `target_position`, `score`. The common detector output. |
| [`TargetDetection`] | `{ kind: TargetKind, corners: Vec<LabeledCorner> }`. Uniform wrapper across all detector types. |
| [`TargetKind`] | `Chessboard`, `ChArUco`, `PuzzleBoard`, `CheckerboardMarker`. Non-exhaustive. |
| [`GridCoords`] | Integer `(i, j)` grid index, with `i` right, `j` down. Labels are always rebased so that the bounding-box minimum sits at `(0, 0)`. |
| [`GridAlignment`] / [`GridTransform`] | Dihedral-group D4 (8 transforms) aligning a detected grid to a board-fixed coordinate system. |

## Utilities

- [`homography_from_4pt`] + [`Homography`] — 4-point DLT solver with
  Hartley normalisation.
- [`warp_perspective_gray`] — grayscale perspective warp.
- [`sample_bilinear`] / [`sample_bilinear_fast`] / [`sample_bilinear_u8`] —
  subpixel sampling helpers on a [`GrayImageView`].
- [`cluster_orientations`] + [`OrientationClusteringParams`] — axis-angle
  histogram clustering used by the chessboard detector.
- [`ChessConfig`] + friends — shared ChESS corner-detector configuration
  struct, consumed by every higher-level detector through
  `DetectorParams::chess`.

## Coordinate conventions

- **Image pixels.** Origin at top-left; `x` right, `y` down. Pixel centre
  sampling uses `(x + 0.5, y + 0.5)`.
- **Grid indices.** `(i, j)` with `i` right, `j` down. Grid labels are
  non-negative — every detector rebases the bounding-box minimum to
  `(0, 0)`.
- **Quad / homography corner order.** `TL, TR, BR, BL` (clockwise). Never
  self-crossing.
- **Corner orientation.** A corner's `axes: [AxisEstimate; 2]` holds two
  ordered axis angles with `axes[1] − axes[0] ≈ π/2`. The CCW sweep from
  `axes[0]` to `axes[1]` crosses a dark sector. See the workspace
  [conventions chapter][conv]. (The raw per-corner input type lives in
  the detector crates — e.g. `calib_targets_chessboard::ChessCorner` —
  not here; `calib-targets-core` carries only the shared output types.)

## Quickstart

```rust
use calib_targets_core::{GridCoords, LabeledCorner, TargetDetection, TargetKind};
use nalgebra::Point2;

let corner = LabeledCorner {
    position: Point2::new(10.0, 20.0),
    grid: Some(GridCoords { i: 0, j: 0 }),
    id: None,
    target_position: None,
    score: 1.0,
};

let detection = TargetDetection {
    kind: TargetKind::Chessboard,
    corners: vec![corner],
};

println!(
    "{:?} {}",
    detection.corners[0].position,
    detection.corners.len(),
);
```

## Links

- [Workspace facade `calib-targets`][calib-targets] — the crate most users
  install.
- [Book: conventions chapter][conv]
- [Book: understanding detection output][output]

[calib-targets]: https://docs.rs/calib-targets
[conv]: https://vitalyvorobyev.github.io/calib-targets-rs/conventions.html
[output]: https://vitalyvorobyev.github.io/calib-targets-rs/output.html
