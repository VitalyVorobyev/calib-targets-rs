# calib-targets-aruco

`calib-targets-aruco` provides embedded ArUco/AprilTag-style dictionaries and decoding on **rectified grids**. It does not detect quads or perform image rectification by itself.

## Current API surface

- `Dictionary`: built-in dictionary metadata and packed codes.
- `Matcher`: brute-force matching against a dictionary with rotation handling.
- `ScanDecodeConfig`: how to scan a rectified grid (border size, inset, polarity).
- `scan_decode_markers`: read and decode markers from rectified cells.

The crate expects a rectified view where each chessboard square is approximately `px_per_square` pixels and where cell indices align with the board grid.

## Conventions

- Marker bits are packed row-major with black = 1.
- `Match::rotation` is in `0..=3` such that `observed == rotate(dict_code, rotation)`.
- `border_bits` matches the OpenCV definition (typically 1).

## Status

Decoding is implemented and stable for rectified grids, but quad detection and image-space marker detection are deliberately out of scope. The roadmap chapter details planned improvements and API refinements.
