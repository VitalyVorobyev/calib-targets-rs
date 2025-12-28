# calib-targets-core

`calib-targets-core` provides shared geometric types and utilities. It is intentionally small and purely geometric; it does not depend on any particular corner detector or image pipeline.

![Rectified grid view](../img/rectified_small.png)
*Global rectification output from the chessboard pipeline.*

## Core data types

- `Corner`: raw corner observations from your detector.
  - `position`: image-space pixel coordinates.
  - `orientation`: dominant grid orientation in radians, defined modulo `pi`.
  - `orientation_cluster`: optional cluster index (0 or 1) if clustering is used.
  - `strength`: detector response.
- `GridCoords`: integer grid indices `(i, j)` in board space.
- `LabeledCorner`: a detected corner with optional grid coordinates and logical ID.
- `TargetDetection`: a collection of labeled corners for one board instance.
- `TargetKind`: enum for `Chessboard`, `Charuco`, or `CheckerboardMarker`.

These types are shared across all detectors so downstream code can be target-agnostic.

## Orientation clustering

ChESS corner orientations are only defined modulo `pi`. The clustering utilities help recover two dominant grid directions:

- `cluster_orientations`: histogram-based peak finding followed by 2-means refinement.
- `OrientationClusteringParams`: histogram size, separation thresholds, outlier rejection.
- `compute_orientation_histogram`: debug visualization helper.
- `estimate_grid_axes_from_orientations`: a lightweight fallback when clustering fails.

Chessboard detection uses these helpers to label corners by axis and to reject outliers.

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
