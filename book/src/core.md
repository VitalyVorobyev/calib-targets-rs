# calib-targets-core

`calib-targets-core` provides shared geometric types and utilities. It is intentionally small and purely geometric; it does not depend on any particular corner detector or image pipeline.

![Rectified grid view](img/rectified_small.png)
*Global rectification output from the chessboard pipeline.*

## Core data types

- `AxisEstimate`: one local grid-axis direction — an angle in `[0, π)`
  plus a 1σ angular uncertainty. A corner carries two of these as
  `axes: [AxisEstimate; 2]`; there is no single-orientation field
  (`Corner::orientation` was removed workspace-wide).
- `Coord`: integer grid indices `(u, v)` in board space, with `u`
  right and `v` down. Re-exported from `projective-grid`; it is the
  single canonical grid-coordinate type across the workspace.
- `LabeledCorner`: a detected corner with optional grid coordinates and
  logical ID — `position`, `grid`, `id`, `target_position`, `score`.
- The raw per-corner *input* type is detector-specific and lives in the
  detector crates (e.g. `calib_targets_chessboard::ChessCorner`), not in
  `calib-targets-core`.
- `TargetDetection`: a collection of labeled corners for one board instance.
- `TargetKind`: `Chessboard`, `Charuco`, `CheckerboardMarker`,
  `PuzzleBoard`, or `PuzzlePole`.

These types are shared across all detectors so downstream code can be target-agnostic.

Axis clustering (recovering the two global grid directions from per-corner
axes) is not part of core: it lives in `projective-grid`
(`projective_grid::expert::orientation::cluster_axes`); see
[Axis clustering](algo_axis_clustering.md).

## Homography and rectification

`Homography` is a small wrapper around a `3x3` matrix with helpers for DLT estimation and point mapping:

- `estimate_homography_rect_to_img`: DLT with Hartley normalization for N >= 4 point pairs.
- `homography_from_4pt`: direct 4-point estimation.
- `warp_perspective_gray`: warp a grayscale image using a homography.

For mapping rectified pixels back to the original image, core defines:

- `RectifiedView`: a rectified grayscale image and its mapping info.
- `RectToImgMapper`: either a single global homography or a per-cell mesh map.

Higher-level crates (notably chessboard) wrap these utilities for global or mesh rectification.

## Image utilities

`GrayImage` and `GrayImageView` are lightweight, row-major grayscale buffers with bilinear sampling helpers:

- `sample_bilinear`: float sampling with edge clamp to 0.
- `sample_bilinear_u8`: u8 sampling with clamping to 0..255.

These utilities are used by rectification and marker decoding.

## Conventions recap

- Coordinate system: origin at top-left, `x` right, `y` down.
- Grid coordinates: `i` right, `j` down, and **grid indices refer to corners**.
- Quad order: TL, TR, BR, BL in both source and destination spaces.

If you build on core types, stick to these conventions to avoid subtle alignment bugs.
