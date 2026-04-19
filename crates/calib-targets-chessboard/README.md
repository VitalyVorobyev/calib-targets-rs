# calib-targets-chessboard

[![docs.rs](https://docs.rs/calib-targets-chessboard/badge.svg)](https://docs.rs/calib-targets-chessboard)

Invariant-first chessboard detector. Takes a slice of
[ChESS](https://www.cl.cam.ac.uk/research/rainbow/projects/chess/)
X-junction corners and returns an integer-labelled chessboard grid. It
is **precision-by-construction**: every emitted `(i, j)` label has been
proven to sit at a real grid intersection by a stack of independent
geometric invariants. Missing corners are acceptable; wrong corners
are not.

The detector is tested on a private regression dataset of 120 frames
captured with non-negligible lens distortion and motion blur — the kind
of input that breaks naive corner-graph approaches. On that set it
posts **119 / 120 frames detected with zero wrong `(i, j)` labels** and
a median per-frame latency under 100 µs in release. The pattern-
agnostic pieces of the algorithm live in
[`projective-grid`](../projective-grid).

Algorithm reference: [book chapter](https://vitalyvorobyev.github.io/calib-targets-rs/chessboard.html).

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
            d.cell_size
        );
    }
}

// Same-board, multi-component (e.g., ChArUco markers split the grid):
fn detect_multi(corners: &[Corner]) {
    let det = Detector::new(DetectorParams::default());
    for (k, comp) in det.detect_all(corners).iter().enumerate() {
        println!("component {k}: {} corners", comp.target.corners.len());
    }
}
```

## Public API surface

- `Detector` / `DetectorParams` — main entry point and configuration
  (`#[non_exhaustive]`, `Default`, plus a 3-config `sweep_default()` preset).
- `Detection` — final output: `target: TargetDetection`, `grid_directions`,
  `cell_size`, `strong_indices: Vec<usize>` (input-corner indices in
  `target.corners` order; consumed by ChArUco's marker alignment).
- `DebugFrame` — full per-stage diagnostics (`schema`, `corners[].stage`,
  per-iteration traces, booster summary). Schema-versioned via
  `DEBUG_FRAME_SCHEMA`.
- `InstrumentedResult` / `StageCounts` — compact telemetry derived from
  `DebugFrame`.
- `detect`, `detect_debug`, `detect_all`, `detect_all_debug`,
  `detect_instrumented`, `detect_all_instrumented` — every flavor.

## Pipeline at a glance

1. **Pre-filter** — strength + fit-quality + axes-validity.
2. **Global axes** `(Θ₀, Θ₁)` — circular histogram + double-angle 2-means.
3. **Per-corner cluster label** (canonical / swapped).
4. **Cell size `s`** — cross-cluster nearest-neighbor mode.
5. **Seed** — 2×2 quad whose 4 edges are self-consistent. `s` comes OUT
   of the seed, never in.
6. **Grow** — BFS attaching one corner per step under the full invariant
   stack.
7. **Validate** — line collinearity + local-H residual; blacklist + reseed
   loop until convergence (capped).
8. **Recall boosters** — line extrapolation, gap fill, component merge,
   weak-cluster rescue. Each strictly adds corners; none relax invariants.

See the book chapter for the full invariant list, parameter reference, and
debugging guide.

## Feature flags

| Feature | Default | Effect |
|---|---|---|
| `tracing` | off | Adds `#[tracing::instrument]` to `Detector::detect`, `detect_debug`, `detect_all`, `detect_all_debug`, `detect_instrumented`, and per-stage entry points. |
| `dataset` | off | Pulls in `serde_json` for the `run_dataset` and `debug_single` examples. |

## Examples and benches

```bash
# Single image → CompactFrame JSON the Python overlay consumes.
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
cargo test -p calib-targets-chessboard --release -- --ignored # full private-dataset precision contract
```

The ignored `full_dataset_precision_contract` test reads the private
regression dataset from `privatedata/` when available and asserts
119/120 detections / 0 wrong labels. On a fresh public checkout (no
`privatedata/` directory) the test skips — it is not required for CI
to pass.

The public-facing regression harness at `tests/testdata_regression.rs`
runs in every `cargo test` invocation and gates detection on the
committed `testdata/` image set (ChArUco snaps, puzzleboard reference
images, and plain chessboards).
