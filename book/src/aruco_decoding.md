# ArUco Decoding Details

This chapter expands on the marker decoding path in `calib-targets-aruco`. The decoder is grid-first: it samples expected square cells and reads bits in rectified space (or per-cell quads).

## Per-cell decoding

`scan_decode_markers_in_cells` reads marker bits in their own cells given an existing grid of square corners, without warping the full image. ChArUco detection drives this path: only valid grid cells are decoded, and the per-cell work parallelises trivially.

## Sampling model

- Bits are sampled on a regular grid inside the marker area.
- The marker area is defined by `marker_size_rel`, with an extra inset from `inset_frac`.
- A per-marker threshold (Otsu) is computed from sampled intensities.

## Tuning knobs

- `inset_frac` controls how far inside the marker area bits are sampled. Lower values capture more of the marker; higher values are more robust to thin black borders bleeding into the bit grid.
- `min_border_score` is the minimum "frame looks like a marker border" score required to accept a cell. Higher values reject ambiguous cells.
- `dedup_by_id` collapses repeated decodes of the same dictionary ID across cells.
- `marker_size_rel` is the marker side relative to the enclosing chessboard cell and must match the physical board spec.
