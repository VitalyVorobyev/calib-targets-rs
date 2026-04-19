# chessboard-v2

[![docs.rs](https://docs.rs/chessboard-v2/badge.svg)](https://docs.rs/chessboard-v2)

Invariant-first rewrite of the chessboard detector. Slated to replace the
contents of `calib-targets-chessboard` once production rewiring is complete.

The detector takes a slice of [ChESS](https://www.cl.cam.ac.uk/research/rainbow/projects/chess/)
X-junction corners and returns an integer-labelled chessboard grid. It is
**precision-by-construction**: every emitted `(i, j)` label has been proven
to sit at a real grid intersection by a stack of independent geometric
invariants. Missing corners are acceptable; wrong corners are not.

Current performance on the canonical 120-snap regression dataset
(`testdata/3536119669`):

- **119 / 120 frames detected**, average **43 labelled corners** per detection.
- **Zero wrong `(i, j)` labels** — the precision contract.
- Median per-frame latency `< 100 µs` in release (see `benches/`).

Algorithm reference: [book chapter](https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/book/src/chessboard.md)
and [`docs/chessboard_v2_spec.md`](https://github.com/VitalyVorobyev/calib-targets-rs/blob/main/docs/chessboard_v2_spec.md).

## Quickstart

```rust,ignore
use chessboard_v2::{Detector, DetectorParams};
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
| `dataset` | off | Pulls in `serde_json` for the `run_dataset` example. |

## Examples and benches

```bash
# Per-snap dataset run, emits DebugFrame JSON for the Python overlay.
cargo run --release -p chessboard-v2 --features dataset \
    --example run_dataset -- \
    --dataset testdata/3536119669 \
    --out bench_results/chessboard_v2_overlays

# Per-frame timing across representative sub-frames.
cargo bench -p chessboard-v2 --bench chessboard_v2_timing
```

## Tests

```bash
cargo test -p chessboard-v2                                      # unit + smoke
cargo test -p chessboard-v2 --release -- --ignored               # full 120-snap precision contract
```

The `tests/dataset_3536119669.rs` `full_dataset_precision_contract` is the
authoritative regression gate for the 119/120 / 0 wrong labels claim.

## Status

Standalone prototype, slated to replace `calib-targets-chessboard` as
`Detector` / `DetectorParams` / `Detection`. Until the swap, this crate is
not published.
