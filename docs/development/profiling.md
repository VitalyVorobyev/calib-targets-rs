# Profiling the detection pipeline

How to capture flamegraphs and per-span timing for the topological grid build,
the shared `merge_components_local` post-stage, and the marker-decode sweeps.

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
`topo_stage_timing` produces a per-stage breakdown (corner detect → input
adaptation → axis filter → triangulation → edge classification → triangle merge
→ three quad filters → walk → component merge → validation → projective fit →
assembly → clustering → recovery → final geometry gate → output assembly) with
p50/p95/mean/max and git/rustc/CPU metadata — no feature flag needed:

```bash
cargo run --release -p calib-targets-bench --bin topo_stage_timing -- \
    --image-dir testdata/02-topo-grid \
    --orientation-method ring-fit \
    --repeats 100 --warmup 10 \
    --out bench_results/topo-stage.ring_fit.json
```

The spans it reads (`crates/calib-targets-bench/src/bin/topo_stage_timing.rs`
maps span name → report field; update both when adding or renaming a span):

| Crate | Span (function, or `name =`) | Level |
|---|---|---|
| `calib-targets` | `detect_corners` | info |
| `calib-targets-chessboard` | `detect_all_topological` (grid total), `trace_topological` | info |
| `calib-targets-chessboard` | `topological_inputs`, `topological_clustered_augs`, `recover_topological_components`, `topological_post_recovery_component_merge`, `chessboard_geometry_only_recovery`, `chessboard_final_geometry_gate`, `build_topological_detections` | debug |
| `projective-grid` | `merge_components_local` (`num_components`), `validate` (`num_labelled`, `cell_size`), `extend_from_labelled`, `extend_via_local_homography`, `extend_via_global_homography` | info |
| `projective-grid` | `topological/square_detector.rs`: `usable_mask`, `topological_component_merge`, `topological_validation`, `topological_projective_fit`, `topological_assembly` | debug |
| `projective-grid` | `delaunay_triangulate`, `classify_all_edges`, `merge_triangle_pairs`, `label_components` (`walk.rs`) | debug |
| `projective-grid` | `topological/filter.rs::filter_quads`: `topological_quad_filter`, `geometry_quad_filter`, `cell_size_quad_filter` | debug |
| `calib-targets-charuco` | `board_match::{match_board (decode total), sample_cells, build_score_matrix, enumerate_hypotheses, emit_markers}`, `detect_with_corners` | info |
| `calib-targets-puzzleboard` | `detect_inner`, `decode_component`, `sample_all_edges`, `decode_edges`, `build` / `fill` / `class_credit` (`decode/tables.rs`), `origin_scan` | info |

To stream raw span events instead (one enter/exit per call, `time.busy`
wall-clock per span), enable `tracing` on the facade and set `RUST_LOG`:

```bash
RUST_LOG=info cargo run --profile profiling \
    --features "calib-targets/tracing" \
    -p calib-targets-bench --bin bench -- run \
    --image testdata/large.png
```

## Per-span timing — `charuco_stage_timing`

The ChArUco counterpart of `topo_stage_timing`. It installs the same per-span
busy-time `tracing` layer, runs the production `detect` path over warmup + timed
repeats on a public fixture (`testdata/small2.png` by default), and attributes
the board-level matcher's wall time across its five internal spans
(`match_board` is the decode total; `sample_cells`, `build_score_matrix`,
`enumerate_hypotheses`, and `emit_markers` are reported as a percentage of it):

```bash
cargo run --release -p calib-targets-bench --bin charuco_stage_timing -- \
    --image testdata/small2.png \
    --config testdata/charuco_detect_config_small.json \
    --repeats 50 --warmup 5 \
    --out bench_results/charuco-stage.json
```

It measures only the production path, so the `diagnostics`-feature per-cell fills
are not included. The JSON report is a local-only artifact.

## Capture matrix

`run-perf-campaign.sh` covers these cells. Capture them manually only when
narrowing a specific regression:

| Image | Resolution | Target | Tool |
|---|---|---|---|
| `testdata/02-topo-grid/*` | ~1 MP chessboards | chessboard | `topo_stage_timing` (ring-fit **and** disk-fit) |
| `testdata/mid.png` | 1024×576 | chessboard | samply flamegraph |
| `testdata/large.png` | 2048×1536 | chessboard | samply flamegraph |
| `testdata/small2.png` | 22×22 board | charuco | `charuco_stage_timing` (board-matcher span breakdown) |
| `puzzleboard_reference/example4.png` | 4032×3024 | puzzleboard | `dataset_decode` bench + flamegraph |
| private charuco set | native | charuco | `charuco/dataset/decode` bench + flamegraph |
