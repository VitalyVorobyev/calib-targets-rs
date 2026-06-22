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

The detector has no single error variant — a `None` return means some
stage failed. To diagnose, run the chessboard crate's serializable
topological trace, `calib_targets_chessboard::pipeline::trace_topological`,
which is layered over the production path (so it reflects what `detect()`
actually does) and reports per-corner usability plus the labelled
components:

```rust,ignore
use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_chessboard::{pipeline::trace_topological, DetectorParams};
# let img: image::GrayImage = todo!();

let corners = detect_corners(&img, &default_chess_config());
match trace_topological(&corners, &DetectorParams::default()) {
    Ok(trace) => {
        let usable = trace.corners.iter().filter(|c| c.usable).count();
        println!("corners_in: {}", trace.diagnostics.corners_in);
        println!("corners_used: {usable}");
        println!("components: {}", trace.components.len());
        println!("total labels: {}", trace.diagnostics.labels);
    }
    Err(e) => println!("topological stage produced no grid: {e}"),
}
```

**Checklist:**

1. **No ChESS corners found?** `corners.is_empty()` (and
   `trace.diagnostics.corners_in == 0`). The ChESS detector saw nothing —
   check image resolution / contrast; override
   `calib_targets::detect::default_chess_config()` with a custom
   `DetectorConfig` if necessary — e.g.
   `DetectorConfig::chess().with_threshold(Threshold::Absolute(8.0))` to
   drop the noise floor, or `.with_threshold(Threshold::Relative(0.05))`
   for a fraction of the per-frame peak response. The chess-corners 0.10
   release replaced the legacy `(threshold_mode, threshold_value)` pair
   with the tagged-enum `Threshold` shown above.

2. **Corners found but few `usable`?** The strength / fit prefilter or the
   axis-usability gate is rejecting most corners. Lower
   `min_corner_strength`, raise `max_fit_rms_ratio`, and check the axis
   clustering tolerances (`cluster_tol_deg` default `12.0` → try `16.0`;
   `min_peak_weight_fraction` default `0.02` → try `0.01`). A perfectly
   rectilinear board with axes on the π-wrap boundary is handled by
   plateau-aware peak picking.

3. **Usable corners but `Err(NoComponents)`?** The topological builder
   assembled no quad mesh. Try `detect_chessboard_best` with
   `DetectorParams::sweep_default()` (widens the clustering and attachment
   tolerances).

4. **Components found but `detect_chessboard` still `None`?** The final
   geometry check refused the detection (survivors below
   `min_labeled_corners`) or only tiny components survived. Try a wider
   config via `detect_chessboard_best`; if the scene legitimately holds
   multiple boards, use `detect_chessboard_all` and handle each component
   separately.

5. **Multiple same-board components in the scene** (ChArUco markers break
   contiguity): this is expected. Use `detect_chessboard_all`; each piece
   comes back with its own locally-rebased `(i, j)`.

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
   - Strong perspective distortion — raise the chessboard-side attachment
     tolerances (`attach_axis_tol_deg`, `edge_axis_tol_deg`) so more
     corners enter the grid, or use `detect_charuco_best` with a sweep.

3. **`inliers` near threshold:**
   - One or two spurious decodings are pulling the fit off. Raise `min_border_score`
     slightly to reject low-confidence markers.

---

## Common image problems

| Problem | Recommended fix |
|---|---|
| Strong blur | Lower `min_border_score` to `0.65`, enable `multi_threshold` |
| Uneven / gradient lighting | `multi_threshold` (already default) |
| Strong perspective / wide-angle | Raise `edge_axis_tol_deg` / `attach_axis_tol_deg` / `geometry_check_local_h_tol_rel` on the chessboard side |
| Partial occlusion | Use `detect_chessboard_all`; for ChArUco, lower `min_marker_inliers` |
| Multiple same-board components | `detect_chessboard_all`; cap via `max_components` |
| Very small ChArUco board in frame | Raise `CharucoParams.px_per_square` to match actual square size |
| Specular reflections on board | Pre-process with local contrast normalisation (CLAHE); if pre-processing is off the table, lower `min_peak_weight_fraction` so clustering can cope with the reduced corner count |
| Grid components found but detection `None` | Use `detect_chessboard_best`; inspect the `trace_topological` components and the final-check `GeometryCheckTrace.dropped_*` counters |

---

## Getting more help

- Open an issue on [GitHub](https://github.com/VitalyVorobyev/calib-targets-rs/issues)
  and attach the debug log (with `RUST_LOG=debug`), image, and board spec.
- See [Tuning the Detector](tuning.md) for full parameter reference.
- See [Understanding Results](output.md) for field meanings and score thresholds.
