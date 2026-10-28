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

## `detect_chessboard` reports no board

The facade `detect_chessboard` returns `Err(DetectError::NoDetection)` when
no board is found — a miss carries no specific cause, only that some stage
declined to emit a detection. To diagnose, run the chessboard crate's serializable
topological trace, `calib_targets_chessboard::trace_topological`,
which is layered over the production path (so it reflects what `detect()`
actually does) and reports per-corner usability plus the labelled
components:

```rust,ignore
use calib_targets::detect::{default_chess_config, detect_corners};
use calib_targets_chessboard::{trace_topological, ChessboardParams};
# let img: image::GrayImage = todo!();

let corners = detect_corners(&img, &default_chess_config());
match trace_topological(&corners, &ChessboardParams::default()) {
    Ok(trace) => {
        let usable = trace.corners.iter().filter(|c| c.usable).count();
        println!("corners_in: {}", trace.diagnostics.corners_in);
        println!("corners_used: {usable}");
        println!("components: {}", trace.final_components.len());
        println!("total labels: {}", trace.diagnostics.final_labels);
    }
    Err(e) => println!("topological stage produced no grid: {e}"),
}
```

> `trace_topological` (and the `TopologicalTrace` type) are gated behind the
> chessboard crate's **off-by-default `diagnostics` feature**. Enable it with
> `calib-targets-chessboard`'s `diagnostics` feature — or the facade's
> `diagnostics` feature — to call it; the hot `detect_*` path builds no trace.

**Checklist:**

1. **No ChESS corners found?** `corners.is_empty()` (and
   `trace.diagnostics.corners_in == 0`). The ChESS detector saw nothing —
   check image resolution / contrast; override
   `calib_targets::detect::default_chess_config()` with a custom
   `DetectorConfig` if necessary — e.g.
   `DetectorConfig::chess().with_threshold(8.0)` to drop the noise floor
   below the workspace default of `15.0`. Since chess-corners 1.0 the
   threshold is a plain absolute response floor (`f32`); the `Threshold`
   enum and its relative mode are gone — ChESS reads only an absolute
   floor now.

2. **Corners found but few `usable`?** The prefilter is rejecting most
   corners — either on `strength` or because their axes are too uncertain
   to vote (`max(σ₀, σ₁) > axis_align_tol_rad`). Lower
   `min_corner_strength`, and check the axis
   clustering tolerances (`cluster_tol_deg` default `12.0` → try `16.0`;
   `min_peak_weight_fraction` default `0.02` → try `0.01`). A perfectly
   rectilinear board with axes on the π-wrap boundary is handled by
   plateau-aware peak picking.

3. **Usable corners but no `final_components` (or `Err(DetectionFailed)`)?**
   The topological builder assembled no quad mesh. Try `detect_chessboard_best` with
   `ChessboardParams::sweep_default()` (widens the clustering and attachment
   tolerances).

4. **Components found but `detect_chessboard` still reports no board?** The final
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

3. **Blurry or small markers?** Every cell is thresholded on its own and its bits
   are scored softly, so there is no border or threshold knob to lower. What helps
   is resolution: more pixels per marker bit (move closer, or set the corner
   detector's `upscale` for small cells), and a correct `scan.inset_frac`.

4. **Uneven lighting?** Per-cell thresholding already absorbs smooth gradients. Specular
   reflections inside a marker cannot be corrected by thresholding at all.

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
   - The board is barely visible or the markers barely resolved; see the
     `NoMarkers` checklist above. Do not lower `min_marker_inliers` below what
     the board guarantees.

---

## Common image problems

| Problem | Recommended fix |
|---|---|
| Strong blur | More pixels per square: move closer, or the corner detector's `upscale` |
| Uneven / gradient lighting | Nothing to do — markers are thresholded per cell |
| Strong perspective / wide-angle | Raise `edge_axis_tol_deg` / `attach_axis_tol_deg` / `geometry_check_local_h_tol_rel` on the chessboard side |
| Partial occlusion | Use `detect_chessboard_all`; for ChArUco, lower `min_marker_inliers` |
| Multiple same-board components | `detect_chessboard_all`; cap via `max_components` |
| Very small ChArUco board in frame | Raise `CharucoParams.px_per_square` to match actual square size |
| Specular reflections on board | Pre-process with local contrast normalisation (CLAHE); if pre-processing is off the table, lower `min_peak_weight_fraction` so clustering can cope with the reduced corner count |
| Grid components found but detection returns `Err(NoDetection)` | Use `detect_chessboard_best`; inspect `trace_topological_detection` (`final_components`, `chessboard.final_drops`) |

---

## Getting more help

- Open an issue on [GitHub](https://github.com/VitalyVorobyev/calib-targets-rs/issues)
  and attach the debug log (with `RUST_LOG=debug`), image, and board spec.
- See [Tuning the Detector](tuning.md) for full parameter reference.
- See [Understanding Results](output.md) for field meanings and score thresholds.
