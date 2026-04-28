# Profiling the projective-grid pipelines

This page documents how to capture flamegraphs and per-span timing for the
two grid-build pipelines (`GraphBuildAlgorithm::Topological` and
`GraphBuildAlgorithm::ChessboardV2`) plus the shared
`merge_components_local` post-stage.

## TL;DR

```bash
# 1. Install the profiler (macOS).
cargo install samply

# 2. Build a release binary with debug info.
cargo build --profile profiling -p calib-targets-bench --bin bench

# 3. Capture a flamegraph for one image / one algorithm.
samply record -- ./target/profiling/bench run \
    --algorithm chessboard-v2 \
    --image testdata/large.png \
    --target chessboard

# 4. Capture per-span timing (info-level).
RUST_LOG=info cargo run --profile profiling \
    --features "calib-targets/tracing" \
    -p calib-targets-bench --bin bench -- run \
    --algorithm chessboard-v2 \
    --image testdata/large.png \
    --target chessboard
```

## Profiles

The workspace defines a `profiling` Cargo profile that inherits from
`release` but keeps line-table debug info. Use it for any flamegraph
capture so symbols resolve in the viewer:

```toml
[profile.profiling]
inherits = "release"
debug = "line-tables-only"
split-debuginfo = "unpacked"
strip = false
```

`cargo build --profile profiling` produces binaries in `target/profiling/`.
`cargo run --profile profiling` works the same way.

## Profiler choice — samply (macOS default)

`samply` is the recommended profiler on macOS:

- No `sudo`, no `dtrace`, no SIP issues.
- Native to Apple Silicon.
- Uploads the captured profile to `profile.firefox.com` for an interactive
  flamegraph + call-tree view (no data leaves the local machine — the URL
  encodes a server-side ID for *your* upload only, but you can use
  `samply load profile.json` for fully-offline review instead).

Install once:

```bash
cargo install samply
```

Capture and view:

```bash
samply record -- ./target/profiling/bench run \
    --algorithm topological \
    --image testdata/large.png \
    --target chessboard
# → opens the flamegraph in your browser when the run finishes.
```

To save the raw profile (so it can be re-opened, attached to a PR
description, or compared with a later run):

```bash
samply record -o /tmp/topo-large.json.gz -- <command>
samply load /tmp/topo-large.json.gz   # offline viewer
```

For longer runs prefix `samply record --` to whatever invocation you
already use (criterion, an example, a unit test). The profiling profile
applies to release-style builds; criterion uses release by default, so
just rebuild with `--profile profiling` if you need symbols:

```bash
cargo bench -p projective-grid --no-run --profile profiling
samply record -- ./target/profiling/deps/grow-<hash> --bench
```

## Profiler choice — cargo-flamegraph (fallback)

`cargo-flamegraph` works too but uses `dtrace` on macOS, which usually
needs `sudo` and may be blocked by SIP. Install with
`cargo install flamegraph` and run:

```bash
sudo cargo flamegraph --profile profiling \
    -p calib-targets-bench --bin bench -- \
    run --algorithm chessboard-v2 --image testdata/large.png --target chessboard
```

This produces a `flamegraph.svg` next to your invocation directory. Move
or rename it before the next run; otherwise it will be overwritten.

## Tracing — per-span p50/p95 (continuous metrics)

`projective-grid` and the four detector crates all expose an optional
`tracing` Cargo feature. When enabled, the hot-path entry points are
wrapped in `tracing::instrument` so every call produces an enter/exit
event with field metadata:

| Crate | Function | Span level | Fields |
|---|---|---|---|
| `projective-grid` | `build_grid_topological` | info | `num_corners` |
| `projective-grid` | `square::grow::bfs_grow` | info | `num_corners`, `cell_size` |
| `projective-grid` | `merge_components_local` | info | `num_components` |
| `projective-grid` | `square::validate::validate` | info | `num_labelled`, `cell_size` |
| `projective-grid` | `topological::classify::classify_all_edges` | debug | `num_edges` |
| `projective-grid` | `topological::quads::merge_triangle_pairs` | debug | `num_triangles` |
| `projective-grid` | `topological::topo_filter::filter_quads` | debug | `num_quads_in` |
| `projective-grid` | `topological::walk::label_components` | debug | `num_quads` |
| `projective-grid` | `global_step::estimate_global_cell_size` | debug | `num_points` |
| `projective-grid` | `local_step::estimate_local_steps` | debug | `num_points` |
| `calib-targets-chessboard` | `Detector::detect_*` and inner stages | info / debug | (existing) |

Enable the feature on the bench harness or the facade crate and pick a
log level via `RUST_LOG`:

```bash
# Stage-level only (fast, ~one event per detection).
RUST_LOG=info cargo run --profile profiling \
    --features "calib-targets/tracing" \
    -p calib-targets-bench --bin bench -- run \
    --algorithm chessboard-v2 --image testdata/large.png --target chessboard

# All substeps (more events; per-call detail).
RUST_LOG=debug cargo run --profile profiling \
    --features "calib-targets/tracing" \
    -p calib-targets-bench --bin bench -- run \
    --algorithm topological --image testdata/large.png --target chessboard
```

`init_tracing(false)` in `calib-targets-core` already configures
`FmtSpan::CLOSE`, so each span closes with a `time.busy` field that gives
you wall-clock per call. For batched p50/p95 numbers run a multi-image
sweep through the bench harness and post-process the lines (one JSON
event per span if you pass `init_tracing(true)`).

## Recommended profile capture matrix

For a full pre-optimization snapshot, capture `samply` flamegraphs and a
tracing JSON dump for each `(image, algorithm)` cell:

| Image | Resolution | Target | Algorithms |
|---|---|---|---|
| `testdata/mid.png` | 1024×576 (0.6 MP) | chessboard | topological, chessboard-v2 |
| `testdata/large.png` | 2048×1536 (3.1 MP) | chessboard | topological, chessboard-v2 |
| `testdata/puzzleboard_reference/example4.png` | 4032×3024 (12.2 MP) | puzzleboard | topological, chessboard-v2 |

Keep all output under `bench_results/flamegraphs/` (gitignored). Naming
convention:

```
bench_results/flamegraphs/
    <image_slug>.<algorithm>.flame.json.gz   # samply raw profile
    <image_slug>.<algorithm>.tracing.log     # RUST_LOG output
    REPORT.md                                # ranked findings (local-only)
```

## Updating this document

The instrumented-functions table is a manual list — when adding spans to
a new function, update both the table here and the corresponding crate's
`tracing` feature wiring (the consumer-side `tracing` feature must
propagate down to `projective-grid/tracing` via `calib-targets-core`).
