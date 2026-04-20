# calib-targets-chessboard

[![docs.rs](https://docs.rs/calib-targets-chessboard/badge.svg)](https://docs.rs/calib-targets-chessboard)

Invariant-first chessboard detector. Takes a slice of
[ChESS](https://www.cl.cam.ac.uk/research/rainbow/projects/chess/)
X-junction corners and returns an integer-labelled chessboard grid. It is
**precision-by-construction**: every emitted `(i, j)` label has been
proven to sit at a real grid intersection by a stack of independent
geometric invariants. Missing corners are acceptable; wrong corners are
not.

The pattern-agnostic graph-building pieces live in
[`projective-grid`](https://docs.rs/projective-grid).

Most users call this through the facade: [`calib-targets`] exposes
`detect_chessboard`, `detect_chessboard_best`, and `detect_chessboard_all`
as one-call helpers. Install this crate directly only if you already have
ChESS corners and want the detector without the image-loading layer.

[`calib-targets`]: https://docs.rs/calib-targets

Algorithm deep-dive: [book chapter][book-chapter].

## Quickstart

```rust,ignore
use calib_targets_chessboard::{Detector, DetectorParams};
use calib_targets_core::Corner;

fn detect_one(corners: &[Corner]) {
    let det = Detector::new(DetectorParams::default());
    if let Some(d) = det.detect(corners) {
        println!(
            "labelled {} corners; cell ≈ {:.1} px",
            d.target.corners.len(),
            d.cell_size,
        );
    }
}

// Multi-component (e.g. ChArUco markers split the grid into islands):
fn detect_multi(corners: &[Corner]) {
    let det = Detector::new(DetectorParams::default());
    for (k, comp) in det.detect_all(corners).iter().enumerate() {
        println!("component {k}: {} corners", comp.target.corners.len());
    }
}
```

## Inputs

- `&[Corner]` — ChESS X-junction corners from `chess-corners`, with
  `position`, `axes`, `strength`, `contrast`, `fit_rms` populated.
- [`DetectorParams`] — flat configuration struct covering the 8-stage
  pipeline. Use `DetectorParams::default()` for a single config or
  `DetectorParams::sweep_default()` for the 3-config sweep preset.

## Outputs

`Detector::detect` returns `Option<Detection>`:

| Field | Meaning |
|---|---|
| `target: TargetDetection` | Labelled corners with `(i, j)` in `grid`, rebased to a non-negative bounding box, monotonic in `i` and `j`. |
| `grid_directions: [f32; 2]` | The two global grid-axis angles in `[0, π)`. |
| `cell_size: f32` | Pixel spacing of the detected grid (fitted from consistent seed edges). |
| `strong_indices: Vec<usize>` | Index mapping from `target.corners` back into the caller's input slice — used by ChArUco / marker-board alignment. |

`detect_debug` / `detect_all_debug` return [`DebugFrame`] — full per-stage
telemetry (corner outcomes, iteration traces, booster results) emitted as
schema-versioned JSON via the `dataset` feature.

## Configuration

[`DetectorParams`] is flat — 30-plus knobs, grouped by pipeline stage.
Defaults are chosen to post the precision contract above; tune only when a
specific input fails.

| Group | Main knobs | Effect |
|---|---|---|
| ChESS corner detection | `chess: ChessConfig` | Pre-graph feature detection. Drop `threshold_value` to recover blurry boards; raise it to suppress false corners under glare. |
| Clustering | `num_bins`, `peak_min_separation_deg`, `cluster_tol_deg` | Axis-angle histogram + 2-means refinement. Widen tolerances for rotated-camera or strongly perspective boards. |
| Cell size | `cell_size_hint` | Optional hint. Leave `None` so the detector derives cell size from a self-consistent seed (recommended). |
| Seed | `seed_edge_tol`, `seed_axis_tol_deg`, `seed_close_tol` | 2×2 seed-quad validation. |
| Grow | `attach_search_rel`, `attach_axis_tol_deg`, `step_tol`, `edge_axis_tol_deg` | BFS attachment invariants. Rarely need tuning. |
| Validation | `line_tol_rel`, `projective_line_tol_rel`, `local_h_tol_rel`, `max_validation_iters` | Line + local-H residuals. Loosen `local_h_tol_rel` under strong lens distortion; keep `line_tol_rel` tight. |
| Boosters | `enable_line_extrapolation`, `enable_gap_fill`, `enable_component_merge`, `enable_weak_cluster_rescue` | Recall boosters. Each strictly adds corners and never relaxes invariants; disable individually to bisect a recall regression. |
| Output gates | `min_labeled_corners`, `max_components` | Reject too-small / too-fragmented detections. |

See the [parameter reference][tuning-chapter] for field-by-field guidance.

## Tuning difficult cases

- **Small or blurry board** — drop `chess.threshold_value` (e.g. 0.15 →
  0.08), increase ChESS `pyramid_levels` to 2, then try
  `DetectorParams::sweep_default()` which interleaves multiple thresholds.
- **Strong perspective / tilted view** — widen `cluster_tol_deg` and
  `attach_axis_tol_deg` by a few degrees; grow may refuse otherwise-valid
  neighbours at the image edge.
- **Moderate radial distortion (no fisheye)** — loosen `local_h_tol_rel`
  from the default 0.2 to ~0.35; the per-corner local-H check is the
  strictest invariant under curvature.
- **Low-contrast / glare** — switch `chess.threshold_mode` from
  `relative` to `absolute` and set an explicit floor; glare patches
  collapse the relative threshold.
- **Partial occlusion splitting the board into pieces** — use
  `detect_all` rather than `detect`; you get one `Detection` per
  connected component, each with its own rebased `(i, j)` axes.

## Limitations

- Requires **ChESS X-junction corners** as input. Plain Harris / FAST
  corners will not work — the detector reads the per-corner `axes` field.
- **One board per image.** Multiple disjoint boards are not disambiguated;
  the largest one wins.
- **No fisheye support.** Moderate radial distortion degrades gracefully
  thanks to local-invariant validation; severe wide-angle / fisheye lenses
  require distortion-aware preprocessing.
- **Opaque occlusions** that split the board into small pieces may yield
  several components rather than one coherent grid — that is by design
  (no wrong labels), but callers must merge downstream if a single grid is
  required.

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `tracing` | off | Adds `#[tracing::instrument]` to detector entry points. |
| `dataset` | off | Pulls in `serde_json` for the `run_dataset` and `debug_single` examples. |

## Examples and benches

```bash
# Single image → DebugFrame JSON the Python overlay consumes.
cargo run --release -p calib-targets-chessboard --features dataset \
    --example debug_single -- \
    --image path/to/image.png \
    --out-default /tmp/frame.json

# Per-frame timing across representative sub-frames.
cargo bench -p calib-targets-chessboard --bench chessboard_timing
```

## Tests

```bash
cargo test -p calib-targets-chessboard                        # unit + smoke
```

## Related

- [Book: chessboard detector][book-chapter]
- [Book: tuning the detector][tuning-chapter]
- [Book: understanding results][output-chapter]
- [`projective-grid`](https://docs.rs/projective-grid) — the graph /
  validation pieces underneath.

[book-chapter]: https://vitalyvorobyev.github.io/calib-targets-rs/chessboard.html
[tuning-chapter]: https://vitalyvorobyev.github.io/calib-targets-rs/tuning.html
[output-chapter]: https://vitalyvorobyev.github.io/calib-targets-rs/output.html
