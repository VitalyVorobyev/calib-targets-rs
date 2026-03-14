# Troubleshooting

This chapter maps each error variant to a diagnostic checklist. For parameter
descriptions, see [Tuning the Detector](tuning.md).

---

## Reading the debug log

Enable debug logging before anything else:

```bash
RUST_LOG=debug cargo run --example detect_charuco -- testdata/small2.png
```

Or from code:

```rust,no_run
tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).init();
```

Key log lines and what they tell you:

| Log line | Meaning |
|---|---|
| `input_corners=N` | N ChESS corners passed the strength filter |
| `chessboard stage failed: ...` | Grid assembly error; reason follows |
| `marker scan produced N detections` | N cells decoded a valid marker ID |
| `alignment result: inliers=N` | N markers matched the board spec |
| `cell (x,y) failed decode` | That cell did not match any dictionary entry |
| `cell (x,y) passed threshold but no dict match` | Binarization ok, wrong dictionary |

If you do not see these lines, confirm `RUST_LOG=debug` is set in the shell that runs
the binary, not in a parent process.

---

## `ChessboardNotDetected`

The chessboard assembly stage found fewer corners than `min_corners`, or could not form
a connected grid from the detected corners.

**Checklist:**

1. **How many corners were detected?** Look for `input_corners=N` in the log.
   - If `N < min_corners`: lower `min_corners` or lower `min_corner_strength`.
   - If `N` is zero or very small: the ChESS detector found nothing. Check image
     resolution — `px_per_square` in `default_chess_config()` should be close to the
     actual pixel size of one board square.

2. **Corners found but grid assembly fails?**
   - Check `max_spacing_pix`: if the physical board squares are larger than this value
     in pixels, the graph edges are pruned and the grid cannot be assembled.
   - Check `min_spacing_pix`: if two ChESS responses land on the same corner, they may
     confuse the graph. Raise `min_spacing_pix`.

3. **Orientation clustering failing?** If the board is close to axis-aligned and the two
   corner directions are not well separated, try setting `use_orientation_clustering =
   false` (synthetic / controlled images only).

4. **Multiple boards in the scene?** Set `expected_rows` / `expected_cols` so the
   detector only accepts the correct grid size.

---

## `NoMarkers`

All ChESS corners were found and the chessboard grid was assembled, but no ArUco/AprilTag
marker was decoded inside any cell.

**Checklist:**

1. **Correct dictionary?** The `dictionary` field in the board spec must match the one
   used when printing. A mismatch produces `cell (x,y) passed threshold but no dict
   match` in the log for every cell.

2. **Correct `marker_size_rel`?** If the sampled region is the wrong fraction of the
   cell, the bit cells will be misaligned. Verify against the board spec.

3. **Blurry image?**
   - Enable `multi_threshold: true` (already the default for ChArUco).
   - Lower `min_border_score` to `0.65`–`0.70`.

4. **Uneven lighting?** `multi_threshold` handles this automatically. If already enabled,
   check whether the board surface has specular reflections — these cannot be corrected
   by thresholding alone.

5. **Wrong scale?** If `px_per_square` is far from the actual pixel size, the projective
   warp used for cell sampling will produce a very small or very large patch. Adjust
   `px_per_square` in `ChessConfig`.

---

## `AlignmentFailed { inliers: N }`

Markers were decoded, but fewer than `min_marker_inliers` of them matched the board
specification in a geometrically consistent way.

**Checklist:**

1. **`inliers = 0`:** No decoded marker ID appears in the board layout at all.
   - Board spec mismatch: wrong `rows`, `cols`, `dictionary`, or `marker_layout`.
   - Marker IDs may be correct but the layout offset is wrong (e.g. the board was
     generated with a non-zero `first_marker` id).

2. **`inliers` small but non-zero:**
   - Board is partially visible — lower `min_marker_inliers` to the number of markers
     you reliably expect to see.
   - Strong perspective distortion — the homography RANSAC may not converge. Raise
     `orientation_tolerance_deg` so more corners enter the initial grid.

3. **`inliers` near threshold:**
   - One or two spurious decodings are pulling the fit off. Raise `min_border_score`
     slightly to reject low-confidence markers.

---

## Common image problems

| Problem | Recommended fix |
|---|---|
| Strong blur | Lower `min_border_score` to `0.65`, enable `multi_threshold` |
| Uneven / gradient lighting | `multi_threshold` (already default) |
| Strong perspective / wide-angle | Raise `max_spacing_pix`, raise `orientation_tolerance_deg` |
| Partial occlusion | Lower `completeness_threshold`, lower `min_marker_inliers` |
| Very small board in frame | Raise `px_per_square` to match actual pixel size |
| Very large board / high-res image | Raise `max_spacing_pix` to ≥ `image_width / cols / 2` |
| Multiple boards in frame | Set `expected_rows` / `expected_cols` explicitly |
| Specular reflections on board | Pre-process with local contrast normalization (CLAHE) |

---

## Getting more help

- Open an issue on [GitHub](https://github.com/VitalyVorobyev/calib-targets-rs/issues)
  and attach the debug log (with `RUST_LOG=debug`), image, and board spec.
- See [Tuning the Detector](tuning.md) for full parameter reference.
- See [Understanding Results](output.md) for field meanings and score thresholds.
