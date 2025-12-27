# ArUco Decoding Details

This chapter expands on the marker decoding path in `calib-targets-aruco`. The decoder is grid-first: it samples expected square cells and reads bits in rectified space (or per-cell quads).

## When to use per-cell decoding

Use per-cell decoding (`scan_decode_markers_in_cells`) when you already have a grid of square corners and want to avoid warping the full image. It works well with ChArUco detection because you can decode only the valid cells and parallelize across them.

## Sampling model

- Bits are sampled on a regular grid inside the marker area.
- The marker area is defined by `marker_size_rel`, with an extra inset from `inset_frac`.
- A per-marker threshold (Otsu) is computed from sampled intensities.

## Tuning checklist

- If markers are missing, try reducing `inset_frac` slightly.
- If false positives appear, raise `min_border_score` or enable `dedup_by_id`.
- Make sure `marker_size_rel` matches the physical board spec.
