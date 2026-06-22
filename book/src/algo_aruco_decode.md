# ArUco bit decode

> Code: `calib-targets-aruco` (`Dictionary`, `Matcher`,
> `ScanDecodeConfig`, `scan_decode_markers`,
> `scan_decode_markers_in_cells`, `decode_marker_in_cell`).

ArUco bit decode reads a marker's binary code out of an already-located
chessboard cell and matches it against a dictionary. It is deliberately
**grid-aware**, not generic contour/quad detection: the grid stage has
already found where every cell is, so the decoder samples the *expected*
cell in rectified space and reads bits on a regular grid. This sidesteps
the quad-finding and perspective-recovery steps a standalone ArUco
detector spends most of its time on.

## Inputs

The decoder works on **rectified** cells where each chessboard square is
approximately `px_per_square` pixels and cell indices align with the
board grid. Two paths supply that:

- **Rectified-grid scan** (`scan_decode_markers`) — build a single
  rectified image of the board, then scan a regular grid of cells.
- **Per-cell scan** (`scan_decode_markers_in_cells`) — pass a list of
  per-cell image quads and decode each cell directly, with no full-image
  warp. This is the path the ChArUco detector drives; the work is
  proportional to the number of valid cells and parallelises trivially.

## Bit sampling model

Inside each candidate cell:

- The **marker area** is `marker_size_rel` of the square side (ChArUco
  uses `< 1.0`), with an extra `inset_frac` inset to keep the bit grid off
  a thick or blurred border.
- Bits are sampled on a regular grid spanning the marker area.
- A per-marker **Otsu threshold** is computed from the sampled
  intensities, so the decode adapts to local lighting.
- The surrounding black **border ring** is scored; cells whose border
  score is below `min_border_score` are rejected before a dictionary
  lookup is attempted.

## Explicit bit conventions

These three conventions are explicit in the code and must match the
printed board exactly:

- **Bit order** — codes are packed **row-major**.
- **Polarity** — **black = 1**.
- **`border_bits`** — the number of whole black border cells, matching the
  OpenCV definition (typically 1).

## Dictionary matching

`Matcher` brute-forces the sampled code against every dictionary entry
under the four 90° rotations, returning the best match with its
`rotation ∈ 0..=3` (such that `observed == rotate(dict_code, rotation)`)
and a Hamming distance. The rotation is what lets the decoder normalise a
marker seen at any orientation; the Hamming distance feeds the
per-corner `score`. `dedup_by_id` keeps only the best detection per
dictionary ID across cells.

## Why grid-aware, not contour-based

A generic ArUco detector finds quads in the raw image, recovers each
marker's perspective, then decodes. Here the [topological grid
finder](algo_topological_grid.md) has already recovered the *whole board's*
lattice to sub-pixel precision, so the marker's cell quad — and its
rectification — come for free. Decoding becomes a local bit-read with an
adaptive threshold, which is both faster and more robust to the partial /
blurred markers a contour detector would miss.

## Cross-references

- [ChArUco alignment & corner IDs](algo_charuco_alignment.md) — the
  consumer of decoded marker IDs.
- [ChArUco pipeline](pipeline_charuco.md) — the end-to-end target detector.
- [calib-targets-aruco crate](aruco.md) and
  [ArUco Decoding Details](aruco_decoding.md) — the crate's API surface
  and sampling knobs.
