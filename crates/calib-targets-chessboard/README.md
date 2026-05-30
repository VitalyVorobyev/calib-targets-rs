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
use calib_targets_chessboard::{ChessCorner, Detector, DetectorParams};

fn detect_one(corners: &[ChessCorner]) {
    let det = Detector::new(DetectorParams::default());
    if let Some(d) = det.detect(corners) {
        println!("labelled {} corners", d.corners.len());
        for c in &d.corners {
            // c.grid: (i, j) — always present; c.input_index: input-slice index.
            let _ = (c.grid.i, c.grid.j, c.position, c.input_index, c.score);
        }
    }
}

// Multi-component (e.g. ChArUco markers split the grid into islands):
fn detect_multi(corners: &[ChessCorner]) {
    let det = Detector::new(DetectorParams::default());
    for (k, comp) in det.detect_all(corners).iter().enumerate() {
        println!("component {k}: {} corners", comp.corners.len());
    }
}
```

## Inputs

- `&[ChessCorner]` — ChESS X-junction corners from `chess-corners`, with
  `position`, `axes`, `strength`, `contrast`, `fit_rms` populated.
- [`DetectorParams`] — a small stable core (graph-build algorithm,
  output gates, corner-strength floor) plus an opt-in, unstable
  [`AdvancedTuning`] sub-struct of per-stage knobs. Use
  `DetectorParams::default()` for a single config or
  `DetectorParams::sweep_default()` for the 3-config sweep preset.

## Outputs

`Detector::detect` returns `Option<ChessboardDetection>` — the labelled
corner set (`corners: Vec<ChessboardCorner>`, rebased to a non-negative
bounding box and sorted by `(j, i)`) plus a stable
`cell_size: Option<f32>` carrying the seed-derived grid pitch in pixels
(populated on the normal `detect()` path). Each `ChessboardCorner`:

| Field | Meaning |
|---|---|
| `position: Point2<f32>` | Sub-pixel image position. |
| `grid: GridCoords` | The `(i, j)` grid label. Non-optional — a chessboard corner is always labelled. |
| `input_index: usize` | Index back into the caller's input `&[ChessCorner]` slice — used by ChArUco / marker-board alignment. |
| `score: f32` | Corner score. |

`detect_with_diagnostics` / `detect_all_with_diagnostics` return
[`DebugFrame`] — full per-stage telemetry (corner outcomes, iteration
traces, booster results, the seed grid directions and cell size) emitted
as schema-versioned JSON. These entry points and the whole `diagnostics`
module are gated behind the **`diagnostics` cargo feature (off by
default)** — the hot `detect()` path builds no trace. The grid-axis
angles, which are not part of the result contract, are reachable there;
the cell size is also carried on the result (`ChessboardDetection::cell_size`).

## Configuration

[`DetectorParams`] is a small **stable core** of four knobs plus an
opt-in, unstable [`AdvancedTuning`] sub-struct
([`DetectorParams::advanced`]) holding the 40-plus per-stage tuning
knobs. Defaults are chosen to post the precision contract above; tune
only when a specific input fails.

The stable core — the knobs a calibration consumer has a basis to set:

| Knob | Effect |
|---|---|
| `graph_build_algorithm` | Pick the seed-and-grow (`SeedAndGrow`, default) or topological grid builder. |
| `min_labeled_corners` | Reject too-small detections. |
| `max_components` | Cap the number of disconnected pieces returned by `detect_all`. |
| `min_corner_strength` | Drop weak ChESS corners before clustering (`0.0` = off). |

Everything else is a per-stage tuning knob on [`AdvancedTuning`],
attached via `DetectorParams::with_advanced(...)` — grouped by pipeline
stage, all left at `Default` unless an input fails and you have evidence
for the change. **These knobs are not covered by semver** and may change
between minor versions; treat them as an escape hatch, not a stable
contract.

| Group | Main knobs (on `AdvancedTuning`) | Effect |
|---|---|---|
| Clustering | `num_bins`, `peak_min_separation_deg`, `cluster_tol_deg` | Axis-angle histogram + 2-means refinement. Widen tolerances for rotated-camera or strongly perspective boards. |
| Seed | `seed_edge_tol`, `seed_axis_tol_deg`, `seed_close_tol` | 2×2 seed-quad validation. |
| Grow | `attach_search_rel`, `attach_axis_tol_deg`, `step_tol`, `edge_axis_tol_deg` | BFS attachment invariants. Rarely need tuning. |
| Validation | `line_tol_rel`, `local_h_tol_rel`, `max_validation_iters` | Line + local-H residuals. Loosen `local_h_tol_rel` under strong lens distortion; keep `line_tol_rel` tight. |
| Boosters | `enable_weak_cluster_rescue` | Toggle for the weak-cluster rescue booster. Line extrapolation, gap fill, and component merge run unconditionally. |

The cell size is **not** a tuning knob — the detector derives it from a
self-consistent 4-corner seed, so there is nothing to configure.

`advanced` is serialized as a nested `"advanced"` object (it is **not**
flattened) and is omitted entirely when unset; `min_corner_strength` and
the other three stable knobs stay top-level JSON keys.

See the [parameter reference][tuning-chapter] for field-by-field guidance.

## Tuning difficult cases

- **Small or blurry board** — too few `ChessCorner`s reach the
  detector. Tune the upstream ChESS corner detector (its
  `chess-corners` `DetectorConfig` — e.g. lower the corner-response
  threshold, enable a multiscale pyramid for large frames), then try
  `DetectorParams::sweep_default()` which varies clustering/seed
  tolerances on this crate's side.
- **Strong perspective / tilted view** — widen `cluster_tol_deg` and
  `attach_axis_tol_deg` by a few degrees; grow may refuse otherwise-valid
  neighbours at the image edge.
- **Moderate radial distortion (no fisheye)** — loosen `local_h_tol_rel`
  from the default 0.2 to ~0.35; the per-corner local-H check is the
  strictest invariant under curvature.
- **Low-contrast / glare** — glare patches starve the corner detector;
  adjust the upstream ChESS `DetectorConfig` thresholding (an absolute
  floor survives glare better than a relative one) so enough corners
  reach this crate.
- **Partial occlusion splitting the board into pieces** — use
  `detect_all` rather than `detect`; you get one `ChessboardDetection`
  per connected component, each with its own rebased `(i, j)` axes.

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
| `diagnostics` | off | Exposes the `diagnostics` module (`DebugFrame`, per-stage traces, `StageCounts`, `DEBUG_FRAME_SCHEMA`) and the `detect*_with_diagnostics` entry points. Without it the hot path builds no trace. |
| `dataset` | off | Pulls in `serde_json` for the `run_dataset` and `debug_single` examples; implies `diagnostics`. |

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
