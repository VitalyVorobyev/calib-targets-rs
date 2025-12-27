# calib-targets-aruco

`calib-targets-aruco` provides embedded ArUco/AprilTag-style dictionaries and decoding on **rectified grids**. It does not detect quads or perform image rectification by itself.

![Mesh-rectified grid](../img/mesh_rectified_small.png)
*Rectified grid used for ArUco/AprilTag decoding.*

## Current API surface

- `Dictionary`: built-in dictionary metadata and packed codes.
- `Matcher`: brute-force matching against a dictionary with rotation handling.
- `ScanDecodeConfig`: how to scan a rectified grid (border size, inset, polarity).
- `scan_decode_markers`: read and decode markers from rectified cells.
- `scan_decode_markers_in_cells`: decode markers from per-cell image quads (no full warp).
- `decode_marker_in_cell`: decode a single marker inside one square cell.

The crate expects a rectified view where each chessboard square is approximately `px_per_square` pixels and where cell indices align with the board grid.

## Decoding paths

There are two supported scanning modes:

- **Rectified grid scan** (`scan_decode_markers`): build a rectified image first and scan a regular grid.
- **Per-cell scan** (`scan_decode_markers_in_cells`): pass a list of per-cell quads and decode each cell directly.

Per-cell scanning avoids building the full rectified image and is easy to parallelize across cells.

## Scan configuration

`ScanDecodeConfig` controls how bit sampling and thresholding behave:

- `border_bits`: number of black border cells (OpenCV typically uses 1).
- `marker_size_rel`: marker size relative to the square size (ChArUco uses < 1.0).
- `inset_frac`: extra inset inside the marker to avoid edge blur.
- `min_border_score`: minimum fraction of border bits that must be black.
- `dedup_by_id`: keep only the best detection per marker id.

If decoding is too sparse on real images, reduce `inset_frac` slightly and re-run.

## Conventions

- Marker bits are packed row-major with black = 1.
- `Match::rotation` is in `0..=3` such that `observed == rotate(dict_code, rotation)`.
- `border_bits` matches the OpenCV definition (typically 1).

## Status

Decoding is implemented and stable for rectified grids, but quad detection and image-space marker detection are deliberately out of scope. The roadmap chapter details planned improvements and API refinements.

For deeper tuning and sampling details, see [ArUco Decoding Details](aruco_decoding.md).
