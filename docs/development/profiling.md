# Profiling the detection pipeline

How to capture flamegraphs and per-span timing for the topological grid-build
pipeline (`GraphBuildAlgorithm::Topological`), the shared
`merge_components_local` post-stage, and the marker-decode sweeps.

For the *ranked* output of these tools — the current bottleneck list and the
optimization backlog — see [`performance.md`](performance.md).

## One-command campaign

`scripts/run-perf-campaign.sh` runs the whole matrix (end-to-end latency,
per-stage breakdown, criterion micro-benches, flamegraphs) and lands every
artifact under `bench_results/perf-campaign/` (gitignored):

```bash
bash scripts/run-perf-campaign.sh            # full campaign
FLAME=0 bash scripts/run-perf-campaign.sh    # skip flamegraphs
REPEATS=200 bash scripts/run-perf-campaign.sh
```

Private-dataset numbers stay in `bench_results/` only; `performance.md` carries
general / public-image numbers (disclosure policy). The rest of this page
documents the individual tools the script drives.

## The `profiling` Cargo profile

Use it for any flamegraph capture so symbols resolve in the viewer
(`target/profiling/`):

```toml
[profile.profiling]
inherits = "release"
debug = "line-tables-only"
split-debuginfo = "unpacked"
strip = false
```

## Flamegraphs — samply (macOS default)

`samply` needs no `sudo`/`dtrace`, is native to Apple Silicon, and renders an
interactive call-tree. The detection of a single frame is too short to sample,
so point samply at a binary that *loops* the work — `topo_stage_timing` with a
high `--repeats`, or a criterion bench run under `--profile-time`:

```bash
cargo install samply
cargo build --profile profiling -p calib-targets-bench --bins

# Grid pipeline (loops the detector --repeats times).
samply record --save-only --no-open -o /tmp/topo.json.gz -- \
    ./target/profiling/topo_stage_timing \
    --image-dir testdata/02-topo-grid --repeats 400 --warmup 10 \
    --out /tmp/topo-stage.json

# A criterion decode bench (runs purely for the profiler, no analysis).
cargo bench -p calib-targets-puzzleboard --bench dataset_decode --no-run --profile profiling
samply record --save-only --no-open -o /tmp/decode.json.gz -- \
    ./target/profiling/deps/dataset_decode-<hash> --bench --profile-time 12 'decode'

samply load /tmp/topo.json.gz   # offline viewer
```

`run-perf-campaign.sh` automates all three captures (and resolves the bench
binary path for you). Fallback profiler: `cargo install flamegraph` then
`sudo cargo flamegraph --profile profiling -p calib-targets-bench --bin bench --
run --image testdata/large.png` (uses `dtrace`; may be blocked by SIP).

## Per-span timing — `topo_stage_timing`

The bench crate compiles the `tracing` feature in unconditionally, so
`topo_stage_timing` produces a 14-stage breakdown (corner-detect → input-adapt →
axis-filter → triangulate → classify → merge → 3 quad filters → walk →
component-merge → clustering → recovery → ordering) with p50/p95/mean/max and
git/rustc/CPU metadata — no feature flag needed:

```bash
cargo run --release -p calib-targets-bench --bin topo_stage_timing -- \
    --image-dir testdata/02-topo-grid \
    --orientation-method ring-fit \
    --repeats 100 --warmup 10 \
    --out bench_results/topo-stage.ring_fit.json
```

The instrumented functions (kept in sync manually — update this table and the
crate's `tracing` wiring together when adding a span):

| Crate | Function | Level | Fields |
|---|---|---|---|
| `projective-grid` | `build_grid_topological` | info | `num_corners` |
| `projective-grid` | `merge_components_local` | info | `num_components` |
| `projective-grid` | `shared::validate::validate` | info | `num_labelled`, `cell_size` |
| `projective-grid` | `topological::classify::classify_all_edges` | debug | `num_edges` |
| `projective-grid` | `topological::quads::merge_triangle_pairs` | debug | `num_triangles` |
| `projective-grid` | `topological::topo_filter::filter_quads` | debug | `num_quads_in` |
| `projective-grid` | `topological::walk::label_components` | debug | `num_quads` |
| `projective-grid` | `global_step::estimate_global_cell_size` | debug | `num_points` |
| `projective-grid` | `local_step::estimate_local_steps` | debug | `num_points` |
| `calib-targets-chessboard` | `Detector::detect_*` and inner stages | info / debug | (existing) |

To stream raw span events instead (one enter/exit per call, `time.busy`
wall-clock per span), enable `tracing` on the facade and set `RUST_LOG`:

```bash
RUST_LOG=info cargo run --profile profiling \
    --features "calib-targets/tracing" \
    -p calib-targets-bench --bin bench -- run \
    --image testdata/large.png
```

## Capture matrix

`run-perf-campaign.sh` covers these cells. Capture them manually only when
narrowing a specific regression:

| Image | Resolution | Target | Tool |
|---|---|---|---|
| `testdata/02-topo-grid/*` | ~1 MP chessboards | chessboard | `topo_stage_timing` (ring-fit **and** disk-fit) |
| `testdata/mid.png` | 1024×576 | chessboard | samply flamegraph |
| `testdata/large.png` | 2048×1536 | chessboard | samply flamegraph |
| `puzzleboard_reference/example4.png` | 4032×3024 | puzzleboard | `dataset_decode` bench + flamegraph |
| private charuco set | native | charuco | `charuco/dataset/decode` bench + flamegraph |
