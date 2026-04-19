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

## `detect_chessboard` returns `None`

The detector has no single error variant — a `None` return means
some stage failed to converge. To diagnose, use
`detect_chessboard_debug` to get a full `DebugFrame` and follow the
chain:

```rust,no_run
use calib_targets::detect::detect_chessboard_debug;
use calib_targets::chessboard::DetectorParams;
# let img: image::GrayImage = todo!();

let frame = detect_chessboard_debug(&img, &DetectorParams::default());
println!("stage counts: {:#?}", frame.corners.iter().fold(
    std::collections::HashMap::new(),
    |mut acc, c| {
        *acc.entry(format!("{:?}", c.stage)).or_insert(0u32) += 1;
        acc
    },
));
println!("grid_directions: {:?}", frame.grid_directions);
println!("cell_size: {:?}", frame.cell_size);
println!("seed: {:?}", frame.seed);
println!("iterations: {:#?}", frame.iterations);
```

**Checklist:**

1. **No ChESS corners found?** Look for `input_count: 0` in the frame.
   The ChESS detector saw nothing. Check image resolution / contrast;
   override `calib_targets::detect::default_chess_config()` with a
   custom `ChessConfig` (lower `threshold_value`, change
   `threshold_mode`) if necessary.

2. **Corners found, `grid_directions: None`?** Clustering failed.
   Most common causes:
   - Noisy axes: raise `cluster_tol_deg` (default `12.0` → try `16.0`).
   - Few real corners: lower `min_peak_weight_fraction` (default
     `0.02` → try `0.01`).
   - Perfectly rectilinear board with axes exactly at the π-wrap
     boundary: the detector handles this via plateau-aware peak picking — if
     you hit this, verify you're on v0.6.0+.

3. **`grid_directions` set, `seed: None`?** Seeding failed — no
   qualifying 2×2 quad was found.
   - Try `detect_chessboard_best` with
     `DetectorParams::sweep_default()` (widens seed tolerances).
   - Raise `seed_edge_tol` (default `0.25`) if the board has
     noticeable cell-size variation under perspective.

4. **`seed` set, `detection: None`?** Validation failed to converge.
   - Check `iterations`: if the labelled count oscillates, raise
     `max_validation_iters` (default `3` → try `6`).
   - Scene may contain multiple boards — try
     `detect_chessboard_all` and handle each component separately.

5. **Multiple same-board components in the scene** (ChArUco markers
   break contiguity): this is expected. Use `detect_chessboard_all`;
   each piece comes back with its own locally-rebased `(i, j)`.

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
   `CharucoParams.px_per_square`.

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
| Strong perspective / wide-angle | Raise `edge_axis_tol_deg` / `attach_axis_tol_deg` / `projective_line_tol_rel` on the chessboard side |
| Partial occlusion | Use `detect_chessboard_all`; for ChArUco, lower `min_marker_inliers` |
| Multiple same-board components | `detect_chessboard_all`; cap via `max_components` |
| Very small ChArUco board in frame | Raise `CharucoParams.px_per_square` to match actual square size |
| Specular reflections on board | Pre-process with local contrast normalisation (CLAHE); if pre-processing is off the table, lower `min_peak_weight_fraction` so clustering can cope with the reduced corner count |
| Validation loop oscillates (seed found, detection `None`) | Raise `max_validation_iters`; inspect `DebugFrame.iterations` to confirm the labelled count is bouncing |

---

## Getting more help

- Open an issue on [GitHub](https://github.com/VitalyVorobyev/calib-targets-rs/issues)
  and attach the debug log (with `RUST_LOG=debug`), image, and board spec.
- See [Tuning the Detector](tuning.md) for full parameter reference.
- See [Understanding Results](output.md) for field meanings and score thresholds.
